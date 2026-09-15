use crate::dsh_worker;
use crate::session_runtime::SessionOwner;
use crate::settings_store::SettingsStore;
use crate::transcript_store::TranscriptStore;
use crate::verification::{self, VerificationCommand, VerificationProfile, VerificationReceipt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub const DELIVERY_SCHEMA_VERSION: u32 = 1;
pub const MAX_IMPLEMENTATION_ATTEMPTS: u32 = 3;

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
    tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .await
        .map_err(|error| format!("run git {}: {error}", args.join(" ")))
}

async fn git_ok(repo: &std::path::Path, args: &[&str]) -> Result<(), String> {
    let output = git_output(repo, args).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
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

async fn persist_emit(
    app: &AppHandle,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
) -> Result<(), String> {
    persist_state(state_path, delivery_slot, transcript, state).await?;
    emit(app, state).await;
    Ok(())
}

async fn ask_user(
    app: &AppHandle,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: &Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    state: &mut DeliveryState,
    question: PendingQuestion,
    resume_phase: DeliveryPhase,
) -> Result<String, String> {
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
    let _ = app.emit(
        "agent-ask-user",
        serde_json::json!({
            "question": question.text,
            "options": question.options,
            "allow_custom": question.allow_custom
        }),
    );
    let answer = receiver
        .await
        .map_err(|_| "delivery answer channel closed".to_string())?;
    state.pending_question = None;
    state.waiting_phase = None;
    state.user_answers.push(UserAnswer {
        question: question.text,
        answer: answer.clone(),
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
    Ok(answer)
}

fn worker_summary(execution: &dsh_worker::WorkerExecution, api_key: &str) -> String {
    match &execution.result {
        Some(result) => format!(
            "DSH exit {:?}; worker status {:?}; {}",
            execution.exit_code,
            result.status,
            dsh_worker::redact_secret(&result.summary, api_key)
        ),
        None if execution.timed_out => "DSH timed out before producing a result".to_string(),
        None => format!(
            "DSH exit {:?}; no valid result.json was produced",
            execution.exit_code
        ),
    }
}

async fn finish_verified(
    app: &AppHandle,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
) -> Result<(), String> {
    clean_runtime(std::path::Path::new(&state.worktree_path)).await?;
    git_ok(std::path::Path::new(&state.worktree_path), &["add", "-A"]).await?;
    let staged = git_output(
        std::path::Path::new(&state.worktree_path),
        &["diff", "--cached", "--quiet"],
    )
    .await?;
    if staged.status.code() == Some(1) {
        git_ok(
            std::path::Path::new(&state.worktree_path),
            &[
                "commit",
                "-m",
                &format!("arena: verified delivery {}", state.session_id),
            ],
        )
        .await?;
    } else if !staged.status.success() {
        return Err(String::from_utf8_lossy(&staged.stderr).trim().to_string());
    }
    state.candidate_commit = Some(verification::candidate_sha(std::path::Path::new(
        &state.worktree_path,
    ))?);
    state.phase = DeliveryPhase::Verified;
    persist_emit(app, state_path, delivery_slot, transcript, state).await
}

async fn freeze_acceptance(
    app: &AppHandle,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    state: &mut DeliveryState,
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
    git_ok(
        worktree,
        &[
            "commit",
            "-m",
            &format!("arena: freeze acceptance {}", state.session_id),
        ],
    )
    .await?;
    state.acceptance_commit = Some(verification::candidate_sha(worktree)?);
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
    app: &AppHandle,
    state_path: &PathBuf,
    delivery_slot: &Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: &Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: &Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    state: &mut DeliveryState,
    model: &dsh_worker::DshModelConfig,
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
        state.last_worker_summary = Some(worker_summary(&execution, &model.api_key));
        let Some(result) = execution.result else {
            return Err(state
                .last_worker_summary
                .clone()
                .unwrap_or_else(|| "acceptance authoring failed".to_string()));
        };
        match result.status {
            dsh_worker::WorkerStatus::NeedsUser => {
                let question = result
                    .question
                    .ok_or_else(|| "worker returned needs_user without a question".to_string())?;
                let pending = PendingQuestion {
                    text: dsh_worker::redact_secret(&question.text, &model.api_key),
                    options: question
                        .options
                        .iter()
                        .map(|value| dsh_worker::redact_secret(value, &model.api_key))
                        .collect(),
                    allow_custom: question.allow_custom,
                    reason: dsh_worker::redact_secret(&question.reason, &model.api_key),
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
                return Err(dsh_worker::redact_secret(&result.summary, &model.api_key));
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
                if result.verification_commands.iter().any(|command| {
                    dsh_worker::contains_secret(&command.program, &model.api_key)
                        || command
                            .args
                            .iter()
                            .any(|arg| dsh_worker::contains_secret(arg, &model.api_key))
                }) {
                    return Err("acceptance worker attempted to place the Agent Brain secret in a verification command".to_string());
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
                            description: dsh_worker::redact_secret(
                                &item.description,
                                &model.api_key,
                            ),
                        })
                        .collect(),
                    constraints: vec![
                        "No deployment or external infrastructure changes".to_string(),
                    ],
                    worker_brief: "Acceptance tests are frozen before implementation".to_string(),
                });
                freeze_acceptance(app, state_path, delivery_slot, transcript, state).await?;
                return Ok(());
            }
        }
    }
}

async fn reset_worktree(state: &DeliveryState, commit: &str) -> Result<(), String> {
    let worktree = std::path::Path::new(&state.worktree_path);
    clean_runtime(worktree).await?;
    git_ok(worktree, &["reset", "--hard", commit]).await?;
    git_ok(worktree, &["clean", "-fd"]).await
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

pub async fn run(
    runtime: Arc<crate::session_runtime::SessionRuntime>,
    app: AppHandle,
    mut state: DeliveryState,
    state_path: PathBuf,
    owner: SessionOwner,
    delivery_slot: Arc<tokio::sync::Mutex<Option<DeliveryState>>>,
    transcript: Arc<std::sync::Mutex<TranscriptStore>>,
    ask_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    settings: Arc<tokio::sync::Mutex<SettingsStore>>,
) {
    let result = async {
        let brain = settings.lock().await.get_agent_brain_config().map_err(|error| format!("read Agent Brain settings: {error}"))?;
        let model = dsh_worker::DshModelConfig { api_key: brain.api_key, base_url: brain.base_url, model: brain.model };
        let patch_dir = state_path.parent().unwrap_or(std::path::Path::new(".")).join("dsh-runtime").join(&state.session_id);
        let worktree = PathBuf::from(&state.worktree_path);
        if state.phase == DeliveryPhase::WaitingForUser {
            let pending = state.pending_question.clone().ok_or_else(|| "waiting delivery has no pending question".to_string())?;
            let resume_phase = state.waiting_phase.clone().unwrap_or(if state.acceptance_commit.is_some() {
                DeliveryPhase::Implementing
            } else {
                DeliveryPhase::AuthoringAcceptance
            });
            let _ = ask_user(&app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, pending, resume_phase).await?;
            if state.phase == DeliveryPhase::Failed { return Err("user cancelled delivery".to_string()); }
        }
        if matches!(state.phase, DeliveryPhase::Preparing | DeliveryPhase::AuthoringAcceptance) && state.acceptance_commit.is_none() {
            if state.phase == DeliveryPhase::Preparing { state.phase = DeliveryPhase::AuthoringAcceptance; }
            run_acceptance_authoring(&app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, &model, &patch_dir).await?;
        }
        if state.acceptance_commit.is_none() { return Err("delivery has no frozen acceptance commit".to_string()); }
        let profile = VerificationProfile { version: 1, commands: state.verification_commands.clone(), protected_paths: state.protected_files.clone() };
        verification::validate_profile(&profile, &worktree)?;
        if state.phase == DeliveryPhase::AcceptanceReady || state.phase == DeliveryPhase::Verifying {
            state.phase = DeliveryPhase::Verifying;
            persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let hashes = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let receipt = verification::verify(&state.session_id, &worktree, &profile, &hashes, &state_path.parent().unwrap_or(std::path::Path::new(".")).join("delivery-evidence").join(&state.session_id), 1).await?;
            state.last_verification = Some(receipt.clone());
            if receipt.verdict == "pass" { finish_verified(&app, &state_path, &delivery_slot, &transcript, &mut state).await?; return Ok::<(), String>(()); }
            if receipt.verdict == "inconclusive" {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("Verification inconclusive; no implementation repair was attempted. Resolve the verification environment and resume to rerun the frozen checks.".to_string());
                persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                return Ok::<(), String>(());
            }
            if !attempts_remaining(state.attempt) { state.phase = DeliveryPhase::Failed; return Ok(()); }
            state.phase = DeliveryPhase::Repairing;
        }
        while attempts_remaining(state.attempt) {
            state.attempt += 1;
            state.phase = DeliveryPhase::Implementing;
            persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let answers = state.user_answers.iter().map(|answer| format!("{} => {}", answer.question, answer.answer)).collect::<Vec<_>>().join("\n");
            let evidence = state.last_verification.as_ref().map(|receipt| format!("verdict={} checks={:?}", receipt.verdict, receipt.checks)).unwrap_or_else(|| "acceptance checks failed".to_string());
            let prompt = dsh_worker::bounded_prompt(&state.objective, &format!("IMPLEMENTATION TASK. Frozen acceptance items are in the repository. Protected files: {}. User decisions:\n{answers}", state.protected_files.join(", ")), Some(&evidence));
            let execution = dsh_worker::run(&worktree, &worktree.join(".arena-runtime"), &patch_dir, &model, &prompt, 1800).await?;
            clean_runtime(&worktree).await?;
            state.last_worker_summary = Some(worker_summary(&execution, &model.api_key));
            let protected_before = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let protected_changed = !verification::protected_files_unchanged(&worktree, &protected_before);
            if protected_changed {
                for path in &state.protected_files { git_ok(&worktree, &["restore", "--source", state.acceptance_commit.as_deref().unwrap_or("HEAD"), "--", path]).await?; }
                state.last_worker_summary = Some("worker attempted to modify protected acceptance files; restored from acceptance commit".to_string());
            } else if let Some(result) = execution.result {
                if result.status == dsh_worker::WorkerStatus::NeedsUser {
                    let question = result.question.ok_or_else(|| "worker returned needs_user without a question".to_string())?;
                    let pending = PendingQuestion {
                        text: dsh_worker::redact_secret(&question.text, &model.api_key),
                        options: question.options.iter().map(|value| dsh_worker::redact_secret(value, &model.api_key)).collect(),
                        allow_custom: question.allow_custom,
                        reason: dsh_worker::redact_secret(&question.reason, &model.api_key),
                    };
                    let _ = ask_user(&app, &state_path, &delivery_slot, &transcript, &ask_tx, &mut state, pending, DeliveryPhase::Implementing).await?;
                    if state.phase == DeliveryPhase::Failed { return Err("user cancelled delivery".to_string()); }
                    state.attempt = state.attempt.saturating_sub(1);
                    continue;
                }
            }
            state.phase = DeliveryPhase::Verifying;
            persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
            let hashes = state.protected_hashes.iter().map(|value| (value.path.clone(), value.sha256.clone())).collect::<Vec<_>>();
            let mut receipt = verification::verify(&state.session_id, &worktree, &profile, &hashes, &state_path.parent().unwrap_or(std::path::Path::new(".")).join("delivery-evidence").join(&state.session_id), 1).await?;
            if protected_changed {
                receipt.verdict = "fail".to_string();
            }
            state.last_verification = Some(receipt.clone());
            if receipt.verdict == "pass" { finish_verified(&app, &state_path, &delivery_slot, &transcript, &mut state).await?; return Ok(()); }
            if receipt.verdict == "inconclusive" {
                state.phase = DeliveryPhase::Failed;
                state.last_worker_summary = Some("Verification inconclusive; product-code repair was not attempted. Resolve the verification environment and resume to rerun the frozen checks.".to_string());
                persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
                break;
            }
            if !attempts_remaining(state.attempt) { state.phase = DeliveryPhase::Failed; break; }
            state.phase = DeliveryPhase::Repairing;
            persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await?;
        }
        Ok::<(), String>(())
    }.await;
    if let Err(error) = result {
        state.phase = DeliveryPhase::Failed;
        state.last_worker_summary = Some(error);
        let _ = persist_emit(&app, &state_path, &delivery_slot, &transcript, &mut state).await;
    }
    runtime.mark_completed(&owner);
}

#[cfg(test)]
mod tests {
    use super::*;

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
            candidate_sha: "candidate".to_string(),
            contract_revision: 1,
            profile_hash: "profile".to_string(),
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
}
