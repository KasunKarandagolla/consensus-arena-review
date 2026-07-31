use std::sync::atomic::Ordering;
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri::Emitter;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;
use tokio::time::timeout;

use crate::agent_brain::{AgentBrain, AgentDecision};
use crate::blueprint_store::{BlueprintSection, SectionStatus};
use crate::browser_backend::NavEvent;
use crate::errors::{AgentError, ErrorKind};
use crate::memory_store::SessionSummaryData;
use crate::orchestrator::{AppState, ModelHealth, SessionConfig};

struct PendingAdoptionCheck {
    model_id: String,
    topic: String,
    prompt_excerpt: String,
}

#[derive(Default)]
struct PendingChunkResponse {
    expected: Option<u32>,
    received: HashMap<u32, String>,
    ended: bool,
}

fn try_assemble_chunks(pending: &PendingChunkResponse) -> Option<String> {
    let expected = pending.expected?;
    if !pending.ended || pending.received.len() != expected as usize { return None; }
    let mut response = String::new();
    for index in 0..expected { response.push_str(pending.received.get(&index)?); }
    (!response.is_empty()).then_some(response)
}

// ── Constants ─────────────────────────────────────────────────────────────────

const RESPONSE_TIMEOUT_SECS: u64 = 300;
const ACTIVE_SUBMIT_REPORT_TIMEOUT_SECS: u64 = 10;

/// IMP-2: Maximum number of retry attempts for participant injection.
/// Attempt 0 is the initial try; attempts 1–3 are retries with backoff.
const MAX_RETRIES: u32 = 3;

/// IMP-2: Exponential backoff base in seconds.
/// Attempt 1 → 2 s, attempt 2 → 4 s, attempt 3 → 8 s (all < 60 s cap).
const BACKOFF_BASE_SECS: u64 = 2;
const MAX_UNCLASSIFIED_CONTINUES: u32 = 1;
const MAX_NO_PROGRESS_DECISIONS: u32 = 5;

fn is_recoverable_active_failure(reason: &str) -> bool {
    matches!(
        reason,
        "prompt_integrity_failed"
            | "integrity_failed"
            | "active_submit_report_missing"
            | "automation_missing"
            | "input_not_found"
            | "composer_not_found"
            | "send_not_found"
            | "submit_helper_missing"
            | "enabled_send_button_not_found_after_retry"
            | "active_submit_failed"
            | "active_window_identity_mismatch"
    )
}

fn recoverable_reason_from_error_text(error: &str) -> Option<&'static str> {
    [
        "automation_missing",
        "active_window_identity_mismatch",
        "composer_not_found",
        "input_not_found",
        "prompt_integrity_failed",
        "submit_helper_missing",
        "send_not_found",
        "enabled_send_button_not_found_after_retry",
    ]
    .into_iter()
    .find(|reason| error.contains(reason))
}

async fn enter_manual_recovery(
    state: &AppState,
    app: &AppHandle,
    agent_id: &str,
    turn: u32,
    reason: &str,
) {
    let browser = state.browser_state.lock().await;
    browser.mark_active_manual_recovery(agent_id, turn, reason);
    let _ = app.emit("active-turn-state", serde_json::json!({
        "event": "active_manual_recovery", "agent_id": agent_id,
        "turn_number": turn, "error": reason,
    }));
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
    ["questions to consult another model", "consult another model", "consult deepseek", "risks", "simplification", "critique"]
        .iter()
        .any(|phrase| response.contains(phrase))
}

fn looks_like_blueprint(response: &str) -> bool {
    let response = response.to_ascii_lowercase();
    response.contains("blueprint")
        || (response.contains("mvp") && (response.contains("feature") || response.contains("scope")))
        || (response.contains("architecture") && response.contains("implementation"))
}

fn has_concrete_proposal_content(response: &str) -> bool {
    let text = response.trim().to_ascii_lowercase();
    if text.split_whitespace().count() < 12 {
        return false;
    }

    let meta_markers = [
        "i will",
        "i'll",
        "as leader",
        "i am ready",
        "i'm ready",
        "please provide the proposal",
        "please present the proposal",
        "awaiting proposal",
        "awaiting the proposal",
        "my role is",
        "for each proposal i will",
        "consult another model",
    ]
        .iter()
        .any(|phrase| text.contains(phrase));

    let concrete_groups: &[&[&str]] = &[
        &[
            "feature",
            "screen",
            "workflow",
            "user flow",
            "endpoint",
            "authentication",
        ],
        &[
            "architecture",
            "component",
            "service",
            "client",
            "server",
            "api",
        ],
        &[
            "storage",
            "database",
            "sqlite",
            "data model",
            "schema",
            "table",
        ],
        &[
            "implement",
            "implementation",
            "build",
            "validate",
            "deploy",
            "test",
        ],
        &[
            "constraint",
            "offline",
            "latency",
            "memory",
            "security",
            "mvp",
            "blueprint",
        ],
    ];
    let concrete_group_count = concrete_groups
        .iter()
        .filter(|group| group.iter().any(|phrase| text.contains(phrase)))
        .count();
    let structured = text.contains('\n')
        || text.contains(": ")
        || text.contains("- ")
        || text.contains("1.")
        || text.contains("2.");

    if meta_markers && concrete_group_count < 2 {
        return false;
    }
    concrete_group_count >= 2 || (concrete_group_count == 1 && structured && text.len() >= 180)
}

fn selected_initial_review_agent(config: &SessionConfig) -> Option<String> {
    config
        .agent_ids
        .iter()
        .find(|agent_id| *agent_id != &config.leader_agent_id)
        .cloned()
}

#[derive(Debug, Clone)]
struct InitialReviewState {
    required_agent: Option<String>,
    in_progress: bool,
    completed: bool,
}

impl InitialReviewState {
    fn new(config: &SessionConfig) -> Self {
        Self {
            required_agent: selected_initial_review_agent(config),
            in_progress: false,
            completed: false,
        }
    }

    fn requires_route(&self) -> bool {
        self.required_agent.is_some() && !self.in_progress && !self.completed
    }

    fn begin(&mut self) {
        self.in_progress = true;
    }

    fn mark_revision_captured(&mut self) {
        self.in_progress = false;
        self.completed = true;
    }
}

fn should_defer_blueprint_until_review(review: &InitialReviewState) -> bool {
    review.required_agent.is_some() && !review.completed
}

fn should_defer_complete_until_review(review: &InitialReviewState) -> bool {
    review.required_agent.is_some() && !review.completed
}

fn can_complete_session(review: &InitialReviewState, has_blueprint: bool) -> bool {
    has_blueprint && !should_defer_complete_until_review(review)
}

fn drain_stale_active_events(nav_rx: &mut Receiver<NavEvent>) -> usize {
    let mut drained = 0usize;
    while let Ok(event) = nav_rx.try_recv() {
        drained = drained.saturating_add(1);
        tracing::warn!("[ACTIVE] Drained pre-turn stale navigation event: {:?}", event);
    }
    drained
}

async fn inject_active_prompt(
    window: tauri::WebviewWindow,
    expected_window_label: &str,
    agent_id: &str,
    prompt: &str,
    turn: u32,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<bool, AgentError> {
    if window.label() != expected_window_label
        || app.get_webview_window(expected_window_label).is_none()
    {
        let error = format!("model window closed for {agent_id}");
        let browser = state.browser_state.lock().await;
        browser.mark_active_turn_failed(agent_id, turn, &error);
        let _ = app.emit("active-turn-state", serde_json::json!({
            "event": "active_window_closed", "agent_id": agent_id, "turn_number": turn,
        }));
        return Err(AgentError::NavigationFailed(error));
    }
    let assignment_ok = {
        let mut browser = state.browser_state.lock().await;
        browser.begin_active_turn(agent_id, turn);
        if !browser.ensure_active_window_assignment(agent_id, expected_window_label) {
            let reason = "active_window_identity_mismatch";
            browser.mark_active_manual_recovery(agent_id, turn, reason);
            false
        } else {
            true
        }
    };
    let _ = app.emit("active-turn-state", serde_json::json!({
        "event": "active_turn_started",
        "agent_id": agent_id,
        "turn_number": turn,
    }));
    if !assignment_ok {
        enter_manual_recovery(
            state,
            app,
            agent_id,
            turn,
            "active_window_identity_mismatch",
        )
        .await;
        return Ok(false);
    }
    if let Err(error) = crate::browser_backend::inject_to_window(
        window, agent_id, prompt, turn, nav_rx, false, true,
    )
    .await
    {
        let message = format!("Failed to inject active prompt for {agent_id}: {error}");
        let reason = [
            "automation_missing",
            "composer_not_found",
            "input_not_found",
            "prompt_integrity_failed",
            "submit_helper_missing",
            "send_not_found",
            "enabled_send_button_not_found_after_retry",
        ]
        .into_iter()
        .find(|reason| message.contains(reason))
        .unwrap_or("active_submit_failed");
        if is_recoverable_active_failure(reason) {
            enter_manual_recovery(state, app, agent_id, turn, reason).await;
            return Ok(false);
        } else {
            let mut browser = state.browser_state.lock().await;
            browser.mark_active_turn_failed(agent_id, turn, &message);
            browser.clear_active_turn(agent_id, turn);
            let _ = app.emit("active-turn-state", serde_json::json!({
                "event": "active_submit_failed",
                "agent_id": agent_id,
                "turn_number": turn,
                "error": reason,
            }));
        }
        return Err(AgentError::InjectionFailed(message));
    }
    {
        let browser = state.browser_state.lock().await;
        browser.mark_active_prompt_injected(agent_id, turn);
        browser.mark_active_waiting(agent_id, turn);
    }
    let _ = app.emit("active-turn-state", serde_json::json!({
        "event": "active_prompt_injected",
        "agent_id": agent_id,
        "turn_number": turn,
    }));
    let _ = app.emit("active-turn-state", serde_json::json!({
        "event": "active_waiting_for_response",
        "agent_id": agent_id,
        "turn_number": turn,
    }));
    Ok(true)
}

async fn finish_active_turn(state: &AppState, agent_id: &str, turn: u32) {
    let mut browser = state.browser_state.lock().await;
    browser.clear_active_turn(agent_id, turn);
}

async fn perform_initial_review(
    target_agent: &str,
    leader_response: &str,
    leader_id: &str,
    participant_turn: u32,
    leader_return_turn: u32,
    config: &SessionConfig,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<String, AgentError> {
    tracing::info!(
        event = "initial_review_route_started",
        target_agent_id = target_agent,
        "Routing the mandatory first concrete proposal review"
    );
    let _ = app.emit("boss-message", serde_json::json!({
        "text": format!(
            "Initial review started: {} is checking the leader proposal.",
            crate::browser_backend::display_name_for(target_agent)
        ),
        "message_type": "status"
    }));
    let _ = app.emit("agent-routing", serde_json::json!({
        "from_model": leader_id,
        "to_model": target_agent,
        "reason": "Mandatory initial participant review"
    }));

    let review_prompt = format!(
        "Review this leader proposal for risks, simplifications, missing MVP constraints, and implementation practicality. Return concise actionable critique for the leader.\n\nProject brief:\n{}\n\nLeader proposal:\n{}",
        config.project_brief, leader_response
    );
    let participant_response = inject_and_wait_with_retry(
        target_agent,
        &review_prompt,
        participant_turn,
        state,
        nav_rx,
        app,
    )
    .await?;
    tracing::info!(
        event = "initial_review_response_captured",
        target_agent_id = target_agent,
        "Captured mandatory participant critique"
    );

    let leader_window = {
        let browser = state.browser_state.lock().await;
        browser.leader_window.clone().ok_or_else(|| {
            AgentError::NavigationFailed(
                "leader window not initialised for initial review return".to_string(),
            )
        })?
    };
    let return_prompt = format!(
        "[Response from {}]\n{}\n\nIncorporate this critique and now produce the revised/final concise blueprint.",
        crate::browser_backend::display_name_for(target_agent),
        participant_response
    );
    let returned_automatically = inject_active_prompt(
        leader_window,
        crate::browser_backend::LEADER_WINDOW_LABEL,
        leader_id,
        &return_prompt,
        leader_return_turn,
        state,
        app,
        nav_rx,
    )
    .await?;
    if returned_automatically {
        tracing::info!(
            event = "initial_review_returned_to_leader",
            target_agent_id = target_agent,
            "Returned mandatory critique to the leader"
        );
    }
    Ok(participant_response)
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
    let mut iteration: u32 = 0;
    let mut pending_adoptions: Vec<PendingAdoptionCheck> = Vec::new();
    let mut models_consulted_since_last_section: Vec<String> = Vec::new();
    let mut iterations_since_last_section: u32 = 0;
    let mut blueprint_titles: Vec<String> = Vec::new();
    let mut routing_observations: Vec<String> = Vec::new();
    let deepseek_selected = config.agent_ids.iter().any(|id| id == "deepseek")
        && leader_id != "deepseek";
    let consult_deepseek_once = config.project_brief.to_ascii_lowercase().contains("consult deepseek once");
    let mut deepseek_consulted = false;
    let mut unclassified_count = 0u32;
    let mut initial_review = InitialReviewState::new(config);
    if let Some(required_agent) = initial_review.required_agent.as_deref() {
        tracing::info!(
            event = "initial_review_required",
            target_agent_id = required_agent,
            "A participant critique and leader revision are required before completion"
        );
        let _ = app.emit("boss-message", serde_json::json!({
            "text": format!(
                "Initial participant review required: {} will critique the first concrete leader proposal.",
                crate::browser_backend::display_name_for(required_agent)
            ),
            "message_type": "status"
        }));
    }

    // IMP-10: Once brain_fail_count >= 3, this flips to true permanently for
    // the rest of this session.  It never flips back — we keep using brain2.
    let mut use_secondary: bool = false;

    let drained = drain_stale_active_events(nav_rx);
    if drained > 0 {
        tracing::warn!("[ACTIVE] Drained {drained} setup-era events before turn 1");
    }
    let leader_window = {
        let browser = state.browser_state.lock().await;
        browser.leader_window.clone().ok_or_else(|| {
            AgentError::NavigationFailed("leader window not initialised for active turn 1".to_string())
        })?
    };
    let first_prompt = format!(
        "Consensus Arena active turn 1 (session {}).\n\nProject brief:\n{}\n\nConstraints: work as the panel leader, keep the first draft practical and concise, and identify decisions or questions worth consulting another model on. Produce the first short proposal/blueprint draft now. Do not answer only CONSENSUS on this active turn. Respond now with the requested draft/proposal.",
        config.session_id, config.project_brief
    );
    inject_active_prompt(
        leader_window,
        crate::browser_backend::LEADER_WINDOW_LABEL,
        &leader_id,
        &first_prompt,
        1,
        state,
        app,
        nav_rx,
    )
    .await?;
    let mut next_leader_turn: u32 = 2;

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
        let leader_response = loop {
            match wait_for_response(&leader_id, active_turn, nav_rx, state, app).await {
                Ok(response) => break response,
                Err(AgentError::Timeout(error)) => {
                    enter_manual_recovery(state, app, &leader_id, active_turn, "active_submit_failed").await;
                    let _ = app.emit("active-turn-state", serde_json::json!({
                        "event": "active_turn_timeout",
                        "agent_id": &leader_id,
                        "turn_number": active_turn,
                    }));
                    let _ = app.emit(
                        "boss-message",
                        serde_json::json!({
                            "text": format!("Leader response was not captured: {error}. Paste the visible response to continue, or wait for browser capture."),
                            "message_type": "status"
                        }),
                    );
                    // Keep the exact active turn registered so a manual response
                    // remains valid after a browser-capture timeout.
                }
                Err(error) => {
                    {
                        let mut browser = state.browser_state.lock().await;
                        browser.mark_active_turn_failed(&leader_id, active_turn, &error.to_string());
                        browser.clear_active_turn(&leader_id, active_turn);
                    }
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
        {
            let browser = state.browser_state.lock().await;
            browser.mark_active_response_captured(&leader_id, active_turn);
        }
        finish_active_turn(state, &leader_id, active_turn).await;

        let _ = app.emit("active-turn-state", serde_json::json!({
            "event": "active_response_captured",
            "agent_id": &leader_id,
            "turn_number": active_turn,
        }));

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

        if initial_review.in_progress && !initial_review.completed {
            initial_review.mark_revision_captured();
            tracing::info!(
                event = "initial_review_returned_to_leader",
                "A matching leader revision proves the critique return was completed"
            );
            tracing::info!(
                event = "initial_review_completed",
                "Participant critique was captured, returned, and revised by the leader"
            );
            let _ = app.emit("boss-message", serde_json::json!({
                "text": "Initial participant review completed. The leader revision is ready for final blueprint decisions.",
                "message_type": "status"
            }));
        }

        if initial_review.requires_route() {
            if !has_concrete_proposal_content(&leader_response) {
                tracing::info!(
                    event = "proposal_gate_reprompted_leader",
                    "Process-only leader output was not routed to a participant"
                );
                let _ = app.emit("boss-message", serde_json::json!({
                    "text": "Leader response was process-only; requesting the actual blueprint before the required participant review.",
                    "message_type": "status"
                }));
                let leader_window = {
                    let browser = state.browser_state.lock().await;
                    browser.leader_window.clone().ok_or_else(|| {
                        AgentError::NavigationFailed(
                            "leader window not initialised for proposal gate".to_string(),
                        )
                    })?
                };
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    "Produce the actual concise MVP blueprint now. Do not describe your role or process.",
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
                next_leader_turn = next_leader_turn.saturating_add(1);
                continue;
            }

            let required_agent = initial_review
                .required_agent
                .clone()
                .ok_or_else(|| {
                    AgentError::UnknownError(
                        "initial review target disappeared before routing".to_string(),
                    )
                })?;
            initial_review.begin();
            let _participant_response = perform_initial_review(
                &required_agent,
                &leader_response,
                &leader_id,
                iteration,
                next_leader_turn,
                config,
                state,
                app,
                nav_rx,
            )
            .await?;
            next_leader_turn = next_leader_turn.saturating_add(1);
            models_consulted_since_last_section.push(required_agent.clone());
            routing_observations.push(format!(
                "Mandatory initial review by {required_agent}"
            ));
            iterations_since_last_section = iterations_since_last_section.saturating_add(1);
            if required_agent == "deepseek" {
                deepseek_consulted = true;
            }
            let _ = app.emit("boss-message", serde_json::json!({
                "text": "Initial participant critique returned. Waiting for the matching leader revision before completion.",
                "message_type": "status"
            }));
            continue;
        }

        let context = format!(
            "Session iteration: {}\nSelected participant IDs: {}\nLeader ID: {}\nDeepSeek selected: {}\nDeepSeek consulted this session: {}\nProject brief requires DeepSeek once: {}\nInitial participant review completed: {}",
            iteration, config.agent_ids.join(", "), leader_id, deepseek_selected,
            deepseek_consulted, consult_deepseek_once, initial_review.completed,
        );
        let _ = app.emit("agent_brain_decision_started", serde_json::json!({
            "iteration": iteration,
            "response_length": leader_response.len(),
        }));

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
        let decision_result = if use_secondary {
            let guard = state.agent_brain_2.lock().await;
            if let Some(b2) = guard.as_ref() {
                b2.decide(&leader_response, &context, mem_ctx).await
            } else {
                drop(guard);
                // No secondary configured — fall back to primary for this iter.
                brain.decide(&leader_response, &context, mem_ctx).await
            }
        } else {
            brain.decide(&leader_response, &context, mem_ctx).await
        };

        let mut decision = match decision_result {
            Ok(d) => {
                // Reset consecutive failure counter on success.
                state.brain_fail_count.store(0, Ordering::SeqCst);
                d
            }
            Err(e) => {
                let count = state.brain_fail_count.fetch_add(1, Ordering::SeqCst) + 1;
                tracing::error!(
                    "[BRAIN] decide() failed (consecutive failures: {}): {}",
                    count,
                    e
                );
                unclassified_count = unclassified_count.saturating_add(1);
                let _ = app.emit("agent_brain_decision_failed", serde_json::json!({
                    "iteration": iteration,
                    "error": "decision parsing or provider request failed",
                    "unclassified_count": unclassified_count,
                }));
                if deepseek_selected && !deepseek_consulted
                    && (consult_deepseek_once || leader_requests_consultation(&leader_response))
                {
                    let _ = app.emit("agent_brain_decision_fallback", serde_json::json!({
                        "kind": "route", "target_agent_id": "deepseek", "unclassified_count": unclassified_count,
                    }));
                    AgentDecision::Route {
                        target_model: "deepseek".to_string(),
                        prompt: format!("Review the leader proposal for risks and simplifications. Return concise actionable critique.\n\nLeader proposal:\n{}", leader_response),
                    }
                } else if looks_like_blueprint(&leader_response) {
                    let _ = app.emit("agent_brain_decision_fallback", serde_json::json!({
                        "kind": "blueprint", "unclassified_count": unclassified_count,
                    }));
                    AgentDecision::Blueprint {
                        section_title: "Draft Blueprint".to_string(),
                        section_content: leader_response.clone(),
                    }
                } else if unclassified_count <= MAX_UNCLASSIFIED_CONTINUES {
                    let _ = app.emit("agent_brain_decision_fallback", serde_json::json!({
                        "kind": "continue", "unclassified_count": unclassified_count,
                    }));
                    AgentDecision::Continue
                } else {
                    let message = "Agent brain could not classify repeated unusable responses. Paste a clearer leader response or stop and restart the session.";
                    let _ = app.emit("boss-message", serde_json::json!({ "text": message, "message_type": "status" }));
                    return Err(AgentError::UnknownError(message.to_string()));
                }
            }
        };

        if deepseek_selected && !deepseek_consulted
            && (consult_deepseek_once || leader_requests_consultation(&leader_response))
            && !matches!(&decision, AgentDecision::Route { target_model, .. } if resolve_selected_agent_id(target_model, config).as_deref() == Some("deepseek"))
        {
            let _ = app.emit("agent_brain_decision_fallback", serde_json::json!({
                "kind": "route", "target_agent_id": "deepseek", "reason": "required_or_requested_consultation",
            }));
            decision = AgentDecision::Route {
                target_model: "deepseek".to_string(),
                prompt: format!("Review the leader proposal for risks, simplifications, and missing MVP constraints. Return concise actionable critique for the leader.\n\nLeader proposal:\n{}", leader_response),
            };
        }

        if matches!(&decision, AgentDecision::Route { .. } | AgentDecision::RouteCompare { .. })
            && !has_concrete_proposal_content(&leader_response)
        {
            // proposal_gate: role/process acknowledgements are not participant input.
            let _ = app.emit("boss-message", serde_json::json!({
                "text": "Leader response was process-only; requesting the actual blueprint before consultation.",
                "message_type": "status"
            }));
            let leader_window = {
                let browser = state.browser_state.lock().await;
                browser.leader_window.clone().ok_or_else(|| AgentError::NavigationFailed(
                    "leader window not initialised for proposal gate".to_string()
                ))?
            };
            inject_active_prompt(
                leader_window,
                crate::browser_backend::LEADER_WINDOW_LABEL,
                &leader_id,
                "Produce the actual concise MVP blueprint now. Do not describe your role or process. Include concrete features, architecture/data flow, and implementation requirements.",
                next_leader_turn,
                state,
                app,
                nav_rx,
            ).await?;
            next_leader_turn = next_leader_turn.saturating_add(1);
            continue;
        }

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
                let target_model = resolve_selected_agent_id(&target_model, config).ok_or_else(|| {
                    AgentError::NavigationFailed(format!(
                        "Agent brain selected an unavailable participant: {target_model}"
                    ))
                })?;
                let _ = app.emit("route_started", serde_json::json!({
                    "iteration": iteration,
                    "route_target_agent_id": &target_model,
                }));
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

                // IMP-2: inject_and_wait_with_retry handles window extraction,
                // navigation, injection, response wait, retries, and health updates.
                let participant_response = inject_and_wait_with_retry(
                    &target_model,
                    &format!(
                        "Consensus Arena participant active turn {iteration}. You are {}.\n\nProject brief:\n{}\n\nLeader proposal:\n{}\n\nRequested review:\n{}\n\nReturn concise risks, simplifications, and actionable critique for the leader.",
                        target_model, config.project_brief, leader_response, prompt
                    ),
                    iteration,
                    state,
                    nav_rx,
                    app,
                )
                .await
                .map_err(|e| {
                    AgentError::InjectionFailed(format!(
                        "Route to {} failed after retries: {}",
                        target_model, e
                    ))
                })?;
                if target_model == "deepseek" {
                    deepseek_consulted = true;
                }

                // Return response to leader window.
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
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    &return_prompt,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
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

                for target_model in &compare_models {
                    let _ = app.emit(
                        "agent-routing",
                        serde_json::json!({
                            "from_model": &leader_id,
                            "to_model":   target_model,
                            "reason":     "Route compare"
                        }),
                    );

                    // IMP-2: retry wrapper handles this participant.
                    let response = inject_and_wait_with_retry(
                        target_model,
                        &prompt,
                        iteration,
                        state,
                        nav_rx,
                        app,
                    )
                    .await
                    .map_err(|e| {
                        AgentError::InjectionFailed(format!(
                            "RouteCompare to {} failed after retries: {}",
                            target_model, e
                        ))
                    })?;

                    combined.push_str(&format!("[{} said]:\n{}\n\n", target_model, response));
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
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    &combined_msg,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Blueprint ─────────────────────────────────────────────────
            AgentDecision::Blueprint {
                section_title,
                section_content,
            } => {
                if should_defer_blueprint_until_review(&initial_review) {
                    tracing::info!(
                        event = "completion_deferred_until_initial_review",
                        decision = "blueprint",
                        "Holding candidate blueprint until participant critique and leader revision"
                    );
                    let _ = app.emit("boss-message", serde_json::json!({
                        "text": "Blueprint completion deferred until the required participant review and leader revision are complete.",
                        "message_type": "status"
                    }));
                    if has_concrete_proposal_content(&section_content) {
                        let required_agent = initial_review
                            .required_agent
                            .clone()
                            .ok_or_else(|| {
                                AgentError::UnknownError(
                                    "initial review target missing while deferring blueprint"
                                        .to_string(),
                                )
                            })?;
                        initial_review.begin();
                        let _participant_response = perform_initial_review(
                            &required_agent,
                            &section_content,
                            &leader_id,
                            iteration,
                            next_leader_turn,
                            config,
                            state,
                            app,
                            nav_rx,
                        )
                        .await?;
                        next_leader_turn = next_leader_turn.saturating_add(1);
                        if required_agent == "deepseek" {
                            deepseek_consulted = true;
                        }
                        let _ = app.emit("boss-message", serde_json::json!({
                            "text": "Deferred blueprint was reviewed. Waiting for the matching leader revision before recording it.",
                            "message_type": "status"
                        }));
                    } else {
                        let leader_window = {
                            let browser = state.browser_state.lock().await;
                            browser.leader_window.clone().ok_or_else(|| {
                                AgentError::NavigationFailed(
                                    "leader window not initialised for deferred blueprint"
                                        .to_string(),
                                )
                            })?
                        };
                        inject_active_prompt(
                            leader_window,
                            crate::browser_backend::LEADER_WINDOW_LABEL,
                            &leader_id,
                            "Produce the actual concise MVP blueprint now. Do not describe your role or process.",
                            next_leader_turn,
                            state,
                            app,
                            nav_rx,
                        )
                        .await?;
                        next_leader_turn = next_leader_turn.saturating_add(1);
                    }
                    continue;
                }

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
                let _ = app.emit("blueprint_emitted", serde_json::json!({
                    "iteration": iteration,
                    "section_title": &section_title,
                }));

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
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    ack,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Continue ──────────────────────────────────────────────────
            AgentDecision::Continue => {
                let continue_prompt = if iterations_since_last_section >= MAX_NO_PROGRESS_DECISIONS {
                    "No new blueprint section has been produced recently. Finalize one actionable blueprint section now, or ask the user for the missing decision."
                } else {
                    "Please continue."
                };
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
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    continue_prompt,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
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
                inject_active_prompt(
                    leader_window,
                    crate::browser_backend::LEADER_WINDOW_LABEL,
                    &leader_id,
                    &context_prompt,
                    next_leader_turn,
                    state,
                    app,
                    nav_rx,
                )
                .await?;
                next_leader_turn = next_leader_turn.saturating_add(1);
            }

            // ── Complete ──────────────────────────────────────────────────
            AgentDecision::Complete => {
                if should_defer_complete_until_review(&initial_review) {
                    tracing::info!(
                        event = "completion_deferred_until_initial_review",
                        decision = "complete",
                        "Complete was overridden until participant critique and leader revision"
                    );
                    let _ = app.emit("boss-message", serde_json::json!({
                        "text": "Completion deferred until the required participant review and leader revision are complete.",
                        "message_type": "status"
                    }));
                    if has_concrete_proposal_content(&leader_response) {
                        let required_agent = initial_review
                            .required_agent
                            .clone()
                            .ok_or_else(|| {
                                AgentError::UnknownError(
                                    "initial review target missing while deferring completion"
                                        .to_string(),
                                )
                            })?;
                        initial_review.begin();
                        let _participant_response = perform_initial_review(
                            &required_agent,
                            &leader_response,
                            &leader_id,
                            iteration,
                            next_leader_turn,
                            config,
                            state,
                            app,
                            nav_rx,
                        )
                        .await?;
                        next_leader_turn = next_leader_turn.saturating_add(1);
                        if required_agent == "deepseek" {
                            deepseek_consulted = true;
                        }
                        let _ = app.emit("boss-message", serde_json::json!({
                            "text": "Early completion was replaced by the required review. Waiting for the matching leader revision.",
                            "message_type": "status"
                        }));
                    } else {
                        let leader_window = {
                            let browser = state.browser_state.lock().await;
                            browser.leader_window.clone().ok_or_else(|| {
                                AgentError::NavigationFailed(
                                    "leader window not initialised for completion proposal gate"
                                        .to_string(),
                                )
                            })?
                        };
                        inject_active_prompt(
                            leader_window,
                            crate::browser_backend::LEADER_WINDOW_LABEL,
                            &leader_id,
                            "Produce the actual concise MVP blueprint now. Do not describe your role or process.",
                            next_leader_turn,
                            state,
                            app,
                            nav_rx,
                        )
                        .await?;
                        next_leader_turn = next_leader_turn.saturating_add(1);
                    }
                    continue;
                }

                if !can_complete_session(&initial_review, !blueprint_titles.is_empty()) {
                    let _ = app.emit("boss-message", serde_json::json!({
                        "text": "A final blueprint is required before completion. Asking the leader to finalize one.",
                        "message_type": "status"
                    }));
                    let leader_window = {
                        let browser = state.browser_state.lock().await;
                        browser.leader_window.clone().ok_or_else(|| {
                            AgentError::NavigationFailed("leader window not initialised".to_string())
                        })?
                    };
                    inject_active_prompt(
                        leader_window,
                        crate::browser_backend::LEADER_WINDOW_LABEL,
                        &leader_id,
                        "Before completing, produce at least one concise finalized blueprint section with a title and actionable content.",
                        next_leader_turn,
                        state,
                        app,
                        nav_rx,
                    ).await?;
                    next_leader_turn = next_leader_turn.saturating_add(1);
                    continue;
                }
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
    // Fast-fail if agent is already in cooldown — no retry, no injection.
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
    } // lock drops

    let mut last_err: Option<AgentError> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            // Exponential backoff: 2s, 4s, 8s — capped at 60 s.
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

            // Re-check cooldown after sleep.
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

        // Extract nav_window and resolve the saved conversation URL, falling
        // back to the model's validated base URL if setup did not save one.
        // Lock is scoped and released before any .await on inject_to_window.
        tracing::debug!(
            "[LOCK] acquiring browser_state for inject_and_wait_with_retry/{}",
            target_model
        );
        let nav_window_available = {
            let browser = state.browser_state.lock().await;
            browser.nav_window.as_ref().is_some_and(|window| {
                app.get_webview_window(window.label()).is_some()
            })
        };
        if !nav_window_available {
            let error = format!("model window closed for {target_model}");
            {
                let browser = state.browser_state.lock().await;
                browser.mark_active_turn_failed(target_model, turn, &error);
            }
            let _ = app.emit("active-turn-state", serde_json::json!({
                "event": "active_window_closed", "agent_id": target_model, "turn_number": turn,
            }));
            update_model_health(state, target_model, false, Some(error.clone())).await;
            return Err(AgentError::NavigationFailed(error));
        }
        let (nav_window, diagnostics, target_url, reuse_active_page, has_saved_conversation_url) = {
            let browser = state.browser_state.lock().await;
            let window = browser.nav_window.clone().ok_or_else(|| {
                AgentError::NavigationFailed(format!("model window closed for {target_model}"))
            })?;
            let saved_conversation_url = browser
                .conversation_urls
                .get(target_model)
                .and_then(|url| url.clone());
            let target_url = saved_conversation_url.clone()
                .or_else(|| {
                    crate::browser_backend::get_agent_config(target_model)
                        .map(|config| config.base_url.to_string())
                })
                .ok_or_else(|| {
                    AgentError::NavigationFailed(format!(
                        "unknown participant model: {target_model}"
                    ))
                })?;
            let diagnostics = browser.diagnostics.clone();
            let has_saved_conversation_url = saved_conversation_url.is_some();
            let reuse_active_page = diagnostics.can_reuse_for_active_injection(
                target_model,
                window.label(),
                has_saved_conversation_url,
            );
            (window, diagnostics, target_url, reuse_active_page, has_saved_conversation_url)
        }; // lock drops here
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

        if !reuse_active_page {
            if let Err(e) = crate::browser_backend::navigate_agent_window(
                app,
                &diagnostics,
                &nav_window,
                target_model,
                "nav",
                &target_url,
            ) {
            let kind = e.kind();
            if kind == ErrorKind::Permanent || attempt == MAX_RETRIES {
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
        } else {
            tracing::info!(
                "[INJECT] reusing composer-ready nav page for {} turn={}",
                target_model,
                turn
            );
        }

        // Attempt injection.
        let assignment_ok = {
            let mut browser = state.browser_state.lock().await;
            browser.begin_active_turn(target_model, turn);
            if !browser.ensure_active_window_assignment(
                target_model,
                crate::browser_backend::NAV_WINDOW_LABEL,
            ) {
                let reason = "active_window_identity_mismatch";
                browser.mark_active_manual_recovery(target_model, turn, reason);
                false
            } else {
                true
            }
        };
        let _ = app.emit("active-turn-state", serde_json::json!({
            "event": "active_turn_started",
            "agent_id": target_model,
            "turn_number": turn,
        }));
        let wait_ready = !reuse_active_page;
        let injection = if assignment_ok {
            crate::browser_backend::inject_to_window(
                nav_window.clone(),
                target_model,
                prompt,
                turn,
                nav_rx,
                wait_ready,
                true,
            )
            .await
        } else {
            Err(AgentError::InjectionFailed(
                "active_window_identity_mismatch".to_string(),
            ))
        };
        let injection = match injection {
            Err(error)
                if wait_ready
                    && matches!(&error, AgentError::Timeout(message) if message.contains("waiting for ready signal"))
                    && diagnostics.can_reuse_for_active_injection(
                        target_model,
                        nav_window.label(),
                        has_saved_conversation_url,
                    ) => {
                tracing::warn!(
                    "[INJECT] {} timed out waiting for a fresh ready signal; composer diagnostics permit one reuse retry",
                    target_model
                );
                crate::browser_backend::inject_to_window(
                    nav_window,
                    target_model,
                    prompt,
                    turn,
                    nav_rx,
                    false,
                    true,
                )
                .await
            }
            result => result,
        };
        match injection {
            Err(e)
                if recoverable_reason_from_error_text(&e.to_string()).is_some() =>
            {
                let reason = recoverable_reason_from_error_text(&e.to_string())
                    .unwrap_or("active_submit_failed");
                enter_manual_recovery(state, app, target_model, turn, reason).await;
                // Keep this exact turn alive and enter the normal response wait.
                // A manual browser Send or provide_manual_model_response can
                // still satisfy the participant turn.
            }
            Err(e) => {
                if matches!(&e, AgentError::Timeout(_)) {
                    enter_manual_recovery(state, app, target_model, turn, "active_submit_failed").await;
                } else {
                    let mut browser = state.browser_state.lock().await;
                    browser.mark_active_turn_failed(target_model, turn, &e.to_string());
                    browser.clear_active_turn(target_model, turn);
                }
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
                if kind == ErrorKind::Permanent || attempt == MAX_RETRIES {
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
                continue; // retry
            }
            Ok(()) => {
                let browser = state.browser_state.lock().await;
                browser.mark_active_prompt_injected(target_model, turn);
                browser.mark_active_waiting(target_model, turn);
                let _ = app.emit("active-turn-state", serde_json::json!({
                    "event": "active_prompt_injected",
                    "agent_id": target_model,
                    "turn_number": turn,
                }));
                let _ = app.emit("active-turn-state", serde_json::json!({
                    "event": "active_waiting_for_response",
                    "agent_id": target_model,
                    "turn_number": turn,
                }));
            }
        }

        // Keep waiting on this exact turn after a capture timeout. Re-injecting
        // would replace the recovery state and make a manual Send/response stale.
        loop {
            match wait_for_response(target_model, turn, nav_rx, state, app).await {
                Ok(response) => {
                    {
                        let browser = state.browser_state.lock().await;
                        browser.mark_active_response_captured(target_model, turn);
                    }
                    finish_active_turn(state, target_model, turn).await;
                    let _ = app.emit("active-turn-state", serde_json::json!({
                        "event": "active_response_captured",
                        "agent_id": target_model,
                        "turn_number": turn,
                    }));
                    let _ = app.emit("agent-message", serde_json::json!({
                        "agent_id": target_model,
                        "role": "participant",
                        "response": &response,
                        "tokens": 0,
                        "iteration": turn,
                        "source_type": "browser_or_manual"
                    }));
                    // IMP-5: Mark agent healthy on success.
                    update_model_health(state, target_model, true, None).await;
                    return Ok(response);
                }
                Err(e @ AgentError::Timeout(_)) => {
                    enter_manual_recovery(
                        state,
                        app,
                        target_model,
                        turn,
                        "active_submit_failed",
                    )
                    .await;
                    let _ = app.emit("active-turn-state", serde_json::json!({
                        "event": "active_turn_timeout",
                        "agent_id": target_model,
                        "turn_number": turn,
                    }));
                    let _ = app.emit("boss-message", serde_json::json!({
                        "text": format!(
                            "{} response was not captured: {}. Paste the visible response to continue, or wait for browser capture.",
                            crate::browser_backend::display_name_for(target_model),
                            e
                        ),
                        "message_type": "status"
                    }));
                }
                Err(e) => {
                    {
                        let mut browser = state.browser_state.lock().await;
                        browser.mark_active_turn_failed(target_model, turn, &e.to_string());
                        browser.clear_active_turn(target_model, turn);
                    }
                    if matches!(&e, AgentError::InjectionFailed(_)) {
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
                    if kind == ErrorKind::Permanent || attempt == MAX_RETRIES {
                        update_model_health(state, target_model, false, Some(e.to_string())).await;
                        return Err(e);
                    }
                    tracing::warn!(
                        "[RETRY] Wait for response from {} failed (attempt {}): {}",
                        target_model,
                        attempt,
                        e
                    );
                    last_err = Some(e);
                    break;
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
    state: &AppState,
    app: &AppHandle,
) -> Result<String, AgentError> {
    let deadline = Duration::from_secs(RESPONSE_TIMEOUT_SECS);
    let started = tokio::time::Instant::now();
    let mut submit_report_seen = false;
    let mut submit_report_missing_recorded = false;
    let mut chunks: HashMap<String, PendingChunkResponse> = HashMap::new();

    loop {
        let remaining = deadline.saturating_sub(started.elapsed());
        let poll = Duration::from_secs(1).min(remaining);
        match timeout(poll, nav_rx.recv()).await {
            Ok(Some(event)) => {
                // D-040 [NAV]
                tracing::debug!("[NAV] {:?}", event);
                match event {
                    NavEvent::ResponseStart { agent_id: ev_agent, turn: ev_turn, message_id, chunk_count }
                        if ev_agent == agent_id && ev_turn == turn => {
                        let pending = chunks.entry(message_id).or_default();
                        pending.expected = Some(chunk_count);
                        if let Some(response) = try_assemble_chunks(pending) { return Ok(response); }
                    }
                    NavEvent::ResponseChunk { agent_id: ev_agent, turn: ev_turn, message_id, index, text }
                        if ev_agent == agent_id && ev_turn == turn => {
                        let pending = chunks.entry(message_id).or_default();
                        pending.received.insert(index, text);
                        if let Some(response) = try_assemble_chunks(pending) { return Ok(response); }
                    }
                    NavEvent::ResponseEnd { agent_id: ev_agent, turn: ev_turn, message_id }
                        if ev_agent == agent_id && ev_turn == turn => {
                        let pending = chunks.entry(message_id.clone()).or_default();
                        pending.ended = true;
                        if let Some(response) = try_assemble_chunks(pending) {
                            chunks.remove(&message_id);
                            return Ok(response);
                        }
                    }
                    NavEvent::Response(ev_agent, ev_turn, text) => {
                        if ev_agent == agent_id && ev_turn == turn {
                            return Ok(text);
                        }
                    }
                    NavEvent::Done(ev_agent, ev_turn) => {
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
                        agent_id: ev_agent,
                        turn: ev_turn,
                        response,
                    } => {
                        if ev_agent == agent_id && ev_turn == turn {
                            return Ok(response);
                        }
                    }
                    NavEvent::ActiveSubmitReport {
                        agent_id: ev_agent,
                        turn: ev_turn,
                        succeeded: false,
                        method,
                        error,
                        ..
                    } if ev_agent == agent_id && ev_turn == turn => {
                        submit_report_seen = true;
                        // Browser diagnostics already emit active_submit_failed. Keep
                        // this exact turn live: the user can click Send manually and
                        // response capture/manual paste must still satisfy it.
                        tracing::warn!(
                            "[ACTIVE] auto-submit failed for {} turn {} via {}: {:?}; waiting for manual recovery",
                            ev_agent,
                            ev_turn,
                            method,
                            error
                        );
                        let reason = error.as_deref().unwrap_or(method.as_str());
                        if is_recoverable_active_failure(reason) || is_recoverable_active_failure(&method) {
                            enter_manual_recovery(state, app, agent_id, turn, reason).await;
                        }
                    }
                    NavEvent::ActiveSubmitReport { agent_id: ev_agent, turn: ev_turn, .. }
                        if ev_agent == agent_id && ev_turn == turn => {
                        submit_report_seen = true;
                    }
                    NavEvent::Error(ev_agent) => {
                        if ev_agent == agent_id {
                            return Err(AgentError::ExtractionFailed(format!(
                                "Agent {} reported an error",
                                ev_agent
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
                if !submit_report_seen
                    && !submit_report_missing_recorded
                    && started.elapsed() >= Duration::from_secs(ACTIVE_SUBMIT_REPORT_TIMEOUT_SECS)
                {
                    let error = "active_submit_report_missing";
                    enter_manual_recovery(state, app, agent_id, turn, error).await;
                    submit_report_missing_recorded = true;
                }
                if started.elapsed() >= deadline {
                    return Err(AgentError::Timeout(format!(
                        "Agent {} did not respond within {} seconds",
                        agent_id, RESPONSE_TIMEOUT_SECS
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InitialReviewState, PendingChunkResponse, can_complete_session,
        drain_stale_active_events, has_concrete_proposal_content,
        is_recoverable_active_failure,
        selected_initial_review_agent, should_defer_blueprint_until_review,
        should_defer_complete_until_review, try_assemble_chunks,
    };
    use crate::context_manager::SessionType;
    use crate::orchestrator::SessionConfig;

    fn two_model_config() -> SessionConfig {
        SessionConfig {
            session_id: "session".to_string(),
            project_brief: "Build a practical local-first task app".to_string(),
            session_type: SessionType::Mvp,
            agent_ids: vec!["chatgpt".to_string(), "deepseek".to_string()],
            leader_agent_id: "chatgpt".to_string(),
        }
    }

    #[test]
    fn proposal_gate_rejects_role_process_text_and_accepts_a_blueprint() {
        for process_only in [
            "I will act as Leader and await the proposal.",
            "As leader, I'm ready. Please provide the proposal and I will consult another model.",
            "For each proposal I will review the process, identify questions, and coordinate the panel.",
            "Awaiting proposal. I will provide a concise response once the participant is ready.",
        ] {
            assert!(
                !has_concrete_proposal_content(process_only),
                "process-only response was accepted: {process_only}"
            );
        }
        assert!(has_concrete_proposal_content(
            "MVP blueprint: store projects in SQLite, add a task feature, and implement a review flow."
        ));
        assert!(has_concrete_proposal_content(
            "Architecture: a React client calls a small Tauri command layer. Storage: SQLite tables for projects and tasks. Features: create, prioritize, and complete tasks offline. Implementation: validate inputs, add repository tests, and package one local desktop binary."
        ));
    }

    #[test]
    fn two_model_session_requires_first_non_leader_review() {
        let config = two_model_config();
        assert_eq!(
            selected_initial_review_agent(&config).as_deref(),
            Some("deepseek")
        );
        let review = InitialReviewState::new(&config);
        assert!(review.requires_route());
        assert!(should_defer_blueprint_until_review(&review));
        assert!(should_defer_complete_until_review(&review));
        assert!(!can_complete_session(&review, true));
    }

    #[test]
    fn process_only_first_leader_response_is_reprompted_not_routed() {
        let review = InitialReviewState::new(&two_model_config());
        let response =
            "As leader, I will coordinate the review process and await the proposal.";
        assert!(review.requires_route());
        assert!(!has_concrete_proposal_content(response));
    }

    #[test]
    fn captured_leader_revision_completes_the_initial_review_gate() {
        let mut review = InitialReviewState::new(&two_model_config());
        review.begin();
        assert!(review.in_progress);
        assert!(!review.completed);
        review.mark_revision_captured();
        assert!(!review.in_progress);
        assert!(review.completed);
        assert!(!should_defer_blueprint_until_review(&review));
        assert!(!should_defer_complete_until_review(&review));
        assert!(can_complete_session(&review, true));
        assert!(!can_complete_session(&review, false));
    }

    #[test]
    fn setup_era_events_are_drained_before_the_first_active_turn() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        assert!(
            tx.try_send(crate::browser_backend::NavEvent::SetupResponseObserved(
                "chatgpt".to_string(),
            ))
            .is_ok()
        );
        assert!(
            tx.try_send(crate::browser_backend::NavEvent::Response(
                "chatgpt".to_string(),
                0,
                "stale setup answer".to_string(),
            ))
            .is_ok()
        );
        assert!(
            tx.try_send(crate::browser_backend::NavEvent::Done(
                "chatgpt".to_string(),
                0,
            ))
            .is_ok()
        );
        assert_eq!(drain_stale_active_events(&mut rx), 3);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn active_submit_report_missing_is_recoverable() {
        assert!(is_recoverable_active_failure("active_submit_report_missing"));
        assert!(is_recoverable_active_failure("prompt_integrity_failed"));
        assert!(!is_recoverable_active_failure("window_closed"));
    }

    #[test]
    fn chunk_assembly_is_order_and_duplicate_safe() {
        let mut pending = PendingChunkResponse::default();
        // A retried chunk may arrive before its start signal.
        pending.received.insert(1, "world".to_string());
        pending.ended = true;
        assert!(try_assemble_chunks(&pending).is_none());

        pending.expected = Some(2);
        assert!(try_assemble_chunks(&pending).is_none());
        pending.received.insert(0, "hello ".to_string());
        assert_eq!(try_assemble_chunks(&pending).as_deref(), Some("hello world"));

        // Duplicate start/end/chunk signals leave the assembled response stable.
        pending.expected = Some(2);
        pending.ended = true;
        pending.received.insert(1, "world".to_string());
        assert_eq!(try_assemble_chunks(&pending).as_deref(), Some("hello world"));
    }
}
