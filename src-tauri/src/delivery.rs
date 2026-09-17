use crate::dsh_worker;
use crate::product_os::{assemble_build_package, BuildPackage, ProductAuthorityRecords};
use crate::session_runtime::SessionOwner;
use crate::settings_store::SettingsStore;
use crate::transcript_store::TranscriptStore;
use crate::verification::{self, VerificationCommand, VerificationProfile, VerificationReceipt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub const DELIVERY_SCHEMA_VERSION: u32 = 1;
pub const MAX_IMPLEMENTATION_ATTEMPTS: u32 = 3;
const MAX_CANDIDATE_SCAN_FILES: usize = 20_000;
const MAX_CANDIDATE_SCAN_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CANDIDATE_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub fn attempts_remaining(attempt: u32) -> bool {
    attempt < MAX_IMPLEMENTATION_ATTEMPTS
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPhase {
    Preparing,
    AuthoringAcceptance,
    WaitingForUser,
    AcceptanceReady,
    Implementing,
    Verifying,
    Repairing,
    Verified,
    Applied,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryContract {
    pub revision: u32,
    pub objective: String,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub constraints: Vec<String>,
    pub worker_brief: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingQuestion {
    pub text: String,
    pub options: Vec<String>,
    pub allow_custom: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserAnswer {
    pub question: String,
    pub answer: String,
    pub answered_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtectedFileHash {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRuntime {
    Dsh,
    OpenCode,
}

impl Default for DeliveryRuntime {
    fn default() -> Self {
        Self::Dsh
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenCodeTaskState {
    Admitted,
    Running,
    EvidenceReady,
    Verified,
    Invalid,
    Cancelled,
    Failed,
}

impl Default for OpenCodeTaskState {
    fn default() -> Self {
        Self::Admitted
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeEvidence {
    pub evidence_id: String,
    pub kind: String,
    pub summary: String,
    pub result_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeWorkOrder {
    pub work_order_id: String,
    pub project_id: String,
    pub root_session_id: Option<String>,
    pub candidate_id: String,
    pub candidate_revision: u64,
    pub authority_version: String,
    pub acceptance_commit: String,
    pub task_state: OpenCodeTaskState,
    pub evidence_ref: Option<String>,
    pub result_ref: Option<String>,
    pub cancellation_state: Option<String>,
    pub verification_id: Option<String>,
    pub verification_status: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub build_package_id: Option<String>,
    #[serde(default)]
    pub build_package_revision: Option<u64>,
    #[serde(default)]
    pub build_package_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryState {
    pub schema_version: u32,
    pub session_id: String,
    pub objective: String,
    #[serde(alias = "repo_path")]
    pub source_workspace: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub base_commit: String,
    pub phase: DeliveryPhase,
    pub contract: Option<DeliveryContract>,
    pub user_answers: Vec<UserAnswer>,
    pub protected_files: Vec<String>,
    pub protected_hashes: Vec<ProtectedFileHash>,
    pub verification_commands: Vec<VerificationCommand>,
    pub acceptance_commit: Option<String>,
    pub attempt: u32,
    pub pending_question: Option<PendingQuestion>,
    pub waiting_phase: Option<DeliveryPhase>,
    pub candidate_commit: Option<String>,
    pub last_worker_summary: Option<String>,
    pub last_verification: Option<VerificationReceipt>,
    #[serde(default)]
    pub runtime: DeliveryRuntime,
    #[serde(default)]
    pub work_order: Option<OpenCodeWorkOrder>,
    #[serde(default)]
    pub evidence: Vec<OpenCodeEvidence>,
    #[serde(default)]
    pub authority_records: Option<ProductAuthorityRecords>,
    #[serde(default)]
    pub build_package: Option<BuildPackage>,
    pub created_at: i64,
    pub updated_at: i64,
    pub message: String,
}

pub fn transition(phase: DeliveryPhase, next: DeliveryPhase) -> Result<DeliveryPhase, String> {
    let valid = matches!(
        (&phase, &next),
        (DeliveryPhase::Preparing, DeliveryPhase::AuthoringAcceptance)
            | (
                DeliveryPhase::AuthoringAcceptance,
                DeliveryPhase::WaitingForUser
                    | DeliveryPhase::AcceptanceReady
                    | DeliveryPhase::Failed
            )
            | (
                DeliveryPhase::WaitingForUser,
                DeliveryPhase::AuthoringAcceptance
                    | DeliveryPhase::Implementing
                    | DeliveryPhase::Repairing
                    | DeliveryPhase::Failed
            )
            | (
                DeliveryPhase::AcceptanceReady,
                DeliveryPhase::Verifying | DeliveryPhase::Implementing | DeliveryPhase::Failed
            )
            | (
                DeliveryPhase::Implementing,
                DeliveryPhase::Verifying | DeliveryPhase::Repairing | DeliveryPhase::Failed
            )
            | (
                DeliveryPhase::Verifying,
                DeliveryPhase::Verified | DeliveryPhase::Repairing | DeliveryPhase::Failed
            )
            | (
                DeliveryPhase::Repairing,
                DeliveryPhase::Verifying | DeliveryPhase::WaitingForUser | DeliveryPhase::Failed
            )
    );
    if valid || matches!(next, DeliveryPhase::Cancelled | DeliveryPhase::Applied) {
        Ok(next)
    } else {
        Err(format!(
            "invalid delivery transition {:?} -> {:?}",
            phase, next
        ))
    }
}

/// Bind a current, Arena-assembled Product OS package to the existing
/// Delivery state. This is the handoff seam; workers never supply gate facts
/// or call this function.
#[allow(dead_code)]
pub fn bind_build_package(
    state: &mut DeliveryState,
    records: ProductAuthorityRecords,
) -> Result<(), String> {
    let package = assemble_build_package(&records)?;
    let architecture = package.evaluate_current(
        &records,
        crate::evidence_gates::GateId::Architecture,
    )?;
    if architecture.status != crate::evidence_gates::GateStatus::Pass {
        return Err(format!(
            "architecture gate blocks Delivery admission: {}",
            architecture.reason
        ));
    }
    let readiness = package.evaluate_current(
        &records,
        crate::evidence_gates::GateId::BuildReadiness,
    )?;
    if readiness.status != crate::evidence_gates::GateStatus::Pass {
        return Err(format!(
            "Build Package is not actionable: {}",
            readiness.reason
        ));
    }
    state.objective = package.objective.clone();
    state.authority_records = Some(records);
    state.build_package = Some(package);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryPresentation {
    pub schema_version: u32,
    pub session_id: String,
    pub phase: DeliveryPhase,
    pub status_text: String,
    pub attempt: u32,
    pub objective: String,
    pub verification_summary: Option<String>,
    pub candidate_commit: Option<String>,
    pub branch_name: String,
    pub runtime: DeliveryRuntime,
    pub work_order: Option<OpenCodeWorkOrder>,
    pub evidence: Vec<OpenCodeEvidence>,
    pub build_package_id: Option<String>,
    pub build_package_ready: bool,
}

fn status_text(phase: &DeliveryPhase) -> &'static str {
    match phase {
        DeliveryPhase::Preparing => "Preparing project…",
        DeliveryPhase::AuthoringAcceptance => "Defining acceptance…",
        DeliveryPhase::WaitingForUser => "Waiting for your decision…",
        DeliveryPhase::AcceptanceReady => "Acceptance tests ready…",
        DeliveryPhase::Implementing => "Implementing…",
        DeliveryPhase::Verifying => "Running checks…",
        DeliveryPhase::Repairing => "Repairing failed behavior…",
        DeliveryPhase::Verified => "Verified candidate ready",
        DeliveryPhase::Applied => "Applied to the original branch",
        DeliveryPhase::Cancelled => "Cancelled",
        DeliveryPhase::Failed => "Delivery failed",
    }
}

pub fn presentation(state: &DeliveryState) -> DeliveryPresentation {
    DeliveryPresentation {
        schema_version: DELIVERY_SCHEMA_VERSION,
        session_id: state.session_id.clone(),
        phase: state.phase.clone(),
        status_text: status_text(&state.phase).to_string(),
        attempt: state.attempt,
        objective: state.objective.clone(),
        verification_summary: state.last_verification.as_ref().map(|receipt| {
            format!(
                "{} check(s), verdict {}",
                receipt.checks.len(),
                receipt.verdict
            )
        }),
        candidate_commit: state.candidate_commit.clone(),
        branch_name: state.branch_name.clone(),
        runtime: state.runtime.clone(),
        work_order: state.work_order.clone(),
        evidence: state.evidence.clone(),
        build_package_id: state
            .build_package
            .as_ref()
            .map(|package| package.package_id.clone()),
        build_package_ready: state.build_package.is_some() && state.authority_records.is_some(),
    }
}

pub async fn emit(app: &AppHandle, state: &DeliveryState) {
    let _ = app.emit("delivery-state", presentation(state));
}

pub fn persist(path: &PathBuf, state: &DeliveryState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub async fn persist_state(
    path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
) -> Result<(), String> {
    state.updated_at = chrono::Utc::now().timestamp();
    let json = serde_json::to_string(state)
        .map_err(|error| format!("serialize delivery state: {error}"))?;
    persist(path, state)?;
    let session_id = state.session_id.clone();
    let json_for_db = json.clone();
    let db = transcript.clone();
    crate::db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            crate::errors::AgentError::DatabaseError(
                "delivery transcript lock poisoned".to_string(),
            )
        })?;
        store.save_delivery_state(&session_id, &json_for_db, chrono::Utc::now().timestamp())
    })
    .await
    .map_err(|error| format!("persist delivery state: {error}"))?;
    *delivery_slot.lock().await = Some(state.clone());
    Ok(())
}

async fn git_output(repo: &std::path::Path, args: &[&str]) -> Result<std::process::Output, String> {
    crate::git_runtime::output(repo, args).await
}

async fn git_ok(repo: &std::path::Path, args: &[&str]) -> Result<(), String> {
    let output = git_output(repo, args).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub async fn validate_clean_base(repo: &std::path::Path) -> Result<String, String> {
    let dirty = git_output(repo, &["status", "--porcelain", "--untracked-files=all"]).await?;
    if !dirty.status.success() {
        return Err("Could not inspect the Git working tree".to_string());
    }
    if !dirty.stdout.is_empty() {
        return Err(
            "Delivery requires a clean base working tree; Arena will not stash or discard changes"
                .to_string(),
        );
    }
    let head = git_output(repo, &["rev-parse", "HEAD"]).await?;
    if !head.status.success() {
        return Err("Could not resolve repository HEAD".to_string());
    }
    Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
}

pub async fn create_candidate_worktree(
    repo: &std::path::Path,
    worktree: &std::path::Path,
    branch: &str,
    base: &str,
) -> Result<(), String> {
    let args = vec![
        "worktree".into(),
        "add".into(),
        "-b".into(),
        branch.into(),
        worktree.as_os_str().to_os_string(),
        base.into(),
    ];
    let output = crate::git_runtime::output_owned(repo, &args).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not create isolated worktree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

async fn git_names(repo: &std::path::Path, base: &str) -> Result<Vec<String>, String> {
    let tracked = git_output(repo, &["diff", "--name-only", base]).await?;
    if !tracked.status.success() {
        return Err(String::from_utf8_lossy(&tracked.stderr).trim().to_string());
    }
    let untracked = git_output(repo, &["ls-files", "--others", "--exclude-standard"]).await?;
    if !untracked.status.success() {
        return Err(String::from_utf8_lossy(&untracked.stderr)
            .trim()
            .to_string());
    }
    let mut names = String::from_utf8_lossy(&tracked.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&untracked.stdout).lines())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

fn acceptance_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.contains(".arena-runtime") || lower.contains("..") {
        return false;
    }
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/test/")
        || file.contains(".test.")
        || file.contains(".spec.")
        || ((file.ends_with(".json")
            || file.ends_with(".yml")
            || file.ends_with(".yaml")
            || file.ends_with(".toml"))
            && (file.contains("config") || file == "package.json" || file == "cargo.toml"))
}

async fn clean_runtime(worktree: &std::path::Path) -> Result<(), String> {
    let runtime = worktree.join(".arena-runtime");
    if runtime.exists() {
        std::fs::remove_dir_all(runtime).map_err(|error| format!("remove DSH runtime: {error}"))?;
    }
    Ok(())
}

async fn staged_tree_objects(worktree: &std::path::Path) -> Result<Vec<String>, String> {
    let listed = git_output(worktree, &["ls-files", "--stage", "-z"]).await?;
    if !listed.status.success() {
        return Err("could not enumerate staged candidate files for credential scan".to_string());
    }
    let mut objects = Vec::new();
    for entry in listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let header = entry
            .split(|byte| *byte == b'\t')
            .next()
            .ok_or_else(|| "invalid staged candidate index entry".to_string())?;
        let header = std::str::from_utf8(header)
            .map_err(|_| "invalid staged candidate index entry".to_string())?;
        let mut fields = header.split_ascii_whitespace();
        let _mode = fields.next();
        let object = fields
            .next()
            .ok_or_else(|| "invalid staged candidate object entry".to_string())?;
        if !object.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid staged candidate object identity".to_string());
        }
        objects.push(object.to_string());
        if objects.len() > MAX_CANDIDATE_SCAN_FILES {
            return Err("candidate exceeds the credential scan file limit".to_string());
        }
    }
    Ok(objects)
}

async fn committed_tree_objects(
    worktree: &std::path::Path,
    commit: &str,
) -> Result<Vec<String>, String> {
    let listed = git_output(worktree, &["ls-tree", "-r", "-z", "--full-tree", commit]).await?;
    if !listed.status.success() {
        return Err(
            "could not enumerate committed candidate files for credential scan".to_string(),
        );
    }
    let mut objects = Vec::new();
    for entry in listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let header = entry
            .split(|byte| *byte == b'\t')
            .next()
            .ok_or_else(|| "invalid committed candidate tree entry".to_string())?;
        let header = std::str::from_utf8(header)
            .map_err(|_| "invalid committed candidate tree entry".to_string())?;
        let mut fields = header.split_ascii_whitespace();
        let _mode = fields.next();
        let object_type = fields
            .next()
            .ok_or_else(|| "invalid committed candidate object entry".to_string())?;
        let object = fields
            .next()
            .ok_or_else(|| "invalid committed candidate object entry".to_string())?;
        if object_type == "blob" {
            if !object.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("invalid committed candidate object identity".to_string());
            }
            objects.push(object.to_string());
            if objects.len() > MAX_CANDIDATE_SCAN_FILES {
                return Err("candidate exceeds the credential scan file limit".to_string());
            }
        }
    }
    Ok(objects)
}

async fn object_list_contains_secret(
    worktree: &std::path::Path,
    objects: &[String],
    secrets: &[String],
) -> Result<bool, String> {
    let secrets = secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if secrets.is_empty() || objects.is_empty() {
        return Ok(false);
    }
    if objects.len() > MAX_CANDIDATE_SCAN_FILES {
        return Err("candidate exceeds the credential scan file limit".to_string());
    }
    let mut input = String::with_capacity(objects.len().saturating_mul(42));
    for object in objects {
        input.push_str(object);
        input.push('\n');
    }
    let (status, contains) = crate::git_runtime::stream_with_input(
        worktree,
        &["cat-file", "--batch"],
        input.into_bytes(),
        move |stdout| async move { scan_git_blob_stream(stdout, &objects, &secrets).await },
    )
    .await?;
    if !status.success() {
        return Err("candidate credential scan failed".to_string());
    }
    Ok(contains)
}

async fn scan_git_blob_stream<R>(
    stdout: R,
    objects: &[String],
    secrets: &[String],
) -> Result<bool, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut stdout = BufReader::new(stdout);
    let secret_bytes = secrets
        .iter()
        .map(|secret| secret.as_bytes())
        .collect::<Vec<_>>();
    let mut scanned_bytes = 0u64;
    let mut contains = false;
    for _ in objects {
        let mut header = Vec::new();
        if stdout
            .read_until(b'\n', &mut header)
            .await
            .map_err(|_| "could not read candidate credential scan".to_string())?
            == 0
        {
            return Err("candidate credential scan ended early".to_string());
        }
        let header = std::str::from_utf8(&header)
            .map_err(|_| "invalid candidate credential scan response".to_string())?;
        let mut fields = header.split_ascii_whitespace();
        let _object = fields.next();
        let object_type = fields.next();
        let size = fields
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "invalid candidate credential scan response".to_string())?;
        if object_type != Some("blob") {
            return Err("candidate credential scan returned a non-file object".to_string());
        }
        if size > MAX_CANDIDATE_FILE_BYTES {
            return Err("candidate file exceeds the credential scan size limit".to_string());
        }
        scanned_bytes = scanned_bytes.saturating_add(size);
        if scanned_bytes > MAX_CANDIDATE_SCAN_BYTES {
            return Err("candidate exceeds the credential scan byte limit".to_string());
        }
        let mut remaining = size as usize;
        let mut chunk = vec![0u8; 64 * 1024];
        let mut overlap = vec![Vec::new(); secret_bytes.len()];
        while remaining > 0 {
            let read = remaining.min(chunk.len());
            stdout
                .read_exact(&mut chunk[..read])
                .await
                .map_err(|_| "could not read candidate credential scan".to_string())?;
            for (index, secret) in secret_bytes.iter().enumerate() {
                let mut window = std::mem::take(&mut overlap[index]);
                window.extend_from_slice(&chunk[..read]);
                contains |= window
                    .windows(secret.len())
                    .any(|candidate| candidate == *secret);
                let keep = secret.len().saturating_sub(1).min(window.len());
                overlap[index].extend_from_slice(&window[window.len() - keep..]);
            }
            remaining -= read;
        }
        let mut newline = [0u8; 1];
        stdout
            .read_exact(&mut newline)
            .await
            .map_err(|_| "could not read candidate credential scan".to_string())?;
        if newline[0] != b'\n' {
            return Err("invalid candidate credential scan boundary".to_string());
        }
    }
    Ok(contains)
}

async fn staged_tree_contains_secret(
    worktree: &std::path::Path,
    secrets: &[String],
) -> Result<bool, String> {
    let objects = staged_tree_objects(worktree).await?;
    object_list_contains_secret(worktree, &objects, secrets).await
}

async fn committed_tree_contains_secret(
    worktree: &std::path::Path,
    commit: &str,
    secrets: &[String],
) -> Result<bool, String> {
    let objects = committed_tree_objects(worktree, commit).await?;
    object_list_contains_secret(worktree, &objects, secrets).await
}

async fn worktree_contains_secret(
    worktree: &std::path::Path,
    secrets: &[String],
) -> Result<bool, String> {
    let root = worktree.to_path_buf();
    let secrets = secrets.to_vec();
    tokio::task::spawn_blocking(move || {
        crate::commands::directory_contains_credentials(&root, &secrets)
    })
    .await
    .map_err(|_| "could not finish the bounded candidate file scan".to_string())?
}

async fn discard_worker_files(
    worktree: &std::path::Path,
    expected_head: &str,
) -> Result<(), String> {
    git_ok(worktree, &["reset", "--hard", expected_head]).await?;
    git_ok(worktree, &["clean", "-fdx"]).await
}

async fn inspect_worker_output(
    worktree: &std::path::Path,
    expected_head: &str,
    secrets: &[String],
) -> Result<Option<&'static str>, String> {
    git_ok(worktree, &["add", "-A"]).await?;
    let contains_secret = match staged_tree_contains_secret(worktree, secrets).await {
        Ok(value) => value,
        Err(error) => {
            let _ = discard_worker_files(worktree, expected_head).await;
            return Err(error);
        }
    };
    let worktree_contains_secret = match worktree_contains_secret(worktree, secrets).await {
        Ok(value) => value,
        Err(error) => {
            let _ = discard_worker_files(worktree, expected_head).await;
            return Err(error);
        }
    };
    let changed_head = verification::candidate_sha(worktree).await? != expected_head;
    if contains_secret || worktree_contains_secret || changed_head {
        discard_worker_files(worktree, expected_head).await?;
        return Ok(Some(if contains_secret {
            "worker output contained a configured model credential; its changes were discarded"
        } else if worktree_contains_secret {
            "worker files contained a configured model credential; its changes were discarded"
        } else {
            "worker created a Git commit; its changes were discarded"
        }));
    }
    Ok(None)
}

async fn restore_protected_acceptance(
    worktree: &std::path::Path,
    acceptance_commit: &str,
    protected_paths: &[String],
) -> Result<(), String> {
    for path in protected_paths {
        git_ok(
            worktree,
            &[
                "restore",
                "--source",
                acceptance_commit,
                "--staged",
                "--worktree",
                "--",
                path,
            ],
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn persist_emit(
    app: Option<&AppHandle>,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
) -> Result<(), String> {
    persist_state(state_path, delivery_slot, transcript, state).await?;
    if let Some(app) = app {
        emit(app, state).await;
    }
    Ok(())
}

async fn ask_user(
    app: Option<&AppHandle>,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: &Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    state: &mut DeliveryState,
    question: PendingQuestion,
    resume_phase: DeliveryPhase,
    secrets: &[String],
) -> Result<String, String> {
    let question = PendingQuestion {
        text: redact_known_secrets(&question.text, secrets),
        options: question
            .options
            .iter()
            .map(|value| redact_known_secrets(value, secrets))
            .collect(),
        allow_custom: question.allow_custom,
        reason: redact_known_secrets(&question.reason, secrets),
    };
    state.phase = DeliveryPhase::WaitingForUser;
    state.waiting_phase = Some(resume_phase.clone());
    state.pending_question = Some(question.clone());
    persist_emit(app, state_path, delivery_slot, transcript, state).await?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    {
        let mut pending = ask_tx.lock().await;
        if pending.is_some() {
            return Err("another Arena question is already pending".to_string());
        }
        *pending = Some(sender);
    }
    if let Some(app) = app {
        let _ = app.emit(
            "agent-ask-user",
            serde_json::json!({
                "question": question.text,
                "options": question.options,
                "allow_custom": question.allow_custom
            }),
        );
    } else {
        let _ = ask_tx.lock().await.take();
        return Err("delivery needs an owner answer but no UI is attached".to_string());
    }
    let answer = receiver
        .await
        .map_err(|_| "delivery answer channel closed".to_string())?;
    state.pending_question = None;
    state.waiting_phase = None;
    let safe_answer = redact_known_secrets(&answer, secrets);
    state.user_answers.push(UserAnswer {
        question: question.text.clone(),
        answer: safe_answer.clone(),
        answered_at: chrono::Utc::now().timestamp(),
    });
    if answer.trim().eq_ignore_ascii_case("cancelled") {
        state.phase = DeliveryPhase::Failed;
        state.last_worker_summary =
            Some("Build cancelled while waiting for a product decision".to_string());
    } else {
        state.phase = resume_phase;
    }
    persist_emit(app, state_path, delivery_slot, transcript, state).await?;
    Ok(safe_answer)
}

fn worker_summary(execution: &dsh_worker::WorkerExecution, secrets: &[String]) -> String {
    match &execution.result {
        Some(result) => format!(
            "DSH exit {:?}; worker status {:?}; {}",
            execution.exit_code,
            result.status,
            redact_known_secrets(&result.summary, secrets)
        ),
        None if execution.timed_out => "DSH timed out before producing a result".to_string(),
        None => format!(
            "DSH exit {:?}; no valid result.json was produced",
            execution.exit_code
        ),
    }
}

fn worker_result_contains_secret(
    result: &dsh_worker::WorkerResultContract,
    secrets: &[String],
) -> bool {
    let contains = |value: &str| contains_any_secret(value, secrets);
    contains(&result.summary)
        || result.question.as_ref().is_some_and(|question| {
            contains(&question.text)
                || contains(&question.reason)
                || question.options.iter().any(|option| contains(option))
        })
        || result
            .acceptance
            .iter()
            .any(|item| contains(&item.id) || contains(&item.description))
        || result.verification_commands.iter().any(|command| {
            contains(&command.id)
                || contains(&command.program)
                || contains(&command.cwd)
                || command.args.iter().any(|arg| contains(arg))
        })
}

fn contains_any_secret(value: &str, secrets: &[String]) -> bool {
    secrets
        .iter()
        .any(|secret| !secret.is_empty() && dsh_worker::contains_secret(value, secret))
}

fn redact_known_secrets(value: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(value.to_string(), |safe, secret| {
            dsh_worker::redact_secret(&safe, secret)
        })
}

fn configured_secrets(settings: &SettingsStore) -> Result<Vec<String>, String> {
    if !settings.credential_storage_available() || settings.credential_migration_pending() {
        return Err(crate::credentials::secure_storage_help().to_string());
    }
    let mut secrets = ["brain_api_key", "brain_fallback_api_key", "brain2_api_key"]
        .iter()
        .map(|key| {
            settings
                .get(key)
                .map_err(|_| crate::credentials::secure_storage_help().to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .filter(|secret| !secret.is_empty())
        .collect::<Vec<_>>();
    let hackathon = settings
        .get_hackathon_config()
        .map_err(|_| crate::credentials::secure_storage_help().to_string())?;
    secrets.extend(
        hackathon
            .models
            .into_iter()
            .map(|model| model.api_key)
            .filter(|secret| !secret.is_empty()),
    );
    secrets.sort();
    secrets.dedup();
    Ok(secrets)
}

async fn commit_candidate_snapshot(
    worktree: &std::path::Path,
    session_id: &str,
    attempt: u32,
    expected_head: &str,
    secrets: &[String],
) -> Result<String, String> {
    if verification::candidate_sha(worktree).await? != expected_head {
        discard_worker_files(worktree, expected_head).await?;
        return Err(
            "worker changed candidate HEAD before snapshot; its changes were discarded".to_string(),
        );
    }
    let worktree_has_secret = match worktree_contains_secret(worktree, secrets).await {
        Ok(value) => value,
        Err(error) => {
            let _ = discard_worker_files(worktree, expected_head).await;
            return Err(error);
        }
    };
    if worktree_has_secret {
        discard_worker_files(worktree, expected_head).await?;
        return Err(
            "candidate worktree files contained a configured model credential; the attempt was discarded"
                .to_string(),
        );
    }
    git_ok(worktree, &["add", "-A"]).await?;
    let staged_has_secret = match staged_tree_contains_secret(worktree, secrets).await {
        Ok(value) => value,
        Err(error) => {
            let _ = discard_worker_files(worktree, expected_head).await;
            return Err(error);
        }
    };
    if staged_has_secret {
        discard_worker_files(worktree, expected_head).await?;
        return Err(
            "candidate contained a configured model credential; the attempt was discarded"
                .to_string(),
        );
    }
    let staged = git_output(worktree, &["diff", "--cached", "--quiet"]).await?;
    if staged.status.code() == Some(1) {
        git_ok(
            worktree,
            &[
                "commit",
                "-m",
                &format!("arena: delivery {session_id} attempt {attempt}"),
            ],
        )
        .await?;
    } else if !staged.status.success() {
        return Err(String::from_utf8_lossy(&staged.stderr).trim().to_string());
    }
    let candidate = verification::candidate_sha(worktree).await?;
    if committed_tree_contains_secret(worktree, &candidate, secrets).await? {
        discard_worker_files(worktree, expected_head).await?;
        return Err(
            "candidate commit contained a configured model credential; the attempt was discarded"
                .to_string(),
        );
    }
    if verification::candidate_sha(worktree).await? != candidate {
        discard_worker_files(worktree, expected_head).await?;
        return Err(
            "candidate HEAD changed during credential validation; the attempt was discarded"
                .to_string(),
        );
    }
    Ok(candidate)
}

async fn finish_verified(
    app: Option<&AppHandle>,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
    verified_candidate: &str,
) -> Result<(), String> {
    let worktree = std::path::Path::new(&state.worktree_path);
    clean_runtime(worktree).await?;
    let current = verification::candidate_sha(worktree).await?;
    if current != verified_candidate {
        return Err(
            "candidate HEAD changed after verification; refusing to mark Verified".to_string(),
        );
    }
    let clean = git_output(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )
    .await?;
    if !clean.status.success() || !clean.stdout.is_empty() {
        return Err(
            "candidate worktree changed after verification; refusing to mark Verified".to_string(),
        );
    }
    state.candidate_commit = Some(verified_candidate.to_string());
    state.phase = DeliveryPhase::Verified;
    persist_emit(app, state_path, delivery_slot, transcript, state).await
}

async fn freeze_acceptance(
    app: Option<&AppHandle>,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
    secrets: &[String],
) -> Result<(), String> {
    let worktree = std::path::Path::new(&state.worktree_path);
    let changed = git_names(worktree, &state.base_commit).await?;
    if changed.is_empty() || changed.iter().any(|path| !acceptance_path(path)) {
        return Err(
            "acceptance authoring changed no tests or changed production files".to_string(),
        );
    }
    for path in &changed {
        git_ok(worktree, &["add", "--", path]).await?;
    }
    if staged_tree_contains_secret(worktree, secrets).await? {
        reset_worktree(state, &state.base_commit).await?;
        return Err(
            "acceptance contained a configured model credential; the attempt was discarded"
                .to_string(),
        );
    }
    if verification::candidate_sha(worktree).await? != state.base_commit {
        reset_worktree(state, &state.base_commit).await?;
        return Err(
            "candidate HEAD changed during acceptance freeze; the attempt was discarded"
                .to_string(),
        );
    }
    git_ok(
        worktree,
        &[
            "commit",
            "-m",
            &format!("arena: freeze acceptance {}", state.session_id),
        ],
    )
    .await?;
    let acceptance_commit = verification::candidate_sha(worktree).await?;
    if committed_tree_contains_secret(worktree, &acceptance_commit, secrets).await? {
        reset_worktree(state, &state.base_commit).await?;
        return Err(
            "frozen acceptance contained a configured model credential; the attempt was discarded"
                .to_string(),
        );
    }
    if verification::candidate_sha(worktree).await? != acceptance_commit {
        reset_worktree(state, &state.base_commit).await?;
        return Err(
            "candidate HEAD changed during acceptance validation; the attempt was discarded"
                .to_string(),
        );
    }
    state.acceptance_commit = Some(acceptance_commit);
    state.protected_files = changed;
    state.protected_hashes = verification::protected_hashes(worktree, &state.protected_files)?
        .into_iter()
        .map(|(path, sha256)| ProtectedFileHash { path, sha256 })
        .collect();
    let profile = VerificationProfile {
        version: 1,
        commands: state.verification_commands.clone(),
        protected_paths: state.protected_files.clone(),
    };
    verification::validate_profile(&profile, worktree)?;
    state.phase = DeliveryPhase::AcceptanceReady;
    persist_emit(app, state_path, delivery_slot, transcript, state).await
}

async fn run_acceptance_authoring(
    app: Option<&AppHandle>,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: &Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    state: &mut DeliveryState,
    model: &dsh_worker::DshModelConfig,
    secrets: &[String],
    patch_dir: &std::path::Path,
) -> Result<(), String> {
    loop {
        state.phase = DeliveryPhase::AuthoringAcceptance;
        persist_emit(app, state_path, delivery_slot, transcript, state).await?;
        let decisions = state
            .user_answers
            .iter()
            .map(|answer| format!("{} => {}", answer.question, answer.answer))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = dsh_worker::bounded_prompt(
            &state.objective,
            &format!(
                "AUTHOR ACCEPTANCE TESTS ONLY. Do not implement production behavior.\nUser decisions:\n{decisions}"
            ),
            None,
        );
        let worker_base =
            verification::candidate_sha(std::path::Path::new(&state.worktree_path)).await?;
        let execution = dsh_worker::run(
            std::path::Path::new(&state.worktree_path),
            &std::path::Path::new(&state.worktree_path).join(".arena-runtime"),
            patch_dir,
            model,
            &prompt,
            1800,
        )
        .await?;
        clean_runtime(std::path::Path::new(&state.worktree_path)).await?;
        if let Some(reason) = inspect_worker_output(
            std::path::Path::new(&state.worktree_path),
            &worker_base,
            secrets,
        )
        .await?
        {
            return Err(reason.to_string());
        }
        state.last_worker_summary = Some(worker_summary(&execution, secrets));
        let Some(result) = execution.result else {
            return Err(state
                .last_worker_summary
                .clone()
                .unwrap_or_else(|| "acceptance authoring failed".to_string()));
        };
        if worker_result_contains_secret(&result, secrets) {
            return Err("worker result contained the configured credential; no result metadata was persisted".to_string());
        }
        match result.status {
            dsh_worker::WorkerStatus::NeedsUser => {
                let question = result
                    .question
                    .ok_or_else(|| "worker returned needs_user without a question".to_string())?;
                let pending = PendingQuestion {
                    text: redact_known_secrets(&question.text, secrets),
                    options: question
                        .options
                        .iter()
                        .map(|value| redact_known_secrets(value, secrets))
                        .collect(),
                    allow_custom: question.allow_custom,
                    reason: redact_known_secrets(&question.reason, secrets),
                };
                let _ = ask_user(
                    app,
                    state_path,
                    delivery_slot,
                    transcript,
                    ask_tx,
                    state,
                    pending,
                    DeliveryPhase::AuthoringAcceptance,
                    secrets,
                )
                .await?;
                if state.phase == DeliveryPhase::Failed {
                    return Err(state
                        .last_worker_summary
                        .clone()
                        .unwrap_or_else(|| "user cancelled delivery".to_string()));
                }
            }
            dsh_worker::WorkerStatus::Failed => {
                return Err(redact_known_secrets(&result.summary, secrets));
            }
            dsh_worker::WorkerStatus::Complete => {
                if !execution.exit_code.is_some_and(|code| code == 0) {
                    return Err("acceptance worker exited unsuccessfully".to_string());
                }
                if result.acceptance.is_empty() || result.verification_commands.is_empty() {
                    return Err(
                        "acceptance worker produced no acceptance items or verification commands"
                            .to_string(),
                    );
                }
                state.verification_commands = result.verification_commands;
                state.contract = Some(DeliveryContract {
                    revision: 1,
                    objective: state.objective.clone(),
                    acceptance_criteria: result
                        .acceptance
                        .iter()
                        .map(|item| AcceptanceCriterion {
                            id: item.id.clone(),
                            description: redact_known_secrets(&item.description, secrets),
                        })
                        .collect(),
                    constraints: vec![
                        "No deployment or external infrastructure changes".to_string(),
                    ],
                    worker_brief: "Acceptance tests are frozen before implementation".to_string(),
                });
                freeze_acceptance(app, state_path, delivery_slot, transcript, state, secrets)
                    .await?;
                return Ok(());
            }
        }
    }
}

async fn reset_worktree(state: &DeliveryState, commit: &str) -> Result<(), String> {
    let worktree = std::path::Path::new(&state.worktree_path);
    clean_runtime(worktree).await?;
    git_ok(worktree, &["reset", "--hard", commit]).await?;
    git_ok(worktree, &["clean", "-fdx"]).await
}

pub fn recovery_requires_worktree_reset(state: &DeliveryState, implementation: bool) -> bool {
    if !implementation {
        return true;
    }
    !state
        .last_verification
        .as_ref()
        .is_some_and(|receipt| receipt.verdict == "inconclusive")
}

pub async fn reset_for_recovery(state: &DeliveryState, implementation: bool) -> Result<(), String> {
    if !recovery_requires_worktree_reset(state, implementation) {
        clean_runtime(std::path::Path::new(&state.worktree_path)).await?;
        return Ok(());
    }
    let commit = if implementation {
        state
            .acceptance_commit
            .as_deref()
            .ok_or_else(|| "delivery has no acceptance commit to recover from".to_string())?
    } else {
        &state.base_commit
    };
    reset_worktree(state, commit).await
}

pub async fn apply_verified_candidate(state: &mut DeliveryState) -> Result<(), String> {
    if state.phase != DeliveryPhase::Verified {
        return Err("Only a Verified delivery can be applied".to_string());
    }
    let candidate = state
        .candidate_commit
        .clone()
        .ok_or_else(|| "Verified delivery has no candidate commit".to_string())?;
    let source = std::path::Path::new(&state.source_workspace);
    let clean = git_output(source, &["status", "--porcelain", "--untracked-files=all"]).await?;
    if !clean.status.success() || !clean.stdout.is_empty() {
        return Err("Cannot apply: the original repository is not clean".to_string());
    }
    let head = git_output(source, &["rev-parse", "HEAD"]).await?;
    let current = String::from_utf8_lossy(&head.stdout).trim().to_string();
    if current != state.base_commit {
        return Err("Cannot apply: the original repository moved since Build started".to_string());
    }
    let exists = git_output(
        source,
        &["cat-file", "-e", &format!("{candidate}^{{commit}}")],
    )
    .await?;
    if !exists.status.success() {
        return Err("Cannot apply: the verified candidate commit is unavailable".to_string());
    }
    let candidate_worktree = std::path::Path::new(&state.worktree_path);
    if state.runtime == DeliveryRuntime::OpenCode || candidate_worktree.exists() {
        let candidate_head = verification::candidate_sha(candidate_worktree).await?;
        if candidate_head != candidate {
            return Err("Cannot apply: the candidate changed after verification".to_string());
        }
        if let Some(receipt) = &state.last_verification {
            if receipt.verdict != "pass"
                || receipt.candidate_sha != candidate
                || receipt.acceptance_commit != state.acceptance_commit.clone().unwrap_or_default()
                || !receipt.candidate_tree_unchanged
                || !receipt.protected_paths_unchanged
            {
                return Err("Cannot apply: the current verification is not valid for this candidate".to_string());
            }
        } else if state.runtime == DeliveryRuntime::OpenCode {
            return Err("Cannot apply: the candidate has no current independent verification".to_string());
        }
    }
    let ff = git_output(source, &["merge", "--ff-only", &candidate]).await?;
    if !ff.status.success() {
        return Err(format!(
            "Cannot apply safely: {}",
            String::from_utf8_lossy(&ff.stderr).trim()
        ));
    }
    state.phase = DeliveryPhase::Applied;
    Ok(())
}

async fn run_inner(
    app: Option<&AppHandle>,
    mut state: DeliveryState,
    state_path: PathBuf,
    delivery_slot: Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    settings: Arc<tokio::sync::Mutex<SettingsStore>>,
) -> Result<DeliveryState, String> {
    let result = async {
        let (brain, secrets) = {
            let settings = settings.lock().await;
            let brain = settings
                .get_agent_brain_config()
                .map_err(|_| crate::credentials::secure_storage_help().to_string())?;
            let secrets = configured_secrets(&settings)?;
            (brain, secrets)
        };
        let model = dsh_worker::DshModelConfig { api_key: brain.api_key, base_url: brain.base_url, model: brain.model };
        state.objective = redact_known_secrets(&state.objective, &secrets);
        let patch_dir = state_path.parent().unwrap_or(std::path::Path::new(".")).join("dsh-runtime").join(&state.session_id);
        let worktree = PathBuf::from(&state.worktree_path);
        if state.phase == DeliveryPhase::WaitingForUser {
            let pending = state.pending_question.clone().ok_or_else(|| "waiting delivery has no pending question".to_string())?;
            let resume_phase = state.waiting_phase.clone().unwrap_or(if state.acceptance_commit.is_some() {
                DeliveryPhase::Implementing
            } else {
                DeliveryPhase::AuthoringAcceptance
            });
            let _ = ask_user(app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, pending, resume_phase, &secrets).await?;
            if state.phase == DeliveryPhase::Failed { return Err("user cancelled delivery".to_string()); }
        }
        if matches!(state.phase, DeliveryPhase::Preparing | DeliveryPhase::AuthoringAcceptance) && state.acceptance_commit.is_none() {
            if state.phase == DeliveryPhase::Preparing { state.phase = DeliveryPhase::AuthoringAcceptance; }
            run_acceptance_authoring(app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, &model, &secrets, &patch_dir).await?;
        }
        if state.acceptance_commit.is_none() { return Err("delivery has no frozen acceptance commit".to_string()); }
        let profile = VerificationProfile { version: 1, commands: state.verification_commands.clone(), protected_paths: state.protected_files.clone() };
        verification::validate_profile(&profile, &worktree)?;
        let canonical = PathBuf::from(&state.source_workspace);
        let canonical_protected_before = verification::protected_hashes(&canonical, &state.protected_files)?;
        let acceptance_commit = state
            .acceptance_commit
            .clone()
            .ok_or_else(|| "delivery has no frozen acceptance commit".to_string())?;
        if state.phase == DeliveryPhase::AcceptanceReady || state.phase == DeliveryPhase::Verifying {
            state.phase = DeliveryPhase::Verifying;
            persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let hashes = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let attempt_id = format!("{}/attempt/{}", state.session_id, state.attempt);
            let receipt = verification::verify(&state.session_id, &attempt_id, &acceptance_commit, &worktree, &profile, &hashes, &state_path.parent().unwrap_or(std::path::Path::new(".")).join("delivery-evidence").join(&state.session_id), 1).await?;
            state.last_verification = Some(receipt.clone());
            if !receipt.candidate_tree_unchanged {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("verification changed the candidate worktree; no repair or Verified transition was allowed".to_string());
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                return Ok::<(), String>(());
            }
            if receipt.verdict == "pass" { finish_verified(app, &state_path, &delivery_slot, &transcript, &mut state, &receipt.candidate_sha).await?; return Ok::<(), String>(()); }
            if receipt.verdict == "inconclusive" {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("Verification inconclusive; no implementation repair was attempted. Resolve the verification environment and resume to rerun the frozen checks.".to_string());
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                return Ok::<(), String>(());
            }
            if !attempts_remaining(state.attempt) {
                state.phase = DeliveryPhase::Failed;
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                return Ok(());
            }
            state.phase = DeliveryPhase::Repairing;
        }
        while attempts_remaining(state.attempt) {
            state.attempt += 1;
            state.phase = DeliveryPhase::Implementing;
            persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let answers = state.user_answers.iter().map(|answer| format!("{} => {}", answer.question, answer.answer)).collect::<Vec<_>>().join("\n");
            let evidence = state.last_verification.as_ref().map(|receipt| format!("verdict={} checks={:?}", receipt.verdict, receipt.checks)).unwrap_or_else(|| "acceptance checks failed".to_string());
            let prompt = dsh_worker::bounded_prompt(&state.objective, &format!("IMPLEMENTATION TASK. Frozen acceptance items are in the repository. Protected files: {}. User decisions:\n{answers}", state.protected_files.join(", ")), Some(&evidence));
            let worker_base = verification::candidate_sha(&worktree).await?;
            let execution = dsh_worker::run(&worktree, &worktree.join(".arena-runtime"), &patch_dir, &model, &prompt, 1800).await?;
            clean_runtime(&worktree).await?;
            if let Some(reason) = inspect_worker_output(&worktree, &worker_base, &secrets).await? {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some(reason.to_string());
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            }
            state.last_worker_summary = Some(worker_summary(&execution, &secrets));
            let protected_before = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let protected_changed = !verification::protected_files_unchanged(&worktree, &protected_before);
            let canonical_changed = !verification::protected_files_unchanged(&canonical, &canonical_protected_before);
            if protected_changed || canonical_changed {
                restore_protected_acceptance(&worktree, &acceptance_commit, &state.protected_files).await?;
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some(if canonical_changed {
                    "worker attempted to modify canonical protected acceptance; candidate was discarded before commit".to_string()
                } else {
                    "worker attempted to modify protected acceptance files; candidate was discarded before commit".to_string()
                });
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            } else if let Some(result) = execution.result {
                if result.status == dsh_worker::WorkerStatus::NeedsUser {
                    let question = result.question.ok_or_else(|| "worker returned needs_user without a question".to_string())?;
                    let pending = PendingQuestion {
                        text: redact_known_secrets(&question.text, &secrets),
                        options: question.options.iter().map(|value| redact_known_secrets(value, &secrets)).collect(),
                        allow_custom: question.allow_custom,
                        reason: redact_known_secrets(&question.reason, &secrets),
                    };
                    let _ = ask_user(app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, pending, DeliveryPhase::Implementing, &secrets).await?;
                    if state.phase == DeliveryPhase::Failed { return Err("user cancelled delivery".to_string()); }
                    state.attempt = state.attempt.saturating_sub(1);
                    continue;
                }
            }
            // Persist the exact content snapshot before verification so the
            // receipt's candidate SHA and a later Verified candidate SHA agree.
            let candidate_sha = commit_candidate_snapshot(&worktree, &state.session_id, state.attempt, &worker_base, &secrets).await?;
            state.phase = DeliveryPhase::Verifying;
            persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let hashes = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let attempt_id = format!("{}/attempt/{}", state.session_id, state.attempt);
            let receipt = verification::verify(&state.session_id, &attempt_id, &acceptance_commit, &worktree, &profile, &hashes, &state_path.parent().unwrap_or(std::path::Path::new(".")).join("delivery-evidence").join(&state.session_id), 1).await?;
            if receipt.candidate_sha != candidate_sha {
                return Err("verification receipt candidate did not match the implementation snapshot".to_string());
            }
            state.last_verification = Some(receipt.clone());
            if !receipt.candidate_tree_unchanged {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("verification changed the candidate worktree; no repair or Verified transition was allowed".to_string());
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            }
            if receipt.verdict == "pass" { finish_verified(app, &state_path, &delivery_slot, &transcript, &mut state, &receipt.candidate_sha).await?; return Ok(()); }
            if receipt.verdict == "inconclusive" {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("Verification inconclusive; product-code repair was not attempted. Resolve the verification environment and resume to rerun the frozen checks.".to_string());
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            }
            if !attempts_remaining(state.attempt) {
                state.phase = DeliveryPhase::Failed;
                persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            }
            state.phase = DeliveryPhase::Repairing;
            persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await?;
        }
        Ok::<(), String>(())
    }.await;
    if let Err(error) = result {
        state.phase = DeliveryPhase::Failed;
        state.last_worker_summary = Some(error.clone());
        let _ = persist_emit(app, &state_path, &delivery_slot, &transcript, &mut state).await;
        return Err(error);
    }
    Ok(state)
}

pub async fn run(
    runtime: Arc<crate::session_runtime::SessionRuntime>,
    app: AppHandle,
    state: DeliveryState,
    state_path: PathBuf,
    owner: SessionOwner,
    delivery_slot: Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    settings: Arc<tokio::sync::Mutex<SettingsStore>>,
) {
    if state.runtime == DeliveryRuntime::OpenCode {
        let _ = crate::opencode_adapter::run_delivery(
            Some(&app),
            state,
            state_path,
            delivery_slot,
            transcript,
            settings,
        )
        .await;
    } else {
        let _ = run_inner(
            Some(&app),
            state,
            state_path,
            delivery_slot,
            transcript,
            ask_tx,
            settings,
        )
        .await;
    }
    runtime.mark_completed(&owner);
}

/// Run the production Delivery supervisor without a Tauri window.
///
/// This is intentionally a narrow dogfood boundary for backend qualification:
/// it uses the same state, worker, acceptance, verifier, and persistence code
/// as the Tauri command path while omitting only UI event emission. A worker
/// request for an owner decision still fails safely because this harness has no
/// owner-answer channel.
pub async fn run_backend_qualification(
    runtime: Arc<crate::session_runtime::SessionRuntime>,
    state: DeliveryState,
    state_path: PathBuf,
    delivery_slot: Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    settings: Arc<tokio::sync::Mutex<SettingsStore>>,
) -> Result<DeliveryState, String> {
    let permit = runtime.try_acquire_start(state.session_id.clone())?;
    let owner = permit.owner();
    let task_owner = owner.clone();
    let task_runtime = runtime.clone();
    let (activate_tx, activate_rx) = tokio::sync::oneshot::channel();
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let result = if activate_rx.await.is_ok() {
            run_inner(
                None,
                state,
                state_path,
                delivery_slot,
                transcript,
                ask_tx,
                settings,
            )
            .await
        } else {
            Err("backend qualification task was not activated".to_string())
        };
        task_runtime.mark_completed(&task_owner);
        let _ = result_tx.send(result);
    });
    permit.commit(task, activate_tx)?;
    result_rx
        .await
        .map_err(|_| "backend qualification task stopped before returning".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_fixture_command(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("Git fixture command should start")
    }

    fn git_fixture_success(repo: &std::path::Path, args: &[&str]) -> String {
        let output = git_fixture_command(repo, args);
        assert!(
            output.status.success(),
            "Git fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn apply_guard_fixture(name: &str) -> (PathBuf, String, String, String) {
        let root = std::env::temp_dir().join(format!(
            "consensus arena apply {name} {}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create apply fixture");
        git_fixture_success(&root, &["init"]);
        git_fixture_success(&root, &["config", "user.email", "arena@example.invalid"]);
        git_fixture_success(&root, &["config", "user.name", "Arena Fixture"]);
        std::fs::write(root.join("base.txt"), "base\n").expect("write base");
        git_fixture_success(&root, &["add", "."]);
        git_fixture_success(&root, &["commit", "-m", "base"]);
        let base = git_fixture_success(&root, &["rev-parse", "HEAD"]);
        let base_branch = git_fixture_success(&root, &["branch", "--show-current"]);
        git_fixture_success(&root, &["switch", "-c", "arena-candidate"]);
        std::fs::write(root.join("candidate.txt"), "candidate\n").expect("write candidate");
        git_fixture_success(&root, &["add", "."]);
        git_fixture_success(&root, &["commit", "-m", "candidate"]);
        let candidate = git_fixture_success(&root, &["rev-parse", "HEAD"]);
        git_fixture_success(&root, &["switch", &base_branch]);
        (root, base, candidate, base_branch)
    }

    #[tokio::test]
    async fn production_admission_and_worktree_creation_handle_paths_with_spaces() {
        let (repo, expected_base, _, _) = apply_guard_fixture("admission spaces");
        let base = validate_clean_base(&repo)
            .await
            .expect("production admission should accept a clean repository");
        assert_eq!(base, expected_base);
        let worktree = repo
            .parent()
            .expect("fixture parent")
            .join("candidate worktree");
        create_candidate_worktree(&repo, &worktree, "arena-delivery/admission", &base)
            .await
            .expect("production worktree creation should accept paths with spaces");
        assert_eq!(git_fixture_success(&worktree, &["rev-parse", "HEAD"]), base);
        std::fs::write(repo.join("owner-change.txt"), "must not be discarded\n")
            .expect("make original checkout dirty");
        assert!(
            validate_clean_base(&repo)
                .await
                .expect_err("dirty base must be refused")
                .contains("clean base working tree")
        );
        let removed = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&worktree)
            .current_dir(&repo)
            .status()
            .expect("remove fixture worktree");
        assert!(removed.success());
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn staged_and_worker_committed_blobs_are_scanned_before_candidate_admission() {
        let (repo, _, _, _) = apply_guard_fixture("candidate credential scan");
        let expected_head = git_fixture_success(&repo, &["rev-parse", "HEAD"]);
        let key = "candidate-scan-synthetic-secret-7c8391";
        let secrets = vec![key.to_string()];
        let mut content = vec![b'x'; 64 * 1024 - 8];
        content.extend_from_slice(key.as_bytes());
        std::fs::write(repo.join("suspect.txt"), content).expect("write staged sentinel file");
        git_fixture_success(&repo, &["add", "suspect.txt"]);
        assert!(
            staged_tree_contains_secret(&repo, &secrets)
                .await
                .expect("scan staged object")
        );
        git_fixture_success(&repo, &["commit", "-m", "untrusted worker commit"]);
        let worker_commit = git_fixture_success(&repo, &["rev-parse", "HEAD"]);
        assert!(
            committed_tree_contains_secret(&repo, &worker_commit, &secrets)
                .await
                .expect("scan committed candidate tree")
        );

        let reason = inspect_worker_output(&repo, &expected_head, &secrets)
            .await
            .expect("inspect untrusted worker output")
            .expect("credential-bearing candidate must be rejected");
        assert!(reason.contains("credential"));
        assert_eq!(
            git_fixture_success(&repo, &["rev-parse", "HEAD"]),
            expected_head
        );
        assert!(!repo.join("suspect.txt").exists());
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn ignored_worker_credential_file_is_rejected_and_removed() {
        let (repo, _, _, _) = apply_guard_fixture("ignored credential scan");
        std::fs::write(repo.join(".gitignore"), "ignored-secret.txt\n")
            .expect("write ignored-file rule");
        git_fixture_success(&repo, &["add", ".gitignore"]);
        git_fixture_success(&repo, &["commit", "-m", "ignore worker artifact"]);
        let expected_head = git_fixture_success(&repo, &["rev-parse", "HEAD"]);
        let secret = "ignored-worker-file-secret-18a4";
        let ignored = repo.join("ignored-secret.txt");
        std::fs::write(&ignored, format!("token={secret}\n"))
            .expect("write ignored credential fixture");

        let reason = inspect_worker_output(&repo, &expected_head, &[secret.to_string()])
            .await
            .expect("scan ignored worker output")
            .expect("ignored credential file must be rejected");

        assert!(reason.contains("credential"));
        assert_eq!(
            git_fixture_success(&repo, &["rev-parse", "HEAD"]),
            expected_head
        );
        assert!(
            !ignored.exists(),
            "ignored credential artifact must be cleaned"
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn worker_result_secret_scan_covers_persisted_metadata_fields() {
        let secret = "worker-metadata-secret-5e26";
        let mut result = dsh_worker::WorkerResultContract {
            schema_version: 1,
            status: dsh_worker::WorkerStatus::Complete,
            summary: "implemented".to_string(),
            question: None,
            verification_commands: vec![verification::VerificationCommand {
                id: "build".to_string(),
                program: "cargo".to_string(),
                args: vec!["check".to_string()],
                cwd: ".".to_string(),
                timeout_seconds: 120,
            }],
            acceptance: vec![dsh_worker::AcceptanceItem {
                id: "AC-1".to_string(),
                description: "Build succeeds".to_string(),
            }],
        };
        let secrets = vec![secret.to_string()];
        assert!(!worker_result_contains_secret(&result, &secrets));
        result.acceptance[0].id = secret.to_string();
        assert!(worker_result_contains_secret(&result, &secrets));
        result.acceptance[0].id = "AC-1".to_string();
        result.verification_commands[0].id = secret.to_string();
        assert!(worker_result_contains_secret(&result, &secrets));
        result.verification_commands[0].id = "build".to_string();
        result.verification_commands[0].cwd = secret.to_string();
        assert!(worker_result_contains_secret(&result, &secrets));
    }

    #[tokio::test]
    async fn worker_created_commit_without_secret_is_still_rejected() {
        let (repo, _, _, _) = apply_guard_fixture("unexpected worker commit");
        let expected_head = git_fixture_success(&repo, &["rev-parse", "HEAD"]);
        std::fs::write(repo.join("worker.txt"), "ordinary worker output\n")
            .expect("write ordinary output");
        git_fixture_success(&repo, &["add", "worker.txt"]);
        git_fixture_success(&repo, &["commit", "-m", "untrusted worker commit"]);
        let reason =
            inspect_worker_output(&repo, &expected_head, &["unrelated-sentinel".to_string()])
                .await
                .expect("inspect worker commit")
                .expect("unexpected worker commits must be rejected");
        assert!(reason.contains("Git commit"));
        assert_eq!(
            git_fixture_success(&repo, &["rev-parse", "HEAD"]),
            expected_head
        );
        assert!(!repo.join("worker.txt").exists());
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn production_protected_acceptance_restoration_uses_frozen_git_content() {
        let (repo, _, _, _) = apply_guard_fixture("protected acceptance");
        let worktree = repo
            .parent()
            .expect("fixture parent")
            .join("protected candidate");
        let base = git_fixture_success(&repo, &["rev-parse", "HEAD"]);
        create_candidate_worktree(&repo, &worktree, "arena-delivery/protected", &base)
            .await
            .expect("create candidate worktree");
        let acceptance = worktree.join("acceptance.test");
        std::fs::write(&acceptance, "frozen requirement\n").expect("write acceptance");
        git_fixture_success(&worktree, &["add", "acceptance.test"]);
        git_fixture_success(&worktree, &["commit", "-m", "freeze acceptance"]);
        let frozen_sha = git_fixture_success(&worktree, &["rev-parse", "HEAD"]);
        let protected_before =
            verification::protected_hashes(&worktree, &["acceptance.test".to_string()])
                .expect("capture frozen acceptance hash");
        std::fs::write(&acceptance, "worker tampering\n").expect("tamper acceptance");
        assert!(!verification::protected_files_unchanged(
            &worktree,
            &protected_before
        ));
        restore_protected_acceptance(&worktree, &frozen_sha, &["acceptance.test".to_string()])
            .await
            .expect("restore protected file from acceptance commit");
        let restored = std::fs::read_to_string(&acceptance)
            .expect("read restored acceptance")
            .replace("\r\n", "\n");
        assert_eq!(restored, "frozen requirement\n");
        assert_eq!(
            git_fixture_success(&worktree, &["rev-parse", "HEAD"]),
            frozen_sha
        );
        let status = git_fixture_success(&worktree, &["status", "--porcelain"]);
        assert!(
            status.is_empty(),
            "restored worktree remained dirty: {status:?}"
        );
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&worktree)
            .current_dir(&repo)
            .status();
        let _ = std::fs::remove_dir_all(&repo);
    }

    fn sample_state() -> DeliveryState {
        DeliveryState {
            schema_version: DELIVERY_SCHEMA_VERSION,
            session_id: "session".to_string(),
            objective: "objective".to_string(),
            source_workspace: "/project".to_string(),
            worktree_path: "/arena/worktree".to_string(),
            branch_name: "arena-delivery/session".to_string(),
            base_commit: "base".to_string(),
            phase: DeliveryPhase::Preparing,
            contract: None,
            user_answers: Vec::new(),
            protected_files: Vec::new(),
            protected_hashes: Vec::new(),
            verification_commands: Vec::new(),
            acceptance_commit: None,
            attempt: 0,
            pending_question: None,
            waiting_phase: None,
            candidate_commit: None,
            last_worker_summary: None,
            last_verification: None,
            runtime: DeliveryRuntime::Dsh,
            work_order: None,
            evidence: Vec::new(),
            authority_records: None,
            build_package: None,
            created_at: 1,
            updated_at: 1,
            message: "Preparing project…".to_string(),
        }
    }

    #[test]
    fn state_json_round_trip_preserves_delivery_identity() {
        let state = sample_state();
        let json = serde_json::to_string(&state).expect("serialize state");
        let parsed: DeliveryState = serde_json::from_str(&json).expect("parse state");
        assert_eq!(parsed.session_id, state.session_id);
        assert_eq!(parsed.phase, DeliveryPhase::Preparing);
    }

    #[test]
    fn transitions_are_deterministic() {
        assert!(transition(DeliveryPhase::Preparing, DeliveryPhase::AuthoringAcceptance).is_ok());
        assert!(transition(DeliveryPhase::WaitingForUser, DeliveryPhase::Implementing).is_ok());
        assert!(transition(DeliveryPhase::Verifying, DeliveryPhase::Verified).is_ok());
        assert!(transition(DeliveryPhase::Verified, DeliveryPhase::Repairing).is_err());
    }

    #[test]
    fn implementation_attempt_limit_is_three() {
        assert_eq!(MAX_IMPLEMENTATION_ATTEMPTS, 3);
        assert!(attempts_remaining(2));
        assert!(!attempts_remaining(3));
    }

    #[test]
    fn inconclusive_resume_preserves_candidate_worktree() {
        let mut state = sample_state();
        state.last_verification = Some(VerificationReceipt {
            session_id: state.session_id.clone(),
            attempt_id: format!("{}/attempt/1", state.session_id),
            verification_id: "verification-1".to_string(),
            candidate_sha: "candidate".to_string(),
            acceptance_commit: "acceptance".to_string(),
            contract_revision: 1,
            profile_hash: "profile".to_string(),
            candidate_tree_unchanged: true,
            protected_paths_unchanged: true,
            checks: Vec::new(),
            verdict: "inconclusive".to_string(),
        });
        assert!(!recovery_requires_worktree_reset(&state, true));
        assert!(recovery_requires_worktree_reset(&state, false));
    }

    #[test]
    fn malformed_persisted_state_is_rejected() {
        assert!(serde_json::from_str::<DeliveryState>("{\"phase\":\"failed\"}").is_err());
    }

    #[tokio::test]
    async fn production_apply_guards_reject_dirty_moved_and_non_fast_forward_bases() {
        let (dirty_repo, dirty_base, dirty_candidate, _) = apply_guard_fixture("dirty");
        let mut dirty_state = sample_state();
        dirty_state.source_workspace = dirty_repo.to_string_lossy().into_owned();
        dirty_state.base_commit = dirty_base;
        dirty_state.candidate_commit = Some(dirty_candidate);
        dirty_state.phase = DeliveryPhase::Verified;
        std::fs::write(dirty_repo.join("owner.txt"), "leave untouched\n")
            .expect("dirty source checkout");
        assert!(
            apply_verified_candidate(&mut dirty_state)
                .await
                .expect_err("dirty checkout must not apply")
                .contains("not clean")
        );
        let _ = std::fs::remove_dir_all(&dirty_repo);

        let (moved_repo, moved_base, moved_candidate, _) = apply_guard_fixture("moved");
        let mut moved_state = sample_state();
        moved_state.source_workspace = moved_repo.to_string_lossy().into_owned();
        moved_state.base_commit = moved_base;
        moved_state.candidate_commit = Some(moved_candidate);
        moved_state.phase = DeliveryPhase::Verified;
        std::fs::write(moved_repo.join("moved.txt"), "new source head\n")
            .expect("write moved head");
        git_fixture_success(&moved_repo, &["add", "."]);
        git_fixture_success(&moved_repo, &["commit", "-m", "move original head"]);
        assert!(
            apply_verified_candidate(&mut moved_state)
                .await
                .expect_err("changed HEAD must not apply")
                .contains("moved since Build started")
        );
        let _ = std::fs::remove_dir_all(&moved_repo);

        let (diverged_repo, _, diverged_candidate, _) = apply_guard_fixture("diverged");
        let mut diverged_state = sample_state();
        diverged_state.source_workspace = diverged_repo.to_string_lossy().into_owned();
        std::fs::write(
            diverged_repo.join("diverged.txt"),
            "independent source commit\n",
        )
        .expect("write diverged source");
        git_fixture_success(&diverged_repo, &["add", "."]);
        git_fixture_success(&diverged_repo, &["commit", "-m", "diverge source"]);
        diverged_state.base_commit = git_fixture_success(&diverged_repo, &["rev-parse", "HEAD"]);
        diverged_state.candidate_commit = Some(diverged_candidate);
        diverged_state.phase = DeliveryPhase::Verified;
        assert!(
            apply_verified_candidate(&mut diverged_state)
                .await
                .expect_err("non-fast-forward candidate must not apply")
                .contains("Cannot apply safely")
        );
        let _ = std::fs::remove_dir_all(&diverged_repo);
    }

    #[tokio::test]
    #[ignore = "requires an approved external DSH credential and runtime"]
    async fn backend_qualification_runs_production_delivery_path() {
        let root = std::env::temp_dir().join(format!(
            "consensus arena delivery qualification {}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create qualification fixture");
        let source = root.join("source");
        std::fs::create_dir_all(source.join("tests")).expect("create fixture tests");
        std::fs::write(
            source.join("greet.py"),
            "def greet():\n    return 'before'\n",
        )
        .expect("write fixture source");
        std::fs::write(
            source.join("tests").join("test_greet.py"),
            "from greet import greet\n\ndef test_greet():\n    assert greet() == 'after'\n",
        )
        .expect("write fixture test");
        for args in [
            vec!["init"],
            vec![
                "config",
                "user.email",
                "arena-qualification@example.invalid",
            ],
            vec!["config", "user.name", "Arena Qualification"],
            vec!["add", "."],
            vec!["commit", "-m", "fixture"],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&source)
                .status()
                .expect("run fixture git command");
            assert!(status.success());
        }
        let base = validate_clean_base(&source)
            .await
            .expect("production clean-base admission helper should accept fixture");
        let worktree = root.join("candidate");
        create_candidate_worktree(&source, &worktree, "arena-delivery/qualification", &base)
            .await
            .expect("production worktree creation helper should succeed");

        let api_key = std::env::var("ARENA_DSH_API_KEY")
            .expect("ARENA_DSH_API_KEY must be explicitly supplied for this ignored test");
        let base_url = std::env::var("ARENA_DSH_BASE_URL")
            .unwrap_or_else(|_| "https://integrate.api.nvidia.com/v1".to_string());
        let model = std::env::var("ARENA_DSH_MODEL")
            .unwrap_or_else(|_| "meta/muse-glimmer-30b".to_string());
        let mut settings = SettingsStore::new(":memory:").expect("create in-memory settings");
        settings
            .set("brain_api_key", &api_key)
            .expect("set test brain key");
        settings
            .set("brain_base_url", &base_url)
            .expect("set test brain URL");
        settings
            .set("brain_model", &model)
            .expect("set test brain model");
        let mut state = DeliveryState {
            schema_version: DELIVERY_SCHEMA_VERSION,
            session_id: "qualification-session".to_string(),
            objective: "Make greet return after and keep the test passing".to_string(),
            source_workspace: source.to_string_lossy().into_owned(),
            worktree_path: worktree.to_string_lossy().into_owned(),
            branch_name: "arena-delivery/qualification".to_string(),
            base_commit: base,
            phase: DeliveryPhase::Preparing,
            contract: None,
            user_answers: Vec::new(),
            protected_files: Vec::new(),
            protected_hashes: Vec::new(),
            verification_commands: Vec::new(),
            acceptance_commit: None,
            attempt: 0,
            pending_question: None,
            waiting_phase: None,
            candidate_commit: None,
            last_worker_summary: None,
            last_verification: None,
            runtime: DeliveryRuntime::Dsh,
            work_order: None,
            evidence: Vec::new(),
            authority_records: None,
            build_package: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            message: "Preparing project…".to_string(),
        };
        let state_path = root.join("delivery-state.json");
        let transcript = Arc::new(std::sync::Mutex::new(TranscriptStore::new()));
        let session_config = crate::orchestrator::SessionConfig {
            session_id: state.session_id.clone(),
            project_brief: state.objective.clone(),
            session_type: crate::orchestrator::SessionType::Delivery,
            agent_ids: Vec::new(),
            leader_agent_id: String::new(),
        };
        transcript
            .lock()
            .expect("transcript lock")
            .create_session(&session_config)
            .expect("persist Delivery session row");
        let delivery_slot = Arc::new(tokio::sync::Mutex::new(None));
        let ask_tx = Arc::new(tokio::sync::Mutex::new(None));
        let settings = Arc::new(tokio::sync::Mutex::new(settings));
        persist_state(&state_path, &delivery_slot, &transcript, &mut state)
            .await
            .expect("persist initial production Delivery state");
        let runtime = Arc::new(crate::session_runtime::SessionRuntime::new());
        let mut completed = run_backend_qualification(
            runtime.clone(),
            state,
            state_path.clone(),
            delivery_slot.clone(),
            transcript.clone(),
            ask_tx,
            settings,
        )
        .await
        .expect("production Delivery backend should complete");
        assert_eq!(completed.phase, DeliveryPhase::Verified);
        assert!(completed.acceptance_commit.is_some());
        assert!(!completed.protected_hashes.is_empty());
        assert_eq!(completed.session_id, "qualification-session");
        assert_eq!(
            completed
                .last_verification
                .as_ref()
                .map(|receipt| receipt.verdict.as_str()),
            Some("pass")
        );
        let candidate = completed
            .candidate_commit
            .clone()
            .expect("verified candidate should have a commit");
        let receipt = completed
            .last_verification
            .as_ref()
            .expect("verified state should have a persisted verifier receipt");
        assert_eq!(receipt.session_id, completed.session_id);
        assert_eq!(receipt.candidate_sha, candidate);
        assert_eq!(
            receipt.acceptance_commit,
            completed
                .acceptance_commit
                .as_deref()
                .expect("acceptance SHA")
        );
        assert_eq!(
            receipt.attempt_id,
            format!("{}/attempt/{}", completed.session_id, completed.attempt)
        );
        assert!(!receipt.verification_id.is_empty());
        assert!(receipt.candidate_tree_unchanged);
        assert!(receipt.protected_paths_unchanged);
        let frozen_profile = VerificationProfile {
            version: 1,
            commands: completed.verification_commands.clone(),
            protected_paths: completed.protected_files.clone(),
        };
        assert_eq!(
            receipt.profile_hash,
            verification::profile_hash(&frozen_profile).expect("hash frozen profile")
        );
        apply_verified_candidate(&mut completed)
            .await
            .expect("production Apply should fast-forward fixture");
        let applied_head = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&source)
                .output()
                .expect("resolve applied HEAD")
                .stdout,
        )
        .expect("applied HEAD is utf8");
        assert_eq!(applied_head.trim(), candidate);
        persist_state(&state_path, &delivery_slot, &transcript, &mut completed)
            .await
            .expect("persist production Apply state");
        let reopened: DeliveryState = serde_json::from_slice(
            &std::fs::read(&state_path).expect("read persisted applied state"),
        )
        .expect("reopen persisted Delivery state");
        assert_eq!(reopened.phase, DeliveryPhase::Applied);
        assert_eq!(
            reopened.candidate_commit.as_deref(),
            Some(candidate.as_str())
        );
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&worktree)
            .current_dir(&source)
            .status();
        let _ = std::fs::remove_dir_all(root);
    }
}
