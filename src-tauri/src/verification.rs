use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

const MAX_CAPTURE_BYTES: usize = 128 * 1024;
const MAX_COMMAND_ID_LENGTH: usize = 64;
#[cfg(windows)]
const QUALIFIED_NODE_VERSION: &str = "v22.22.2";
#[cfg(windows)]
const QUALIFIED_NPM_VERSION: &str = "10.9.7";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationCommand {
    pub id: String,
    pub program: String,
    pub args: Vec<String>,
    #[serde(rename = "relative_cwd", alias = "cwd")]
    pub cwd: String,
    pub timeout_seconds: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationProfile {
    pub version: u32,
    pub commands: Vec<VerificationCommand>,
    pub protected_paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckReceipt {
    pub id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout_path: String,
    pub stderr_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReceipt {
    pub session_id: String,
    #[serde(default)]
    pub attempt_id: String,
    #[serde(default)]
    pub verification_id: String,
    pub candidate_sha: String,
    #[serde(default)]
    pub acceptance_commit: String,
    pub contract_revision: u32,
    pub profile_hash: String,
    #[serde(default)]
    pub candidate_tree_unchanged: bool,
    pub protected_paths_unchanged: bool,
    pub checks: Vec<CheckReceipt>,
    pub verdict: String,
}

pub fn load_profile(repo: &Path) -> Result<VerificationProfile, String> {
    let path = repo.join(".arena").join("verification.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("read verification profile: {e}"))?;
        let profile: VerificationProfile =
            serde_json::from_str(&raw).map_err(|e| format!("invalid verification profile: {e}"))?;
        validate_profile(&profile, repo)?;
        return Ok(profile);
    }
    let mut commands = vec![VerificationCommand {
        id: "git-diff-check".into(),
        program: "git".into(),
        args: vec!["diff".into(), "--check".into()],
        cwd: ".".into(),
        timeout_seconds: 60,
    }];
    if repo.join("package.json").exists() {
        commands.push(VerificationCommand {
            id: "frontend-build".into(),
            program: "npm".into(),
            args: vec!["run".into(), "build".into()],
            cwd: ".".into(),
            timeout_seconds: 600,
        });
    }
    if repo.join("Cargo.toml").exists() {
        commands.push(VerificationCommand {
            id: "cargo-check".into(),
            program: "cargo".into(),
            args: vec!["check".into()],
            cwd: ".".into(),
            timeout_seconds: 600,
        });
    }
    if repo.join("src-tauri").join("Cargo.toml").exists() {
        commands.push(VerificationCommand {
            id: "tauri-cargo-check".into(),
            program: "cargo".into(),
            args: vec![
                "check".into(),
                "--manifest-path".into(),
                "src-tauri/Cargo.toml".into(),
            ],
            cwd: ".".into(),
            timeout_seconds: 600,
        });
    }
    let profile = VerificationProfile {
        version: 1,
        commands,
        protected_paths: Vec::new(),
    };
    validate_profile(&profile, repo)?;
    Ok(profile)
}

const ALLOWED_PROGRAMS: &[&str] = &[
    "cargo", "npm", "npx", "pnpm", "yarn", "bun", "node", "python", "python3", "pytest", "uv",
    "go", "dotnet", "mvn", "gradle", "gradlew", "git",
];

fn path_is_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.to_string_lossy().trim().is_empty()
        && !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && !path.to_string_lossy().contains(':')
}

fn command_id_allowed(id: &str) -> bool {
    if id.is_empty()
        || id.len() > MAX_COMMAND_ID_LENGTH
        || id == "."
        || id == ".."
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return false;
    }
    let stem = id
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub fn program_allowed(program: &str) -> bool {
    if program.trim().is_empty() || program.starts_with('-') || !path_is_relative(program) {
        return false;
    }
    let normalized = program.replace('\\', "/");
    let name = normalized.strip_prefix("./").unwrap_or(&normalized);
    !name.contains('/') && ALLOWED_PROGRAMS.contains(&name)
}

pub fn validate_command(repo: &Path, command: &VerificationCommand) -> Result<(), String> {
    if !command_id_allowed(&command.id) || !program_allowed(&command.program) {
        return Err(format!(
            "verification command {} uses a disallowed program",
            command.id
        ));
    }
    if !path_is_relative(&command.cwd) {
        return Err(format!(
            "verification command {} has an invalid relative_cwd",
            command.id
        ));
    }
    let cwd = repo.join(&command.cwd);
    let canonical_repo = repo
        .canonicalize()
        .map_err(|error| format!("resolve verification worktree: {error}"))?;
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|error| format!("resolve verification cwd for {}: {error}", command.id))?;
    if !canonical_cwd.starts_with(&canonical_repo) {
        return Err(format!(
            "verification cwd for {} escapes the worktree",
            command.id
        ));
    }
    if command.timeout_seconds == 0 || command.timeout_seconds > 3600 {
        return Err(format!("invalid timeout for {}", command.id));
    }
    Ok(())
}

pub fn validate_profile(profile: &VerificationProfile, repo: &Path) -> Result<(), String> {
    if profile.version != 1 {
        return Err(format!(
            "unsupported verification profile version {}",
            profile.version
        ));
    }
    if profile.commands.is_empty() {
        return Err("verification profile must contain at least one required check".to_string());
    }
    let mut command_ids = HashSet::new();
    for command in &profile.commands {
        if !command_id_allowed(&command.id)
            || command.program.trim().is_empty()
            || !path_is_relative(&command.cwd)
        {
            return Err(format!("invalid verification command {}", command.id));
        }
        if !command_ids.insert(command.id.to_ascii_lowercase()) {
            return Err(format!("duplicate verification command id {}", command.id));
        }
        validate_command(repo, command)?;
    }
    let mut protected_paths = HashSet::new();
    for path in &profile.protected_paths {
        if !path_is_relative(path) {
            return Err(format!("protected path escapes the worktree: {path}"));
        }
        if !protected_paths.insert(path.clone()) {
            return Err(format!("duplicate protected path: {path}"));
        }
        resolve_protected_file(repo, path)?;
    }
    Ok(())
}

pub fn profile_hash(profile: &VerificationProfile) -> Result<String, String> {
    hash_bytes(&serde_json::to_vec(profile).map_err(|e| e.to_string())?)
}
pub fn protected_hashes(repo: &Path, paths: &[String]) -> Result<Vec<(String, String)>, String> {
    paths
        .iter()
        .map(|path| {
            let protected_file = resolve_protected_file(repo, path)?;
            Ok((path.clone(), hash_file(&protected_file)?))
        })
        .collect()
}

fn resolve_protected_file(repo: &Path, relative: &str) -> Result<PathBuf, String> {
    if !path_is_relative(relative) {
        return Err(format!("protected path escapes the worktree: {relative}"));
    }
    let canonical_repo = repo
        .canonicalize()
        .map_err(|error| format!("resolve protected-file root: {error}"))?;
    let mut current = canonical_repo.clone();
    let components = Path::new(relative)
        .components()
        .filter_map(|component| match component {
            std::path::Component::CurDir => None,
            std::path::Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(format!("protected path is not a file: {relative}"));
    }
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| format!("inspect protected path {relative}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("protected path contains a symlink: {relative}"));
        }
        let is_last = index + 1 == components.len();
        if (is_last && !metadata.is_file()) || (!is_last && !metadata.is_dir()) {
            return Err(format!("protected path is not a regular file: {relative}"));
        }
    }
    let canonical_file = current
        .canonicalize()
        .map_err(|error| format!("resolve protected path {relative}: {error}"))?;
    if !canonical_file.starts_with(&canonical_repo) {
        return Err(format!("protected path escapes the worktree: {relative}"));
    }
    Ok(canonical_file)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("inspect protected path {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "protected path is not a regular file: {}",
            path.display()
        ));
    }
    hash_bytes(
        &std::fs::read(path).map_err(|e| format!("read protected path {}: {e}", path.display()))?,
    )
}

pub fn protected_files_unchanged(repo: &Path, before: &[(String, String)]) -> bool {
    before.iter().all(|(path, hash)| {
        resolve_protected_file(repo, path)
            .and_then(|resolved| hash_file(&resolved))
            .map(|current| current == *hash)
            .unwrap_or(false)
    })
}
fn hash_bytes(bytes: &[u8]) -> Result<String, String> {
    let mut h = Sha256::new();
    h.update(bytes);
    Ok(format!("{:x}", h.finalize()))
}

fn receipt_verdict(
    executed_checks: usize,
    any_failure: bool,
    any_inconclusive: bool,
) -> &'static str {
    if executed_checks == 0 {
        "inconclusive"
    } else if any_failure {
        "fail"
    } else if any_inconclusive {
        "inconclusive"
    } else {
        "pass"
    }
}

fn bound_output(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() > MAX_CAPTURE_BYTES {
        bytes.truncate(MAX_CAPTURE_BYTES);
        bytes.extend_from_slice(b"\n[output truncated]\n");
    }
    bytes
}

fn verification_output_notice(channel: &str, captured_bytes: usize) -> Vec<u8> {
    format!(
        "Raw verifier {channel} intentionally not persisted; {captured_bytes} buffered bytes (may include truncation marker).\n"
    )
    .into_bytes()
}

async fn read_bounded<R>(reader: R) -> Result<Vec<u8>, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(MAX_CAPTURE_BYTES.min(8192));
    reader
        .take((MAX_CAPTURE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("read verification output: {error}"))?;
    Ok(bound_output(bytes))
}

async fn collect_bounded_output(task: &mut JoinHandle<Result<Vec<u8>, String>>) -> Vec<u8> {
    match tokio::time::timeout(Duration::from_secs(1), &mut *task).await {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(_))) | Ok(Err(_)) | Err(_) => {
            if !task.is_finished() {
                task.abort();
            }
            Vec::new()
        }
    }
}

async fn terminate_and_reap(child: &mut Child, label: &str) -> Result<(), String> {
    let kill_error = child.start_kill().err();
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(format!("could not reap {label}: {error}")),
        Err(_) => Err(match kill_error {
            Some(error) => format!("could not terminate {label}: {error}; wait timed out"),
            None => format!("wait for terminated {label} timed out"),
        }),
    }
}

#[cfg(windows)]
fn windows_worktree_root(cwd: &Path) -> Result<PathBuf, String> {
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|error| format!("resolve Windows verification cwd: {error}"))?;
    canonical_cwd
        .ancestors()
        .find(|path| path.join(".git").exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not find the Git worktree for npm verification".to_string())
}

#[cfg(windows)]
fn windows_path_is_within(path: &Path, root: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    let path = normalize(path);
    let root = normalize(root);
    path == root || path.starts_with(&format!("{root}\\"))
}

#[cfg(windows)]
fn windows_process_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or(path)
}

#[cfg(windows)]
fn windows_node_npm_pair(directory: &Path, worktree_root: &Path) -> Option<(PathBuf, PathBuf)> {
    let node = directory.join("node.exe").canonicalize().ok()?;
    if !node.is_file() || windows_path_is_within(&node, worktree_root) {
        return None;
    }
    let npm_layouts = [
        directory
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
        directory
            .join("npm")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
    ];
    npm_layouts.into_iter().find_map(|path| {
        let cli = path.canonicalize().ok()?;
        (cli.is_file() && !windows_path_is_within(&cli, worktree_root)).then_some((
            windows_process_path(node.clone()),
            windows_process_path(cli),
        ))
    })
}

#[cfg(windows)]
fn windows_resolve_npm_pair(cwd: &Path) -> Result<(PathBuf, PathBuf), String> {
    let worktree_root = windows_worktree_root(cwd)?;
    let directories = if let Some(configured) = std::env::var_os("NODE_EXE") {
        let node = PathBuf::from(configured)
            .canonicalize()
            .map_err(|error| format!("NODE_EXE does not resolve to Node: {error}"))?;
        if !node
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("node.exe"))
            || windows_path_is_within(&node, &worktree_root)
        {
            return Err(
                "NODE_EXE must identify Node outside the verification worktree".to_string(),
            );
        }
        vec![
            node.parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "NODE_EXE has no containing installation directory".to_string())?,
        ]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|directory| !directory.as_os_str().is_empty())
            .collect::<Vec<_>>()
    };
    directories
        .iter()
        .find_map(|directory| windows_node_npm_pair(directory, &worktree_root))
        .ok_or_else(|| {
            "could not resolve a trusted Node/npm installation outside the verification worktree"
                .to_string()
        })
}

#[cfg(windows)]
async fn windows_probe_version(
    program: &Path,
    args: &[OsString],
    cwd: &Path,
    label: &str,
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start Windows {label} version probe: {error}"))?;
    let Some(stdout) = child.stdout.take() else {
        let cleanup = terminate_and_reap(&mut child, "Windows version probe child").await;
        return Err(format!(
            "Windows {label} version probe has no stdout pipe; cleanup: {cleanup:?}"
        ));
    };
    let stderr = child.stderr.take();
    let mut stdout_task = tokio::spawn(read_bounded(stdout));
    let mut stderr_task = stderr.map(|pipe| tokio::spawn(read_bounded(pipe)));
    let exit = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(exit)) => exit,
        Ok(Err(error)) => {
            let cleanup = terminate_and_reap(&mut child, "Windows version probe child").await;
            stdout_task.abort();
            if let Some(task) = stderr_task.take() {
                task.abort();
            }
            return Err(format!(
                "could not wait for Windows {label} version probe: {error}; cleanup: {cleanup:?}"
            ));
        }
        Err(_) => {
            let cleanup = terminate_and_reap(&mut child, "Windows version probe child").await;
            stdout_task.abort();
            if let Some(task) = stderr_task.take() {
                task.abort();
            }
            return Err(format!(
                "timed out checking the resolved Windows {label} version; cleanup: {cleanup:?}"
            ));
        }
    };
    let output = collect_bounded_output(&mut stdout_task).await;
    let error_output = if let Some(mut task) = stderr_task.take() {
        collect_bounded_output(&mut task).await
    } else {
        Vec::new()
    };
    if !exit.success() {
        let detail = [
            String::from_utf8_lossy(&error_output).trim().to_string(),
            String::from_utf8_lossy(&output).trim().to_string(),
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
        return Err(if detail.is_empty() {
            format!("resolved Windows {label} version probe failed")
        } else {
            format!("resolved Windows {label} version probe failed: {detail}")
        });
    }
    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

#[cfg(windows)]
async fn resolve_npm_execution(cwd: &Path) -> Result<(PathBuf, PathBuf), String> {
    let (node, npm_cli) = windows_resolve_npm_pair(cwd)?;
    let node_version =
        windows_probe_version(&node, &[OsString::from("--version")], cwd, "Node").await?;
    if node_version != QUALIFIED_NODE_VERSION {
        return Err(format!(
            "Windows npm verification requires Node {QUALIFIED_NODE_VERSION}; resolved Node {node_version}"
        ));
    }
    let npm_version = windows_probe_version(
        &node,
        &[
            npm_cli.as_os_str().to_os_string(),
            OsString::from("--version"),
        ],
        cwd,
        "npm",
    )
    .await?;
    if npm_version != QUALIFIED_NPM_VERSION {
        return Err(format!(
            "Windows npm verification requires npm {QUALIFIED_NPM_VERSION}; resolved npm {npm_version}"
        ));
    }
    Ok((node, npm_cli))
}

async fn command_for_execution(
    command: &VerificationCommand,
    cwd: &Path,
) -> Result<(OsString, Vec<OsString>), String> {
    #[cfg(windows)]
    if command.program == "npm" {
        let (node, cli) = resolve_npm_execution(cwd).await?;
        let mut args = vec![cli.into_os_string()];
        args.extend(command.args.iter().map(OsString::from));
        return Ok((node.into_os_string(), args));
    }

    #[cfg(not(windows))]
    let _ = cwd;
    Ok((
        OsString::from(command.program.as_str()),
        command
            .args
            .iter()
            .map(|argument| OsString::from(argument.as_str()))
            .collect(),
    ))
}

async fn run_check(
    command: &VerificationCommand,
    cwd: PathBuf,
) -> (String, Option<i32>, Vec<u8>, Vec<u8>) {
    let (program, args) = match command_for_execution(command, &cwd).await {
        Ok(resolved) => resolved,
        Err(error) => {
            return (
                "inconclusive".to_string(),
                None,
                Vec::new(),
                error.into_bytes(),
            );
        }
    };
    let mut child: Child = match Command::new(&program)
        .args(&args)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return (
                "inconclusive".to_string(),
                None,
                Vec::new(),
                error.to_string().into_bytes(),
            );
        }
    };

    let Some(stdout) = child.stdout.take() else {
        let _ = terminate_and_reap(&mut child, "verification child").await;
        return (
            "inconclusive".to_string(),
            None,
            Vec::new(),
            b"verification stdout pipe was unavailable".to_vec(),
        );
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = terminate_and_reap(&mut child, "verification child").await;
        return (
            "inconclusive".to_string(),
            None,
            Vec::new(),
            b"verification stderr pipe was unavailable".to_vec(),
        );
    };

    let mut stdout_task = tokio::spawn(read_bounded(stdout));
    let mut stderr_task = tokio::spawn(read_bounded(stderr));
    let wait =
        tokio::time::timeout(Duration::from_secs(command.timeout_seconds), child.wait()).await;

    let (status, exit_code) = match wait {
        Ok(Ok(exit)) => (
            if exit.success() {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            exit.code(),
        ),
        Ok(Err(_error)) => {
            let _ = terminate_and_reap(&mut child, "verification child").await;
            ("inconclusive".to_string(), None)
        }
        Err(_) => {
            let _ = terminate_and_reap(&mut child, "verification child").await;
            ("inconclusive".to_string(), None)
        }
    };
    let stdout = collect_bounded_output(&mut stdout_task).await;
    let stderr = collect_bounded_output(&mut stderr_task).await;
    (status, exit_code, stdout, stderr)
}

pub async fn candidate_sha(repo: &Path) -> Result<String, String> {
    let out = crate::git_runtime::output(repo, &["rev-parse", "HEAD"]).await?;
    if !out.status.success() {
        return Err("candidate has no Git HEAD".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub async fn verify(
    session_id: &str,
    attempt_id: &str,
    acceptance_commit: &str,
    repo: &Path,
    profile: &VerificationProfile,
    protected_before: &[(String, String)],
    evidence: &Path,
    contract_revision: u32,
) -> Result<VerificationReceipt, String> {
    validate_profile(profile, repo)?;
    let initial_candidate_sha = candidate_sha(repo).await?;
    let initial_status =
        crate::git_runtime::output(repo, &["status", "--porcelain", "--untracked-files=all"])
            .await?;
    let initial_tree_clean = initial_status.status.success() && initial_status.stdout.is_empty();
    let verification_id = uuid::Uuid::new_v4().to_string();
    let evidence = evidence.join(&verification_id);
    std::fs::create_dir_all(&evidence).map_err(|e| e.to_string())?;
    let mut checks = Vec::new();
    let mut any_failure = false;
    let mut any_inconclusive = false;
    for command in &profile.commands {
        let started = Instant::now();
        let cwd = repo.join(PathBuf::from(&command.cwd));
        let (status, exit_code, stdout, mut stderr) = run_check(command, cwd).await;
        if status == "inconclusive" && stderr.is_empty() {
            stderr = b"verification timed out".to_vec();
        }
        let stdout_path = evidence.join(format!("{}.stdout", command.id));
        let stderr_path = evidence.join(format!("{}.stderr", command.id));
        let stdout_notice = verification_output_notice("stdout", stdout.len());
        let stderr_notice = verification_output_notice("stderr", stderr.len());
        std::fs::write(&stdout_path, bound_output(stdout_notice)).map_err(|e| e.to_string())?;
        std::fs::write(&stderr_path, bound_output(stderr_notice)).map_err(|e| e.to_string())?;
        if status == "fail" {
            any_failure = true;
        }
        if status == "inconclusive" {
            any_inconclusive = true;
        }
        checks.push(CheckReceipt {
            id: command.id.clone(),
            status: status.into(),
            exit_code,
            duration_ms: started.elapsed().as_millis(),
            stdout_path: stdout_path.to_string_lossy().into(),
            stderr_path: stderr_path.to_string_lossy().into(),
        });
    }
    let final_candidate_sha = candidate_sha(repo).await?;
    let final_status =
        crate::git_runtime::output(repo, &["status", "--porcelain", "--untracked-files=all"])
            .await?;
    let candidate_tree_unchanged = initial_tree_clean
        && final_status.status.success()
        && final_status.stdout.is_empty()
        && final_candidate_sha == initial_candidate_sha;
    if !candidate_tree_unchanged {
        any_failure = true;
    }
    let protected_paths_unchanged = protected_files_unchanged(repo, protected_before);
    if !protected_paths_unchanged {
        any_failure = true;
    }
    let executed_checks = checks.len();
    let receipt = VerificationReceipt {
        session_id: session_id.into(),
        attempt_id: attempt_id.into(),
        verification_id,
        candidate_sha: initial_candidate_sha,
        acceptance_commit: acceptance_commit.into(),
        contract_revision,
        profile_hash: profile_hash(profile)?,
        candidate_tree_unchanged,
        protected_paths_unchanged,
        checks,
        verdict: receipt_verdict(executed_checks, any_failure, any_inconclusive).into(),
    };
    std::fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    const TEST_PYTHON: &str = "python3";
    #[cfg(windows)]
    const TEST_PYTHON: &str = "python";

    fn fixture_git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("start fixture Git command");
        assert!(
            output.status.success(),
            "fixture Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn verifier_fixture(name: &str) -> (PathBuf, String) {
        let repo = std::env::temp_dir().join(format!(
            "consensus arena verifier {name} {}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("create verifier fixture");
        fixture_git(&repo, &["init"]);
        fixture_git(&repo, &["config", "user.email", "arena@example.invalid"]);
        fixture_git(&repo, &["config", "user.name", "Arena Verifier Fixture"]);
        std::fs::write(repo.join("tracked.txt"), "candidate\n").expect("write candidate");
        fixture_git(&repo, &["add", "."]);
        fixture_git(&repo, &["commit", "-m", "candidate"]);
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .expect("resolve fixture HEAD");
        assert!(output.status.success());
        let candidate = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (repo, candidate)
    }
    #[test]
    fn profile_hash_is_stable() {
        let p = VerificationProfile {
            version: 1,
            commands: vec![],
            protected_paths: vec![],
        };
        assert_eq!(profile_hash(&p).unwrap().len(), 64);
    }

    #[test]
    fn rejects_shell_wrappers_and_parent_paths() {
        assert!(!program_allowed("bash"));
        assert!(!program_allowed("sh"));
        assert!(!program_allowed("../cargo"));
        assert!(!path_is_relative("../outside"));
    }

    #[test]
    fn accepts_structured_development_runner_names() {
        assert!(program_allowed("cargo"));
        assert!(program_allowed("./gradlew"));
        assert!(!program_allowed("node_modules/.bin/tester"));
    }

    #[test]
    fn rejects_unsafe_receipt_command_ids() {
        assert!(!command_id_allowed("../outside"));
        assert!(!command_id_allowed("nested/check"));
        assert!(!command_id_allowed("CON"));
        assert!(!command_id_allowed("receipt:check"));
        assert!(command_id_allowed("frontend-build.v1"));
    }

    #[test]
    fn verification_output_is_bounded() {
        let output = bound_output(vec![b'x'; MAX_CAPTURE_BYTES + 1]);
        assert!(output.len() <= MAX_CAPTURE_BYTES + b"\n[output truncated]\n".len());
        assert!(output.ends_with(b"\n[output truncated]\n"));
    }

    #[test]
    fn verifier_output_notice_never_contains_captured_content() {
        let notice = verification_output_notice("stdout", 1234);
        let notice = String::from_utf8(notice).expect("UTF-8 notice");
        assert_eq!(
            notice,
            "Raw verifier stdout intentionally not persisted; 1234 buffered bytes (may include truncation marker).\n"
        );
    }

    #[test]
    fn aggregate_verdict_precedence_is_explicit() {
        assert_eq!(receipt_verdict(2, false, false), "pass");
        assert_eq!(receipt_verdict(1, true, false), "fail");
        assert_eq!(receipt_verdict(1, false, true), "inconclusive");
        assert_eq!(receipt_verdict(2, true, true), "fail");
        assert_eq!(receipt_verdict(2, false, true), "inconclusive");
        assert_eq!(receipt_verdict(2, true, false), "fail");
        assert_eq!(receipt_verdict(0, false, false), "inconclusive");
    }

    #[test]
    fn rejects_empty_and_duplicate_profile_entries() {
        let root = std::env::temp_dir().join(format!(
            "consensus-arena-verification-profile-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test directory");
        std::fs::write(root.join("protected.txt"), "protected").expect("write protected file");
        let command = || VerificationCommand {
            id: "check".to_string(),
            program: "git".to_string(),
            args: vec!["status".to_string()],
            cwd: ".".to_string(),
            timeout_seconds: 10,
        };
        assert!(
            validate_profile(
                &VerificationProfile {
                    version: 1,
                    commands: Vec::new(),
                    protected_paths: Vec::new(),
                },
                &root
            )
            .is_err()
        );
        let mut case_collisions = vec![command(), command()];
        case_collisions[0].id = "Build".to_string();
        case_collisions[1].id = "build".to_string();
        assert!(
            validate_profile(
                &VerificationProfile {
                    version: 1,
                    commands: case_collisions,
                    protected_paths: Vec::new(),
                },
                &root
            )
            .is_err()
        );
        assert!(
            validate_profile(
                &VerificationProfile {
                    version: 1,
                    commands: vec![command(), command()],
                    protected_paths: Vec::new(),
                },
                &root
            )
            .is_err()
        );
        assert!(
            validate_profile(
                &VerificationProfile {
                    version: 1,
                    commands: vec![command()],
                    protected_paths: vec![String::new()],
                },
                &root
            )
            .is_err()
        );
        assert!(
            validate_profile(
                &VerificationProfile {
                    version: 1,
                    commands: vec![command()],
                    protected_paths: vec!["protected.txt".to_string(), "protected.txt".to_string()],
                },
                &root
            )
            .is_err()
        );
        std::fs::remove_dir_all(&root).expect("remove test directory");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn production_verifier_launches_npm_without_a_cmd_shell() {
        let command = VerificationCommand {
            id: "npm-version".to_string(),
            program: "npm".to_string(),
            args: vec!["--version".to_string()],
            cwd: ".".to_string(),
            timeout_seconds: 10,
        };
        let (status, exit_code, stdout, stderr) = run_check(
            &command,
            std::env::current_dir().expect("current directory"),
        )
        .await;
        assert_eq!(status, "pass", "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(exit_code, Some(0));
        assert!(!stdout.is_empty());
    }

    #[test]
    fn protected_hash_change_is_detected() {
        let root = std::env::temp_dir().join(format!(
            "consensus-arena-verification-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test directory");
        let path = root.join("acceptance.test");
        std::fs::write(&path, "before").expect("write initial file");
        let relative = "acceptance.test".to_string();
        let before = protected_hashes(&root, std::slice::from_ref(&relative)).expect("hash file");
        assert!(protected_files_unchanged(&root, &before));
        std::fs::write(&path, "after").expect("change protected file");
        assert!(!protected_files_unchanged(&root, &before));
        std::fs::remove_dir_all(&root).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn protected_paths_reject_symlink_identity() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "consensus-arena-verification-symlink-{}",
            std::process::id()
        ));
        let outside = root.with_extension("outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&root).expect("create test directory");
        std::fs::write(&outside, "protected").expect("write outside target");
        symlink(&outside, root.join("acceptance.test")).expect("create protected symlink");
        assert!(protected_hashes(&root, &["acceptance.test".to_string()]).is_err());
        std::fs::remove_dir_all(&root).expect("remove test directory");
        std::fs::remove_file(&outside).expect("remove outside target");
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn production_verifier_records_pass_fail_timeout_and_attempt_identity() {
        let (repo, candidate) = verifier_fixture("outcomes");
        let profile = VerificationProfile {
            version: 1,
            commands: vec![
                VerificationCommand {
                    id: "pass".to_string(),
                    program: TEST_PYTHON.to_string(),
                    args: vec![
                        "-c".to_string(),
                        "print('not-persisted-test-marker')".to_string(),
                    ],
                    cwd: ".".to_string(),
                    timeout_seconds: 5,
                },
                VerificationCommand {
                    id: "fail".to_string(),
                    program: TEST_PYTHON.to_string(),
                    args: vec!["-c".to_string(), "raise SystemExit(7)".to_string()],
                    cwd: ".".to_string(),
                    timeout_seconds: 5,
                },
                VerificationCommand {
                    id: "timeout".to_string(),
                    program: TEST_PYTHON.to_string(),
                    args: vec!["-c".to_string(), "import time; time.sleep(5)".to_string()],
                    cwd: ".".to_string(),
                    timeout_seconds: 1,
                },
            ],
            protected_paths: Vec::new(),
        };
        let evidence = repo
            .parent()
            .expect("fixture parent")
            .join("outcomes-evidence");
        let receipt = verify(
            "runtime-session",
            "runtime-session/attempt/2",
            &candidate,
            &repo,
            &profile,
            &[],
            &evidence,
            1,
        )
        .await
        .expect("run production verifier");
        assert_eq!(receipt.session_id, "runtime-session");
        assert_eq!(receipt.attempt_id, "runtime-session/attempt/2");
        assert_eq!(receipt.acceptance_commit, candidate);
        assert!(!receipt.verification_id.is_empty());
        assert_eq!(receipt.candidate_sha, candidate);
        assert_eq!(
            receipt.profile_hash,
            profile_hash(&profile).expect("profile hash")
        );
        assert!(receipt.candidate_tree_unchanged);
        assert_eq!(receipt.verdict, "fail");
        assert_eq!(receipt.checks[0].status, "pass");
        let stdout_evidence = std::fs::read_to_string(&receipt.checks[0].stdout_path)
            .expect("read stdout evidence notice");
        assert!(stdout_evidence.contains("intentionally not persisted"));
        assert!(!stdout_evidence.contains("not-persisted-test-marker"));
        assert_eq!(receipt.checks[1].status, "fail");
        assert_eq!(receipt.checks[1].exit_code, Some(7));
        assert_eq!(receipt.checks[2].status, "inconclusive");
        let disk: VerificationReceipt = serde_json::from_slice(
            &std::fs::read(evidence.join(&receipt.verification_id).join("receipt.json"))
                .expect("read durable receipt"),
        )
        .expect("parse durable receipt");
        assert_eq!(disk.attempt_id, receipt.attempt_id);
        assert_eq!(disk.verification_id, receipt.verification_id);
        assert_eq!(disk.acceptance_commit, receipt.acceptance_commit);
        assert_eq!(disk.candidate_sha, receipt.candidate_sha);
        assert_eq!(disk.profile_hash, receipt.profile_hash);
        assert_eq!(disk.verdict, receipt.verdict);
        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(evidence);
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn production_verifier_fails_when_a_passing_check_mutates_candidate_tree() {
        let (repo, candidate) = verifier_fixture("mutation");
        let profile = VerificationProfile {
            version: 1,
            commands: vec![VerificationCommand {
                id: "mutate".to_string(),
                program: TEST_PYTHON.to_string(),
                args: vec![
                    "-c".to_string(),
                    "from pathlib import Path; Path('tracked.txt').write_text('changed\\n')"
                        .to_string(),
                ],
                cwd: ".".to_string(),
                timeout_seconds: 5,
            }],
            protected_paths: Vec::new(),
        };
        let evidence = repo
            .parent()
            .expect("fixture parent")
            .join("mutation-evidence");
        let receipt = verify(
            "mutation-session",
            "mutation-session/attempt/0",
            &candidate,
            &repo,
            &profile,
            &[],
            &evidence,
            1,
        )
        .await
        .expect("run production verifier");
        assert_eq!(receipt.candidate_sha, candidate);
        assert_eq!(receipt.verdict, "fail");
        assert!(!receipt.candidate_tree_unchanged);
        assert_eq!(receipt.checks[0].status, "pass");
        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(evidence);
    }
}
