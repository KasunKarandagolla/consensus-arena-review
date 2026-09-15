use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

const MAX_CAPTURE_BYTES: usize = 128 * 1024;

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
    pub candidate_sha: String,
    pub contract_revision: u32,
    pub profile_hash: String,
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
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && !path.to_string_lossy().contains(':')
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
    if command.id.trim().is_empty() || !program_allowed(&command.program) {
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
    for command in &profile.commands {
        if command.id.trim().is_empty()
            || command.program.trim().is_empty()
            || !path_is_relative(&command.cwd)
        {
            return Err(format!("invalid verification command {}", command.id));
        }
        validate_command(repo, command)?;
    }
    for path in &profile.protected_paths {
        if !path_is_relative(path) {
            return Err(format!("protected path escapes the worktree: {path}"));
        }
        let resolved = repo.join(path);
        if !resolved.exists() {
            return Err(format!("protected path does not exist: {path}"));
        }
    }
    Ok(())
}

pub fn profile_hash(profile: &VerificationProfile) -> Result<String, String> {
    hash_bytes(&serde_json::to_vec(profile).map_err(|e| e.to_string())?)
}
pub fn protected_hashes(repo: &Path, paths: &[String]) -> Result<Vec<(String, String)>, String> {
    paths
        .iter()
        .map(|p| Ok((p.clone(), hash_file(&repo.join(p))?)))
        .collect()
}
fn hash_file(path: &Path) -> Result<String, String> {
    hash_bytes(
        &std::fs::read(path).map_err(|e| format!("read protected path {}: {e}", path.display()))?,
    )
}

pub fn protected_files_unchanged(repo: &Path, before: &[(String, String)]) -> bool {
    before.iter().all(|(path, hash)| {
        hash_file(&repo.join(path))
            .map(|current| current == *hash)
            .unwrap_or(false)
    })
}
fn hash_bytes(bytes: &[u8]) -> Result<String, String> {
    let mut h = Sha256::new();
    h.update(bytes);
    Ok(format!("{:x}", h.finalize()))
}

fn receipt_verdict(all_pass: bool, any_inconclusive: bool) -> &'static str {
    if all_pass {
        "pass"
    } else if any_inconclusive {
        "inconclusive"
    } else {
        "fail"
    }
}

fn bound_output(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() > MAX_CAPTURE_BYTES {
        bytes.truncate(MAX_CAPTURE_BYTES);
        bytes.extend_from_slice(b"\n[output truncated]\n");
    }
    bytes
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

async fn run_check(
    command: &VerificationCommand,
    cwd: PathBuf,
) -> (String, Option<i32>, Vec<u8>, Vec<u8>) {
    let mut child: Child = match Command::new(&command.program)
        .args(&command.args)
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
        let _ = child.kill().await;
        let _ = child.wait().await;
        return (
            "inconclusive".to_string(),
            None,
            Vec::new(),
            b"verification stdout pipe was unavailable".to_vec(),
        );
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill().await;
        let _ = child.wait().await;
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
        Ok(Err(_error)) => ("inconclusive".to_string(), None),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            ("inconclusive".to_string(), None)
        }
    };
    let stdout = collect_bounded_output(&mut stdout_task).await;
    let stderr = collect_bounded_output(&mut stderr_task).await;
    (status, exit_code, stdout, stderr)
}

pub fn candidate_sha(repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("candidate has no Git HEAD".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub async fn verify(
    session_id: &str,
    repo: &Path,
    profile: &VerificationProfile,
    protected_before: &[(String, String)],
    evidence: &Path,
    contract_revision: u32,
) -> Result<VerificationReceipt, String> {
    validate_profile(profile, repo)?;
    std::fs::create_dir_all(evidence).map_err(|e| e.to_string())?;
    let mut checks = Vec::new();
    let mut all_pass = true;
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
        let stdout = bound_output(stdout);
        let stderr = bound_output(stderr);
        std::fs::write(&stdout_path, &stdout).map_err(|e| e.to_string())?;
        std::fs::write(&stderr_path, &stderr).map_err(|e| e.to_string())?;
        if status != "pass" {
            all_pass = false;
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
    let protected_paths_unchanged = protected_files_unchanged(repo, protected_before);
    if !protected_paths_unchanged {
        all_pass = false;
    }
    let receipt = VerificationReceipt {
        session_id: session_id.into(),
        candidate_sha: candidate_sha(repo)?,
        contract_revision,
        profile_hash: profile_hash(profile)?,
        protected_paths_unchanged,
        checks,
        verdict: receipt_verdict(all_pass, any_inconclusive).into(),
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
    fn verification_output_is_bounded() {
        let output = bound_output(vec![b'x'; MAX_CAPTURE_BYTES + 1]);
        assert!(output.len() <= MAX_CAPTURE_BYTES + b"\n[output truncated]\n".len());
        assert!(output.ends_with(b"\n[output truncated]\n"));
    }

    #[test]
    fn infrastructure_evidence_is_inconclusive() {
        assert_eq!(receipt_verdict(true, false), "pass");
        assert_eq!(receipt_verdict(false, false), "fail");
        assert_eq!(receipt_verdict(false, true), "inconclusive");
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
}
