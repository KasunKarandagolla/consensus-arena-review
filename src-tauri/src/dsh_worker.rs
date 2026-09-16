use crate::verification::VerificationCommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

pub const RESULT_SCHEMA_VERSION: u32 = 1;
pub const QUALIFIED_DSH_VERSION: &str = "0.1.5-rc.1";
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const API_KEY_ENV: &str = "ARENA_DSH_API_KEY";
const MAX_MODEL_OUTPUT_TOKENS: u32 = 4096;
const DSH_PROBE_TIMEOUT_SECONDS: u64 = 5;

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
    let mut child = Command::new(executable)
        .args(args)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    match tokio::time::timeout(
        Duration::from_secs(DSH_PROBE_TIMEOUT_SECONDS),
        collect_process(&mut child),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
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

async fn collect_process(child: &mut Child) -> Result<WorkerExecution, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "DSH stdout pipe was unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "DSH stderr pipe was unavailable".to_string())?;
    let stdout_task = tokio::spawn(read_bounded(stdout));
    let stderr_task = tokio::spawn(read_bounded(stderr));
    let status = child
        .wait()
        .await
        .map_err(|error| format!("wait for DSH: {error}"))?;
    let stdout = stdout_task
        .await
        .map_err(|error| format!("collect DSH stdout: {error}"))??;
    let stderr = stderr_task
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

pub fn bounded_prompt(objective: &str, details: &str, repair: Option<&str>) -> String {
    format!(
        "You are a disposable Consensus Arena implementation worker. Work only inside the current repository.\n\nOBJECTIVE:\n{objective}\n\nARENA CONTRACT:\n{details}\n{}\n\nDo not deploy or access production infrastructure. Do not commit. Do not weaken, remove, skip, or rewrite protected acceptance tests. If a genuine product decision is missing, stop and write a valid .arena-runtime/result.json with status needs_user instead of guessing. When the bounded task is complete, write .arena-runtime/result.json before exiting with schema_version 1, status complete, summary, verification_commands as program/args/relative_cwd objects, and acceptance items. A missing or invalid result file is failure. Your prose is not acceptance authority.",
        repair
            .map(|value| format!("\nPREVIOUS VERIFICATION EVIDENCE:\n{value}"))
            .unwrap_or_default()
    )
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
    let mut command = Command::new(executable);
    command
        .args(["--profile", "headless", "--patch"])
        .arg(&patch_path)
        .arg(prompt)
        .current_dir(worktree)
        .env(API_KEY_ENV, &model.api_key)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dsh_home) = std::env::var_os("ARENA_DSH_HOME") {
        command.env("DSH_HOME", dsh_home);
    }
    let mut child = command.spawn().map_err(|error| {
        format!("could not start DSH; install DSH or configure ARENA_DSH_EXECUTABLE: {error}")
    })?;
    let mut execution = match tokio::time::timeout(
        Duration::from_secs(timeout_seconds),
        collect_process(&mut child),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
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
    if result_file.is_file() {
        let raw = std::fs::read_to_string(&result_file)
            .map_err(|error| format!("read Arena worker result: {error}"))?;
        let parsed: WorkerResultContract = serde_json::from_str(&raw)
            .map_err(|error| format!("invalid Arena worker result: {error}"))?;
        if parsed.schema_version != RESULT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported Arena worker result schema {}",
                parsed.schema_version
            ));
        }
        execution.result = Some(parsed);
    }
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::*;
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
