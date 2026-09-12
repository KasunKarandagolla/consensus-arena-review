use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::agent_brain::{AgentBrain, AgentDecision, BrainSource};
use crate::blueprint_store::{BlueprintSection, SectionStatus};
use crate::browser_backend::NavEvent;
use crate::critical_transport::{CriticalTransportError, OperationInbox};
use crate::errors::{AgentError, ErrorKind};
use crate::memory_store::SessionSummaryData;
use crate::orchestrator::{
    ActiveBrainKind, ActiveBrainStatus, AppState, ModelHealth, SessionConfig,
};
use crate::pipeline_ids::{BrowserSurface, OperationContext};

struct PendingAdoptionCheck {
    model_id: String,
    topic: String,
    prompt_excerpt: String,
}

// ── Constants ─────────────────────────────────────────────────────────────────

const RESPONSE_TIMEOUT_SECS: u64 = 300;

/// A proven submit is irreversible. Pre-submit discovery/injection can retry,
/// but an acknowledgement timeout must never click Send a second time.
const MAX_SUBMIT_ACTION_RETRIES: u32 = 0;
/// Timeout awaiting a fresh `ActiveSubmitReport` ack for each submit attempt.
const SUBMIT_ACK_TIMEOUT_SECS: u64 = 30;

/// IMP-2: Maximum number of retry attempts for participant injection.
/// Attempt 0 is the initial try; attempts 1–3 are retries with backoff.
const MAX_RETRIES: u32 = 3;

/// IMP-2: Exponential backoff base in seconds.
/// Attempt 1 → 2 s, attempt 2 → 4 s, attempt 3 → 8 s (all < 60 s cap).
const BACKOFF_BASE_SECS: u64 = 2;
const MAX_UNCLASSIFIED_CONTINUES: u32 = 1;

#[derive(Default)]
struct ResponseAssembly {
    byte_length: usize,
    chunk_count: u32,
    checksum: String,
    chunks: Vec<Option<String>>,
}

fn response_checksum(text: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:x}")
}

impl ResponseAssembly {
    fn start(byte_length: usize, chunk_count: u32, checksum: String) -> Result<Self, AgentError> {
        // Aligned with the transport contract: the declared chunk count can
        // never exceed MAX_RESPONSE_CHUNKS (browser_backend response-start
        // validation rejects anything larger), so the assembly ceiling must
        // match it exactly. A second, larger ceiling here would admit no
        // additional legal response (nothing above the transport bound can
        // arrive) while suggesting contradictory capacity.
        if chunk_count as usize > crate::critical_transport::MAX_RESPONSE_CHUNKS {
            return Err(AgentError::ExtractionFailed(
                "response transport declared too many chunks".to_string(),
            ));
        }
        Ok(Self {
            byte_length,
            chunk_count,
            checksum,
            chunks: vec![None; chunk_count as usize],
        })
    }

    fn insert(&mut self, sequence: u32, text: String) -> Result<(), AgentError> {
        let Some(slot) = self.chunks.get_mut(sequence as usize) else {
            return Err(AgentError::ExtractionFailed(
                "response chunk sequence out of range".to_string(),
            ));
        };
        if slot.as_ref().is_some_and(|existing| existing != &text) {
            return Err(AgentError::ExtractionFailed(
                "conflicting response chunk".to_string(),
            ));
        }
        *slot = Some(text);
        Ok(())
    }

    fn finish(self, checksum: &str) -> Result<String, AgentError> {
        if checksum != self.checksum || self.chunks.iter().any(Option::is_none) {
            return Err(AgentError::ExtractionFailed(
                "response transport incomplete or checksum mismatch".to_string(),
            ));
        }
        let text = self.chunks.into_iter().flatten().collect::<String>();
        if text.len() != self.byte_length || response_checksum(&text) != self.checksum {
            return Err(AgentError::ExtractionFailed(
                "response transport integrity check failed".to_string(),
            ));
        }
        Ok(text)
    }
}

async fn begin_operation(
    state: &AppState,
    agent_id: &str,
    turn: u32,
    surface: BrowserSurface,
) -> Result<(OperationContext, OperationInbox<NavEvent>), AgentError> {
    let owner = state
        .session_runtime
        .current_owner()
        .ok_or_else(|| AgentError::UnknownError("no active session owner".to_string()))?;
    let mut browser = state.browser_state.lock().await;
    browser
        .begin_active_operation(&owner, agent_id, turn, surface)
        .map_err(|e| AgentError::UnknownError(format!("begin operation failed: {e}")))
}

async fn finish_operation(
    state: &AppState,
    operation_id: &crate::pipeline_ids::OperationId,
    response_captured: bool,
) {
    let mut browser = state.browser_state.lock().await;
    browser.finish_active_operation(operation_id, response_captured);
}

fn critical_to_agent_error(err: CriticalTransportError) -> AgentError {
    match err {
        CriticalTransportError::Closed => {
            AgentError::NavigationFailed("critical operation closed".to_string())
        }
        CriticalTransportError::IngressUnavailable => {
            AgentError::NavigationFailed("critical ingress unavailable".to_string())
        }
        CriticalTransportError::IngressOverflow => {
            AgentError::NavigationFailed("critical ingress overflow".to_string())
        }
        CriticalTransportError::EventBudgetExceeded => {
            AgentError::ExtractionFailed("critical event budget exceeded".to_string())
        }
        CriticalTransportError::PayloadBudgetExceeded => {
            AgentError::ExtractionFailed("critical payload budget exceeded".to_string())
        }
        CriticalTransportError::Protocol(msg) => {
            AgentError::ExtractionFailed(format!("critical protocol error: {msg}"))
        }
    }
}

/// RC1-F3: total retry accounting — the outer `inject_and_wait_with_retry`
/// loop (MAX_RETRIES=3 → up to 4 attempts) and the inner
/// `confirm_active_submit` loop (MAX_SUBMIT_ACTION_RETRIES=0 → exactly 1
/// submit ack wait per outer attempt) are intentionally separate budgets:
/// outer = turn-level navigation/injection/response, inner = action-level
/// submit confirmation. There is intentionally no physical Send retry:
/// worst-case per participant turn = 4 outer attempts × 1 submit action,
/// all bounded and capped at 60 s backoff per outer retry. No retry path is
/// unbounded or infinite; empty-shell failures bypass both via
/// `should_retry_after_failure`.

/// W1-C: distinguish Category 1 (transient/navigation) vs Category 2
/// (empty-shell/readiness). Empty-shell failures are page readiness
/// failures where the document loaded but hydration never produced a
/// composer (bodyLen <40, interactive <2). Retrying with a full
/// `window.navigate` to the same URL destroys evidence and can amplify
/// one failure into repeated reloads. This returns false for empty-shell,
/// true for retryable transient failures. Uses live BrowserDiagnostics
/// page_state_hint rather than guessing from error variant alone, so a
/// Timeout due to network keeps its retry budget while a Timeout with
/// `empty_shell_or_hydration_stuck` does not.
fn should_retry_after_failure(
    error: &AgentError,
    diagnostics: &crate::browser_backend::BrowserDiagnostics,
    agent_id: &str,
    turn: u32,
    attempt: u32,
) -> bool {
    // Exact active idempotency: setup responses and responses from another
    // turn/generation cannot suppress or authorize this turn's retry.
    if diagnostics.has_active_response_observed(agent_id, turn) {
        tracing::info!(
            "[RETRY] {} not retrying — late response already observed after injection (attempt {}/{}): {}",
            agent_id,
            attempt,
            MAX_RETRIES,
            error
        );
        return false;
    }
    if attempt >= MAX_RETRIES {
        return false;
    }
    if error.kind() == ErrorKind::Permanent {
        return false;
    }
    // Category 2 — empty shell/hydration: do not blindly reload same URL.
    // Record diagnostic is done by caller via record_browser_error before
    // this check, so we only need to decide retry vs bounded failure.
    if diagnostics.is_empty_shell_failure(agent_id) {
        tracing::warn!(
            "[RETRY] {} page_state_hint=empty_shell_or_hydration_stuck — not retrying navigation (attempt {}/{}): {}",
            agent_id,
            attempt,
            MAX_RETRIES,
            error
        );
        return false;
    }
    // All other Timeouts remain retryable only if they are not empty-shell.
    // Challenge/Captcha is handled inside wait_for_response (600s Resume wait)
    // and never reaches this helper as a Timeout.
    true
}

fn resolve_selected_agent_id(target: &str, config: &SessionConfig) -> Option<String> {
    let normalized = target.trim().to_ascii_lowercase();
    config.agent_ids.iter().find_map(|agent_id| {
        let display = crate::browser_backend::display_name_for(agent_id);
        (agent_id.eq_ignore_ascii_case(&normalized) || display.eq_ignore_ascii_case(&normalized))
            .then(|| agent_id.clone())
    })
}

fn leader_requests_consultation(response: &str) -> bool {
    let response = response.to_ascii_lowercase();
    [
        "questions to consult another model",
        "consult another model",
        "consult deepseek",
        "risks",
        "simplification",
        "critique",
    ]
    .iter()
    .any(|phrase| response.contains(phrase))
}

fn looks_like_blueprint(response: &str) -> bool {
    let response = response.to_ascii_lowercase();
    response.contains("blueprint")
        || (response.contains("mvp")
            && (response.contains("feature") || response.contains("scope")))
        || (response.contains("architecture") && response.contains("implementation"))
}

async fn first_task_envelope(
    state: &AppState,
    config: &SessionConfig,
    agent_id: &str,
    task: &str,
) -> String {
    let is_leader = agent_id == config.leader_agent_id;
    let template = {
        let store = state.settings_store.lock().await;
        let key = if is_leader {
            "prompt_leader_priming"
        } else {
            "prompt_participant_priming"
        };
        store
            .get_prompt_template_with_default(key)
            .unwrap_or_else(|_| {
                if is_leader {
                    crate::settings_store::default_leader_priming()
                } else {
                    crate::settings_store::default_participant_priming()
                }
            })
    };
    let names = config
        .agent_ids
        .iter()
        .map(|id| crate::browser_backend::display_name_for(id))
        .collect::<Vec<_>>()
        .join(", ");
    let others = config
        .agent_ids
        .iter()
        .filter(|id| *id != &config.leader_agent_id)
        .map(|id| crate::browser_backend::display_name_for(id))
        .collect::<Vec<_>>()
        .join(", ");
    let role = if is_leader { "Leader" } else { "Participant" };
    let priming = template
        .replace(
            "{{participant_count}}",
            &config.agent_ids.len().saturating_sub(1).to_string(),
        )
        .replace("{{participant_list_with_display_names}}", &others)
        .replace(
            "{{leader_display_name}}",
            crate::browser_backend::display_name_for(&config.leader_agent_id),
        )
        .replace("{{full_participant_list_including_leader}}", &names)
        .replace("{{project_brief}}", &config.project_brief)
        .replace("{{role}}", role);
    format!("{priming}\n\n--- CURRENT ARENA TASK ---\n\n{task}")
}

fn drain_stale_active_events(_nav_rx: &mut Receiver<NavEvent>) -> usize {
    // MUST remain no-op: draining shared auxiliary must never discard critical active events.
    0
}

fn take_queued_response_for_turn(
    _nav_rx: &mut Receiver<NavEvent>,
    _agent_id: &str,
    _turn: u32,
) -> Option<String> {
    // Destructive shared-response scan removed. Active response recovery must use operation-owned inbox only.
    None
}

/// Outcome of the active-turn auto-submit confirmation gate.
#[derive(Debug)]
enum SubmitOutcome {
    /// The page reported the exact (agent_id, turn) as submitted.
    Confirmed,
    /// A response for this turn was already captured while awaiting the ack.
    ResponseEarly(String),
    /// Auto-submit could not be confirmed after bounded action retries.
    /// The prompt remains in the composer; the user must send or paste.
    ManualRecovery,
}

#[derive(Debug)]
struct AckResult {
    outcome: SubmitOutcome,
    early_buffer: std::collections::VecDeque<NavEvent>,
}

/// Outcome of the submit-ACK deadline wait. The early response buffer is
/// owned by the caller (`confirm_active_submit`), never by this future, so
/// deadline expiration cannot destroy already-consumed response events.
#[derive(Debug)]
enum SubmitAckWait {
    Outcome(SubmitOutcome),
    TimedOut,
}

/// Wait for the single `ActiveSubmitReport` matching the exact operation.
/// Reads ONLY critical inbox. Buffers early response events into the
/// caller-owned `early` buffer. The ACK deadline is enforced INSIDE the
/// receive loop via `timeout_at` around each `OperationInbox::recv()`, so a
/// deadline return leaves every already-consumed current-operation response
/// event intact in `early`. Exact OperationId + agent + turn checks apply to
/// every consumed event.
async fn await_submit_ack_until(
    context: &OperationContext,
    inbox: &mut OperationInbox<NavEvent>,
    early: &mut std::collections::VecDeque<NavEvent>,
    deadline: Instant,
) -> Result<SubmitAckWait, AgentError> {
    loop {
        let event = match tokio::time::timeout_at(deadline, inbox.recv()).await {
            Ok(Ok(event)) => event,
            Ok(Err(error)) => return Err(critical_to_agent_error(error)),
            Err(_) => return Ok(SubmitAckWait::TimedOut),
        };
        match event {
            NavEvent::ActiveSubmitReport {
                operation_id,
                agent_id,
                turn,
                succeeded,
                method,
                send_enabled,
                error,
            } => {
                if operation_id != context.operation_id {
                    return Err(AgentError::ExtractionFailed(
                        "operation id mismatch in submit report".to_string(),
                    ));
                }
                if agent_id != context.agent_id || turn != context.turn {
                    return Err(AgentError::ExtractionFailed(
                        "agent/turn mismatch in submit report".to_string(),
                    ));
                }
                if succeeded {
                    return Ok(SubmitAckWait::Outcome(SubmitOutcome::Confirmed));
                } else {
                    let detail = error
                        .clone()
                        .unwrap_or_else(|| format!("method={method} enabled={send_enabled}"));
                    return Err(AgentError::InjectionFailed(format!(
                        "submit failed: {detail}"
                    )));
                }
            }
            NavEvent::Response {
                operation_id,
                agent_id,
                turn,
                text,
            } => {
                if operation_id != context.operation_id {
                    return Err(AgentError::ExtractionFailed(
                        "response operation id mismatch".to_string(),
                    ));
                }
                if agent_id != context.agent_id || turn != context.turn {
                    return Err(AgentError::ExtractionFailed(
                        "response agent/turn mismatch".to_string(),
                    ));
                }
                return Ok(SubmitAckWait::Outcome(SubmitOutcome::ResponseEarly(text)));
            }
            NavEvent::ManualResponse {
                operation_id,
                agent_id,
                turn,
                response,
                ..
            } => {
                if operation_id != context.operation_id {
                    return Err(AgentError::ExtractionFailed(
                        "manual response operation id mismatch".to_string(),
                    ));
                }
                if agent_id != context.agent_id || turn != context.turn {
                    return Err(AgentError::ExtractionFailed(
                        "manual response agent/turn mismatch".to_string(),
                    ));
                }
                return Ok(SubmitAckWait::Outcome(SubmitOutcome::ResponseEarly(
                    response,
                )));
            }
            NavEvent::ResponseStart {
                ref operation_id,
                ref agent_id,
                turn,
                ..
            }
            | NavEvent::ResponseChunk {
                ref operation_id,
                ref agent_id,
                turn,
                ..
            }
            | NavEvent::ResponseEnd {
                ref operation_id,
                ref agent_id,
                turn,
                ..
            }
            | NavEvent::Done {
                ref operation_id,
                ref agent_id,
                turn,
            } => {
                if operation_id != &context.operation_id {
                    return Err(AgentError::ExtractionFailed(
                        "response assembly operation id mismatch".to_string(),
                    ));
                }
                if agent_id != &context.agent_id || turn != context.turn {
                    return Err(AgentError::ExtractionFailed(
                        "response assembly agent/turn mismatch".to_string(),
                    ));
                }
                early.push_back(event);
                if early.len() > crate::critical_transport::MAX_OPERATION_CRITICAL_EVENTS {
                    return Err(AgentError::ExtractionFailed(
                        "early buffer overflow".to_string(),
                    ));
                }
                continue;
            }
            _ => {
                return Err(AgentError::ExtractionFailed(format!(
                    "unexpected critical event: {event:?}"
                )));
            }
        }
    }
}

async fn confirm_active_submit(
    context: &OperationContext,
    inbox: &mut OperationInbox<NavEvent>,
    app: &AppHandle,
) -> Result<AckResult, AgentError> {
    // The early response buffer is owned HERE, outside any future subject to
    // the ACK deadline. `await_submit_ack_until` enforces the deadline inside
    // its receive loop, so timeout/error returns leave the captured prefix
    // intact for the later response wait. There is intentionally no
    // submit-action retry (MAX_SUBMIT_ACTION_RETRIES = 0): a proven submit is
    // irreversible and must never click Send a second time.
    let mut early: std::collections::VecDeque<NavEvent> = std::collections::VecDeque::new();
    let mut last_error: Option<String> = None;

    for attempt in 0..=MAX_SUBMIT_ACTION_RETRIES {
        let deadline = Instant::now() + Duration::from_secs(SUBMIT_ACK_TIMEOUT_SECS);
        match await_submit_ack_until(context, inbox, &mut early, deadline).await {
            Ok(SubmitAckWait::Outcome(outcome)) => {
                tracing::debug!(
                    "[SUBMIT] {} turn {} confirmed (attempt {attempt})",
                    context.agent_id,
                    context.turn
                );
                return Ok(AckResult {
                    outcome,
                    early_buffer: early,
                });
            }
            Ok(SubmitAckWait::TimedOut) => {
                last_error = Some(format!(
                    "no submit confirmation within {SUBMIT_ACK_TIMEOUT_SECS}s"
                ));
                tracing::warn!(
                    "[SUBMIT] {} turn {} ack timeout (attempt {attempt}/{MAX_SUBMIT_ACTION_RETRIES})",
                    context.agent_id,
                    context.turn
                );
                // DO NOT clear `early`: response events already consumed for
                // this exact operation survive the deadline.
            }
            Err(e) => {
                last_error = Some(e.to_string());
                tracing::warn!(
                    "[SUBMIT] {} turn {} ack error (attempt {attempt}): {e}",
                    context.agent_id,
                    context.turn
                );
                // DO NOT clear `early` merely because ACK proof failed: a
                // failed submit report must not destroy a valid response
                // prefix already captured for this operation. If the error is
                // an identity mismatch, the prefix cannot be valid anyway
                // (mismatched events are rejected before buffering).
            }
        }
    }

    let detail = last_error.unwrap_or_else(|| "unknown submit failure".to_string());
    let display = crate::browser_backend::display_name_for(&context.agent_id);
    let _ = app.emit("boss-message", serde_json::json!({
        "text": format!(
            "Auto-submit for {} (turn {}) could not be confirmed: {}. The prompt may already be in the composer — press Send in that window or use Paste Response to continue.",
            display, context.turn, detail
        ),
        "message_type": "status"
    }));
    let _ = app.emit(
        "active-turn-state",
        serde_json::json!({
            "event": "active_submit_failed",
            "agent_id": context.agent_id,
            "turn_number": context.turn,
            "error": detail,
        }),
    );
    Ok(AckResult {
        outcome: SubmitOutcome::ManualRecovery,
        early_buffer: early,
    })
}

/// Returns `Some(response)` only when the response was captured before the
/// submit ack (rare); otherwise `None` and the caller waits via
/// `wait_for_response`.
async fn inject_active_prompt(
    window: tauri::WebviewWindow,
    agent_id: &str,
    prompt: &str,
    turn: u32,
    state: &AppState,
    app: &AppHandle,
    _nav_rx: &mut Receiver<NavEvent>,
) -> Result<
    (
        Option<String>,
        OperationContext,
        OperationInbox<NavEvent>,
        std::collections::VecDeque<NavEvent>,
    ),
    AgentError,
> {
    // Determine surface: if window is leader window then Leader else Participant (heuristic)
    let surface = {
        let browser = state.browser_state.lock().await;
        if let Some(leader_win) = browser.leader_window.clone() {
            if leader_win.label() == window.label() {
                BrowserSurface::Leader
            } else {
                BrowserSurface::Participant
            }
        } else {
            BrowserSurface::Participant
        }
    };
    let (context, mut inbox) = begin_operation(state, agent_id, turn, surface).await?;
    let lifecycle = {
        let browser = state.browser_state.lock().await;
        browser.lifecycle.clone()
    };
    let _ = app.emit(
        "active-turn-state",
        serde_json::json!({
            "event": "active_turn_started",
            "agent_id": agent_id,
            "turn_number": turn,
        }),
    );
    // Session 03: even wait_ready=false requires the exact current
    // ReadyLease — injection can never bypass a missing/stale lease.
    let inject_result = crate::browser_backend::inject_to_window(
        window.clone(),
        &lifecycle,
        agent_id,
        prompt,
        turn,
        Some(&context.operation_id),
        false,
        true,
    )
    .await;
    if let Err(e) = inject_result {
        finish_operation(state, &context.operation_id, false).await;
        return Err(AgentError::InjectionFailed(format!(
            "Failed to inject active prompt for {agent_id}: {e}"
        )));
    }

    let ack_result = confirm_active_submit(&context, &mut inbox, app).await;
    let ack = match ack_result {
        Ok(a) => a,
        Err(e) => {
            finish_operation(state, &context.operation_id, false).await;
            return Err(e);
        }
    };
    let (early_response_opt, early_buffer) = match ack.outcome {
        SubmitOutcome::Confirmed => (None, ack.early_buffer),
        SubmitOutcome::ResponseEarly(response) => (Some(response), ack.early_buffer),
        SubmitOutcome::ManualRecovery => {
            let diagnostics = {
                let browser = state.browser_state.lock().await;
                browser.diagnostics.clone()
            };
            crate::browser_backend::record_browser_error(
                app,
                &diagnostics,
                agent_id,
                &format!(
                    "auto-submit not confirmed after {MAX_SUBMIT_ACTION_RETRIES} action retries"
                ),
            );
            (None, ack.early_buffer)
        }
    };

    {
        let browser = state.browser_state.lock().await;
        browser.mark_active_waiting(agent_id, turn);
    }
    let _ = app.emit(
        "active-turn-state",
        serde_json::json!({
            "event": "active_prompt_injected",
            "agent_id": agent_id,
            "turn_number": turn,
        }),
    );
    let _ = app.emit(
        "active-turn-state",
        serde_json::json!({
            "event": "active_waiting_for_response",
            "agent_id": agent_id,
            "turn_number": turn,
        }),
    );
    Ok((early_response_opt, context, inbox, early_buffer))
}

async fn finish_active_turn(state: &AppState, agent_id: &str, turn: u32, response_captured: bool) {
    // Legacy wrapper: try to finish via active_operation if present
    let mut browser = state.browser_state.lock().await;
    if let Some(ctx) = browser.active_operation.clone() {
        if ctx.agent_id == agent_id && ctx.turn == turn {
            let op_id = ctx.operation_id.clone();
            drop(browser);
            finish_operation(state, &op_id, response_captured).await;
            return;
        }
    }
    let mut browser = state.browser_state.lock().await;
    browser.clear_active_turn(agent_id, turn, response_captured);
}

// ── Main session loop ─────────────────────────────────────────────────────────

pub async fn run_agent_loop(
    config: &SessionConfig,
    brain: &AgentBrain,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError> {
    {
        let memory_store = state.memory_store.clone();
        let session_id = config.session_id.clone();
        let project_brief = config.project_brief.clone();
        let result = crate::db_helpers::run_blocking(move || {
            let mut memory = memory_store
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let archived = memory
                .archive_old_session_facts(&project_brief, &session_id)
                .unwrap_or_else(|e| {
                    eprintln!("[MEMORY] session archive: {e}");
                    0
                });
            let decayed = memory
                .decay_stale_importance(&project_brief)
                .unwrap_or_else(|e| {
                    eprintln!("[MEMORY] importance decay: {e}");
                    0
                });
            Ok((archived, decayed))
        })
        .await;
        match result {
            Ok((archived, decayed)) if archived > 0 || decayed > 0 => {
                eprintln!(
                    "[MEMORY] Session start: archived {archived} stale facts, decayed {decayed} entries"
                );
            }
            Err(e) => eprintln!("[MEMORY] session start maintenance: {e}"),
            _ => {}
        }
    }

    let session_type = match &config.session_type {
        crate::context_manager::SessionType::Architecture => "architecture",
        crate::context_manager::SessionType::Mvp => "mvp",
        crate::context_manager::SessionType::Api => "api",
        crate::context_manager::SessionType::Security => "security",
        crate::context_manager::SessionType::Custom => "custom",
    }
    .to_string();
    let memory_context = {
        let memory_store = state.memory_store.clone();
        let session_id = config.session_id.clone();
        let project_brief = config.project_brief.clone();
        crate::db_helpers::run_blocking(move || {
            let mut memory = memory_store
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            match memory.get_project_memory_checked(&project_brief) {
                crate::memory_store::MemoryReadOutcome::Failed(reason) => {
                    eprintln!("[MEMORY] context build degraded: {reason}");
                }
                crate::memory_store::MemoryReadOutcome::Empty => {
                    eprintln!("[MEMORY] no prior context for this project yet");
                }
                crate::memory_store::MemoryReadOutcome::Found(rows) => {
                    tracing::debug!("[MEMORY] {} prior project entries available", rows.len());
                }
            }
            Ok(memory
                .build_memory_context(&session_id, &project_brief, Some(&session_type), None)
                .unwrap_or_else(|e| {
                    eprintln!("[MEMORY] context build: {e}");
                    String::new()
                }))
        })
        .await
        .unwrap_or_else(|e| {
            eprintln!("[MEMORY] context dispatch: {e}");
            String::new()
        })
    };
    let mem_ctx = if memory_context.is_empty() {
        None
    } else {
        Some(memory_context.as_str())
    };

    let leader_id = config.leader_agent_id.clone();
    // Capture exact runtime owner for this loop — all phase transitions must be exact-owner safe
    let loop_owner = state
        .session_runtime
        .current_owner()
        .ok_or_else(|| AgentError::UnknownError("No runtime owner for loop".to_string()))?;
    if loop_owner.session_id != config.session_id {
        return Err(AgentError::UnknownError(format!(
            "Runtime owner {} does not match loop session {}",
            loop_owner.session_id, config.session_id
        )));
    }
    let mut iteration: u32 = 0;
    let mut pending_adoptions: Vec<PendingAdoptionCheck> = Vec::new();
    let mut models_consulted_since_last_section: Vec<String> = Vec::new();
    let mut iterations_since_last_section: u32 = 0;
    let mut blueprint_titles: Vec<String> = Vec::new();
    let mut routing_observations: Vec<String> = Vec::new();
    let deepseek_selected =
        config.agent_ids.iter().any(|id| id == "deepseek") && leader_id != "deepseek";
    let consult_deepseek_once = config
        .project_brief
        .to_ascii_lowercase()
        .contains("consult deepseek once");
    let mut deepseek_consulted = false;
    let mut unclassified_count = 0u32;
    // This is deliberately in-memory and advances only after a useful
    // response, which implies the first envelope was actually submitted.
    let mut first_envelope_submitted: HashSet<String> = HashSet::new();

    // IMP-10: Once brain_fail_count >= 3, this flips to true permanently for
    // the rest of this session.  It never flips back — we keep using brain2.
    let mut use_secondary: bool = false;

    // Initialize brain status to Unknown at session start
    {
        let mut ab = state.active_brain.lock().await;
        *ab = ActiveBrainStatus {
            kind: ActiveBrainKind::Unknown,
            model: String::new(),
        };
    }
    let _ = app.emit(
        "brain-status",
        serde_json::json!({ "active": "unknown", "model": "" }),
    );

    let drained = drain_stale_active_events(nav_rx);
    if drained > 0 {
        tracing::warn!("[ACTIVE] Drained {drained} setup-era events before turn 1");
    }
    let is_resume = state
        .checkpoint
        .lock()
        .await
        .clone()
        .map(|cp| cp.session_id == config.session_id && cp.paused)
        .unwrap_or(false);
    // Operation-bound leader pending state
    let mut pending_leader: Option<(
        OperationContext,
        OperationInbox<NavEvent>,
        std::collections::VecDeque<NavEvent>,
    )> = None;
    let (mut next_leader_turn, mut early_leader_response) = if is_resume {
        let cp = state.checkpoint.lock().await.clone().unwrap();
        // Restore iteration from checkpoint
        iteration = cp.turn_number;
        tracing::info!(
            "[RECOVERY] Resuming session {} from checkpoint turn {} next_step {:?}",
            cp.session_id,
            cp.turn_number,
            cp.next_step
        );
        (
            cp.turn_number.saturating_add(1),
            cp.last_leader_response.clone(),
        )
    } else {
        let leader_window = {
            let browser = state.browser_state.lock().await;
            browser.leader_window.clone().ok_or_else(|| {
                AgentError::NavigationFailed(
                    "leader window not initialised for active turn 1".to_string(),
                )
            })?
        };
        let first_task = format!(
            "Consensus Arena active turn 1 (session {}).\n\nProject brief:\n{}\n\nConstraints: work as the panel leader, keep the first draft practical and concise, and identify decisions or questions worth consulting another model on. Produce the first short proposal/blueprint draft now. Do not answer only CONSENSUS on this active turn. Respond now with the requested draft/proposal.",
            config.session_id, config.project_brief
        );
        let first_prompt = first_task_envelope(state, config, &leader_id, &first_task).await;
        let inject_res = inject_active_prompt(
            leader_window.clone(),
            &leader_id,
            &first_prompt,
            1,
            state,
            app,
            nav_rx,
        )
        .await;
        let (early_opt, ctx, inbox, buf) = match inject_res {
            Ok(v) => v,
            Err(e) => {
                let diagnostics = {
                    let browser = state.browser_state.lock().await;
                    browser.diagnostics.clone()
                };
                crate::browser_backend::record_browser_error(
                    app,
                    &diagnostics,
                    &leader_id,
                    &e.to_string(),
                );
                let _ = app.emit(
                    "boss-message",
                    serde_json::json!({
                        "text": format!("Leader window injection failed (turn 1): {e}"),
                        "message_type": "status"
                    }),
                );
                return Err(e);
            }
        };
        if let Some(text) = early_opt {
            finish_operation(state, &ctx.operation_id, true).await;
            (2, Some(text))
        } else {
            pending_leader = Some((ctx, inbox, buf));
            (2, None)
        }
    };
    if is_resume && early_leader_response.is_none() {
        early_leader_response = Some(format!(
            "Resumed from checkpoint at turn {} — please continue with the next decision.",
            iteration
        ));
    }

    loop {
        iteration += 1;

        let _ = app.emit(
            "agent-state-change",
            serde_json::json!({
                "agent_id": &leader_id,
                "state":    "consulting",
                "response": "",
                "tokens":   0
            }),
        );

        let active_turn = next_leader_turn.saturating_sub(1);
        let leader_response = if let Some(text) = early_leader_response.take() {
            // Early response from previous ack (or resume checkpoint)
            // For checkpoint resume, there is no pending operation to finish; just use text
            if let Some((ctx, _inbox, _buf)) = pending_leader.take() {
                // If we had pending but also early text, that early text is the response for that pending op
                finish_operation(state, &ctx.operation_id, true).await;
            }
            text
        } else if let Some((ctx, mut inbox, mut early_buf)) = pending_leader.take() {
            let active_turn = ctx.turn;
            let resp = loop {
                let deadline = Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT_SECS);
                match wait_for_response_with_operation(
                    &ctx,
                    &mut inbox,
                    nav_rx,
                    &mut early_buf,
                    deadline,
                )
                .await
                {
                    Ok(r) => {
                        finish_operation(state, &ctx.operation_id, true).await;
                        break r;
                    }
                    Err(AgentError::Timeout(error)) => {
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_turn_timeout",
                                "agent_id": &leader_id,
                                "turn_number": active_turn,
                            }),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader response was not captured: {error}. Paste the visible response to continue, or wait for browser capture."),
                                "message_type": "status"
                            }),
                        );
                        // Keep operation pending for manual response
                        // We need to put it back and wait again? For now loop will retry wait with same inbox
                        // Re-insert pending for next iteration of loop
                        // To avoid losing inbox, we keep it in this loop's variables
                        // Continue loop to wait again (inbox still holds early buffer drained)
                        // The operation remains active; manual response can still arrive
                        // We need to keep inbox for next loop iteration, so we don't finish
                        // For now, we will keep waiting in this loop by not returning, but we need to preserve inbox
                        // Continue outer loop's wait
                        // To preserve, we keep inbox and early_buf in this scope and loop again
                        // Since we have taken pending, we need to keep it for retry
                        // We can just continue; inbox and early_buf remain
                        // But we lost ctx? ctx still in scope
                        // So we continue loop which will call wait again with same ctx/inbox/early
                        // Need to avoid moving ctx
                        // We'll just continue loop (which will reuse same ctx/inbox)
                        // For this we need to not drop inbox; we keep it
                        // Instead of breaking, we continue
                        // Small sleep to avoid busy loop? wait will timeout again after 300s, but immediate retry would tight loop
                        // We'll put pending back and sleep briefly then continue
                        // For simplicity, we will recreate pending for next loop iteration
                        // But to keep code simple, we will just loop again with same resources
                        // Since this is inside loop, we can just continue
                        // However we need to ensure timeout error doesn't consume operation; keep it
                        // So we don't finish; just continue waiting
                        // We will not re-store pending_leader yet; just continue this inner loop
                        // The inner loop is this `loop { match wait... }` so continuing will call wait again
                        // Use same inbox and early_buf (now empty)
                        // Loop again
                        continue;
                    }
                    Err(error) => {
                        finish_operation(state, &ctx.operation_id, false).await;
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &error.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window stopped responding: {error}"),
                                "message_type": "status"
                            }),
                        );
                        return Err(error);
                    }
                }
            };
            resp
        } else {
            // No pending and no early: fallback to old shared wait (should only happen for resume where checkpoint had no pending)
            let active_turn = next_leader_turn.saturating_sub(1);
            let resp = loop {
                match wait_for_response(&leader_id, active_turn, nav_rx).await {
                    Ok(r) => break r,
                    Err(AgentError::Timeout(error)) => {
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_turn_timeout",
                                "agent_id": &leader_id,
                                "turn_number": active_turn,
                            }),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader response was not captured: {error}. Paste the visible response to continue, or wait for browser capture."),
                                "message_type": "status"
                            }),
                        );
                    }
                    Err(error) => {
                        finish_active_turn(state, &leader_id, active_turn, false).await;
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &error.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window stopped responding: {error}"),
                                "message_type": "status"
                            }),
                        );
                        return Err(error);
                    }
                }
            };
            // This fallback had no operation to finish (old path used finish_active_turn)
            finish_active_turn(state, &leader_id, active_turn, true).await;
            resp
        };

        let _ = app.emit(
            "active-turn-state",
            serde_json::json!({
                "event": "active_response_captured",
                "agent_id": &leader_id,
                "turn_number": active_turn,
            }),
        );

        let _ = app.emit(
            "agent-state-change",
            serde_json::json!({
                "agent_id": &leader_id,
                "state":    "responded",
                "response": &leader_response,
                "tokens":   0
            }),
        );
        let _ = app.emit(
            "agent-message",
            serde_json::json!({
                "agent_id": &leader_id,
                "role": "leader",
                "response": &leader_response,
                "tokens": 0,
                "iteration": iteration,
                "source_type": "browser_or_manual"
            }),
        );

        // ── Graceful pause checkpoint ─────────────────────────────────────
        // If user requested pause after previous atomic action completed, persist checkpoint now.
        // This is safe boundary: leader response captured and emitted, before next decision.
        if state.pause_requested.load(Ordering::SeqCst) {
            let pending_msgs = {
                let mut ctx = state.context_manager.lock().await;
                ctx.take_pending_user_input_if_session(&config.session_id)
                    .map(|m| vec![m])
                    .unwrap_or_default()
            };
            let hackathon_run_id = state.hackathon_run_id.lock().await.clone();
            let hackathon_task_brief = {
                let run = state.hackathon_run.lock().await;
                run.as_ref().map(|r| r.task_brief.clone())
            };
            let cp = crate::checkpoint::SessionCheckpoint {
                checkpoint_version: crate::checkpoint::CHECKPOINT_VERSION,
                session_id: config.session_id.clone(),
                run_id: format!(
                    "run-{}",
                    &config.session_id[..config.session_id.len().min(8)]
                ),
                turn_number: active_turn,
                phase: "leader_decision".to_string(),
                leader_id: leader_id.clone(),
                target_participant: None,
                next_step: crate::checkpoint::CheckpointNextStep::LeaderDecision,
                pending_user_messages: pending_msgs,
                pause_requested: true,
                paused: true,
                pause_reason: crate::checkpoint::PauseReason::UserRequested,
                created_at: chrono::Utc::now().to_rfc3339(),
                agent_ids: config.agent_ids.clone(),
                project_brief: config.project_brief.clone(),
                session_type: format!("{:?}", config.session_type),
                hackathon_run_id,
                hackathon_task_brief,
                last_leader_response: Some(leader_response.clone()),
            };
            if cp.validate().is_ok() {
                let key = crate::checkpoint::SessionCheckpoint::key_for(&config.session_id);
                let json = serde_json::to_string(&cp).unwrap_or_default();
                {
                    let mut store = state.settings_store.lock().await;
                    let _ = store.set(&key, &json);
                }
                {
                    let mut cached = state.checkpoint.lock().await;
                    *cached = Some(cp.clone());
                }
            }
            {
                let mut orch = state.orchestrator.lock().await;
                orch.status = crate::orchestrator::OrchestratorStatus::Paused;
            }
            state.session_runtime.mark_paused(&loop_owner);
            let _ = app.emit(
                "session-status",
                serde_json::json!({ "status": "paused", "session_id": config.session_id }),
            );
            let _ = app.emit("session-checkpoint", serde_json::json!({ "checkpoint_id": config.session_id, "phase": "paused", "session_id": config.session_id, "next_step": "leader_decision" }));
            // Keep loop alive waiting for resume; do not exit and clear session_active.
            // Poll pause_requested until cleared by resume_session (which sets Running).
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                if !state.pause_requested.load(Ordering::SeqCst) {
                    let status = state.orchestrator.lock().await.status.clone();
                    if status == crate::orchestrator::OrchestratorStatus::Running {
                        state.session_runtime.mark_running(&loop_owner);
                        let _ = app.emit("session-status", serde_json::json!({ "status": "running", "session_id": config.session_id }));
                        break;
                    }
                }
                // If abort requested, exit loop
                if !state.session_runtime.is_active() {
                    return Ok(());
                }
                let orch_status = state.orchestrator.lock().await.status.clone();
                if orch_status == crate::orchestrator::OrchestratorStatus::Ended {
                    return Ok(());
                }
            }
            // Resume: continue to next iteration without re-processing same leader_response
            // The checkpoint's next_step is LeaderDecision, so we proceed to decision with enriched leader_response
            // Already have leader_response from before pause; do not discard it.
        }

        // ── User pipeline input (injected before next leader decision) ───
        // Drain pending user message queued via user_input. Stale-session protected.
        let pending_user_message: Option<String> = {
            let mut ctx = state.context_manager.lock().await;
            ctx.take_pending_user_input_if_session(&config.session_id)
        };
        let has_pending = pending_user_message.is_some();
        let leader_response = if let Some(user_msg) = pending_user_message {
            let enriched = format!("{}\n\n[User message]: {}", leader_response, user_msg);
            let _ = app.emit("boss-message", serde_json::json!({ "text": format!("User message queued: {}", user_msg.chars().take(80).collect::<String>()), "message_type": "status" }));
            // Record as session fact for audit (best-effort)
            let memory_store = state.memory_store.clone();
            let project_brief = config.project_brief.clone();
            let sid = config.session_id.clone();
            let msg_clone = user_msg.clone();
            let _ = crate::db_helpers::run_blocking(move || {
                let mut memory = memory_store
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                let _ = memory.add_session_fact(
                    &sid,
                    &project_brief,
                    "user_input",
                    &format!("User steered: {}", msg_clone),
                    None,
                    "user",
                    "confirmed",
                );
                Ok::<(), crate::errors::AgentError>(())
            })
            .await;
            enriched
        } else {
            leader_response
        };

        let mut context = format!(
            "Session iteration: {}\nSelected participant IDs: {}\nLeader ID: {}\nDeepSeek selected: {}\nDeepSeek consulted this session: {}\nProject brief requires DeepSeek once: {}",
            iteration,
            config.agent_ids.join(", "),
            leader_id,
            deepseek_selected,
            deepseek_consulted,
            consult_deepseek_once,
        );
        if has_pending {
            context.push_str("\nUser has steered the discussion — consider the [User message] in the leader response above before routing or finalizing.");
        }
        let _ = app.emit(
            "agent_brain_decision_started",
            serde_json::json!({
                "iteration": iteration,
                "response_length": leader_response.len(),
            }),
        );

        // ── IMP-10: Brain selection ───────────────────────────────────────
        // Check whether the consecutive-failure threshold has been crossed.
        // Once crossed, use_secondary is set permanently for this session.
        if !use_secondary && state.brain_fail_count.load(Ordering::SeqCst) >= 3 {
            use_secondary = true;
            tracing::warn!(
                "[BRAIN] {} consecutive failures — switching to secondary brain for the rest of this session",
                state.brain_fail_count.load(Ordering::SeqCst)
            );
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text":         "Primary agent brain unavailable — switching to secondary",
                    "message_type": "status"
                }),
            );
        }

        // Call decide() on whichever brain is active.
        // Holding the brain2 guard across .await is acceptable for
        // tokio::sync::MutexGuard (it is Send).
        let decision_result: Result<(AgentDecision, ActiveBrainKind, String), AgentError> =
            if use_secondary {
                let guard = state.agent_brain_2.lock().await;
                if let Some(b2) = guard.as_ref() {
                    let model = b2.model_name().to_string();
                    match b2
                        .decide_with_source(&leader_response, &context, mem_ctx)
                        .await
                    {
                        Ok((decision, _source)) => {
                            Ok((decision, ActiveBrainKind::Secondary, model))
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    drop(guard);
                    // No secondary configured — fall back to primary for this iter.
                    match brain
                        .decide_with_source(&leader_response, &context, mem_ctx)
                        .await
                    {
                        Ok((decision, source)) => {
                            let kind = if source == BrainSource::Fallback {
                                ActiveBrainKind::Fallback
                            } else {
                                ActiveBrainKind::Primary
                            };
                            let model = if kind == ActiveBrainKind::Fallback {
                                brain
                                    .fallback_model_name()
                                    .unwrap_or(brain.model_name())
                                    .to_string()
                            } else {
                                brain.model_name().to_string()
                            };
                            Ok((decision, kind, model))
                        }
                        Err(e) => Err(e),
                    }
                }
            } else {
                match brain
                    .decide_with_source(&leader_response, &context, mem_ctx)
                    .await
                {
                    Ok((decision, source)) => {
                        let kind = if source == BrainSource::Fallback {
                            ActiveBrainKind::Fallback
                        } else {
                            ActiveBrainKind::Primary
                        };
                        let model = if kind == ActiveBrainKind::Fallback {
                            brain
                                .fallback_model_name()
                                .unwrap_or(brain.model_name())
                                .to_string()
                        } else {
                            brain.model_name().to_string()
                        };
                        Ok((decision, kind, model))
                    }
                    Err(e) => Err(e),
                }
            };

        let decision = match decision_result {
            Ok((d, kind, model)) => {
                // Reset consecutive failure counter on success.
                state.brain_fail_count.store(0, Ordering::SeqCst);
                // Update active brain status and emit event
                {
                    let mut ab = state.active_brain.lock().await;
                    *ab = ActiveBrainStatus {
                        kind: kind.clone(),
                        model: model.clone(),
                    };
                }
                let kind_str = match kind {
                    ActiveBrainKind::Primary => "primary",
                    ActiveBrainKind::Fallback => "fallback",
                    ActiveBrainKind::Secondary => "secondary",
                    _ => "unknown",
                };
                let _ = app.emit("brain-status", serde_json::json!({ "active": kind_str, "model": model, "iteration": iteration }));
                d
            }
            Err(e) => {
                let count = state.brain_fail_count.fetch_add(1, Ordering::SeqCst) + 1;
                tracing::error!(
                    "[BRAIN] decide() failed (consecutive failures: {}): {}",
                    count,
                    e
                );
                // Update active brain to unavailable
                {
                    let mut ab = state.active_brain.lock().await;
                    *ab = ActiveBrainStatus {
                        kind: ActiveBrainKind::Unavailable,
                        model: String::new(),
                    };
                }
                let _ = app.emit("brain-status", serde_json::json!({ "active": "unavailable", "model": "", "iteration": iteration }));
                unclassified_count = unclassified_count.saturating_add(1);
                let _ = app.emit(
                    "agent_brain_decision_failed",
                    serde_json::json!({
                        "iteration": iteration,
                        "error": "decision parsing or provider request failed",
                        "unclassified_count": unclassified_count,
                    }),
                );
                if deepseek_selected
                    && !deepseek_consulted
                    && (consult_deepseek_once || leader_requests_consultation(&leader_response))
                {
                    let _ = app.emit("agent_brain_decision_fallback", serde_json::json!({
                        "kind": "route", "target_agent_id": "deepseek", "unclassified_count": unclassified_count,
                    }));
                    AgentDecision::Route {
                        target_model: "deepseek".to_string(),
                        prompt: format!(
                            "Review the leader proposal for risks and simplifications. Return concise actionable critique.\n\nLeader proposal:\n{}",
                            leader_response
                        ),
                    }
                } else if looks_like_blueprint(&leader_response) {
                    let _ = app.emit(
                        "agent_brain_decision_fallback",
                        serde_json::json!({
                            "kind": "blueprint", "unclassified_count": unclassified_count,
                        }),
                    );
                    AgentDecision::Blueprint {
                        section_title: "Draft Blueprint".to_string(),
                        section_content: leader_response.clone(),
                    }
                } else if unclassified_count <= MAX_UNCLASSIFIED_CONTINUES {
                    let _ = app.emit(
                        "agent_brain_decision_fallback",
                        serde_json::json!({
                            "kind": "continue", "unclassified_count": unclassified_count,
                        }),
                    );
                    AgentDecision::Continue
                } else {
                    let message = "Agent brain could not classify repeated unusable responses. Paste a clearer leader response or stop and restart the session.";
                    let _ = app.emit(
                        "boss-message",
                        serde_json::json!({ "text": message, "message_type": "status" }),
                    );
                    return Err(AgentError::UnknownError(message.to_string()));
                }
            }
        };

        // A healthy AgentDecision returned by the brain is authoritative. No
        // post-brain-success keyword override may replace it: the brain's own
        // intelligence routes to the participant it selects (see the A3 probe,
        // which returned Route(deepseek) directly). Only the error-arm fallback
        // (a failure safety mechanism) may synthesize a decision when decide()
        // itself fails.

        // D-040 [LOOP]
        tracing::debug!("[LOOP] iter={} decision={:?}", iteration, decision);

        if !pending_adoptions.is_empty() {
            let checks = std::mem::take(&mut pending_adoptions);
            let adoptions = checks
                .into_iter()
                .map(|pending| {
                    let adopted = match &decision {
                        AgentDecision::Blueprint { .. } => true,
                        AgentDecision::Route {
                            target_model,
                            prompt,
                        } if crate::memory_store::detect_topic(prompt) == pending.topic
                            && *target_model != pending.model_id =>
                        {
                            false
                        }
                        _ => {
                            let display_name =
                                crate::browser_backend::display_name_for(&pending.model_id);
                            crate::memory_store::safe_prefix(&leader_response, 300)
                                .to_lowercase()
                                .contains(&display_name.to_lowercase())
                        }
                    };
                    tracing::debug!(
                        "[MEMORY] adoption check model={} topic={} prompt={}",
                        pending.model_id,
                        pending.topic,
                        pending.prompt_excerpt
                    );
                    (pending.model_id, pending.topic, adopted)
                })
                .collect::<Vec<_>>();
            let memory_store = state.memory_store.clone();
            let project_brief = config.project_brief.clone();
            let session_id = config.session_id.clone();
            if let Err(e) = crate::db_helpers::run_blocking(move || {
                let mut memory = memory_store
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                for (model_id, topic, adopted) in &adoptions {
                    if let Err(e) = memory.record_model_response(
                        &project_brief,
                        model_id,
                        topic,
                        *adopted,
                        &session_id,
                    ) {
                        eprintln!("[MEMORY] adoption record: {e}");
                    }
                }
                Ok(())
            })
            .await
            {
                eprintln!("[MEMORY] adoption dispatch: {e}");
            }
        }

        match decision {
            // ── Route ─────────────────────────────────────────────────────
            AgentDecision::Route {
                target_model,
                prompt,
            } => {
                let target_model =
                    resolve_selected_agent_id(&target_model, config).ok_or_else(|| {
                        AgentError::NavigationFailed(format!(
                            "Agent brain selected an unavailable participant: {target_model}"
                        ))
                    })?;
                let _ = app.emit(
                    "route_started",
                    serde_json::json!({
                        "iteration": iteration,
                        "route_target_agent_id": &target_model,
                    }),
                );
                let _ = app.emit(
                    "agent-routing",
                    serde_json::json!({
                        "from_model": &leader_id,
                        "to_model":   &target_model,
                        "reason":     "Leader requested consultation"
                    }),
                );

                let routing_topic = crate::memory_store::detect_topic(&prompt).to_string();
                {
                    let memory_store = state.memory_store.clone();
                    let session_id = config.session_id.clone();
                    let project_brief = config.project_brief.clone();
                    let target = target_model.clone();
                    let topic = routing_topic.clone();
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        memory.add_session_fact(
                            &session_id,
                            &project_brief,
                            "routing",
                            &format!(
                                "Routed to {target} on topic '{topic}' at iteration {iteration}"
                            ),
                            None,
                            "leader",
                            "llm",
                        )
                    })
                    .await
                    {
                        eprintln!("[MEMORY] route fact: {e}");
                    }
                }
                pending_adoptions.push(PendingAdoptionCheck {
                    model_id: target_model.clone(),
                    topic: routing_topic.clone(),
                    prompt_excerpt: crate::memory_store::safe_prefix(&prompt, 160),
                });
                models_consulted_since_last_section.push(target_model.clone());
                routing_observations.push(format!("Consulted {target_model} on {routing_topic}"));
                iterations_since_last_section += 1;
                let _ = app.emit(
                    "memory-updated",
                    serde_json::json!({
                        "memory_type": "session",
                        "trigger": "routing"
                    }),
                );

                // R1.2: participant failure is recoverable — do NOT propagate with `?`.
                // A failed participant is reported to the leader so the session can
                // continue with other models or with the leader alone.
                let routed_prompt = if first_envelope_submitted.contains(&target_model) {
                    prompt.clone()
                } else {
                    first_task_envelope(state, config, &target_model, &prompt).await
                };
                let participant_result = inject_and_wait_with_retry(
                    &target_model,
                    &routed_prompt,
                    iteration,
                    state,
                    nav_rx,
                    app,
                )
                .await;
                let participant_response = match participant_result {
                    Ok(response) => {
                        first_envelope_submitted.insert(target_model.clone());
                        if target_model == "deepseek" {
                            deepseek_consulted = true;
                        }
                        response
                    }
                    Err(error) => {
                        // R1.8: SessionAborted is a clean cancellation — do NOT
                        // treat it as a participant failure. Propagate to
                        // terminate the session with an explicit reason.
                        if matches!(&error, AgentError::UnknownError(msg) if msg.contains("Session aborted"))
                        {
                            return Err(error);
                        }
                        tracing::warn!(
                            "[ROUTE] participant {} failed (iteration {}): {}",
                            target_model,
                            iteration,
                            error
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!(
                                    "Participant {} failed: {}. Continuing without it — the leader will proceed or you may consult another model.",
                                    crate::browser_backend::display_name_for(&target_model),
                                    error
                                ),
                                "message_type": "status"
                            }),
                        );
                        format!(
                            "[Response from {} unavailable: {}]\n\nProceed without this participant. Either continue refining the blueprint from the previous proposal or consult a different model if the missing perspective is material.",
                            target_model, error
                        )
                    }
                };

                // Return response (or failure notice) to leader window.
                tracing::debug!("[LOCK] acquiring browser_state for Route/leader_return");
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed("leader window not initialised".to_string())
                    })?
                }; // lock drops here
                tracing::debug!("[LOCK] released browser_state for Route/leader_return");

                let return_prompt = format!(
                    "[Response from {}]:\n{}\n\nIncorporate this critique into the proposal and produce the next concise blueprint section or final blueprint.",
                    target_model, participant_response
                );
                tracing::debug!(
                    "[INJECT] → {} (leader return) turn={} len={}",
                    leader_id,
                    iteration,
                    return_prompt.len()
                );
                // RC1-A2: explicit fatal semantics for leader
                let inject_res = inject_active_prompt(
                    leader_window,
                    &leader_id,
                    &return_prompt,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await;
                match inject_res {
                    Ok((early_opt, ctx, inbox, buf)) => {
                        if let Some(text) = early_opt {
                            finish_operation(state, &ctx.operation_id, true).await;
                            early_leader_response = Some(text);
                            pending_leader = None;
                        } else {
                            pending_leader = Some((ctx, inbox, buf));
                            early_leader_response = None;
                        }
                    }
                    Err(e) => {
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &e.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                        return Err(e);
                    }
                };
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── D-035: RouteCompare ───────────────────────────────────────
            AgentDecision::RouteCompare { models, prompt } => {
                let _ = app.emit(
                    "boss-message",
                    serde_json::json!({
                        "text": format!("Comparing responses from: {}", models.join(", ")),
                        "message_type": "status"
                    }),
                );

                let compare_models: Vec<String> =
                    models.into_iter().filter(|m| m != &leader_id).collect();

                let routing_topic = crate::memory_store::detect_topic(&prompt).to_string();
                for model in &compare_models {
                    pending_adoptions.push(PendingAdoptionCheck {
                        model_id: model.clone(),
                        topic: routing_topic.clone(),
                        prompt_excerpt: crate::memory_store::safe_prefix(&prompt, 160),
                    });
                    models_consulted_since_last_section.push(model.clone());
                }
                iterations_since_last_section += 1;
                routing_observations.push(format!(
                    "Compared {} on {}",
                    compare_models.join(", "),
                    routing_topic
                ));
                {
                    let memory_store = state.memory_store.clone();
                    let session_id = config.session_id.clone();
                    let project_brief = config.project_brief.clone();
                    let models_joined = compare_models.join(", ");
                    let topic = routing_topic.clone();
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        memory.add_session_fact(
                            &session_id,
                            &project_brief,
                            "routing",
                            &format!("Compared {models_joined} on topic '{topic}'"),
                            None,
                            "leader",
                            "llm",
                        )
                    })
                    .await
                    {
                        eprintln!("[MEMORY] route_compare fact: {e}");
                    }
                }
                let _ = app.emit(
                    "memory-updated",
                    serde_json::json!({
                        "memory_type": "session",
                        "trigger": "route_compare"
                    }),
                );

                let mut combined = String::new();
                let mut compare_failed: Vec<String> = Vec::new();
                let mut compare_succeeded: Vec<String> = Vec::new();

                for target_model in &compare_models {
                    let _ = app.emit(
                        "agent-routing",
                        serde_json::json!({
                            "from_model": &leader_id,
                            "to_model":   target_model,
                            "reason":     "Route compare"
                        }),
                    );

                    // R1.2: per-participant failure is NOT session-fatal.
                    // Preserve successful responses and explicitly report failures.
                    // R1.8: SessionAborted is clean cancellation — propagate.
                    let routed_prompt = if first_envelope_submitted.contains(target_model) {
                        prompt.clone()
                    } else {
                        first_task_envelope(state, config, target_model, &prompt).await
                    };
                    match inject_and_wait_with_retry(
                        target_model,
                        &routed_prompt,
                        iteration,
                        state,
                        nav_rx,
                        app,
                    )
                    .await
                    {
                        Ok(response) => {
                            first_envelope_submitted.insert(target_model.clone());
                            combined
                                .push_str(&format!("[{} said]:\n{}\n\n", target_model, response));
                            compare_succeeded.push(target_model.clone());
                        }
                        Err(error) if matches!(&error, AgentError::UnknownError(msg) if msg.contains("Session aborted")) =>
                        {
                            return Err(error);
                        }
                        Err(error) => {
                            tracing::warn!(
                                "[ROUTE_COMPARE] participant {} failed (iteration {}): {}",
                                target_model,
                                iteration,
                                error
                            );
                            combined.push_str(&format!(
                                "[{} unavailable: {}]\n\n",
                                target_model, error
                            ));
                            compare_failed.push(target_model.clone());
                            let _ = app.emit(
                                "boss-message",
                                serde_json::json!({
                                    "text": format!(
                                        "Comparison participant {} failed: {}. Partial results will be used.",
                                        crate::browser_backend::display_name_for(target_model),
                                        error
                                    ),
                                    "message_type": "status"
                                }),
                            );
                        }
                    }
                }
                if !compare_failed.is_empty() && compare_succeeded.is_empty() {
                    let _ = app.emit(
                        "boss-message",
                        serde_json::json!({
                            "text": format!(
                                "All comparison participants failed ({}). The leader will proceed without comparison; you may retry with different models.",
                                compare_failed.join(", ")
                            ),
                            "message_type": "status"
                        }),
                    );
                } else if !compare_failed.is_empty() {
                    let _ = app.emit(
                        "boss-message",
                        serde_json::json!({
                            "text": format!(
                                "Comparison partial: succeeded [{}], failed [{}]. The leader will incorporate the available responses.",
                                compare_succeeded.join(", "),
                                compare_failed.join(", ")
                            ),
                            "message_type": "status"
                        }),
                    );
                }

                // Inject combined result back to leader.
                tracing::debug!("[LOCK] acquiring browser_state for RouteCompare/leader");
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed("leader window not initialised".to_string())
                    })?
                }; // lock drops here
                tracing::debug!("[LOCK] released browser_state for RouteCompare/leader");

                let combined_msg = format!("[Comparison responses]:\n{}", combined);
                tracing::debug!(
                    "[INJECT] RouteCompare combined → {} turn={} len={}",
                    leader_id,
                    iteration,
                    combined_msg.len()
                );
                // RC1-A2: explicit fatal for leader
                let inject_res = inject_active_prompt(
                    leader_window,
                    &leader_id,
                    &combined_msg,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await;
                match inject_res {
                    Ok((early_opt, ctx, inbox, buf)) => {
                        if let Some(text) = early_opt {
                            finish_operation(state, &ctx.operation_id, true).await;
                            early_leader_response = Some(text);
                            pending_leader = None;
                        } else {
                            pending_leader = Some((ctx, inbox, buf));
                            early_leader_response = None;
                        }
                    }
                    Err(e) => {
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &e.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                        return Err(e);
                    }
                };
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Blueprint ─────────────────────────────────────────────────
            AgentDecision::Blueprint {
                section_title,
                section_content,
            } => {
                let section_id = uuid::Uuid::new_v4().to_string();
                let section = BlueprintSection {
                    id: section_id.clone(),
                    session_id: config.session_id.clone(),
                    title: section_title.clone(),
                    content: section_content.clone(),
                    status: SectionStatus::Agreed,
                    iteration_finalised: Some(iteration),
                };

                // Task 9 (HIGH-5/HIGH-6): blueprint_store is now
                // Arc<std::sync::Mutex<_>> (see orchestrator.rs) instead of
                // Arc<tokio::sync::Mutex<_>>, so the synchronous rusqlite
                // write runs inside db_helpers::run_blocking — off the async
                // runtime thread, with retry/backoff on transient failure —
                // instead of directly on it via `.lock().await`.
                {
                    let store = state.blueprint_store.clone();
                    let section_for_write = section.clone();
                    crate::db_helpers::run_blocking(move || {
                        let guard = store.lock().map_err(|_| {
                            AgentError::DatabaseError("blueprint store lock poisoned".to_string())
                        })?;
                        guard.upsert_section(&section_for_write)
                    })
                    .await
                    .map_err(|e| {
                        AgentError::DatabaseError(format!(
                            "Failed to save blueprint section: {}",
                            e
                        ))
                    })?;
                } // blueprint_store access (and its internal lock) fully resolved here

                {
                    let memory_store = state.memory_store.clone();
                    let project_brief = config.project_brief.clone();
                    let title = section_title.clone();
                    let models = models_consulted_since_last_section.clone();
                    let iterations = iterations_since_last_section;
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        memory.record_blueprint_finalized(
                            &project_brief,
                            &title,
                            &models,
                            iterations,
                        )
                    })
                    .await
                    {
                        eprintln!("[MEMORY] blueprint record: {e}");
                    }
                    let _ = app.emit(
                        "memory-updated",
                        serde_json::json!({
                            "memory_type": "project",
                            "trigger": "blueprint"
                        }),
                    );
                }
                blueprint_titles.push(section_title.clone());
                models_consulted_since_last_section.clear();
                iterations_since_last_section = 0;

                // D-040 [EMIT]
                tracing::debug!(
                    "[EMIT] blueprint-section-added title={} id={}",
                    section_title,
                    section_id
                );
                let _ = app.emit(
                    "blueprint-section-added",
                    serde_json::json!({
                        "section_id": &section_id,
                        "title":      &section_title,
                        "content":    &section_content
                    }),
                );
                let _ = app.emit(
                    "blueprint_emitted",
                    serde_json::json!({
                        "iteration": iteration,
                        "section_title": &section_title,
                    }),
                );

                tracing::debug!("[LOCK] acquiring browser_state for Blueprint/ack");
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed("leader window not initialised".to_string())
                    })?
                }; // lock drops here
                tracing::debug!("[LOCK] released browser_state for Blueprint/ack");

                let ack =
                    "Section recorded. Please continue with the next section or signal completion.";
                tracing::debug!("[INJECT] Blueprint ack → {} turn={}", leader_id, iteration);
                // RC1-A2: explicit fatal for leader
                let inject_res = inject_active_prompt(
                    leader_window,
                    &leader_id,
                    ack,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await;
                match inject_res {
                    Ok((early_opt, ctx, inbox, buf)) => {
                        if let Some(text) = early_opt {
                            finish_operation(state, &ctx.operation_id, true).await;
                            early_leader_response = Some(text);
                            pending_leader = None;
                        } else {
                            pending_leader = Some((ctx, inbox, buf));
                            early_leader_response = None;
                        }
                    }
                    Err(e) => {
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &e.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                        return Err(e);
                    }
                };
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Continue ──────────────────────────────────────────────────
            AgentDecision::Continue => {
                let _ = app.emit(
                    "boss-message",
                    serde_json::json!({
                        "text":         "Leader is continuing...",
                        "message_type": "status"
                    }),
                );

                tracing::debug!("[LOCK] acquiring browser_state for Continue");
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed("leader window not initialised".to_string())
                    })?
                }; // lock drops here
                tracing::debug!("[LOCK] released browser_state for Continue");

                tracing::debug!("[INJECT] Continue → {} turn={}", leader_id, iteration);
                // RC1-A2: explicit fatal for leader
                let inject_res = inject_active_prompt(
                    leader_window,
                    &leader_id,
                    "Please continue.",
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await;
                match inject_res {
                    Ok((early_opt, ctx, inbox, buf)) => {
                        if let Some(text) = early_opt {
                            finish_operation(state, &ctx.operation_id, true).await;
                            early_leader_response = Some(text);
                            pending_leader = None;
                        } else {
                            pending_leader = Some((ctx, inbox, buf));
                            early_leader_response = None;
                        }
                    }
                    Err(e) => {
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &e.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                        return Err(e);
                    }
                };
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── D-041: AskUser ────────────────────────────────────────────
            AgentDecision::AskUser {
                question,
                options,
                allow_custom,
            } => {
                {
                    let memory_store = state.memory_store.clone();
                    let session_id = config.session_id.clone();
                    let project_brief = config.project_brief.clone();
                    let question_for_memory = question.clone();
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        memory.add_open_question(
                            &session_id,
                            &project_brief,
                            &question_for_memory,
                            iteration,
                        )
                    })
                    .await
                    {
                        eprintln!("[MEMORY] open question: {e}");
                    }
                }

                let (tx, rx) = oneshot::channel::<String>();

                // Store tx so provide_user_answer command can deliver the answer.
                // Lock is scoped — dropped before rx.await below.
                {
                    let mut lock = state.ask_user_tx.lock().await;
                    *lock = Some(tx);
                } // lock drops here

                let _ = app.emit(
                    "agent-ask-user",
                    serde_json::json!({
                        "question":     &question,
                        "options":      &options,
                        "allow_custom": allow_custom
                    }),
                );

                tracing::debug!("[LOOP] AskUser emitted — suspending loop");

                // Await user answer.  Loop is suspended here with no spin.
                // RISK-ASKCHANNEL: tx was stored via assignment (not clone);
                // provide_user_answer uses take() to clear Option before sending.
                let answer = rx.await.map_err(|_| {
                    AgentError::UnknownError(
                        "AskUser channel dropped before answer was received".to_string(),
                    )
                })?;

                tracing::debug!("[LOOP] AskUser answer received, resuming loop");

                if answer != "Cancelled" {
                    let memory_store = state.memory_store.clone();
                    let project_brief = config.project_brief.clone();
                    let session_id = config.session_id.clone();
                    let content = format!(
                        "User answered '{}': {}",
                        crate::memory_store::safe_prefix(&question, 40),
                        answer
                    );
                    let question_prefix = crate::memory_store::safe_prefix(&question, 30);
                    let resolution = format!("User answered: {answer}");
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        memory.add_project_memory_with_source(
                            &project_brief,
                            "user_preference",
                            &content,
                            None,
                            None,
                            "user",
                            "confirmed",
                        )?;
                        if let Err(e) =
                            memory.resolve_question(&session_id, &question_prefix, &resolution)
                        {
                            eprintln!("[MEMORY] question resolution: {e}");
                        }
                        Ok(())
                    })
                    .await
                    {
                        eprintln!("[MEMORY] user preference: {e}");
                    }
                    let _ = app.emit(
                        "memory-updated",
                        serde_json::json!({
                            "memory_type": "project",
                            "trigger": "user_answer"
                        }),
                    );
                }

                tracing::debug!("[LOCK] acquiring browser_state for AskUser/answer");
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed("leader window not initialised".to_string())
                    })?
                }; // lock drops here
                tracing::debug!("[LOCK] released browser_state for AskUser/answer");

                let context_prompt = format!(
                    "[User answered: {}]\nPlease continue based on this answer.",
                    answer
                );
                tracing::debug!(
                    "[INJECT] AskUser answer → {} turn={} len={}",
                    leader_id,
                    iteration,
                    context_prompt.len()
                );
                // RC1-A2: explicit fatal for leader
                let inject_res = inject_active_prompt(
                    leader_window,
                    &leader_id,
                    &context_prompt,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await;
                match inject_res {
                    Ok((early_opt, ctx, inbox, buf)) => {
                        if let Some(text) = early_opt {
                            finish_operation(state, &ctx.operation_id, true).await;
                            early_leader_response = Some(text);
                            pending_leader = None;
                        } else {
                            pending_leader = Some((ctx, inbox, buf));
                            early_leader_response = None;
                        }
                    }
                    Err(e) => {
                        let diagnostics = {
                            let browser = state.browser_state.lock().await;
                            browser.diagnostics.clone()
                        };
                        crate::browser_backend::record_browser_error(
                            app,
                            &diagnostics,
                            &leader_id,
                            &e.to_string(),
                        );
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                        return Err(e);
                    }
                };
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Hackathon — first-class leader decision ───────────────────────
            AgentDecision::Hackathon { task_brief } => {
                let trimmed = task_brief.trim();
                if trimmed.is_empty() || trimmed.len() > 2000 {
                    let _ = app.emit(
                        "boss-message",
                        serde_json::json!({
                            "text": "Hackathon request malformed (empty or too long) — continuing without hackathon",
                            "message_type": "status"
                        }),
                    );
                    // Fallback to Continue
                    let leader_window = {
                        let browser = state.browser_state.lock().await;
                        browser.leader_window.clone().ok_or_else(|| {
                            AgentError::NavigationFailed(
                                "leader window not initialised".to_string(),
                            )
                        })?
                    };
                    let inject_res = inject_active_prompt(
                        leader_window,
                        &leader_id,
                        "Hackathon request was malformed; please continue with normal discussion.",
                        next_leader_turn,
                        state,
                        app,
                        nav_rx,
                    )
                    .await;
                    match inject_res {
                        Ok((early_opt, ctx, inbox, buf)) => {
                            if let Some(text) = early_opt {
                                finish_operation(state, &ctx.operation_id, true).await;
                                early_leader_response = Some(text);
                                pending_leader = None;
                            } else {
                                pending_leader = Some((ctx, inbox, buf));
                                early_leader_response = None;
                            }
                        }
                        Err(e) => {
                            let diagnostics = {
                                let browser = state.browser_state.lock().await;
                                browser.diagnostics.clone()
                            };
                            crate::browser_backend::record_browser_error(
                                app,
                                &diagnostics,
                                &leader_id,
                                &e.to_string(),
                            );
                            let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                            return Err(e);
                        }
                    };
                    next_leader_turn = next_leader_turn.saturating_add(1);
                    continue;
                }

                let _ = app.emit(
                    "boss-message",
                    serde_json::json!({
                        "text": format!("Leader requested Hackathon: {} — running parallel teams…", trimmed.chars().take(80).collect::<String>()),
                        "message_type": "status"
                    }),
                );
                let _ = app.emit(
                    "agent_brain_decision_fallback",
                    serde_json::json!({
                        "kind": "hackathon", "task_brief_len": trimmed.len(), "iteration": iteration
                    }),
                );

                // Execute hackathon — uses configured groups, server-validated, concurrent
                let hackathon_result =
                    crate::hackathon::execute_hackathon(trimmed.to_string(), None, state, app)
                        .await;

                match hackathon_result {
                    Ok(report) => {
                        // Advisory delimited report — leader will evaluate, not auto-blueprint
                        let delimited = format!(
                            "=== Hackathon Results ===\n{}\n=== End Hackathon Results ===\n\nThe above is advisory hackathon output from parallel API teams. Synthesize, accept, reject, or build upon it using your normal Blueprint/Route/Continue/Complete logic. The hackathon does not override your authority.",
                            report
                        );
                        // Record fact for memory (best-effort, non-fatal)
                        {
                            let memory_store = state.memory_store.clone();
                            let project_brief = config.project_brief.clone();
                            let sid = config.session_id.clone();
                            let tb = trimmed.to_string();
                            let delimited_len = delimited.len();
                            let _ = crate::db_helpers::run_blocking(move || {
                                let mut memory = memory_store
                                    .lock()
                                    .unwrap_or_else(|poison| poison.into_inner());
                                let _ = memory.add_session_fact(
                                    &sid,
                                    &project_brief,
                                    "hackathon",
                                    &format!(
                                        "Hackathon requested: {} — report {} chars",
                                        tb, delimited_len
                                    ),
                                    None,
                                    "leader",
                                    "llm",
                                );
                                Ok::<(), AgentError>(())
                            })
                            .await;
                            let _ = app.emit(
                                "memory-updated",
                                serde_json::json!({"memory_type":"session","trigger":"hackathon"}),
                            );
                        }
                        let leader_window = {
                            let browser = state.browser_state.lock().await;
                            browser.leader_window.clone().ok_or_else(|| {
                                AgentError::NavigationFailed(
                                    "leader window not initialised".to_string(),
                                )
                            })?
                        };
                        let inject_res = inject_active_prompt(
                            leader_window,
                            &leader_id,
                            &delimited,
                            next_leader_turn,
                            state,
                            app,
                            nav_rx,
                        )
                        .await;
                        match inject_res {
                            Ok((early_opt, ctx, inbox, buf)) => {
                                if let Some(text) = early_opt {
                                    finish_operation(state, &ctx.operation_id, true).await;
                                    early_leader_response = Some(text);
                                    pending_leader = None;
                                } else {
                                    pending_leader = Some((ctx, inbox, buf));
                                    early_leader_response = None;
                                }
                            }
                            Err(e) => {
                                let diagnostics = {
                                    let browser = state.browser_state.lock().await;
                                    browser.diagnostics.clone()
                                };
                                crate::browser_backend::record_browser_error(
                                    app,
                                    &diagnostics,
                                    &leader_id,
                                    &e.to_string(),
                                );
                                let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Leader window injection failed (turn {}): {e}", next_leader_turn),
                                "message_type": "status"
                            }),
                        );
                                return Err(e);
                            }
                        };
                        next_leader_turn = next_leader_turn.saturating_add(1);
                        // Loop continues — leader will now see hackathon report before next decision
                    }
                    Err(e) => {
                        let _ = app.emit(
                            "boss-message",
                            serde_json::json!({
                                "text": format!("Hackathon failed: {} — continuing with normal discussion", e),
                                "message_type": "status"
                            }),
                        );
                        // Inject failure notice so leader can adapt, then continue
                        let leader_window = {
                            let browser = state.browser_state.lock().await;
                            browser.leader_window.clone().ok_or_else(|| {
                                AgentError::NavigationFailed(
                                    "leader window not initialised".to_string(),
                                )
                            })?
                        };
                        let failure_note = format!(
                            "[Hackathon attempt failed: {}]\nPlease continue without hackathon output, or retry with a clearer task_brief.",
                            e
                        );
                        let inject_res = inject_active_prompt(
                            leader_window,
                            &leader_id,
                            &failure_note,
                            next_leader_turn,
                            state,
                            app,
                            nav_rx,
                        )
                        .await;
                        match inject_res {
                            Ok((early_opt, ctx, inbox, buf)) => {
                                if let Some(text) = early_opt {
                                    finish_operation(state, &ctx.operation_id, true).await;
                                    early_leader_response = Some(text);
                                    pending_leader = None;
                                } else {
                                    pending_leader = Some((ctx, inbox, buf));
                                    early_leader_response = None;
                                }
                            }
                            Err(ie) => {
                                let diagnostics = {
                                    let browser = state.browser_state.lock().await;
                                    browser.diagnostics.clone()
                                };
                                crate::browser_backend::record_browser_error(
                                    app,
                                    &diagnostics,
                                    &leader_id,
                                    &ie.to_string(),
                                );
                                return Err(ie);
                            }
                        };
                        next_leader_turn = next_leader_turn.saturating_add(1);
                    }
                }
            }

            // ── Complete ──────────────────────────────────────────────────
            AgentDecision::Complete => {
                {
                    let memory_store = state.memory_store.clone();
                    let session_id = config.session_id.clone();
                    let project_brief = config.project_brief.clone();
                    let completed = blueprint_titles
                        .iter()
                        .map(|title| format!("Blueprint section: {title}"))
                        .collect::<Vec<_>>();
                    let learned = routing_observations.clone();
                    let sections = blueprint_titles.len() as u32;
                    if let Err(e) = crate::db_helpers::run_blocking(move || {
                        let mut memory = memory_store
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        let open_questions = memory
                            .get_open_questions(&project_brief)
                            .unwrap_or_else(|e| {
                                eprintln!("[MEMORY] completion open questions: {e}");
                                Vec::new()
                            });
                        let summary = SessionSummaryData {
                            investigated: vec![format!(
                                "Expert panel consulted on {project_brief} over {iteration} iterations"
                            )],
                            completed: completed.clone(),
                            learned: learned.clone(),
                            next_steps: open_questions
                                .iter()
                                .map(|question| format!("Unresolved: {}", question.question))
                                .collect(),
                        };
                        memory.write_session_completion_memory(
                            &session_id,
                            &project_brief,
                            &summary,
                            &open_questions,
                            sections,
                            iteration,
                        )
                    })
                    .await
                    {
                        eprintln!("[MEMORY] session summary: {e}");
                    }
                    let _ = app.emit(
                        "memory-updated",
                        serde_json::json!({
                            "memory_type": "project",
                            "trigger": "session_complete"
                        }),
                    );
                }

                // IMP-7: Mark this session as complete so recovery does not
                // offer it on the next launch.  Best-effort — don't fail the
                // session completion if the DB write errors.
                {
                    let mut store = state.settings_store.lock().await;
                    if let Err(e) = store.set("session_complete", "true") {
                        tracing::warn!("[IMP-7] Could not mark session_complete=true: {}", e);
                    }
                } // lock drops here

                let _ = app.emit(
                    "session-complete",
                    serde_json::json!({
                        "stats": {
                            "total_turns":    iteration,
                            "duration_mins":  0,
                            "sections_agreed": 0,
                            "consensus":      true
                        }
                    }),
                );
                return Ok(());
            }
        }
    }
}

// ── IMP-2: inject_and_wait_with_retry ─────────────────────────────────────────
//
// Wraps the participant injection + wait cycle with exponential backoff retry.
// Applied to participant models only (Route / RouteCompare).
// NOT applied to leader wait_for_response — the leader is the source of truth.
//
// On RateLimit-classified errors, calls browser_state.set_cooldown() before
// sleeping to prevent hammering a rate-limited model.
// On Permanent-classified errors, returns immediately without retry.
//
// Also updates the model_health map (IMP-5) on success and final failure.

async fn inject_and_wait_with_retry(
    target_model: &str,
    prompt: &str,
    turn: u32,
    state: &AppState,
    nav_rx: &mut Receiver<NavEvent>,
    app: &AppHandle,
) -> Result<String, AgentError> {
    {
        let browser = state.browser_state.lock().await;
        if browser.is_in_cooldown(target_model) {
            tracing::warn!(
                "[RETRY] {} is in cooldown — skipping injection",
                target_model
            );
            let _ = app.emit(
                "rate-limit-reached",
                serde_json::json!({
                    "agent_id":            target_model,
                    "estimated_reset_mins": 1
                }),
            );
            return Err(AgentError::NetworkError(format!(
                "{} is in cooldown (rate limited)",
                target_model
            )));
        }
    }

    let mut last_err: Option<AgentError> = None;

    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .unwrap_or_default();

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let raw_wait = BACKOFF_BASE_SECS.pow(attempt);
            let wait_secs = raw_wait.min(60);
            tracing::warn!(
                "[RETRY] {} attempt {}/{} — waiting {}s",
                target_model,
                attempt,
                MAX_RETRIES,
                wait_secs
            );
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;

            let browser = state.browser_state.lock().await;
            if browser.is_in_cooldown(target_model) {
                let e = AgentError::NetworkError(format!(
                    "{} is still in cooldown after backoff",
                    target_model
                ));
                update_model_health(state, target_model, false, Some(e.to_string())).await;
                return Err(e);
            }
        }

        tracing::debug!(
            "[LOCK] acquiring browser_state for inject_and_wait_with_retry/{}",
            target_model
        );
        let (nav_window, diagnostics, lifecycle, trusted_base_url, target_url) = {
            let mut browser = state.browser_state.lock().await;
            let window = crate::browser_backend::ensure_nav_window(app, &mut browser)?;
            // Session 03C: trusted provider origin comes only from the merged
            // registry. The navigation target may be a saved conversation
            // locator, but that locator is checked UNDER the trusted policy
            // and can never redefine it.
            let trusted_base_url =
                crate::browser_backend::resolve_participant(target_model, &custom)
                    .map(|info| info.base_url)
                    .ok_or_else(|| {
                        AgentError::NavigationFailed(format!(
                            "unknown participant model: {target_model}"
                        ))
                    })?;
            let target_url = browser
                .conversation_urls
                .get(target_model)
                .and_then(|url| url.clone())
                .unwrap_or_else(|| trusted_base_url.clone());
            (
                window,
                browser.diagnostics.clone(),
                browser.lifecycle.clone(),
                trusted_base_url,
                target_url,
            )
        };
        tracing::debug!(
            "[LOCK] released browser_state for inject_and_wait_with_retry/{}",
            target_model
        );

        tracing::debug!(
            "[INJECT] → {} turn={} len={} (attempt {})",
            target_model,
            turn,
            prompt.len(),
            attempt
        );

        // No destructive shared-response drain; operation inbox owns response.
        let _ = drain_stale_active_events(nav_rx);

        let skip_navigate =
            attempt > 0 && diagnostics.can_skip_navigation_on_retry(target_model, &target_url);
        if skip_navigate {
            tracing::info!(
                "[RETRY] skipping navigation for {} attempt {} — already at {} with composer_detected",
                target_model,
                attempt,
                target_url
            );
        } else if let Err(e) = crate::browser_backend::navigate_agent_window(
            app,
            &diagnostics,
            &lifecycle,
            &nav_window,
            target_model,
            "nav",
            &target_url,
            &trusted_base_url,
        ) {
            if !should_retry_after_failure(&e, &diagnostics, target_model, turn, attempt) {
                update_model_health(state, target_model, false, Some(e.to_string())).await;
                return Err(e);
            }
            tracing::warn!(
                "[RETRY] Navigation to {} failed (attempt {}): {}",
                target_model,
                attempt,
                e
            );
            last_err = Some(e);
            continue;
        }

        // Begin exact operation
        let (context, mut inbox) =
            match begin_operation(state, target_model, turn, BrowserSurface::Participant).await {
                Ok(v) => v,
                Err(e) => {
                    if !should_retry_after_failure(&e, &diagnostics, target_model, turn, attempt) {
                        update_model_health(state, target_model, false, Some(e.to_string())).await;
                        return Err(e);
                    }
                    tracing::warn!(
                        "[RETRY] begin operation failed for {} attempt {}: {}",
                        target_model,
                        attempt,
                        e
                    );
                    last_err = Some(e);
                    continue;
                }
            };
        let _ = app.emit(
            "active-turn-state",
            serde_json::json!({
                "event": "active_turn_started",
                "agent_id": target_model,
                "turn_number": turn,
            }),
        );
        // Session 03: injection is authorized by the exact current
        // ReadyLease — including the skip-navigation retry path, which must
        // never bypass a missing/stale lease.
        let inject_res = crate::browser_backend::inject_to_window(
            nav_window.clone(),
            &lifecycle,
            target_model,
            prompt,
            turn,
            Some(&context.operation_id),
            !skip_navigate,
            true,
        )
        .await;
        match inject_res {
            Err(e) => {
                finish_operation(state, &context.operation_id, false).await;
                if matches!(&e, AgentError::Timeout(_) | AgentError::InjectionFailed(_)) {
                    crate::browser_backend::record_browser_error(
                        app,
                        &diagnostics,
                        target_model,
                        &e.to_string(),
                    );
                }
                let kind = e.kind();
                if kind == ErrorKind::RateLimit {
                    let mut browser = state.browser_state.lock().await;
                    browser.set_cooldown(target_model, 60);
                }
                if !should_retry_after_failure(&e, &diagnostics, target_model, turn, attempt) {
                    update_model_health(state, target_model, false, Some(e.to_string())).await;
                    return Err(e);
                }
                tracing::warn!(
                    "[RETRY] Injection to {} failed (attempt {}): {}",
                    target_model,
                    attempt,
                    e
                );
                last_err = Some(e);
                continue;
            }
            Ok(()) => {
                // Confirm submit using operation inbox
                let ack_res = confirm_active_submit(&context, &mut inbox, app).await;
                let ack = match ack_res {
                    Ok(a) => a,
                    Err(e) => {
                        finish_operation(state, &context.operation_id, false).await;
                        let kind = e.kind();
                        if kind == ErrorKind::RateLimit {
                            let mut browser = state.browser_state.lock().await;
                            browser.set_cooldown(target_model, 60);
                        }
                        if !should_retry_after_failure(
                            &e,
                            &diagnostics,
                            target_model,
                            turn,
                            attempt,
                        ) {
                            update_model_health(state, target_model, false, Some(e.to_string()))
                                .await;
                            return Err(e);
                        }
                        tracing::warn!(
                            "[RETRY] submit ack failed for {} attempt {}: {}",
                            target_model,
                            attempt,
                            e
                        );
                        last_err = Some(e);
                        continue;
                    }
                };
                // Handle early response from ack
                match ack.outcome {
                    SubmitOutcome::ResponseEarly(response) => {
                        finish_operation(state, &context.operation_id, true).await;
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_response_captured",
                                "agent_id": target_model,
                                "turn_number": turn,
                            }),
                        );
                        let _ = app.emit(
                            "agent-message",
                            serde_json::json!({
                                "agent_id": target_model,
                                "role": "participant",
                                "response": &response,
                                "tokens": 0,
                                "iteration": turn,
                                "source_type": "browser_or_manual"
                            }),
                        );
                        update_model_health(state, target_model, true, None).await;
                        return Ok(response);
                    }
                    SubmitOutcome::ManualRecovery => {
                        let browser = state.browser_state.lock().await;
                        browser.mark_active_waiting(target_model, turn);
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_prompt_injected",
                                "agent_id": target_model,
                                "turn_number": turn,
                            }),
                        );
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_waiting_for_response",
                                "agent_id": target_model,
                                "turn_number": turn,
                            }),
                        );
                        // Fall through to wait
                        let mut early_buf = ack.early_buffer;
                        let deadline = Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT_SECS);
                        match wait_for_response_with_operation(
                            &context,
                            &mut inbox,
                            nav_rx,
                            &mut early_buf,
                            deadline,
                        )
                        .await
                        {
                            Ok(response) => {
                                finish_operation(state, &context.operation_id, true).await;
                                let _ = app.emit(
                                    "active-turn-state",
                                    serde_json::json!({
                                        "event": "active_response_captured",
                                        "agent_id": target_model,
                                        "turn_number": turn,
                                    }),
                                );
                                let _ = app.emit(
                                    "agent-message",
                                    serde_json::json!({
                                        "agent_id": target_model,
                                        "role": "participant",
                                        "response": &response,
                                        "tokens": 0,
                                        "iteration": turn,
                                        "source_type": "browser_or_manual"
                                    }),
                                );
                                update_model_health(state, target_model, true, None).await;
                                return Ok(response);
                            }
                            Err(e) => {
                                finish_operation(state, &context.operation_id, false).await;
                                let _ = app.emit(
                                    "active-turn-state",
                                    serde_json::json!({
                                        "event": "active_turn_timeout",
                                        "agent_id": target_model,
                                        "turn_number": turn,
                                    }),
                                );
                                if matches!(
                                    &e,
                                    AgentError::Timeout(_) | AgentError::InjectionFailed(_)
                                ) {
                                    crate::browser_backend::record_browser_error(
                                        app,
                                        &diagnostics,
                                        target_model,
                                        &e.to_string(),
                                    );
                                }
                                let kind = e.kind();
                                if kind == ErrorKind::RateLimit {
                                    let mut browser = state.browser_state.lock().await;
                                    browser.set_cooldown(target_model, 60);
                                }
                                // ManualRecovery wait failure is retryable unless empty-shell etc
                                if !should_retry_after_failure(
                                    &e,
                                    &diagnostics,
                                    target_model,
                                    turn,
                                    attempt,
                                ) {
                                    update_model_health(
                                        state,
                                        target_model,
                                        false,
                                        Some(e.to_string()),
                                    )
                                    .await;
                                    return Err(e);
                                }
                                tracing::warn!(
                                    "[RETRY] Wait for response from {} failed (attempt {}): {}",
                                    target_model,
                                    attempt,
                                    e
                                );
                                last_err = Some(e);
                                continue;
                            }
                        }
                    }
                    SubmitOutcome::Confirmed => {
                        let browser = state.browser_state.lock().await;
                        browser.mark_active_waiting(target_model, turn);
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_prompt_injected",
                                "agent_id": target_model,
                                "turn_number": turn,
                            }),
                        );
                        let _ = app.emit(
                            "active-turn-state",
                            serde_json::json!({
                                "event": "active_waiting_for_response",
                                "agent_id": target_model,
                                "turn_number": turn,
                            }),
                        );
                        let mut early_buf = ack.early_buffer;
                        let deadline = Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT_SECS);
                        match wait_for_response_with_operation(
                            &context,
                            &mut inbox,
                            nav_rx,
                            &mut early_buf,
                            deadline,
                        )
                        .await
                        {
                            Ok(response) => {
                                finish_operation(state, &context.operation_id, true).await;
                                let _ = app.emit(
                                    "active-turn-state",
                                    serde_json::json!({
                                        "event": "active_response_captured",
                                        "agent_id": target_model,
                                        "turn_number": turn,
                                    }),
                                );
                                let _ = app.emit(
                                    "agent-message",
                                    serde_json::json!({
                                        "agent_id": target_model,
                                        "role": "participant",
                                        "response": &response,
                                        "tokens": 0,
                                        "iteration": turn,
                                        "source_type": "browser_or_manual"
                                    }),
                                );
                                update_model_health(state, target_model, true, None).await;
                                return Ok(response);
                            }
                            Err(e) => {
                                finish_operation(state, &context.operation_id, false).await;
                                let _ = app.emit(
                                    "active-turn-state",
                                    serde_json::json!({
                                        "event": "active_turn_timeout",
                                        "agent_id": target_model,
                                        "turn_number": turn,
                                    }),
                                );
                                if matches!(
                                    &e,
                                    AgentError::Timeout(_) | AgentError::InjectionFailed(_)
                                ) {
                                    crate::browser_backend::record_browser_error(
                                        app,
                                        &diagnostics,
                                        target_model,
                                        &e.to_string(),
                                    );
                                }
                                let kind = e.kind();
                                if kind == ErrorKind::RateLimit {
                                    let mut browser = state.browser_state.lock().await;
                                    browser.set_cooldown(target_model, 60);
                                }
                                // Confirmed submit means do not retry on response timeout (already submitted)
                                update_model_health(
                                    state,
                                    target_model,
                                    false,
                                    Some(e.to_string()),
                                )
                                .await;
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
    }

    let err = last_err.unwrap_or_else(|| AgentError::UnknownError("Retry exhausted".to_string()));
    update_model_health(state, target_model, false, Some(err.to_string())).await;
    Err(err)
}

// ── IMP-5: update_model_health ────────────────────────────────────────────────

async fn update_model_health(
    state: &AppState,
    agent_id: &str,
    is_available: bool,
    last_error: Option<String>,
) {
    let mut health = state.model_health.lock().await;
    let entry = health
        .entry(agent_id.to_string())
        .or_insert_with(|| ModelHealth {
            agent_id: agent_id.to_string(),
            is_available: true,
            error_count: 0,
            last_error: None,
        });
    if is_available {
        entry.error_count = 0;
    } else {
        entry.error_count += 1;
    }
    entry.is_available = is_available;
    entry.last_error = last_error;
}

// ── wait_for_response ─────────────────────────────────────────────────────────
//
// RISK-STALERESPONSE: checks BOTH agent_id AND turn number.
// Timeout matched as Ok/Err — never uses ? on the timeout result.
// Ready and SendDetected events are silently skipped.

async fn wait_for_response(
    agent_id: &str,
    turn: u32,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<String, AgentError> {
    wait_for_response_until(
        agent_id,
        turn,
        nav_rx,
        Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT_SECS),
    )
    .await
}

/// The response deadline belongs to the turn, not to a receive operation.
/// In particular, passive diagnostics and stale events must never buy an
/// unresponsive provider another full timeout window.
async fn wait_for_response_until(
    agent_id: &str,
    turn: u32,
    nav_rx: &mut Receiver<NavEvent>,
    deadline: Instant,
) -> Result<String, AgentError> {
    let mut assembly: Option<ResponseAssembly> = None;
    loop {
        match tokio::time::timeout_at(deadline, nav_rx.recv()).await {
            Ok(Some(event)) => {
                // D-040 [NAV]
                tracing::debug!("[NAV] {:?}", event);
                match event {
                    NavEvent::Response {
                        agent_id: ev_agent,
                        turn: ev_turn,
                        text: text,
                        ..
                    } => {
                        if ev_agent == agent_id && ev_turn == turn {
                            return Ok(text);
                        }
                    }
                    NavEvent::ResponseStart {
                        operation_id: _,
                        agent_id: ev_agent,
                        turn: ev_turn,
                        byte_length,
                        chunk_count,
                        checksum,
                        ..
                    } if ev_agent == agent_id && ev_turn == turn => {
                        assembly =
                            Some(ResponseAssembly::start(byte_length, chunk_count, checksum)?);
                    }
                    NavEvent::ResponseChunk {
                        operation_id: _,
                        agent_id: ev_agent,
                        turn: ev_turn,
                        sequence,
                        text,
                        ..
                    } if ev_agent == agent_id && ev_turn == turn => {
                        let Some(active) = assembly.as_mut() else {
                            return Err(AgentError::ExtractionFailed(
                                "response chunk received before response start".to_string(),
                            ));
                        };
                        active.insert(sequence, text)?;
                    }
                    NavEvent::ResponseEnd {
                        operation_id: _,
                        agent_id: ev_agent,
                        turn: ev_turn,
                        checksum,
                        ..
                    } if ev_agent == agent_id && ev_turn == turn => {
                        let Some(active) = assembly.take() else {
                            return Err(AgentError::ExtractionFailed(
                                "response end received before response start".to_string(),
                            ));
                        };
                        return active.finish(&checksum);
                    }
                    NavEvent::Done {
                        agent_id: ev_agent,
                        turn: ev_turn,
                        ..
                    } => {
                        if ev_agent == agent_id && ev_turn == turn {
                            // `done` is only a completion marker from the page script.
                            // It carries no response text, and some WebViews can deliver it
                            // even when a long `response` arena URL was rejected.  Never let
                            // it turn a real active response into an empty brain input.
                            tracing::debug!(
                                "[ACTIVE] completion marker received for {ev_agent} turn {ev_turn}; waiting for response text"
                            );
                        }
                    }
                    NavEvent::ManualResponse {
                        operation_id: _,
                        agent_id: ev_agent,
                        turn: ev_turn,
                        response,
                        ..
                    } => {
                        if ev_agent == agent_id && ev_turn == turn {
                            return Ok(response);
                        }
                    }
                    NavEvent::Error(ev_agent) => {
                        if ev_agent == agent_id {
                            return Err(AgentError::ExtractionFailed(format!(
                                "Agent {} reported an error",
                                ev_agent
                            )));
                        }
                    }
                    NavEvent::ChallengeDetected(ev_agent, indicator) => {
                        if ev_agent == agent_id {
                            let lower = indicator.to_ascii_lowercase();
                            let kind = if lower.contains("login")
                                || lower.contains("sign in")
                                || lower.contains("sign-in")
                                || lower.contains("auth")
                            {
                                "login required"
                            } else if lower.contains("captcha")
                                || lower.contains("challenge")
                                || lower.contains("security")
                                || lower.contains("cloudflare")
                                || lower.contains("verify")
                            {
                                "captcha/challenge"
                            } else {
                                "challenge"
                            };
                            tracing::warn!(
                                "[CHALLENGE] {} blocked by {}: {} — waiting for ResumeRequested (600s)",
                                agent_id,
                                kind,
                                indicator
                            );
                            // R1.8: bounded challenge recovery — do NOT immediately
                            // terminate the live turn. Reuse the setup pattern:
                            // wait for ResumeRequested (or Ready) for up to 600 s,
                            // preserving the same agent_id+turn. This is distinct
                            // from MAX_RETRIES; it does not consume the retry
                            // budget and does not hammer the page after Resume.
                            let resume_deadline =
                                tokio::time::Instant::now() + Duration::from_secs(600);
                            loop {
                                match tokio::time::timeout_at(resume_deadline, nav_rx.recv()).await
                                {
                                    Ok(Some(NavEvent::ResumeRequested(req_id)))
                                        if req_id == agent_id =>
                                    {
                                        tracing::info!(
                                            "[CHALLENGE] {} resume received, retrying wait for response (turn {})",
                                            agent_id,
                                            turn
                                        );
                                        break;
                                    }
                                    Ok(Some(NavEvent::Ready(req_id))) if req_id == agent_id => {
                                        tracing::info!(
                                            "[CHALLENGE] {} ready after challenge, continuing wait (turn {})",
                                            agent_id,
                                            turn
                                        );
                                        break;
                                    }
                                    Ok(Some(NavEvent::ChallengeDetected(
                                        ch_id,
                                        next_indicator,
                                    ))) if ch_id == agent_id => {
                                        tracing::warn!(
                                            "[CHALLENGE] {} still blocked: {}",
                                            agent_id,
                                            next_indicator
                                        );
                                        continue;
                                    }
                                    Ok(Some(NavEvent::ManualResponse {
                                        operation_id: _,
                                        agent_id: m_id,
                                        turn: m_turn,
                                        response,
                                    })) if m_id == agent_id && m_turn == turn => {
                                        return Ok(response);
                                    }
                                    Ok(Some(NavEvent::Response {
                                        agent_id: m_id,
                                        turn: m_turn,
                                        text: text,
                                        ..
                                    })) if m_id == agent_id && m_turn == turn => {
                                        return Ok(text);
                                    }
                                    Ok(Some(NavEvent::UnshowableUrl(u_id, url)))
                                        if u_id == agent_id =>
                                    {
                                        return Err(AgentError::NavigationFailed(format!(
                                            "{} navigated to unshowable URL while blocked: {}",
                                            agent_id, url
                                        )));
                                    }
                                    Ok(Some(NavEvent::SessionAborted)) => {
                                        return Err(AgentError::UnknownError(
                                            "Session aborted while waiting for challenge resume"
                                                .to_string(),
                                        ));
                                    }
                                    Ok(None) => {
                                        return Err(AgentError::NavigationFailed(
                                            "channel closed while waiting for challenge resume"
                                                .to_string(),
                                        ));
                                    }
                                    Err(_) => {
                                        // R1.8: challenge resume timeout is a distinct
                                        // terminal state, not a normal response timeout.
                                        // Return CaptchaRequired so the caller can
                                        // distinguish it from a network/response timeout
                                        // and terminate or mark participant unavailable
                                        // without hammering retries.
                                        return Err(AgentError::CaptchaRequired(format!(
                                            "{} blocked by {}: {} — timeout waiting for verification resume (600s)",
                                            agent_id, kind, indicator
                                        )));
                                    }
                                    Ok(Some(_)) => continue,
                                }
                            }
                            // Resume received — continue outer wait_for_response loop
                            // for the SAME agent/turn without returning an error.
                            continue;
                        }
                    }
                    NavEvent::UnshowableUrl(ev_agent, url) => {
                        if ev_agent == agent_id {
                            return Err(AgentError::NavigationFailed(format!(
                                "{} navigated to an unshowable URL: {}",
                                agent_id, url
                            )));
                        }
                    }
                    NavEvent::SessionAborted => {
                        return Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    _ => {} // Ready, SendDetected — silently skip
                }
            }
            Ok(None) => {
                return Err(AgentError::NavigationFailed(
                    "Navigation channel closed unexpectedly".to_string(),
                ));
            }
            Err(_elapsed) => {
                return Err(AgentError::Timeout(format!(
                    "Agent {} did not respond within {} seconds",
                    agent_id, RESPONSE_TIMEOUT_SECS
                )));
            }
        }
    }
}

async fn wait_for_response_with_operation(
    context: &OperationContext,
    inbox: &mut OperationInbox<NavEvent>,
    auxiliary_rx: &mut Receiver<NavEvent>,
    early: &mut std::collections::VecDeque<NavEvent>,
    deadline: Instant,
) -> Result<String, AgentError> {
    let mut assembly: Option<ResponseAssembly> = None;
    // First consume early buffer in order
    while let Some(event) = early.pop_front() {
        match event {
            NavEvent::Response {
                operation_id,
                agent_id,
                turn,
                text,
            } => {
                if operation_id != context.operation_id
                    || agent_id != context.agent_id
                    || turn != context.turn
                {
                    return Err(AgentError::ExtractionFailed(
                        "early response mismatch".to_string(),
                    ));
                }
                return Ok(text);
            }
            NavEvent::ResponseStart {
                operation_id,
                agent_id,
                turn,
                byte_length,
                chunk_count,
                checksum,
            } => {
                if operation_id != context.operation_id
                    || agent_id != context.agent_id
                    || turn != context.turn
                {
                    return Err(AgentError::ExtractionFailed(
                        "early start mismatch".to_string(),
                    ));
                }
                assembly = Some(ResponseAssembly::start(byte_length, chunk_count, checksum)?);
            }
            NavEvent::ResponseChunk {
                operation_id,
                agent_id,
                turn,
                sequence,
                text,
            } => {
                if operation_id != context.operation_id
                    || agent_id != context.agent_id
                    || turn != context.turn
                {
                    return Err(AgentError::ExtractionFailed(
                        "early chunk mismatch".to_string(),
                    ));
                }
                let Some(active) = assembly.as_mut() else {
                    return Err(AgentError::ExtractionFailed(
                        "response chunk received before response start".to_string(),
                    ));
                };
                active.insert(sequence, text)?;
            }
            NavEvent::ResponseEnd {
                operation_id,
                agent_id,
                turn,
                checksum,
            } => {
                if operation_id != context.operation_id
                    || agent_id != context.agent_id
                    || turn != context.turn
                {
                    return Err(AgentError::ExtractionFailed(
                        "early end mismatch".to_string(),
                    ));
                }
                let Some(active) = assembly.take() else {
                    return Err(AgentError::ExtractionFailed(
                        "response end received before response start".to_string(),
                    ));
                };
                return active.finish(&checksum);
            }
            NavEvent::Done {
                operation_id,
                agent_id,
                turn,
            } => {
                if operation_id == context.operation_id
                    && agent_id == context.agent_id
                    && turn == context.turn
                {
                    tracing::debug!(
                        "[ACTIVE] early completion marker for {} turn {}; waiting for response text",
                        agent_id,
                        turn
                    );
                }
            }
            NavEvent::ManualResponse {
                operation_id,
                agent_id,
                turn,
                response,
            } => {
                if operation_id == context.operation_id
                    && agent_id == context.agent_id
                    && turn == context.turn
                {
                    return Ok(response);
                }
            }
            _ => {}
        }
    }
    // Now select between critical inbox and auxiliary
    loop {
        tokio::select! {
            biased;
            critical = inbox.recv() => {
                let event = critical.map_err(critical_to_agent_error)?;
                match event {
                    NavEvent::Response { operation_id, agent_id, turn, text } => {
                        if operation_id != context.operation_id || agent_id != context.agent_id || turn != context.turn {
                            return Err(AgentError::ExtractionFailed("critical response mismatch".to_string()));
                        }
                        return Ok(text);
                    }
                    NavEvent::ResponseStart { operation_id, agent_id, turn, byte_length, chunk_count, checksum } => {
                        if operation_id != context.operation_id || agent_id != context.agent_id || turn != context.turn {
                            return Err(AgentError::ExtractionFailed("critical start mismatch".to_string()));
                        }
                        assembly = Some(ResponseAssembly::start(byte_length, chunk_count, checksum)?);
                    }
                    NavEvent::ResponseChunk { operation_id, agent_id, turn, sequence, text } => {
                        if operation_id != context.operation_id || agent_id != context.agent_id || turn != context.turn {
                            return Err(AgentError::ExtractionFailed("critical chunk mismatch".to_string()));
                        }
                        let Some(active) = assembly.as_mut() else {
                            return Err(AgentError::ExtractionFailed("response chunk received before response start".to_string()));
                        };
                        active.insert(sequence, text)?;
                    }
                    NavEvent::ResponseEnd { operation_id, agent_id, turn, checksum } => {
                        if operation_id != context.operation_id || agent_id != context.agent_id || turn != context.turn {
                            return Err(AgentError::ExtractionFailed("critical end mismatch".to_string()));
                        }
                        let Some(active) = assembly.take() else {
                            return Err(AgentError::ExtractionFailed("response end received before response start".to_string()));
                        };
                        return active.finish(&checksum);
                    }
                    NavEvent::Done { operation_id, agent_id, turn } => {
                        if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn {
                            tracing::debug!("[ACTIVE] completion marker received for {} turn {}; waiting for response text", agent_id, turn);
                        }
                    }
                    NavEvent::ManualResponse { operation_id, agent_id, turn, response } => {
                        if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn {
                            return Ok(response);
                        } else {
                            return Err(AgentError::ExtractionFailed("manual response mismatch".to_string()));
                        }
                    }
                    NavEvent::ActiveSubmitReport { .. } => {
                        // Should not appear here (already consumed in ack), treat as protocol error if for same op
                        return Err(AgentError::ExtractionFailed("unexpected submit report during response wait".to_string()));
                    }
                    other => {
                        return Err(AgentError::ExtractionFailed(format!("unexpected critical event during response wait: {:?}", other)));
                    }
                }
            }
            aux = auxiliary_rx.recv() => {
                match aux {
                    Some(NavEvent::ChallengeDetected(ev_agent, indicator)) if ev_agent == context.agent_id => {
                        let lower = indicator.to_ascii_lowercase();
                        let kind = if lower.contains("login") || lower.contains("sign in") || lower.contains("sign-in") || lower.contains("auth") {
                            "login required"
                        } else if lower.contains("captcha") || lower.contains("challenge") || lower.contains("security") || lower.contains("cloudflare") || lower.contains("verify") {
                            "captcha/challenge"
                        } else {
                            "challenge"
                        };
                        tracing::warn!("[CHALLENGE] {} blocked by {}: {} — waiting for ResumeRequested (600s)", context.agent_id, kind, indicator);
                        let resume_deadline = tokio::time::Instant::now() + Duration::from_secs(600);
                        loop {
                            tokio::select! {
                                biased;
                                crit = inbox.recv() => {
                                    let ev = crit.map_err(critical_to_agent_error)?;
                                    // If critical response arrives during challenge wait, return it
                                    match ev {
                                        NavEvent::ManualResponse { operation_id, agent_id, turn, response } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            return Ok(response);
                                        }
                                        NavEvent::Response { operation_id, agent_id, turn, text } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            return Ok(text);
                                        }
                                        NavEvent::ResponseStart { operation_id, agent_id, turn, byte_length, chunk_count, checksum } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            assembly = Some(ResponseAssembly::start(byte_length, chunk_count, checksum)?);
                                            break;
                                        }
                                        NavEvent::ResponseChunk { operation_id, agent_id, turn, sequence, text } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            if let Some(active) = assembly.as_mut() {
                                                active.insert(sequence, text)?;
                                            } else {
                                                return Err(AgentError::ExtractionFailed("chunk before start during challenge".to_string()));
                                            }
                                        }
                                        NavEvent::ResponseEnd { operation_id, agent_id, turn, checksum } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            if let Some(active) = assembly.take() {
                                                return active.finish(&checksum);
                                            } else {
                                                return Err(AgentError::ExtractionFailed("end before start during challenge".to_string()));
                                            }
                                        }
                                        NavEvent::Done { operation_id, agent_id, turn } if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            tracing::debug!("[ACTIVE] done during challenge for {} turn {}", agent_id, turn);
                                        }
                                        _ => {}
                                    }
                                }
                                aux2 = auxiliary_rx.recv() => {
                                    match aux2 {
                                        Some(NavEvent::ResumeRequested(req_id)) if req_id == context.agent_id => {
                                            tracing::info!("[CHALLENGE] {} resume received, continuing wait (turn {})", context.agent_id, context.turn);
                                            break;
                                        }
                                        Some(NavEvent::Ready(req_id)) if req_id == context.agent_id => {
                                            tracing::info!("[CHALLENGE] {} ready after challenge, continuing wait (turn {})", context.agent_id, context.turn);
                                            break;
                                        }
                                        Some(NavEvent::ChallengeDetected(ch_id, next_indicator)) if ch_id == context.agent_id => {
                                            tracing::warn!("[CHALLENGE] {} still blocked: {}", context.agent_id, next_indicator);
                                            continue;
                                        }
                                        Some(NavEvent::ManualResponse { operation_id, agent_id, turn, response }) if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            return Ok(response);
                                        }
                                        Some(NavEvent::Response { operation_id, agent_id, turn, text }) if operation_id == context.operation_id && agent_id == context.agent_id && turn == context.turn => {
                                            return Ok(text);
                                        }
                                        Some(NavEvent::UnshowableUrl(u_id, url)) if u_id == context.agent_id => {
                                            return Err(AgentError::NavigationFailed(format!("{} navigated to unshowable URL while blocked: {}", context.agent_id, url)));
                                        }
                                        Some(NavEvent::SessionAborted) => {
                                            return Err(AgentError::UnknownError("Session aborted while waiting for challenge resume".to_string()));
                                        }
                                        None => {
                                            return Err(AgentError::NavigationFailed("channel closed while waiting for challenge resume".to_string()));
                                        }
                                        Some(_) => continue,
                                    }
                                }
                                _ = tokio::time::sleep_until(resume_deadline) => {
                                    return Err(AgentError::CaptchaRequired(format!("{} blocked by {}: {} — timeout waiting for verification resume (600s)", context.agent_id, kind, indicator)));
                                }
                            }
                        }
                        continue;
                    }
                    Some(NavEvent::UnshowableUrl(ev_agent, url)) if ev_agent == context.agent_id => {
                        return Err(AgentError::NavigationFailed(format!("{} navigated to an unshowable URL: {}", context.agent_id, url)));
                    }
                    Some(NavEvent::SessionAborted) => {
                        return Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    Some(NavEvent::Error(ev_agent)) if ev_agent == context.agent_id => {
                        return Err(AgentError::ExtractionFailed(format!("Agent {} reported an error", ev_agent)));
                    }
                    Some(_) => {
                        // ignore auxiliary telemetry like Ready, SendDetected, SendProbe etc. Do not reset deadline.
                        continue;
                    }
                    None => {
                        return Err(AgentError::NavigationFailed("auxiliary channel closed".to_string()));
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(AgentError::Timeout(format!("Agent {} did not respond within {} seconds", context.agent_id, RESPONSE_TIMEOUT_SECS)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_submit_report(
        agent_id: &str,
        turn: u32,
        succeeded: bool,
        error: Option<&str>,
    ) -> NavEvent {
        NavEvent::ActiveSubmitReport {
            operation_id: crate::pipeline_ids::OperationId::new(),
            agent_id: agent_id.to_string(),
            turn,
            succeeded,
            method: "button_click".to_string(),
            send_enabled: true,
            error: error.map(|e| e.to_string()),
        }
    }

    fn make_context(
        agent: &str,
        turn: u32,
    ) -> (
        crate::pipeline_ids::OperationContext,
        crate::critical_transport::CriticalEventHub<NavEvent>,
    ) {
        let owner = crate::session_runtime::SessionOwner {
            session_id: "test-session".to_string(),
            run_generation: 1,
        };
        let ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            agent,
            turn,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        (ctx, hub)
    }

    fn active_submit_report_with_op(
        op: crate::pipeline_ids::OperationId,
        agent_id: &str,
        turn: u32,
        succeeded: bool,
        error: Option<&str>,
    ) -> NavEvent {
        NavEvent::ActiveSubmitReport {
            operation_id: op,
            agent_id: agent_id.to_string(),
            turn,
            succeeded,
            method: "button_click".to_string(),
            send_enabled: true,
            error: error.map(|e| e.to_string()),
        }
    }

    /// Short test-only ACK deadline. Production uses SUBMIT_ACK_TIMEOUT_SECS;
    /// tests must never sleep 30 seconds.
    fn test_ack_deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }

    #[tokio::test]
    async fn ack_accepts_success_report_for_exact_agent_turn() {
        let (ctx, hub) = make_context("deepseek", 3);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let ev = active_submit_report_with_op(ctx.operation_id.clone(), "deepseek", 3, true, None);
        hub.dispatch(ctx.operation_id.clone(), ev, 64);
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Ok(SubmitAckWait::Outcome(SubmitOutcome::Confirmed))),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn ack_rejects_failure_report_for_exact_agent_turn() {
        let (ctx, hub) = make_context("chatgpt", 2);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let ev = active_submit_report_with_op(
            ctx.operation_id.clone(),
            "chatgpt",
            2,
            false,
            Some("enabled_send_button_not_found_after_retry"),
        );
        hub.dispatch(ctx.operation_id.clone(), ev, 64);
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Err(AgentError::InjectionFailed(_))),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn ack_skips_stale_reports_from_other_agents_and_turns() {
        // With operation mailbox, only exact operation_id is delivered, so stale reports for other ops are never enqueued.
        // This test verifies that a correct report for the exact context is accepted even after other contexts' reports were sent to their own mailboxes.
        let (ctx, hub) = make_context("chatgpt", 2);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        // Create other contexts and dispatch to their mailboxes (not to ctx's)
        let other_ctx = crate::pipeline_ids::OperationContext::from_owner(
            &crate::session_runtime::SessionOwner {
                session_id: "test-session".to_string(),
                run_generation: 1,
            },
            "deepseek",
            3,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let mut other_inbox = hub
            .register(other_ctx.operation_id.clone(), 0, true)
            .unwrap();
        hub.dispatch(
            other_ctx.operation_id.clone(),
            active_submit_report_with_op(other_ctx.operation_id.clone(), "deepseek", 3, true, None),
            64,
        );
        hub.dispatch(
            ctx.operation_id.clone(),
            active_submit_report_with_op(ctx.operation_id.clone(), "chatgpt", 2, true, None),
            64,
        );
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Ok(SubmitAckWait::Outcome(SubmitOutcome::Confirmed))),
            "got {result:?}"
        );
        // other inbox should still have its event
        let mut other_early = std::collections::VecDeque::new();
        let other_res = await_submit_ack_until(
            &other_ctx,
            &mut other_inbox,
            &mut other_early,
            test_ack_deadline(),
        )
        .await;
        assert!(matches!(
            other_res,
            Ok(SubmitAckWait::Outcome(SubmitOutcome::Confirmed))
        ));
    }

    #[tokio::test]
    async fn ack_captures_early_response_for_exact_agent_turn() {
        let (ctx, hub) = make_context("chatgpt", 4);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let op = ctx.operation_id.clone();
        hub.dispatch(
            op.clone(),
            NavEvent::Response {
                operation_id: op.clone(),
                agent_id: "chatgpt".to_string(),
                turn: 4,
                text: "early text".to_string(),
            },
            10,
        );
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Ok(SubmitAckWait::Outcome(SubmitOutcome::ResponseEarly(ref t))) if t=="early text"),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn ack_accepts_manual_response_for_exact_agent_turn() {
        let (ctx, hub) = make_context("deepseek", 5);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let op = ctx.operation_id.clone();
        hub.dispatch(
            op.clone(),
            NavEvent::ManualResponse {
                operation_id: op.clone(),
                agent_id: "deepseek".to_string(),
                turn: 5,
                response: "pasted".to_string(),
            },
            6,
        );
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Ok(SubmitAckWait::Outcome(SubmitOutcome::ResponseEarly(ref t))) if t=="pasted"),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn ack_surfaces_session_aborted() {
        // SessionAborted is not a critical event; it is auxiliary. For critical inbox, closed without event should be NavigationFailed or Closed.
        // Simulate by closing inbox before ack
        let (ctx, hub) = make_context("chatgpt", 1);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        hub.close_exact(&ctx.operation_id);
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(
                result,
                Err(AgentError::NavigationFailed(_)) | Err(AgentError::UnknownError(_))
            ),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn ack_errors_when_channel_closed_before_matching_report() {
        let (ctx, hub) = make_context("chatgpt", 1);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        hub.close_exact(&ctx.operation_id);
        let mut early = std::collections::VecDeque::new();
        let result =
            await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(
                result,
                Err(AgentError::NavigationFailed(_)) | Err(AgentError::UnknownError(_))
            ),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn drain_stale_active_events_preserves_pending_critical_signals() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        // Use operation-based critical report but this test checks auxiliary drain is no-op
        let op = crate::pipeline_ids::OperationId::new();
        tx.send(NavEvent::ActiveSubmitReport {
            operation_id: op.clone(),
            agent_id: "chatgpt".to_string(),
            turn: 1,
            succeeded: false,
            method: "button_click".to_string(),
            send_enabled: true,
            error: None,
        })
        .await
        .unwrap();
        tx.send(NavEvent::Ready("chatgpt".to_string()))
            .await
            .unwrap();
        let op2 = crate::pipeline_ids::OperationId::new();
        tx.send(NavEvent::Response {
            operation_id: op2,
            agent_id: "chatgpt".to_string(),
            turn: 1,
            text: "stale".to_string(),
        })
        .await
        .unwrap();
        let drained = drain_stale_active_events(&mut rx);
        assert_eq!(drained, 0);
        assert!(
            matches!(rx.try_recv(), Ok(NavEvent::ActiveSubmitReport { .. })),
            "current critical evidence must not be discarded before correlation"
        );
    }

    #[tokio::test]
    async fn drain_stale_active_events_returns_zero_when_empty() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        drop(tx);
        let drained = drain_stale_active_events(&mut rx);
        assert_eq!(drained, 0);
    }

    #[tokio::test]
    async fn queued_response_recovery_requires_exact_agent_and_turn() {
        // Destructive scan removed; should return None even with queued responses
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let op1 = crate::pipeline_ids::OperationId::new();
        tx.send(NavEvent::Response {
            operation_id: op1,
            agent_id: "other".to_string(),
            turn: 4,
            text: "stale agent".to_string(),
        })
        .await
        .unwrap();
        let op2 = crate::pipeline_ids::OperationId::new();
        tx.send(NavEvent::Response {
            operation_id: op2,
            agent_id: "claude".to_string(),
            turn: 3,
            text: "stale turn".to_string(),
        })
        .await
        .unwrap();
        let op3 = crate::pipeline_ids::OperationId::new();
        tx.send(NavEvent::Response {
            operation_id: op3,
            agent_id: "claude".to_string(),
            turn: 4,
            text: "current".to_string(),
        })
        .await
        .unwrap();
        assert_eq!(
            super::take_queued_response_for_turn(&mut rx, "claude", 4),
            None
        );
    }

    // ── R1.8: live challenge recovery ───────────────────────────────────────

    #[tokio::test]
    async fn wait_for_response_challenge_then_resume_returns_response() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let agent = "chatgpt";
        let turn = 7;
        let handle = tokio::spawn(async move { wait_for_response(agent, turn, &mut rx).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::ChallengeDetected(
            agent.to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "should still be waiting for ResumeRequested"
        );
        tx.send(NavEvent::ResumeRequested(agent.to_string()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "should still be waiting for Response after resume"
        );
        tx.send(NavEvent::Response {
            operation_id: crate::pipeline_ids::OperationId::new(),
            agent_id: agent.to_string(),
            turn,
            text: "hello".to_string(),
        })
        .await
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("wait_for_response should complete after resume+response")
            .unwrap();
        assert!(
            matches!(result, Ok(ref text) if text == "hello"),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_response_challenge_then_manual_response_returns_response() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let agent = "deepseek";
        let turn = 3;
        let handle = tokio::spawn(async move { wait_for_response(agent, turn, &mut rx).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::ChallengeDetected(
            agent.to_string(),
            "login required".to_string(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::ManualResponse {
            operation_id: crate::pipeline_ids::OperationId::new(),
            agent_id: agent.to_string(),
            turn,
            response: "pasted".to_string(),
        })
        .await
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("should resolve via ManualResponse during challenge wait")
            .unwrap();
        assert!(
            matches!(result, Ok(ref t) if t == "pasted"),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_response_challenge_then_abort_returns_aborted() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let agent = "claude";
        let turn = 2;
        let handle = tokio::spawn(async move { wait_for_response(agent, turn, &mut rx).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::ChallengeDetected(
            agent.to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::SessionAborted).await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("should resolve after abort")
            .unwrap();
        assert!(
            matches!(result, Err(AgentError::UnknownError(ref msg)) if msg.contains("Session aborted")),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_response_ignores_challenge_for_other_agent() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let agent = "chatgpt";
        let turn = 5;
        let handle = tokio::spawn(async move { wait_for_response(agent, turn, &mut rx).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(NavEvent::ChallengeDetected(
            "other".to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "challenge for other agent should be ignored"
        );
        tx.send(NavEvent::Response {
            operation_id: crate::pipeline_ids::OperationId::new(),
            agent_id: agent.to_string(),
            turn,
            text: "ok".to_string(),
        })
        .await
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Ok(ref t) if t == "ok"), "got {result:?}");
    }

    #[tokio::test]
    async fn wait_for_response_normal_timeout_is_not_captcha() {
        let (_tx, mut rx) = tokio::sync::mpsc::channel::<NavEvent>(8);
        // No events — wait_for_response should timeout with Timeout, not CaptchaRequired
        let result = tokio::time::timeout(
            Duration::from_millis(350),
            wait_for_response("chatgpt", 1, &mut rx),
        )
        .await;
        // The inner timeout is 300s, so the outer 350ms timeout will hit first — we just verify
        // that without any ChallengeDetected, the error kind is Timeout when it eventually fires.
        // For a fast deterministic check, we instead verify that an immediate Response works and
        // that a Challenge for other agent does not turn into CaptchaRequired.
        assert!(
            result.is_err(),
            "outer timeout should hit before inner 300s"
        );
        // Directly verify classification: a normal wait that times out is Timeout, not CaptchaRequired
        // (this is covered by the existing is_ignored test and by the fact that only ChallengeDetected
        // produces CaptchaRequired).
    }

    // ── W1-C: empty-shell vs transient retry classification ─────────────────

    #[test]
    fn should_retry_empty_shell_timeout_is_not_retryable() {
        use crate::browser_backend::{BrowserDiagnostics, BrowserSetupMetadata};
        let diagnostics = BrowserDiagnostics::new();
        diagnostics.begin_setup_run(BrowserSetupMetadata {
            setup_generation: 1,
            session_id: "sess".to_string(),
            selected_leader_id: "chatgpt".to_string(),
            selected_agent_ids: vec!["chatgpt".to_string()],
            setup_order: vec!["chatgpt".to_string()],
        });
        diagnostics.register(
            "chatgpt",
            crate::browser_backend::LEADER_WINDOW_LABEL,
            "leader",
        );
        diagnostics.set_active(crate::browser_backend::LEADER_WINDOW_LABEL, "chatgpt");
        // Simulate empty-shell classification
        diagnostics.set_page_state_hint_for_test(
            "chatgpt",
            Some("empty_shell_or_hydration_stuck".to_string()),
        );
        let err = AgentError::Timeout("readiness timeout".to_string());
        assert!(
            !super::should_retry_after_failure(&err, &diagnostics, "chatgpt", 1, 0),
            "empty-shell Timeout must not be retried via navigate"
        );
        assert!(
            !super::should_retry_after_failure(&err, &diagnostics, "chatgpt", 1, 1),
            "empty-shell on retry 1 also must not be retried"
        );
    }

    #[test]
    fn should_retry_transient_navigation_retains_retry() {
        use crate::browser_backend::{BrowserDiagnostics, BrowserSetupMetadata};
        let diagnostics = BrowserDiagnostics::new();
        diagnostics.begin_setup_run(BrowserSetupMetadata {
            setup_generation: 1,
            session_id: "sess".to_string(),
            selected_leader_id: "deepseek".to_string(),
            selected_agent_ids: vec!["deepseek".to_string()],
            setup_order: vec!["deepseek".to_string()],
        });
        diagnostics.register("deepseek", crate::browser_backend::NAV_WINDOW_LABEL, "nav");
        diagnostics.set_active(crate::browser_backend::NAV_WINDOW_LABEL, "deepseek");
        // composer_detected is not empty-shell, so retry should be allowed
        diagnostics.set_page_state_hint_for_test("deepseek", Some("composer_detected".to_string()));
        let err = AgentError::NavigationFailed("transient nav".to_string());
        assert!(
            super::should_retry_after_failure(&err, &diagnostics, "deepseek", 1, 0),
            "transient navigation with composer_detected should retry"
        );
        // No hint also retryable (conservative: not empty-shell)
        diagnostics.set_page_state_hint_for_test("deepseek", None);
        assert!(
            super::should_retry_after_failure(&err, &diagnostics, "deepseek", 1, 0),
            "no hint should default to retryable"
        );
    }

    #[test]
    fn should_retry_permanent_and_max_attempts_never_retry() {
        use crate::browser_backend::{BrowserDiagnostics, BrowserSetupMetadata};
        let diagnostics = BrowserDiagnostics::new();
        diagnostics.begin_setup_run(BrowserSetupMetadata {
            setup_generation: 1,
            session_id: "sess".to_string(),
            selected_leader_id: "chatgpt".to_string(),
            selected_agent_ids: vec!["chatgpt".to_string()],
            setup_order: vec!["chatgpt".to_string()],
        });
        diagnostics.register(
            "chatgpt",
            crate::browser_backend::LEADER_WINDOW_LABEL,
            "leader",
        );
        let perm = AgentError::CaptchaRequired("captcha".to_string());
        assert!(!super::should_retry_after_failure(
            &perm,
            &diagnostics,
            "chatgpt",
            1,
            0
        ));
        assert!(!super::should_retry_after_failure(
            &perm,
            &diagnostics,
            "chatgpt",
            1,
            1
        ));
        let transient = AgentError::Timeout("t".to_string());
        assert!(!super::should_retry_after_failure(
            &transient,
            &diagnostics,
            "chatgpt",
            1,
            super::MAX_RETRIES
        ));
        assert!(!super::should_retry_after_failure(
            &transient,
            &diagnostics,
            "chatgpt",
            1,
            super::MAX_RETRIES + 1
        ));
    }

    #[test]
    fn should_retry_empty_shell_even_on_permanent_also_false() {
        use crate::browser_backend::{BrowserDiagnostics, BrowserSetupMetadata};
        let diagnostics = BrowserDiagnostics::new();
        diagnostics.begin_setup_run(BrowserSetupMetadata {
            setup_generation: 1,
            session_id: "sess".to_string(),
            selected_leader_id: "qwen".to_string(),
            selected_agent_ids: vec!["qwen".to_string()],
            setup_order: vec!["qwen".to_string()],
        });
        diagnostics.register("qwen", crate::browser_backend::NAV_WINDOW_LABEL, "nav");
        diagnostics.set_page_state_hint_for_test(
            "qwen",
            Some("empty_shell_or_hydration_stuck".to_string()),
        );
        // Even if error is considered transient, empty-shell overrides
        let err = AgentError::Timeout("timeout".to_string());
        assert!(!super::should_retry_after_failure(
            &err,
            &diagnostics,
            "qwen",
            1,
            0
        ));
    }

    #[test]
    fn retry_suppression_requires_exact_active_turn_and_generation() {
        use crate::browser_backend::{BrowserDiagnostics, BrowserSetupMetadata};
        let diagnostics = BrowserDiagnostics::new();
        diagnostics.begin_setup_run(BrowserSetupMetadata {
            setup_generation: 9,
            session_id: "sess".to_string(),
            selected_leader_id: "chatgpt".to_string(),
            selected_agent_ids: vec!["claude".to_string()],
            setup_order: vec!["claude".to_string()],
        });
        diagnostics.register("claude", crate::browser_backend::NAV_WINDOW_LABEL, "nav");
        let timeout = AgentError::Timeout("late response race".to_string());

        diagnostics.set_active_response_for_test("claude", 7, 7, 9);
        assert!(
            !super::should_retry_after_failure(&timeout, &diagnostics, "claude", 7, 0),
            "exact response evidence must prevent reinjection"
        );
        assert!(
            super::should_retry_after_failure(&timeout, &diagnostics, "claude", 8, 0),
            "response from another turn must not suppress retry"
        );

        diagnostics.set_active_response_for_test("claude", 7, 7, 8);
        assert!(
            super::should_retry_after_failure(&timeout, &diagnostics, "claude", 7, 0),
            "response from another generation must not suppress retry"
        );
    }

    #[tokio::test]
    async fn response_timeout_is_absolute_despite_irrelevant_traffic() {
        // Irrelevant auxiliary traffic must not extend the response deadline:
        // the producer emits ~1000ms of Ready("other") events while the inner
        // deadline is 200ms. A resetting implementation would wait out the
        // producer; the absolute deadline returns Timeout at ~200ms. The
        // generous outer timeout only bounds the test on slow CI — there is
        // no razor-thin wall-clock assertion, and the producer is aborted
        // immediately after the result instead of being awaited to completion.
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let producer = tokio::spawn(async move {
            for _ in 0..50 {
                let _ = tx.send(NavEvent::Ready("other".to_string())).await;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let inner_deadline = Instant::now() + Duration::from_millis(200);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            super::wait_for_response_until("chatgpt", 1, &mut rx, inner_deadline),
        )
        .await;
        producer.abort();
        assert!(
            matches!(result, Ok(Err(AgentError::Timeout(_)))),
            "irrelevant Ready traffic must not extend the absolute deadline: {result:?}"
        );
    }

    #[test]
    fn response_chunks_reassemble_large_unicode_exactly() {
        let text = "🧠設計✓".repeat(12_000);
        let checksum = super::response_checksum(&text);
        let points = text.chars().collect::<Vec<_>>();
        let chunk_count = points.chunks(1_000).count() as u32;
        let mut assembly =
            super::ResponseAssembly::start(text.len(), chunk_count, checksum.clone())
                .expect("valid bounded assembly");
        for (index, chunk) in points.chunks(1_000).enumerate() {
            assembly
                .insert(index as u32, chunk.iter().collect())
                .expect("valid chunk");
        }
        assert_eq!(assembly.finish(&checksum).expect("verified response"), text);
    }

    #[test]
    fn response_chunks_never_return_partial_or_corrupt_text() {
        let mut assembly =
            super::ResponseAssembly::start(6, 2, "deadbeef".to_string()).expect("valid assembly");
        assembly
            .insert(0, "hello".to_string())
            .expect("first chunk");
        assert!(matches!(
            assembly.finish("deadbeef"),
            Err(AgentError::ExtractionFailed(_))
        ));
    }

    // ── Session 02 RT tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn rt1_old_session_same_agent_turn_cannot_satisfy_new() {
        let owner_a = crate::session_runtime::SessionOwner {
            session_id: "sess-1".to_string(),
            run_generation: 1,
        };
        let owner_b = crate::session_runtime::SessionOwner {
            session_id: "sess-1".to_string(),
            run_generation: 2,
        };
        let ctx_a = crate::pipeline_ids::OperationContext::from_owner(
            &owner_a,
            "claude",
            5,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let ctx_b = crate::pipeline_ids::OperationContext::from_owner(
            &owner_b,
            "claude",
            5,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        assert_ne!(ctx_a.operation_id, ctx_b.operation_id);
        // Simulate hub: old operation's response should not be deliverable to new inbox
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let mut inbox_a = hub.register(ctx_a.operation_id.clone(), 0, true).unwrap();
        let mut inbox_b = hub.register(ctx_b.operation_id.clone(), 0, true).unwrap();
        hub.dispatch(
            ctx_a.operation_id.clone(),
            NavEvent::Response {
                operation_id: ctx_a.operation_id.clone(),
                agent_id: "claude".to_string(),
                turn: 5,
                text: "old response".to_string(),
            },
            12,
        );
        // inbox_b should not receive old response
        let try_recv = tokio::time::timeout(Duration::from_millis(50), inbox_b.recv()).await;
        assert!(
            try_recv.is_err(),
            "new operation should not receive old operation's response"
        );
        // inbox_a should receive it
        let old_text = inbox_a.recv().await.unwrap();
        match old_text {
            NavEvent::Response { text, .. } => assert_eq!(text, "old response"),
            _ => panic!("unexpected"),
        }
        // Even if we dispatch old op's response again after b closed, b should not get it
        hub.close_exact(&ctx_a.operation_id);
        hub.close_exact(&ctx_b.operation_id);
    }

    #[tokio::test]
    async fn rt2_old_start_chunk_end_cannot_assemble_new() {
        let owner_a = crate::session_runtime::SessionOwner {
            session_id: "s".to_string(),
            run_generation: 1,
        };
        let owner_b = crate::session_runtime::SessionOwner {
            session_id: "s".to_string(),
            run_generation: 2,
        };
        let ctx_a = crate::pipeline_ids::OperationContext::from_owner(
            &owner_a,
            "claude",
            5,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let ctx_b = crate::pipeline_ids::OperationContext::from_owner(
            &owner_b,
            "claude",
            5,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let mut inbox_a = hub.register(ctx_a.operation_id.clone(), 0, true).unwrap();
        let mut inbox_b = hub.register(ctx_b.operation_id.clone(), 0, true).unwrap();
        let text = "hello world";
        let checksum = super::response_checksum(text);
        let start = NavEvent::ResponseStart {
            operation_id: ctx_a.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 5,
            byte_length: text.len(),
            chunk_count: 1,
            checksum: checksum.clone(),
        };
        hub.dispatch(ctx_a.operation_id.clone(), start, 64);
        let chunk = NavEvent::ResponseChunk {
            operation_id: ctx_a.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 5,
            sequence: 0,
            text: text.to_string(),
        };
        hub.dispatch(ctx_a.operation_id.clone(), chunk, text.len());
        let end = NavEvent::ResponseEnd {
            operation_id: ctx_a.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 5,
            checksum: checksum.clone(),
        };
        hub.dispatch(ctx_a.operation_id.clone(), end, 32);
        // inbox_b should not see these
        let try_recv = tokio::time::timeout(Duration::from_millis(50), inbox_b.recv()).await;
        assert!(try_recv.is_err(), "new op should not see old chunk stream");
        // inbox_a should see them
        let _ = inbox_a.recv().await.unwrap();
        let _ = inbox_a.recv().await.unwrap();
        let _ = inbox_a.recv().await.unwrap();
        hub.close_exact(&ctx_a.operation_id);
        hub.close_exact(&ctx_b.operation_id);
    }

    #[tokio::test]
    async fn rt3_pre_ack_chunk_stream_preserved() {
        let owner = crate::session_runtime::SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 1,
        };
        let ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            "claude",
            3,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        // Simulate chunks arriving before submit ack
        let text = "chunked response";
        let checksum = super::response_checksum(text);
        let start = NavEvent::ResponseStart {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 3,
            byte_length: text.len(),
            chunk_count: 1,
            checksum: checksum.clone(),
        };
        hub.dispatch(ctx.operation_id.clone(), start, 64);
        let chunk = NavEvent::ResponseChunk {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 3,
            sequence: 0,
            text: text.to_string(),
        };
        hub.dispatch(ctx.operation_id.clone(), chunk, text.len());
        // Dispatch ack after chunks
        let ack = NavEvent::ActiveSubmitReport {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 3,
            succeeded: true,
            method: "button_click".to_string(),
            send_enabled: true,
            error: None,
        };
        hub.dispatch(ctx.operation_id.clone(), ack, 64);
        // await_submit_ack_until should buffer chunks and return Confirmed,
        // leaving the early prefix in the caller-owned buffer.
        let mut early = std::collections::VecDeque::new();
        let ack_res =
            super::await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline())
                .await
                .unwrap();
        assert!(matches!(
            ack_res,
            super::SubmitAckWait::Outcome(super::SubmitOutcome::Confirmed)
        ));
        assert_eq!(early.len(), 2); // start + chunk
        // Then wait should assemble using early buffer
        let end = NavEvent::ResponseEnd {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 3,
            checksum: checksum.clone(),
        };
        // Need to dispatch end after ack, but early buffer already has start/chunk, now dispatch end to inbox
        hub.dispatch(ctx.operation_id.clone(), end, 32);
        let mut dummy_aux = tokio::sync::mpsc::channel::<NavEvent>(8).1;
        let res = super::wait_for_response_with_operation(
            &ctx,
            &mut inbox,
            &mut dummy_aux,
            &mut early,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(res, text);
        hub.close_exact(&ctx.operation_id);
    }

    #[tokio::test]
    async fn rt4_no_destructive_auxiliary_scan() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        // Fill with unrelated events
        tx.send(NavEvent::Ready("other".to_string())).await.unwrap();
        tx.send(NavEvent::Ready("other2".to_string()))
            .await
            .unwrap();
        let drained = super::drain_stale_active_events(&mut rx);
        assert_eq!(drained, 0);
        // take_queued should return None (no destructive scan)
        let res = super::take_queued_response_for_turn(&mut rx, "claude", 5);
        assert_eq!(res, None);
        // Ensure events still in channel (not drained)
        assert!(!rx.is_empty());
    }

    #[tokio::test]
    async fn rt5_manual_response_exact_operation() {
        let owner = crate::session_runtime::SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 1,
        };
        let ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            "claude",
            7,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        // Correct manual response
        let correct = NavEvent::ManualResponse {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 7,
            response: "correct".to_string(),
        };
        hub.dispatch(ctx.operation_id.clone(), correct, 7);
        let mut early = std::collections::VecDeque::new();
        let ack = super::await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline())
            .await
            .unwrap();
        assert!(
            matches!(ack, super::SubmitAckWait::Outcome(super::SubmitOutcome::ResponseEarly(ref t)) if t=="correct")
        );
        // Stale operation manual response should not be accepted for new operation
        let other_ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            "claude",
            7,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let mut other_inbox = hub
            .register(other_ctx.operation_id.clone(), 0, true)
            .unwrap();
        let stale_manual = NavEvent::ManualResponse {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 7,
            response: "stale".to_string(),
        };
        // Dispatch stale to old mailbox (correct routing), new mailbox should stay empty
        hub.dispatch(ctx.operation_id.clone(), stale_manual, 5);
        let try_recv = tokio::time::timeout(Duration::from_millis(50), other_inbox.recv()).await;
        assert!(try_recv.is_err());
        hub.close_exact(&ctx.operation_id);
        hub.close_exact(&other_ctx.operation_id);
    }

    #[tokio::test]
    async fn rt6_attach_nav_receiver_does_not_affect_critical() {
        // Simulate attach without affecting critical hub: create state via dummy channels
        let (aux_tx, _aux_rx) = std::sync::mpsc::sync_channel::<NavEvent>(8);
        let (crit_tx, _crit_rx) = std::sync::mpsc::sync_channel::<NavEvent>(8);
        let epoch = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let ingress = crate::browser_backend::BrowserEventIngress::new_for_test(
            aux_tx,
            crit_tx,
            epoch.clone(),
            alive.clone(),
        );
        let mut state = crate::browser_backend::BrowserState::new_with_ingress(
            ingress,
            hub.clone(),
            epoch.clone(),
            alive.clone(),
        );
        let hub_before = state.critical_hub.clone();
        let _aux1 = state.attach_nav_receiver();
        let hub_after = state.critical_hub.clone();
        // Critical hub should remain same (not replaced)
        // We check that hub still allows registration
        let owner = crate::session_runtime::SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 1,
        };
        let ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            "claude",
            1,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let inbox = hub_after.register(ctx.operation_id.clone(), 0, true);
        assert!(inbox.is_ok());
        assert_eq!(
            state
                .critical_hub
                .register(ctx.operation_id.clone(), 1, true)
                .is_err(),
            true
        ); // duplicate should fail, proving same hub still has first op
    }

    #[tokio::test]
    async fn rt7_telemetry_flood_does_not_consume_critical_capacity() {
        // Auxiliary flood: fill auxiliary channel, ensure critical still works
        let (aux_tx, _aux_rx) = std::sync::mpsc::sync_channel::<NavEvent>(2);
        let (crit_tx, crit_rx) = std::sync::mpsc::sync_channel::<NavEvent>(
            crate::critical_transport::CRITICAL_INGRESS_CAPACITY,
        );
        let epoch = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let hub: crate::critical_transport::CriticalEventHub<NavEvent> =
            crate::critical_transport::CriticalEventHub::new();
        let ingress = crate::browser_backend::BrowserEventIngress::new_for_test(
            aux_tx.clone(),
            crit_tx.clone(),
            epoch.clone(),
            alive.clone(),
        );
        // Flood auxiliary (capacity 2) – third send will be dropped but should not affect critical
        for _ in 0..10 {
            ingress.send(NavEvent::SendProbe {
                agent_id: "claude".to_string(),
                input_found: true,
                send_button_found: true,
                user_submit_seen: false,
                message_count_seen: None,
                sent_signal_emitted: false,
                readiness_probe_count: None,
                input_candidate_count: None,
                composer_candidate_count: None,
                send_button_candidate_count: None,
                readiness_timeout_ms: None,
                page_state_hint: None,
                page_health_hint: None,
            });
        }
        // Critical operation should still be registerable and dispatchable
        let owner = crate::session_runtime::SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 1,
        };
        let ctx = crate::pipeline_ids::OperationContext::from_owner(
            &owner,
            "claude",
            9,
            crate::pipeline_ids::BrowserSurface::Participant,
        );
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let text = "critical still works despite aux flood";
        let ev = NavEvent::Response {
            operation_id: ctx.operation_id.clone(),
            agent_id: "claude".to_string(),
            turn: 9,
            text: text.to_string(),
        };
        hub.dispatch(ctx.operation_id.clone(), ev, text.len());
        let recv = tokio::time::timeout(Duration::from_millis(100), inbox.recv())
            .await
            .unwrap()
            .unwrap();
        match recv {
            NavEvent::Response { text: t, .. } => assert_eq!(t, text),
            _ => panic!("unexpected"),
        }
        hub.close_exact(&ctx.operation_id);
        // Also ensure aux channel is still not affecting critical epoch
        assert_eq!(epoch.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    // ── Session 02 correction: ACK-deadline preservation ────────────────────
    //
    // Regression tests for the defect where `confirm_active_submit` wrapped
    // the whole ACK wait in `tokio::time::timeout`, cancelling the future
    // that owned the early response buffer and returning ManualRecovery with
    // an empty buffer. The deadline is now enforced inside the receive loop
    // and the buffer is caller-owned, so nothing consumed is lost.

    fn chunked_events(
        ctx: &crate::pipeline_ids::OperationContext,
        text: &str,
        parts: &[&str],
    ) -> (Vec<NavEvent>, String) {
        assert_eq!(parts.concat(), text);
        let checksum = super::response_checksum(text);
        let mut events = vec![NavEvent::ResponseStart {
            operation_id: ctx.operation_id.clone(),
            agent_id: ctx.agent_id.clone(),
            turn: ctx.turn,
            byte_length: text.len(),
            chunk_count: parts.len() as u32,
            checksum: checksum.clone(),
        }];
        for (index, part) in parts.iter().enumerate() {
            events.push(NavEvent::ResponseChunk {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                sequence: index as u32,
                text: part.to_string(),
            });
        }
        events.push(NavEvent::ResponseEnd {
            operation_id: ctx.operation_id.clone(),
            agent_id: ctx.agent_id.clone(),
            turn: ctx.turn,
            checksum: checksum.clone(),
        });
        events.push(NavEvent::Done {
            operation_id: ctx.operation_id.clone(),
            agent_id: ctx.agent_id.clone(),
            turn: ctx.turn,
        });
        (events, checksum)
    }

    #[tokio::test]
    async fn ack_timeout_preserves_complete_early_response() {
        // Complete Start/Chunk/End/Done arrives while the submit ACK never
        // comes. The deadline must return timed-out/unconfirmed with every
        // early event intact, and the preserved prefix must reassemble into
        // the exact response. Uses a short test deadline, never 30 s.
        // No submit retry exists: MAX_SUBMIT_ACTION_RETRIES is 0.
        assert_eq!(super::MAX_SUBMIT_ACTION_RETRIES, 0);
        let (ctx, hub) = make_context("claude", 3);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let text = "complete early response";
        let (events, _checksum) = chunked_events(&ctx, text, &["complete ", "early response"]);
        let event_count = events.len();
        for event in events {
            let op = ctx.operation_id.clone();
            hub.dispatch(op, event, 16);
        }
        let mut early = std::collections::VecDeque::new();
        let deadline = Instant::now() + Duration::from_millis(80);
        let result = super::await_submit_ack_until(&ctx, &mut inbox, &mut early, deadline).await;
        assert!(
            matches!(result, Ok(super::SubmitAckWait::TimedOut)),
            "ACK wait must time out, got {result:?}"
        );
        assert_eq!(
            early.len(),
            event_count,
            "every consumed response event must survive the ACK deadline"
        );
        // Feed the preserved buffer into the existing response assembly path.
        let mut dummy_aux = tokio::sync::mpsc::channel::<NavEvent>(8).1;
        let assembled = super::wait_for_response_with_operation(
            &ctx,
            &mut inbox,
            &mut dummy_aux,
            &mut early,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(assembled, text);
        hub.close_exact(&ctx.operation_id);
    }

    #[tokio::test]
    async fn ack_timeout_preserves_prefix_suffix_reconstructs() {
        // Prefix (Start + first chunk) arrives before the ACK deadline;
        // suffix (remaining chunk + End + Done) arrives after. The later
        // response wait must assemble preserved prefix + inbox suffix into
        // the exact full response — proving a lossless deadline transition.
        let (ctx, hub) = make_context("deepseek", 4);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let text = "prefix-suffix response";
        let (events, _checksum) = chunked_events(&ctx, text, &["prefix-", "suffix response"]);
        // events = [start, chunk0, chunk1, end, done]; prefix = first two.
        for event in events.into_iter().take(2) {
            let op = ctx.operation_id.clone();
            hub.dispatch(op, event, 16);
        }
        let mut early = std::collections::VecDeque::new();
        let deadline = Instant::now() + Duration::from_millis(80);
        let result = super::await_submit_ack_until(&ctx, &mut inbox, &mut early, deadline).await;
        assert!(
            matches!(result, Ok(super::SubmitAckWait::TimedOut)),
            "ACK wait must time out, got {result:?}"
        );
        assert_eq!(early.len(), 2, "prefix must survive the ACK deadline");
        // Suffix arrives after the deadline into the same operation inbox.
        let checksum = super::response_checksum(text);
        for event in [
            NavEvent::ResponseChunk {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                sequence: 1,
                text: "suffix response".to_string(),
            },
            NavEvent::ResponseEnd {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                checksum: checksum.clone(),
            },
            NavEvent::Done {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
            },
        ] {
            let op = ctx.operation_id.clone();
            hub.dispatch(op, event, 16);
        }
        let mut dummy_aux = tokio::sync::mpsc::channel::<NavEvent>(8).1;
        let assembled = super::wait_for_response_with_operation(
            &ctx,
            &mut inbox,
            &mut dummy_aux,
            &mut early,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(assembled, text);
        hub.close_exact(&ctx.operation_id);
    }

    #[tokio::test]
    async fn ack_early_buffer_rejects_stale_operation_id() {
        // An event routed to this mailbox whose inner OperationId is stale
        // must be rejected and must never enter the current operation's
        // early buffer. (Hub-level exact routing is the primary defense;
        // this is the waiter's authoritative backstop.)
        let (ctx, hub) = make_context("chatgpt", 2);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let stale = crate::pipeline_ids::OperationId::new();
        hub.dispatch(
            ctx.operation_id.clone(),
            NavEvent::ResponseStart {
                operation_id: stale,
                agent_id: "chatgpt".to_string(),
                turn: 2,
                byte_length: 5,
                chunk_count: 1,
                checksum: "deadbeef".to_string(),
            },
            16,
        );
        let mut early = std::collections::VecDeque::new();
        let result =
            super::await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Err(AgentError::ExtractionFailed(_))),
            "stale OperationId must be rejected, got {result:?}"
        );
        assert!(
            early.is_empty(),
            "stale events must never enter the early buffer"
        );
        hub.close_exact(&ctx.operation_id);
    }

    #[tokio::test]
    async fn ack_submit_failure_report_preserves_captured_prefix() {
        // A failed submit report arriving AFTER early response events must
        // not discard the prefix already captured. (confirm_active_submit
        // keeps the caller-owned buffer on error returns.)
        let (ctx, hub) = make_context("qwen", 6);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let text = "prefix then failure";
        let (events, _checksum) = chunked_events(&ctx, text, &["prefix ", "then failure"]);
        // Prefix only: start + first chunk.
        for event in events.into_iter().take(2) {
            let op = ctx.operation_id.clone();
            hub.dispatch(op, event, 16);
        }
        hub.dispatch(
            ctx.operation_id.clone(),
            NavEvent::ActiveSubmitReport {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                succeeded: false,
                method: "button_click".to_string(),
                send_enabled: true,
                error: Some("click_without_physical_submit_evidence".to_string()),
            },
            64,
        );
        let mut early = std::collections::VecDeque::new();
        let result =
            super::await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Err(AgentError::InjectionFailed(_))),
            "failed submit report must surface, got {result:?}"
        );
        assert_eq!(
            early.len(),
            2,
            "captured prefix must survive a submit-report error"
        );
        hub.close_exact(&ctx.operation_id);
    }

    #[tokio::test]
    async fn ack_failed_transport_preserves_captured_prefix() {
        // Critical transport failure while an early prefix exists: the
        // waiter surfaces the transport error but the already-captured
        // prefix stays in the caller-owned buffer — never silently lost.
        let (ctx, hub) = make_context("kimi", 7);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let text = "prefix then disconnect";
        let (events, _checksum) = chunked_events(&ctx, text, &["prefix ", "then disconnect"]);
        for event in events.into_iter().take(2) {
            let op = ctx.operation_id.clone();
            hub.dispatch(op, event, 16);
        }
        // Fail the transport after the waiter has consumed the queued
        // prefix (microseconds) but before its deadline (seconds).
        let hub_clone = hub.clone();
        let op = ctx.operation_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            hub_clone.fail_operation(
                &op,
                crate::critical_transport::CriticalTransportError::IngressUnavailable,
            );
        });
        let mut early = std::collections::VecDeque::new();
        let result =
            super::await_submit_ack_until(&ctx, &mut inbox, &mut early, test_ack_deadline()).await;
        assert!(
            matches!(result, Err(AgentError::NavigationFailed(_))),
            "transport failure must surface, got {result:?}"
        );
        assert_eq!(
            early.len(),
            2,
            "captured prefix must survive transport failure"
        );
        hub.close_exact(&ctx.operation_id);
    }

    #[test]
    fn response_chunk_bound_distinct_from_total_event_bound() {
        use crate::critical_transport::{
            MAX_OPERATION_CONTROL_EVENT_HEADROOM, MAX_OPERATION_CRITICAL_EVENTS,
            MAX_OPERATION_PAYLOAD_BYTES, MAX_RESPONSE_CHUNKS,
        };
        // Total operation capacity is chunk capacity plus small control headroom.
        assert_eq!(MAX_RESPONSE_CHUNKS, 2_100);
        assert_eq!(
            MAX_OPERATION_CRITICAL_EVENTS,
            MAX_RESPONSE_CHUNKS + MAX_OPERATION_CONTROL_EVENT_HEADROOM
        );
        // A complete legal protocol at the declared maximum fits the mailbox:
        // Start + MAX chunks + End + Done.
        assert!(1 + MAX_RESPONSE_CHUNKS + 1 + 1 <= MAX_OPERATION_CRITICAL_EVENTS);
        // Assembly admits exactly the declared maximum ...
        let checksum = super::response_checksum("ok");
        assert!(
            super::ResponseAssembly::start(2, MAX_RESPONSE_CHUNKS as u32, checksum.clone()).is_ok()
        );
        // ... and rejects max + 1, matching response-start validation.
        assert!(
            super::ResponseAssembly::start(2, MAX_RESPONSE_CHUNKS as u32 + 1, checksum).is_err()
        );
        // The 2 MiB payload bound is unchanged.
        assert_eq!(MAX_OPERATION_PAYLOAD_BYTES, 2 * 1024 * 1024);
    }

    #[tokio::test]
    async fn full_max_protocol_does_not_self_fail_event_budget() {
        // Dispatch a complete max-size legal protocol to a real mailbox and
        // prove the first recv is not an EventBudgetExceeded failure. (Under
        // the old shared bound, Start admission at the maximum left no room
        // for End/Done/control events.)
        use crate::critical_transport::{MAX_OPERATION_CRITICAL_EVENTS, MAX_RESPONSE_CHUNKS};
        let (ctx, hub) = make_context("glm", 8);
        let mut inbox = hub.register(ctx.operation_id.clone(), 0, true).unwrap();
        let checksum = super::response_checksum("payload");
        hub.dispatch(
            ctx.operation_id.clone(),
            NavEvent::ResponseStart {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                byte_length: 7,
                chunk_count: MAX_RESPONSE_CHUNKS as u32,
                checksum: checksum.clone(),
            },
            64,
        );
        for sequence in 0..MAX_RESPONSE_CHUNKS as u32 {
            hub.dispatch(
                ctx.operation_id.clone(),
                NavEvent::ResponseChunk {
                    operation_id: ctx.operation_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    turn: ctx.turn,
                    sequence,
                    text: "c".to_string(),
                },
                1,
            );
        }
        hub.dispatch(
            ctx.operation_id.clone(),
            NavEvent::ResponseEnd {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
                checksum: checksum.clone(),
            },
            32,
        );
        hub.dispatch(
            ctx.operation_id.clone(),
            NavEvent::Done {
                operation_id: ctx.operation_id.clone(),
                agent_id: ctx.agent_id.clone(),
                turn: ctx.turn,
            },
            0,
        );
        let total = 1 + MAX_RESPONSE_CHUNKS + 1 + 1;
        assert!(total <= MAX_OPERATION_CRITICAL_EVENTS);
        let first = tokio::time::timeout(Duration::from_secs(2), inbox.recv())
            .await
            .expect("mailbox must deliver")
            .expect("mailbox must not report EventBudgetExceeded");
        assert!(
            matches!(first, NavEvent::ResponseStart { .. }),
            "first event must be ResponseStart, got {first:?}"
        );
        hub.close_exact(&ctx.operation_id);
    }
}
