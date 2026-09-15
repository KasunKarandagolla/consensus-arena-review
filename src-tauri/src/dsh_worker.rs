use crate::verification::VerificationCommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

pub const RESULT_SCHEMA_VERSION: u32 = 1;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const API_KEY_ENV: &str = "ARENA_DSH_API_KEY";

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

fn default_true() -> bool {
    true
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

async fn collect_process(mut child: Child) -> Result<WorkerExecution, String> {
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
        "- id: llm-pi-ai\n  config:\n    providers:\n      arena_primary:\n        apiKeyEnv: {API_KEY_ENV}\n        api: openai-completions\n        baseURL: {}\n        models:\n          - id: {}\n            name: Arena primary\n            contextWindow: 128000\n- id: agent-default-model\n  config:\n    provider: arena_primary\n    model: {}\n",
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

    let executable = std::env::var_os("ARENA_DSH_EXECUTABLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dsh"));
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
    let child = command.spawn().map_err(|error| {
        format!("could not start DSH; install DSH or configure ARENA_DSH_EXECUTABLE: {error}")
    })?;
    let mut execution =
        match tokio::time::timeout(Duration::from_secs(timeout_seconds), collect_process(child))
            .await
        {
            Ok(result) => result?,
            Err(_) => WorkerExecution {
                exit_code: None,
                timed_out: true,
                stdout: String::new(),
                stderr: "DSH worker timed out".to_string(),
                result: None,
            },
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
}
