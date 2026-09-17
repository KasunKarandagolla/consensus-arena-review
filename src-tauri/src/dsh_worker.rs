use crate::verification::VerificationCommand;
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;
use std::{future::Future, pin::Pin};
use tokio::io::AsyncReadExt;
use tokio::process::{ChildStderr, ChildStdout, Command};

pub const RESULT_SCHEMA_VERSION: u32 = 1;
pub const QUALIFIED_DSH_VERSION: &str = "0.1.5-rc.1";
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_RESULT_BYTES: u64 = 256 * 1024;
const API_KEY_ENV: &str = "ARENA_DSH_API_KEY";
const MAX_MODEL_OUTPUT_TOKENS: u32 = 4096;
const DSH_PROBE_TIMEOUT_SECONDS: u64 = 5;
const CHILD_CLEANUP_TIMEOUT_SECONDS: u64 = 5;
const OUTPUT_READER_CLEANUP_TIMEOUT_SECONDS: u64 = 1;

type OutputReaderTask = tokio::task::JoinHandle<Result<String, String>>;
type ContainedChild = Box<dyn ChildWrapper>;

#[cfg(unix)]
struct KillProcessGroupOnDrop {
    child: Option<ContainedChild>,
    guard: Option<ContainedChild>,
    finished: bool,
    termination_requested: bool,
    empty_stdout: Option<ChildStdout>,
    empty_stderr: Option<ChildStderr>,
}

#[cfg(unix)]
impl std::fmt::Debug for KillProcessGroupOnDrop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KillProcessGroupOnDrop")
            .field("finished", &self.finished)
            .field("termination_requested", &self.termination_requested)
            .finish_non_exhaustive()
    }
}

#[cfg(unix)]
impl ChildWrapper for KillProcessGroupOnDrop {
    fn inner(&self) -> &dyn ChildWrapper {
        self.child
            .as_deref()
            .or_else(|| self.guard.as_deref())
            .unwrap_or(self)
    }

    fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
        // When both slots are empty, return the wrapper itself before taking
        // either field borrow. This state is only reachable after internal
        // extraction; retaining the fallback keeps ChildWrapper's contract.
        if self.child.is_none() && self.guard.is_none() {
            return self;
        }
        if let Some(child) = self.child.as_deref_mut() {
            return child;
        }
        if let Some(guard) = self.guard.as_deref_mut() {
            return guard;
        }
        unreachable!("a process-group child slot changed without an intervening mutation")
    }

    fn into_inner(mut self: Box<Self>) -> Box<dyn ChildWrapper> {
        if !self.finished {
            if let Err(error) = self.request_group_termination() {
                tracing::warn!(error = %error, "worker process-group cleanup request failed");
            }
        }
        match (self.child.take(), self.guard.take()) {
            (Some(child), _) => child,
            (None, Some(guard)) => guard,
            (None, None) => self,
        }
    }

    fn stdout(&mut self) -> &mut Option<ChildStdout> {
        match self.child.as_deref_mut() {
            Some(child) => child.stdout(),
            None => &mut self.empty_stdout,
        }
    }

    fn stderr(&mut self) -> &mut Option<ChildStderr> {
        match self.child.as_deref_mut() {
            Some(child) => child.stderr(),
            None => &mut self.empty_stderr,
        }
    }

    fn start_kill(&mut self) -> std::io::Result<()> {
        self.request_group_termination()
    }

    fn id(&self) -> Option<u32> {
        self.child.as_deref().and_then(ChildWrapper::id)
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self.child.as_deref_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }

    fn wait(&mut self) -> Pin<Box<dyn Future<Output = std::io::Result<ExitStatus>> + Send + '_>> {
        Box::pin(async move {
            let Some(child) = self.child.as_deref_mut() else {
                return Err(std::io::Error::other(
                    "worker process group handle is no longer available",
                ));
            };
            let status = child.wait().await;
            let cleanup = async {
                self.request_group_termination()?;
                if let Some(guard) = self.guard.as_deref_mut() {
                    guard.wait().await?;
                }
                Ok::<(), std::io::Error>(())
            }
            .await;
            self.finished = cleanup.is_ok();
            match (status, cleanup) {
                (Ok(status), Ok(())) => Ok(status),
                (Err(error), Ok(())) => Err(error),
                (Ok(_), Err(error)) => Err(error),
                (Err(wait_error), Err(cleanup_error)) => Err(std::io::Error::other(format!(
                    "{wait_error}; worker process-group cleanup failed: {cleanup_error}"
                ))),
            }
        })
    }
}

#[cfg(unix)]
impl KillProcessGroupOnDrop {
    fn request_group_termination(&mut self) -> std::io::Result<()> {
        if self.termination_requested {
            return Ok(());
        }
        let Some(guard) = self.guard.as_deref_mut() else {
            return Ok(());
        };
        match guard.start_kill() {
            Ok(()) => {
                self.termination_requested = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(unix)]
impl Drop for KillProcessGroupOnDrop {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.request_group_termination() {
                tracing::error!(error = %error, "worker process-group cleanup request failed during drop");
            }
        }
    }
}

fn spawn_contained(command: Command) -> std::io::Result<ContainedChild> {
    #[cfg(unix)]
    {
        // Keep a live process in the worker's group until Arena has sent the
        // final group signal. This prevents the numeric PGID from being reused
        // in the interval after the worker leader exits and before cleanup.
        #[cfg(not(test))]
        let mut anchor_command = {
            let executable = std::env::current_exe()?;
            let mut command = Command::new(executable);
            command.arg("--arena-internal-process-group-guard");
            for variable in [
                "LD_LIBRARY_PATH",
                "DYLD_LIBRARY_PATH",
                "DYLD_FALLBACK_LIBRARY_PATH",
            ] {
                if let Some(value) = std::env::var_os(variable) {
                    command.env(variable, value);
                }
            }
            command
        };
        #[cfg(test)]
        let mut anchor_command = {
            let mut command = Command::new("sleep");
            command.arg("86400");
            command
        };
        anchor_command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .env_clear();
        let mut anchor = CommandWrap::from(anchor_command);
        anchor.wrap(ProcessGroup::leader());
        anchor.wrap(KillOnDrop);
        let guard = anchor.spawn()?;
        let group_id = guard
            .id()
            .ok_or_else(|| std::io::Error::other("worker process group guard has no PID"))?;

        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(ProcessGroup::attach_to(group_id));
        wrapped.wrap(KillOnDrop);
        let child = wrapped.spawn()?.into_inner();
        return Ok(Box::new(KillProcessGroupOnDrop {
            child: Some(child),
            guard: Some(guard),
            finished: false,
            termination_requested: false,
            empty_stdout: None,
            empty_stderr: None,
        }));
    }
    #[cfg(not(unix))]
    {
        let mut wrapped = CommandWrap::from(command);
        #[cfg(windows)]
        wrapped.wrap(JobObject);
        wrapped.wrap(KillOnDrop);
        let child = wrapped.spawn()?;
        Ok(child)
    }
}

#[derive(Debug, Clone)]
pub struct DshModelConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Complete,
    NeedsUser,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerQuestion {
    pub text: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_custom: bool,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResultContract {
    pub schema_version: u32,
    pub status: WorkerStatus,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub question: Option<WorkerQuestion>,
    #[serde(default)]
    pub verification_commands: Vec<VerificationCommand>,
    #[serde(default)]
    pub acceptance: Vec<AcceptanceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceItem {
    pub id: String,
    pub description: String,
}

#[derive(Debug)]
pub struct WorkerExecution {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<WorkerResultContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshPrerequisiteStatus {
    pub available: bool,
    pub compatible: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub message: String,
}

fn default_true() -> bool {
    true
}

fn configured_executable() -> PathBuf {
    std::env::var_os("ARENA_DSH_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dsh"))
}

const SAFE_WORKER_ENVIRONMENT: &[&str] = &[
    "PATH",
    "PATHEXT",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "TMP",
    "TEMP",
    "TMPDIR",
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

fn sanitized_environment<I>(vars: I, api_key: Option<&str>) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut safe = Vec::new();
    let mut configured_dsh_home = None;
    for (key, value) in vars {
        if key.eq_ignore_ascii_case(OsStr::new("ARENA_DSH_HOME")) {
            configured_dsh_home = Some(value);
            continue;
        }
        if SAFE_WORKER_ENVIRONMENT
            .iter()
            .any(|allowed| key.eq_ignore_ascii_case(OsStr::new(allowed)))
        {
            safe.push((key, value));
        }
    }
    if let Some(dsh_home) = configured_dsh_home {
        safe.push((OsString::from("DSH_HOME"), dsh_home));
    }
    if let Some(api_key) = api_key {
        safe.push((OsString::from(API_KEY_ENV), OsString::from(api_key)));
    }
    safe
}

fn apply_sanitized_environment(command: &mut Command, api_key: Option<&str>) {
    command.env_clear();
    for (key, value) in sanitized_environment(std::env::vars_os(), api_key) {
        command.env(key, value);
    }
}

#[cfg(windows)]
fn resolve_windows_executable(executable: &Path) -> Result<PathBuf, String> {
    let has_path = executable.components().count() > 1 || executable.is_absolute();
    if has_path {
        if executable.is_file() {
            return Ok(executable.to_path_buf());
        }
        return Err(format!(
            "configured DSH executable does not exist: {executable:?}"
        ));
    }

    let path = std::env::var_os("PATH").unwrap_or_default();
    let path_ext =
        std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    let extensions = std::env::split_paths(&path_ext).collect::<Vec<_>>();
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(executable);
        if candidate.is_file() {
            return Ok(candidate);
        }
        for extension in &extensions {
            let extension = extension.to_string_lossy();
            if !extension.is_empty()
                && candidate.extension().is_some_and(|existing| {
                    existing
                        .to_string_lossy()
                        .eq_ignore_ascii_case(extension.trim_start_matches('.'))
                })
            {
                continue;
            }
            let candidate_with_extension =
                candidate.with_extension(extension.trim_start_matches('.'));
            if candidate_with_extension.is_file() {
                return Ok(candidate_with_extension);
            }
        }
    }
    Err("configured DSH executable was not found on PATH".to_string())
}

#[cfg(windows)]
fn command_entrypoint(executable: &Path) -> Result<(PathBuf, Vec<OsString>), String> {
    let executable = resolve_windows_executable(executable)?;
    let is_cmd_shim = executable
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"));
    if !is_cmd_shim {
        return Ok((executable, Vec::new()));
    }

    let parent = executable
        .parent()
        .ok_or_else(|| "Windows DSH command shim has no parent directory".to_string())?;
    let candidates = [
        parent
            .parent()
            .unwrap_or(parent)
            .join("@deepseek-ai/dsh/lib/bin.js"),
        parent.join("node_modules/@deepseek-ai/dsh/lib/bin.js"),
    ];
    let script = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "Windows DSH .cmd shim is not beside the expected @deepseek-ai/dsh package entrypoint"
                .to_string()
        })?;
    Ok((PathBuf::from("node"), vec![script.into_os_string()]))
}

#[cfg(not(windows))]
fn command_entrypoint(executable: &Path) -> Result<(PathBuf, Vec<OsString>), String> {
    Ok((executable.to_path_buf(), Vec::new()))
}

fn build_command(executable: &Path, args: &[&str]) -> Result<Command, String> {
    let (program, prefix_args) = command_entrypoint(executable)?;
    let mut command = Command::new(program);
    command.args(prefix_args).args(args);
    apply_sanitized_environment(&mut command, None);
    Ok(command)
}

fn reported_version(output: &str) -> Option<String> {
    output
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        })
        .filter_map(|token| token.strip_prefix('v').or(Some(token)))
        .find(|token| {
            token
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && token.contains('.')
        })
        .map(str::to_string)
}

async fn run_probe(executable: &std::path::Path, args: &[&str]) -> Result<WorkerExecution, String> {
    let mut command = build_command(executable, args)?;
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = spawn_contained(command).map_err(|error| error.to_string())?;
    let (mut stdout_task, mut stderr_task) = spawn_output_readers_or_terminate(&mut *child).await?;
    match tokio::time::timeout(
        Duration::from_secs(DSH_PROBE_TIMEOUT_SECONDS),
        collect_process(&mut *child, &mut stdout_task, &mut stderr_task),
    )
    .await
    {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => {
            stop_output_readers(&mut stdout_task, &mut stderr_task).await;
            if let Err(cleanup_error) = terminate_child(&mut *child).await {
                return Err(format!("{error}; {cleanup_error}"));
            }
            Err(error)
        }
        Err(_) => {
            stop_output_readers(&mut stdout_task, &mut stderr_task).await;
            terminate_child(&mut *child)
                .await
                .map_err(|error| format!("probe timed out; {error}"))?;
            Err("probe timed out".to_string())
        }
    }
}

pub async fn check_prerequisite() -> DshPrerequisiteStatus {
    let executable = configured_executable();
    let executable_text = executable.to_string_lossy().into_owned();
    let version_probe = match run_probe(&executable, &["--version"]).await {
        Ok(probe) => probe,
        Err(_) => {
            return DshPrerequisiteStatus {
                available: false,
                compatible: false,
                executable: Some(executable_text),
                version: None,
                message: format!(
                    "Arena needs the external DSH build worker before a build can start. Install DSH {QUALIFIED_DSH_VERSION} or configure ARENA_DSH_EXECUTABLE, then try again."
                ),
            };
        }
    };
    let version = reported_version(&format!(
        "{}\n{}",
        version_probe.stdout, version_probe.stderr
    ));
    if version.as_deref() != Some(QUALIFIED_DSH_VERSION) {
        return DshPrerequisiteStatus {
            available: true,
            compatible: false,
            executable: Some(executable_text),
            version,
            message: format!(
                "The configured DSH build worker is not compatible with Arena V1. Arena requires DSH {QUALIFIED_DSH_VERSION}."
            ),
        };
    }

    let profile_probe = run_probe(&executable, &["--profile", "headless", "--help"]).await;
    let profile_ok = profile_probe
        .as_ref()
        .is_ok_and(|probe| probe.exit_code == Some(0));
    if !profile_ok {
        return DshPrerequisiteStatus {
            available: true,
            compatible: false,
            executable: Some(executable_text),
            version,
            message: "The configured DSH build worker does not expose Arena's headless profile."
                .to_string(),
        };
    }

    DshPrerequisiteStatus {
        available: true,
        compatible: true,
        executable: Some(executable_text),
        version,
        message: format!("DSH {QUALIFIED_DSH_VERSION} is ready for Arena V1 Build mode."),
    }
}

fn bounded_text(mut bytes: Vec<u8>) -> String {
    if bytes.len() > MAX_OUTPUT_BYTES {
        bytes.truncate(MAX_OUTPUT_BYTES);
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        text.push_str("\n[output truncated]");
        text
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

pub fn redact_secret(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[REDACTED]")
    }
}

pub fn contains_secret(text: &str, secret: &str) -> bool {
    !secret.is_empty() && text.contains(secret)
}

async fn read_bounded<R>(reader: R) -> Result<String, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
    reader
        .take((MAX_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("read DSH output: {error}"))?;
    Ok(bounded_text(bytes))
}

fn spawn_output_readers(
    child: &mut dyn ChildWrapper,
) -> Result<(OutputReaderTask, OutputReaderTask), String> {
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| "DSH stdout pipe was unavailable".to_string())?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| "DSH stderr pipe was unavailable".to_string())?;
    Ok((
        tokio::spawn(read_bounded(stdout)),
        tokio::spawn(read_bounded(stderr)),
    ))
}

async fn spawn_output_readers_or_terminate(
    child: &mut dyn ChildWrapper,
) -> Result<(OutputReaderTask, OutputReaderTask), String> {
    match spawn_output_readers(child) {
        Ok(readers) => Ok(readers),
        Err(error) => match terminate_child(child).await {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!("{error}; {cleanup_error}")),
        },
    }
}

async fn collect_process(
    child: &mut dyn ChildWrapper,
    stdout_task: &mut OutputReaderTask,
    stderr_task: &mut OutputReaderTask,
) -> Result<WorkerExecution, String> {
    let status = child
        .wait()
        .await
        .map_err(|error| format!("wait for DSH: {error}"))?;
    let stdout = (&mut *stdout_task)
        .await
        .map_err(|error| format!("collect DSH stdout: {error}"))??;
    let stderr = (&mut *stderr_task)
        .await
        .map_err(|error| format!("collect DSH stderr: {error}"))??;
    Ok(WorkerExecution {
        exit_code: status.code(),
        timed_out: false,
        stdout,
        stderr,
        result: None,
    })
}

async fn stop_output_readers(
    stdout_task: &mut OutputReaderTask,
    stderr_task: &mut OutputReaderTask,
) {
    stdout_task.abort();
    stderr_task.abort();
    let _ = tokio::time::timeout(
        Duration::from_secs(OUTPUT_READER_CLEANUP_TIMEOUT_SECONDS),
        async {
            let _ = (&mut *stdout_task).await;
            let _ = (&mut *stderr_task).await;
        },
    )
    .await;
}

async fn terminate_child(child: &mut dyn ChildWrapper) -> Result<(), String> {
    if let Err(kill_error) = child.start_kill() {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => return Err(format!("could not terminate DSH child: {kill_error}")),
            Err(wait_error) => {
                return Err(format!(
                    "could not terminate DSH child ({kill_error}) or inspect it ({wait_error})"
                ));
            }
        }
    }
    match tokio::time::timeout(
        Duration::from_secs(CHILD_CLEANUP_TIMEOUT_SECONDS),
        child.wait(),
    )
    .await
    {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(format!("could not reap DSH child: {error}")),
        Err(_) => Err("timed out while waiting for the DSH child to exit".to_string()),
    }
}

fn yaml_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn write_runtime_patch(path: &Path, config: &DshModelConfig) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "invalid DSH runtime patch path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| format!("create DSH runtime data: {error}"))?;
    let patch = format!(
        "- id: llm-pi-ai\n  config:\n    providers:\n      arena_primary:\n        apiKeyEnv: {API_KEY_ENV}\n        api: openai-completions\n        baseURL: {}\n        models:\n          - id: {}\n            name: Arena primary\n            contextWindow: 128000\n            maxTokens: {MAX_MODEL_OUTPUT_TOKENS}\n- id: agent-default-model\n  config:\n    provider: arena_primary\n    model: {}\n",
        yaml_string(&config.base_url),
        yaml_string(&config.model),
        yaml_string(&config.model)
    );
    std::fs::write(path, patch).map_err(|error| format!("write DSH runtime patch: {error}"))
}

fn result_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("result.json")
}

fn read_worker_result(result_file: &Path) -> Result<Option<WorkerResultContract>, String> {
    let metadata = match std::fs::symlink_metadata(result_file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("inspect Arena worker result: {error}")),
    };
    if !metadata.is_file() {
        return Err("Arena worker result must be a regular file".to_string());
    }
    if metadata.len() > MAX_RESULT_BYTES {
        return Err("Arena worker result exceeds the size limit".to_string());
    }
    let mut result = std::fs::File::open(result_file)
        .map_err(|error| format!("read Arena worker result: {error}"))?;
    let mut raw = String::new();
    result
        .take(MAX_RESULT_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|error| format!("read Arena worker result: {error}"))?;
    if raw.len() as u64 > MAX_RESULT_BYTES {
        return Err("Arena worker result exceeds the size limit".to_string());
    }
    let parsed: WorkerResultContract = serde_json::from_str(&raw)
        .map_err(|error| format!("invalid Arena worker result: {error}"))?;
    if parsed.schema_version != RESULT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported Arena worker result schema {}",
            parsed.schema_version
        ));
    }
    Ok(Some(parsed))
}

pub fn bounded_prompt(objective: &str, details: &str, repair: Option<&str>) -> String {
    format!(
        "You are a disposable Consensus Arena implementation worker. Work only inside the current repository.\n\nOBJECTIVE:\n{objective}\n\nARENA CONTRACT:\n{details}\n{}\n\nDo not deploy or access production infrastructure. Do not commit. Do not weaken, remove, skip, or rewrite protected acceptance tests. If a genuine product decision is missing, stop and write a valid .arena-runtime/result.json with status needs_user instead of guessing. When the bounded task is complete, write .arena-runtime/result.json before exiting with schema_version 1, status complete, summary, verification_commands as program/args/relative_cwd objects, and acceptance items. A missing or invalid result file is failure. Your prose is not acceptance authority.",
        repair
            .map(|value| format!("\nPREVIOUS VERIFICATION EVIDENCE:\n{value}"))
            .unwrap_or_default()
    )
}

fn attach_worker_result(execution: &mut WorkerExecution, result_file: &Path) -> Result<(), String> {
    if !execution.timed_out {
        execution.result = read_worker_result(result_file)?;
    }
    Ok(())
}

pub async fn run(
    worktree: &Path,
    runtime_dir: &Path,
    patch_dir: &Path,
    model: &DshModelConfig,
    prompt: &str,
    timeout_seconds: u64,
) -> Result<WorkerExecution, String> {
    if model.api_key.trim().is_empty()
        || model.base_url.trim().is_empty()
        || model.model.trim().is_empty()
    {
        return Err("Arena's primary Agent Brain is not configured for Build mode".to_string());
    }
    if runtime_dir != worktree.join(".arena-runtime") {
        return Err("invalid DSH runtime directory".to_string());
    }
    if runtime_dir.exists() {
        std::fs::remove_dir_all(runtime_dir)
            .map_err(|error| format!("reset DSH runtime directory: {error}"))?;
    }
    std::fs::create_dir_all(runtime_dir)
        .map_err(|error| format!("create DSH runtime directory: {error}"))?;
    let patch_path = patch_dir.join("headless.patch.yml");
    write_runtime_patch(&patch_path, model)?;

    let executable = configured_executable();
    let (program, prefix_args) = command_entrypoint(&executable)?;
    let mut command = Command::new(program);
    command
        .args(prefix_args)
        .args(["--profile", "headless", "--patch"])
        .arg(&patch_path)
        .arg(prompt)
        .current_dir(worktree)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    apply_sanitized_environment(&mut command, Some(&model.api_key));
    let mut child = spawn_contained(command).map_err(|error| {
        format!("could not start DSH; install DSH or configure ARENA_DSH_EXECUTABLE: {error}")
    })?;
    let (mut stdout_task, mut stderr_task) = spawn_output_readers_or_terminate(&mut *child).await?;
    let mut execution = match tokio::time::timeout(
        Duration::from_secs(timeout_seconds),
        collect_process(&mut *child, &mut stdout_task, &mut stderr_task),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            stop_output_readers(&mut stdout_task, &mut stderr_task).await;
            if let Err(cleanup_error) = terminate_child(&mut *child).await {
                return Err(format!("{error}; {cleanup_error}"));
            }
            return Err(error);
        }
        Err(_) => {
            stop_output_readers(&mut stdout_task, &mut stderr_task).await;
            terminate_child(&mut *child).await?;
            WorkerExecution {
                exit_code: None,
                timed_out: true,
                stdout: String::new(),
                stderr: "DSH worker timed out".to_string(),
                result: None,
            }
        }
    };
    let result_file = result_path(runtime_dir);
    attach_worker_result(&mut execution, &result_file)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    fn spawn_shell_descendant(pid_file: &Path) -> ContainedChild {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("sleep 60 & child=$!; printf '%s' \"$child\" > \"$1\"; wait")
            .arg("arena-process-tree-fixture")
            .arg(pid_file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_contained(command).expect("spawn contained shell fixture")
    }

    #[cfg(target_os = "linux")]
    fn spawn_exiting_shell_with_descendant(pid_file: &Path) -> ContainedChild {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("sleep 60 >/dev/null 2>&1 & child=$!; printf '%s' \"$child\" > \"$1\"; exit 0")
            .arg("arena-process-tree-exit-fixture")
            .arg(pid_file)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        spawn_contained(command).expect("spawn contained exiting shell fixture")
    }

    #[cfg(target_os = "linux")]
    async fn await_fixture_pid(pid_file: &Path) -> u32 {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(value) = std::fs::read_to_string(pid_file) {
                    if let Ok(pid) = value.parse() {
                        return pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant should publish its PID")
    }

    #[cfg(target_os = "linux")]
    fn spawn_descendant_fixture(pid_file: &Path) -> ContainedChild {
        spawn_shell_descendant(pid_file)
    }

    #[cfg(target_os = "linux")]
    fn process_is_executing(pid: u32) -> bool {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            return true;
        };
        !matches!(fields.split_whitespace().next(), Some("Z" | "X"))
    }

    #[cfg(target_os = "linux")]
    fn process_parent_pid(pid: u32) -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (_, fields) = stat.rsplit_once(") ")?;
        fields.split_whitespace().nth(1)?.parse().ok()
    }

    #[cfg(target_os = "linux")]
    async fn await_reparented_descendant(pid: u32, worker_pid: u32) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if process_is_executing(pid)
                    && process_parent_pid(pid).is_some_and(|parent| parent != worker_pid)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant should be alive and reparented before worker collection");
    }

    #[cfg(target_os = "linux")]
    async fn assert_process_stops(pid: u32) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while process_is_executing(pid) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("descendant should stop executing after group termination");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn process_group_termination_stops_worker_descendants() {
        let root =
            std::env::temp_dir().join(format!("arena process group {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create process group fixture directory");
        let pid_file = root.join("descendant.pid");
        let mut child = spawn_shell_descendant(&pid_file);
        let descendant_pid = await_fixture_pid(&pid_file).await;

        terminate_child(&mut *child)
            .await
            .expect("terminate worker process group");
        assert_process_stops(descendant_pid).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn normal_worker_exit_stops_reparented_descendants() {
        let root = std::env::temp_dir().join(format!(
            "arena process group normal exit {}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create process group fixture directory");
        let pid_file = root.join("descendant.pid");
        let mut child = spawn_exiting_shell_with_descendant(&pid_file);
        let descendant_pid = await_fixture_pid(&pid_file).await;
        let worker_pid = child.id().expect("fixture worker should have a PID");
        await_reparented_descendant(descendant_pid, worker_pid).await;
        let (mut stdout_task, mut stderr_task) = spawn_output_readers_or_terminate(&mut *child)
            .await
            .expect("start fixture output readers");

        let execution = collect_process(&mut *child, &mut stdout_task, &mut stderr_task)
            .await
            .expect("collect successful worker exit");

        assert_eq!(execution.exit_code, Some(0));
        assert_process_stops(descendant_pid).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn dropping_worker_owner_requests_process_group_termination() {
        let root =
            std::env::temp_dir().join(format!("arena process group drop {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create process group fixture directory");
        let pid_file = root.join("descendant.pid");
        let child = spawn_shell_descendant(&pid_file);
        let descendant_pid = await_fixture_pid(&pid_file).await;

        drop(child);
        assert_process_stops(descendant_pid).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    async fn await_fixture_pid(pid_file: &Path) -> u32 {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(value) = std::fs::read_to_string(pid_file) {
                    if let Ok(pid) = value.trim().parse() {
                        return pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("descendant should publish its PID")
    }

    #[cfg(windows)]
    fn spawn_descendant_fixture(pid_file: &Path) -> ContainedChild {
        spawn_powershell_descendant(pid_file)
    }

    #[cfg(windows)]
    fn spawn_powershell_descendant(pid_file: &Path) -> ContainedChild {
        let pid_path = pid_file.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$descendant = Start-Process -FilePath 'ping.exe' -ArgumentList @('127.0.0.1','-t') -PassThru; Set-Content -LiteralPath '{pid_path}' -Value $descendant.Id; Start-Sleep -Seconds 60"
        );
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_contained(command).expect("spawn contained PowerShell fixture")
    }

    #[cfg(windows)]
    fn windows_process_is_running(pid: u32) -> bool {
        let filter = format!("PID eq {pid}");
        let Ok(output) = std::process::Command::new("tasklist.exe")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
        else {
            return true;
        };
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
    }

    #[cfg(windows)]
    async fn assert_windows_process_stops(pid: u32) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while windows_process_is_running(pid) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("Job Object termination should stop its descendant");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn job_object_termination_stops_worker_descendants() {
        let root = std::env::temp_dir().join(format!("arena process job {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create process job fixture directory");
        let pid_file = root.join("descendant.pid");
        let mut child = spawn_powershell_descendant(&pid_file);
        let descendant_pid = await_fixture_pid(&pid_file).await;

        terminate_child(&mut *child)
            .await
            .expect("terminate worker Job Object");
        assert_windows_process_stops(descendant_pid).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[tokio::test]
    async fn session_runtime_owner_abort_terminates_worker_descendants() {
        let root = std::env::temp_dir().join(format!(
            "arena owner abort process tree {}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create owner abort process fixture");
        let pid_file = root.join("descendant.pid");
        let child = spawn_descendant_fixture(&pid_file);
        let descendant_pid = await_fixture_pid(&pid_file).await;

        let runtime = std::sync::Arc::new(crate::session_runtime::SessionRuntime::new());
        let permit = runtime
            .try_acquire_start("process-tree-abort-fixture".to_string())
            .expect("acquire SessionRuntime owner");
        let owner = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let _owned_worker = child;
            if activate_rx.await.is_ok() {
                let _ = started_tx.send(());
                std::future::pending::<()>().await;
            }
        });
        permit
            .commit(handle, activate_tx)
            .expect("commit SessionRuntime worker owner");
        started_rx
            .await
            .expect("SessionRuntime task should become active");

        let stop_guard = runtime
            .stop_owner(&owner)
            .await
            .expect("stop exact SessionRuntime owner")
            .expect("expected owner should be stopped");
        stop_guard.finish();

        #[cfg(target_os = "linux")]
        assert_process_stops(descendant_pid).await;
        #[cfg(windows)]
        assert_windows_process_stops(descendant_pid).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn worker_environment_keeps_runtime_paths_and_drops_unrelated_secrets() {
        let vars = vec![
            (OsString::from("PATH"), OsString::from("/usr/bin")),
            (
                OsString::from("OPENAI_API_KEY"),
                OsString::from("ambient-openai-secret"),
            ),
            (
                OsString::from("AWS_SECRET_ACCESS_KEY"),
                OsString::from("ambient-cloud-secret"),
            ),
            (
                OsString::from("ARENA_DSH_API_KEY"),
                OsString::from("ambient-arena-secret"),
            ),
            (
                OsString::from("ARENA_DSH_HOME"),
                OsString::from("/tmp/qualified-dsh-home"),
            ),
        ];
        let sanitized = sanitized_environment(vars, Some("configured-agent-brain-key"));
        let values = sanitized
            .into_iter()
            .map(|(key, value)| (key.to_string_lossy().into_owned(), value))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(values.get("PATH"), Some(&OsString::from("/usr/bin")));
        assert_eq!(
            values.get("DSH_HOME"),
            Some(&OsString::from("/tmp/qualified-dsh-home"))
        );
        assert_eq!(
            values.get(API_KEY_ENV),
            Some(&OsString::from("configured-agent-brain-key"))
        );
        assert!(!values.contains_key("OPENAI_API_KEY"));
        assert!(!values.contains_key("AWS_SECRET_ACCESS_KEY"));
    }

    #[cfg(windows)]
    #[test]
    fn npm_windows_shim_resolves_to_node_entrypoint_without_a_shell() {
        let root = std::env::temp_dir().join(format!("arena dsh shim {}", uuid::Uuid::new_v4()));
        let shim_dir = root.join("node_modules/.bin");
        let script = root.join("node_modules/@deepseek-ai/dsh/lib/bin.js");
        std::fs::create_dir_all(script.parent().expect("script parent"))
            .expect("create npm package tree");
        std::fs::create_dir_all(&shim_dir).expect("create npm shim directory");
        std::fs::write(shim_dir.join("dsh.cmd"), "@echo off\r\n").expect("write shim marker");
        std::fs::write(&script, "// test entrypoint\n").expect("write package entrypoint");

        let (program, args) = command_entrypoint(&shim_dir.join("dsh.cmd"))
            .expect("resolve npm shim to Node entrypoint");

        assert_eq!(program, PathBuf::from("node"));
        let resolved_script = args
            .first()
            .expect("resolved entrypoint argument")
            .to_string_lossy()
            .replace('\\', "/");
        let expected_script = script.to_string_lossy().replace('\\', "/");
        assert_eq!(resolved_script, expected_script);
        std::fs::remove_dir_all(root).expect("remove test tree");
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "requires the explicitly installed Windows DSH qualification runtime"]
    async fn installed_windows_dsh_passes_arena_prerequisite_probe() {
        let executable = std::env::var_os("ARENA_DSH_EXECUTABLE")
            .expect("Windows DSH qualification must configure ARENA_DSH_EXECUTABLE");
        assert!(
            Path::new(&executable).is_file(),
            "configured DSH executable does not exist"
        );

        let status = check_prerequisite().await;
        println!(
            "DSH_PREREQUISITE_AVAILABLE={} DSH_PREREQUISITE_COMPATIBLE={} DSH_VERSION={:?} MESSAGE={}",
            status.available, status.compatible, status.version, status.message
        );
        assert!(
            status.available,
            "Arena could not launch the installed Windows DSH executable: {}",
            status.message
        );
        assert_eq!(
            status.version.as_deref(),
            Some(QUALIFIED_DSH_VERSION),
            "Arena did not observe the exact qualified DSH version"
        );
        assert!(
            status.compatible,
            "Arena's five-second headless capability prerequisite probe failed: {}",
            status.message
        );
    }

    #[test]
    fn parses_complete_result() {
        let raw = r#"{"schema_version":1,"status":"complete","summary":"ok","verification_commands":[],"acceptance":[{"id":"AC-001","description":"works"}]}"#;
        let result: WorkerResultContract = serde_json::from_str(raw).expect("valid result");
        assert_eq!(result.status, WorkerStatus::Complete);
        assert_eq!(result.acceptance[0].id, "AC-001");
    }
    #[test]
    fn parses_needs_user_result() {
        let raw = r#"{"schema_version":1,"status":"needs_user","summary":"choice","question":{"text":"Which?","options":["A","B"],"allow_custom":true,"reason":"product decision"}}"#;
        let result: WorkerResultContract = serde_json::from_str(raw).expect("valid result");
        assert_eq!(result.status, WorkerStatus::NeedsUser);
        assert_eq!(result.question.expect("question").options.len(), 2);
    }

    #[test]
    fn worker_result_reader_enforces_size_and_regular_file_boundary() {
        let root =
            std::env::temp_dir().join(format!("arena-worker-result-limit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create worker result fixture");
        let result = root.join("result.json");
        std::fs::write(&result, br#"{"schema_version":1,"status":"complete"}"#)
            .expect("write valid bounded result");
        assert!(
            read_worker_result(&result)
                .expect("read valid result")
                .is_some()
        );
        std::fs::write(&result, vec![b' '; MAX_RESULT_BYTES as usize + 1])
            .expect("write oversized result");
        assert!(
            read_worker_result(&result)
                .expect_err("oversized result should be rejected")
                .contains("size limit")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn worker_result_written_before_timeout_is_never_accepted() {
        let root = std::env::temp_dir().join(format!(
            "arena-worker-timeout-result-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create timed-out worker fixture");
        let result_file = root.join("result.json");
        std::fs::write(
            &result_file,
            br#"{"schema_version":1,"status":"complete","summary":"stale success"}"#,
        )
        .expect("write result before simulated timeout");
        let mut execution = WorkerExecution {
            exit_code: None,
            timed_out: true,
            stdout: String::new(),
            stderr: "DSH worker timed out".to_string(),
            result: None,
        };

        attach_worker_result(&mut execution, &result_file)
            .expect("timed-out execution should be handled");

        assert!(execution.result.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn generated_patch_sets_bounded_model_output() {
        let path = std::env::temp_dir().join(format!(
            "arena-dsh-runtime-patch-{}-test.yml",
            std::process::id()
        ));
        let config = DshModelConfig {
            api_key: "test-key".to_string(),
            base_url: "https://example.invalid/v1".to_string(),
            model: "example/model".to_string(),
        };

        write_runtime_patch(&path, &config).expect("runtime patch should be written");
        let patch = std::fs::read_to_string(&path).expect("runtime patch should be readable");

        assert!(patch.contains("apiKeyEnv: ARENA_DSH_API_KEY"));
        assert!(patch.contains("maxTokens: 4096"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reports_supported_dsh_version_from_common_cli_output() {
        assert_eq!(
            reported_version("dsh 0.1.5-rc.1\n"),
            Some("0.1.5-rc.1".to_string())
        );
        assert_eq!(
            reported_version("v0.1.5-rc.1"),
            Some("0.1.5-rc.1".to_string())
        );
    }

    #[test]
    fn does_not_accept_another_dsh_version() {
        assert_ne!(
            reported_version("dsh 0.1.5-rc.2"),
            Some(QUALIFIED_DSH_VERSION.to_string())
        );
    }
}
