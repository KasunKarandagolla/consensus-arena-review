use std::collections::HashSet;
use std::sync::atomic::Ordering;

use crate::agent_brain::AgentBrain;
use crate::browser_backend::{
    NavEvent, create_windows, ensure_nav_window, get_agent_config, navigate_agent_window, record_nav_event,
    record_setup_completion, resolve_participant,
};
use crate::settings_store::CustomParticipant;
use crate::context_manager::ContextManager;
use crate::errors::AgentError;
use crate::orchestrator::{AppState, OrchestratorStatus, SessionConfig, SessionType};
use crate::session_runner::{run_debate, run_setup};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

fn redact_diagnostic_text(value: &str) -> String {
    let mut redact_next = false;
    value
        .split_whitespace()
        .map(|part| {
            if redact_next {
                redact_next = false;
                return "[REDACTED]".to_string();
            }

            let lower = part.to_ascii_lowercase();
            if lower == "bearer" {
                redact_next = true;
                return part.to_string();
            }
            if lower.contains("api_key")
                || lower.contains("apikey")
                || lower.starts_with("sk-")
                || lower.starts_with("token-")
            {
                return "[REDACTED]".to_string();
            }

            let is_long_secret_like = part.len() >= 32
                && part.chars().any(|ch| ch.is_ascii_alphabetic())
                && part.chars().any(|ch| ch.is_ascii_digit())
                && !part.contains('/');
            if is_long_secret_like {
                "[REDACTED]".to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn settings_command_error(stage: &str, error: impl std::fmt::Display) -> String {
    let detail = redact_diagnostic_text(&error.to_string());
    let message = format!("{stage}: {detail}");
    tracing::error!("[SETTINGS] {message}");
    message
}

fn validate_brain_fields(base_url: &str, model: &str) -> Result<(), String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err("API base URL is required".to_string());
    }

    let parsed =
        reqwest::Url::parse(base_url).map_err(|e| format!("API base URL is invalid: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("API base URL must be an absolute http:// or https:// URL".to_string());
    }
    if model.trim().is_empty() {
        return Err("Model name is required".to_string());
    }
    Ok(())
}

/// P1: session participants are validated against the MERGED registry (the
/// seven built-ins plus any persisted custom participants). Built-in ids are
/// authoritative; a custom entry colliding with a built-in id would be
/// re-validated at save time, so here the merged resolver simply returns the
/// built-in.
fn validate_session_agents(
    agent_ids: &[String],
    leader_agent_id: &str,
    custom: &[CustomParticipant],
) -> Result<(), String> {
    if agent_ids.len() < 2 {
        return Err("Select at least two participants.".to_string());
    }

    let mut seen = HashSet::new();
    for agent_id in agent_ids {
        if !seen.insert(agent_id.clone()) {
            return Err(format!("Duplicate participant selected: {agent_id}"));
        }
        if resolve_participant(agent_id, custom).is_none() {
            return Err(format!("Unknown participant selected: {agent_id}"));
        }
    }

    if !seen.contains(leader_agent_id) {
        return Err(format!(
            "Selected leader must also be included in participants: {leader_agent_id}"
        ));
    }

    Ok(())
}

// ── Session management ────────────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub async fn start_session(
    project_brief: String,
    session_type: String,
    agent_ids: Vec<String>,
    leader_agent_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| settings_command_error("Failed to read custom participants", e))?;
    validate_session_agents(&agent_ids, &leader_agent_id, &custom)?;

    // IMP-3: Concurrency guard — prevent two sessions from running simultaneously.
    // compare_exchange(false → true): if already true, return error immediately.
    state
        .session_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| {
            "A session is already active. Use Stop to end it before starting a new one.".to_string()
        })?;

    let stype = match session_type.as_str() {
        "architecture" => SessionType::Architecture,
        "mvp" => SessionType::Mvp,
        "api" => SessionType::Api,
        "security" => SessionType::Security,
        _ => SessionType::Custom,
    };

    let config = SessionConfig {
        session_id: Uuid::new_v4().to_string(),
        project_brief: project_brief.clone(),
        session_type: stype.clone(),
        agent_ids: agent_ids.clone(),
        leader_agent_id: leader_agent_id.clone(),
    };
    let setup_order = config.setup_order();
    let setup_generation = state
        .setup_generation
        .fetch_add(1, Ordering::SeqCst)
        .saturating_add(1);

    tracing::info!(
        "[SETUP] generation={} session_id={} selected_leader_id={} selected_agent_ids={:?} setup_order={:?}",
        setup_generation,
        config.session_id,
        leader_agent_id,
        agent_ids,
        setup_order
    );

    // IMP-7: Record session start so recovery can find it on next launch.
    // session_complete = false until AgentDecision::Complete is reached.
    {
        let mut store = state.settings_store.lock().await;
        store
            .set("last_session_id", &config.session_id)
            .map_err(|e| e.to_string())?;
        store
            .set("session_complete", "false")
            .map_err(|e| e.to_string())?;
    }

    // IMP-10: Reset brain fail counter for the new session.
    state.brain_fail_count.store(0, Ordering::SeqCst);

    // Task 10 (HIGH-7): reset per-agent token counts at the session boundary.
    // Previously these accumulated across the entire app process lifetime —
    // a session starting now must not inherit counts from whatever ran before
    // it. token_budget stays a plain in-memory tokio::sync::Mutex (no rusqlite
    // involved), so this is a direct lock + call, not a db_helpers call.
    {
        let mut tb = state.token_budget.lock().await;
        tb.reset_all();
    }

    // Update orchestrator
    {
        let mut orch = state.orchestrator.lock().await;
        orch.status = OrchestratorStatus::Setup;
        orch.current_session = Some(config.clone());
        orch.current_iteration = 0;
    }

    // Reset context manager
    {
        let mut ctx = state.context_manager.lock().await;
        *ctx = ContextManager::new(project_brief, stype);
    }

    // Create transcript session.
    //
    // Task 9 (HIGH-5/HIGH-6): transcript_store is now Arc<std::sync::Mutex<_>>
    // (see orchestrator.rs) instead of Arc<tokio::sync::Mutex<_>>, so this
    // synchronous rusqlite write runs inside db_helpers::run_blocking — off
    // the async runtime thread, with retry/backoff on transient failure —
    // instead of directly on it via `.lock().await`.
    {
        let store = state.transcript_store.clone();
        let cfg = config.clone();
        crate::db_helpers::run_blocking(move || {
            let mut guard = store.lock().map_err(|_| {
                AgentError::DatabaseError("transcript store lock poisoned".to_string())
            })?;
            guard.create_session(&cfg)
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    // Build a fresh std::sync::mpsc channel and create windows.
    let (std_nav_tx, std_nav_rx) = std::sync::mpsc::sync_channel::<NavEvent>(256);
    let browser_diagnostics = {
        let mut browser = state.browser_state.lock().await;
        let new_browser = crate::browser_backend::BrowserState::new(std_nav_tx.clone());
        *browser = new_browser;
        if let Err(error) = create_windows(
            &app,
            &mut browser,
            &agent_ids,
            &leader_agent_id,
            &config.session_id,
            setup_generation,
            &setup_order,
            &custom,
        ) {
            state.session_active.store(false, Ordering::SeqCst);
            {
                let mut orch = state.orchestrator.lock().await;
                orch.status = OrchestratorStatus::Ended;
            }
            return Err(error.to_string());
        }
        browser.diagnostics.clone()
    };

    // Bridge std::sync::mpsc → tokio::sync::mpsc for async session runner.
    let (tokio_tx, tokio_rx) = tokio::sync::mpsc::channel::<NavEvent>(256);
    let bridge_app = app.clone();
    std::thread::spawn(move || {
        while let Ok(event) = std_nav_rx.recv() {
            record_nav_event(&bridge_app, &browser_diagnostics, &event);
            if tokio_tx.blocking_send(event).is_err() {
                break;
            }
        }
    });

    app.emit(
        "session-status",
        json!({
            "status": "setup",
            "session_id": config.session_id.clone(),
            "setup_generation": setup_generation,
            "selected_leader_id": leader_agent_id.clone(),
            "selected_agent_ids": agent_ids.clone(),
            "setup_order": setup_order.clone(),
        }),
    )
    .map_err(|e| e.to_string())?;

    // Clone all Arc fields so the spawned task owns them.
    let config_clone = config.clone();
    let app_clone = app.clone();

    let orch_clone = state.orchestrator.clone();
    let ts_clone = state.transcript_store.clone();
    let tb_clone = state.token_budget.clone();
    let sv_clone = state.session_vault.clone();
    let bs_clone = state.browser_state.clone();
    let ctx_clone = state.context_manager.clone();
    let bp_clone = state.blueprint_store.clone();
    let ss_clone = state.settings_store.clone();
    let ab_clone = state.agent_brain.clone();
    let aut_clone = state.ask_user_tx.clone();
    let ab2_clone = state.agent_brain_2.clone();
    // IMP-3 / IMP-5 / IMP-10: new fields
    let sa_clone = state.session_active.clone();
    let mh_clone = state.model_health.clone();
    let bfc_clone = state.brain_fail_count.clone();
    let mem_clone = state.memory_store.clone();
    let memory_health = state.last_memory_health.clone();
    let setup_generation_clone = state.setup_generation.clone();
    let active_brain_clone = state.active_brain.clone();

    tokio::spawn(async move {
        let state_ref = AppState {
            orchestrator: orch_clone.clone(),
            transcript_store: ts_clone,
            token_budget: tb_clone,
            session_vault: sv_clone,
            browser_state: bs_clone,
            context_manager: ctx_clone,
            blueprint_store: bp_clone,
            settings_store: ss_clone,
            agent_brain: ab_clone,
            ask_user_tx: aut_clone,
            agent_brain_2: ab2_clone,
            session_active: sa_clone.clone(),
            model_health: mh_clone,
            brain_fail_count: bfc_clone,
            memory_store: mem_clone,
            last_memory_health: memory_health,
            setup_generation: setup_generation_clone,
            active_brain: active_brain_clone,
        };

        let mut nav_rx = tokio_rx;

        // Browser readiness is not a terminal session failure. Keep the
        // windows/session/generation alive and wait for a focused retry.
        loop {
            match run_setup(&config_clone, &state_ref, &app_clone, &mut nav_rx).await {
                Ok(()) => break,
                Err(e) => {
                    let agent_id = {
                        let browser = state_ref.browser_state.lock().await;
                        browser.diagnostics.mark_setup_failed_recoverable()
                    };
                    let message = format!("Complete login/loading/security check in the model window, then retry setup. {}", e);
                    app_clone.emit("boss-message", json!({ "text": message, "message_type": "status" })).ok();
                    app_clone.emit("setup-agent-failed", json!({
                        "agent_id": agent_id,
                        "recoverable": true
                    })).ok();
                    match nav_rx.recv().await {
                        Some(NavEvent::ResumeRequested(_)) => continue,
                        Some(NavEvent::SetupManualConfirmed(agent_id)) => {
                            app_clone.emit("setup-agent-complete", json!({
                                "agent_id": agent_id,
                                "conversation_url": ""
                            })).ok();
                            continue;
                        }
                        Some(NavEvent::SessionAborted) | None => {
                            let mut orch = orch_clone.lock().await;
                            orch.status = OrchestratorStatus::Ended;
                            app_clone.emit("session-status", json!({ "status": "ended" })).ok();
                            sa_clone.store(false, Ordering::SeqCst);
                            return;
                        }
                        Some(_) => continue,
                    }
                }
            }
        }

        // Transition to running
        {
            let mut orch = state_ref.orchestrator.lock().await;
            orch.status = OrchestratorStatus::Running;
        }

        // IMP-3: sa_clone stays accessible after state_ref is moved into run_debate.
        // Debate / autonomous loop phase
        if let Err(e) = run_debate(config_clone, state_ref, app_clone.clone(), nav_rx).await {
            app_clone
                .emit(
                    "boss-message",
                    json!({
                        "text": format!("Debate error: {}", e),
                        "message_type": "status"
                    }),
                )
                .ok();
            {
                let mut orch = orch_clone.lock().await;
                orch.status = OrchestratorStatus::Ended;
            }
            app_clone
                .emit("session-status", json!({ "status": "ended" }))
                .ok();
        }

        // IMP-3: reset flag in ALL remaining exit paths — exit paths 2 and 3.
        // run_debate either completed successfully (Ok) or errored (Err);
        // either way the session loop is over.
        sa_clone.store(false, Ordering::SeqCst);
    });

    Ok(())
}

#[tauri::command]
pub async fn pause_session(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut orch = state.orchestrator.lock().await;
    orch.status = OrchestratorStatus::Paused;
    app.emit("session-status", json!({ "status": "paused" }))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_session(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut orch = state.orchestrator.lock().await;
    orch.status = OrchestratorStatus::Running;
    app.emit("session-status", json!({ "status": "running" }))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn abort_session(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // IMP-3: Reset session_active so a new session can be started.
    state.session_active.store(false, Ordering::SeqCst);

    {
        let mut ask = state.ask_user_tx.lock().await;
        *ask = None;
    }

    {
        let browser = state.browser_state.lock().await;
        let _ = browser.nav_tx.try_send(NavEvent::SessionAborted);
    }

    {
        let mut orch = state.orchestrator.lock().await;
        orch.status = OrchestratorStatus::Ended;
    }

    app.emit("session-status", json!({ "status": "ended" }))
        .map_err(|e| e.to_string())
}

// ── User interaction ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn user_input(text: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut ctx = state.context_manager.lock().await;
    ctx.set_pending_user_input(text);
    Ok(())
}

/// D-041: Deliver the user's answer from the AskUser popup to the suspended
/// run_agent_loop.  Uses take() to atomically remove the sender and prevent
/// any possibility of a double-send (RISK-ASKCHANNEL resolved).
/// Returns Err if no question is currently pending.
#[tauri::command]
pub async fn provide_user_answer(
    answer: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // take() atomically removes the sender and clears the Option.
    let tx = {
        let mut lock = state.ask_user_tx.lock().await;
        lock.take()
    }; // lock drops here

    match tx {
        Some(sender) => sender
            .send(answer)
            .map_err(|_| "Answer channel dropped — session may have ended".to_string()),
        None => Err("No pending ask_user question".to_string()),
    }
}

/// D-039/D-038 (ATOMIC): Construct AgentBrain first (validation). If that
/// succeeds, read any existing fallback config from settings and attach it.
/// Only then write to the DB. Only then update AppState.
/// On any failure the state is unchanged.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_agent_brain_config(
    api_key: String,
    base_url: String,
    model: String,
    system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    validate_brain_fields(&base_url, &model)
        .map_err(|e| settings_command_error("Agent brain validation failed", e))?;

    // STEP A: Construct primary brain first. Fail fast before any DB writes.
    let brain = AgentBrain::new(
        api_key.clone(),
        base_url.clone(),
        model.clone(),
        system_prompt.clone(),
    )
    .map_err(|e| settings_command_error("Agent brain validation failed", e))?;

    // STEP B: Read any existing fallback config so it can be attached to the
    // new brain instance. Lock is scoped — dropped before Step C.
    let (fb_key, fb_url, fb_model) = {
        let store = state.settings_store.lock().await;
        let k = store
            .get_fallback_api_key()
            .map_err(|e| settings_command_error("Failed to read fallback brain config", e))?
            .unwrap_or_default();
        let u = store
            .get_fallback_base_url()
            .map_err(|e| settings_command_error("Failed to read fallback brain config", e))?
            .unwrap_or_default();
        let m = store
            .get_fallback_model()
            .map_err(|e| settings_command_error("Failed to read fallback brain config", e))?
            .unwrap_or_default();
        (k, u, m)
    }; // settings_store lock drops here

    // Attach fallback if configured.
    let brain = if !fb_key.is_empty() && !fb_url.is_empty() && !fb_model.is_empty() {
        brain.with_fallback(fb_key, fb_url, fb_model)
    } else {
        brain
    };

    // STEP C: Persist primary config to DB.
    {
        let mut store = state.settings_store.lock().await;
        store
            .set("brain_api_key", &api_key)
            .map_err(|e| settings_command_error("Failed to save agent brain settings", e))?;
        store
            .set("brain_base_url", &base_url)
            .map_err(|e| settings_command_error("Failed to save agent brain settings", e))?;
        store
            .set("brain_model", &model)
            .map_err(|e| settings_command_error("Failed to save agent brain settings", e))?;
        store
            .set("brain_system_prompt", &system_prompt)
            .map_err(|e| settings_command_error("Failed to save agent brain settings", e))?;
    } // lock drops here

    // STEP D: Update live brain in AppState. Tokio's mutex lock is infallible,
    // so there is no error value to map as "Failed to update live agent brain".
    // Only reached if A–C all succeeded.
    {
        let mut brain_lock = state.agent_brain.lock().await;
        *brain_lock = Some(brain);
    }

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn setup_agent_sent(
    agent_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut browser = state.browser_state.lock().await;
    browser.pending_sends.insert(agent_id);
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn captcha_resolved(
    agent_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut browser = state.browser_state.lock().await;
    browser.captcha_resolved.insert(agent_id.clone());
    browser
        .nav_tx
        .try_send(NavEvent::ResumeRequested(agent_id))
        .map_err(|e| format!("Could not resume browser readiness wait: {e}"))?;
    Ok(())
}

/// Re-focus and re-probe the agent currently blocked in Phase 1 setup without
/// creating a new session or changing setup_order/setup_generation.
#[tauri::command(rename_all = "snake_case")]
pub async fn retry_setup_agent(
    agent_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let config = {
        let orchestrator = state.orchestrator.lock().await;
        orchestrator.current_session.clone()
    }.ok_or_else(|| "No setup session is active".to_string())?;
    if !config.agent_ids.iter().any(|id| id == &agent_id) {
        return Err("Agent is not part of the active setup".to_string());
    }
    // P2: resolve through the MERGED registry so a persisted custom participant
    // can be retried. Unknown ids keep the same rejection as before.
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .unwrap_or_default();
    let agent = resolve_participant(&agent_id, &custom)
        .ok_or_else(|| "Unknown setup agent".to_string())?;
    let (window, diagnostics, window_kind, nav_tx) = {
        let browser = state.browser_state.lock().await;
        let is_leader = agent_id == config.leader_agent_id;
        let window = browser.select_window(is_leader)
            .ok_or_else(|| "Model window is not available".to_string())?;
        (window, browser.diagnostics.clone(), if is_leader { "leader" } else { "nav" }, browser.nav_tx.clone())
    };
    navigate_agent_window(
        &app,
        &diagnostics,
        &window,
        &agent_id,
        window_kind,
        &agent.base_url,
    )
    .map_err(|error| error.to_string())?;
    nav_tx.try_send(NavEvent::ResumeRequested(agent_id))
        .map_err(|error| format!("Could not request setup retry: {error}"))?;
    Ok(())
}

/// User-confirmed recovery path for a prompt that was visibly sent or answered
/// but whose browser event was missed. This advances only the currently
/// expected setup agent; it neither clicks Send nor manufactures a browser
/// send/response signal.
#[tauri::command(rename_all = "snake_case")]
pub async fn confirm_setup_agent(
    agent_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if !state.session_active.load(Ordering::SeqCst) {
        return Err("No active setup session".to_string());
    }
    let setup_order = {
        let orchestrator = state.orchestrator.lock().await;
        let config = orchestrator
            .current_session
            .as_ref()
            .ok_or_else(|| "No setup session is active".to_string())?;
        if orchestrator.status != OrchestratorStatus::Setup {
            return Err("Manual confirmation is only available during setup".to_string());
        }
        config.setup_order()
    };
    if !setup_order.iter().any(|id| id == &agent_id) {
        return Err("Agent is not part of the active setup order".to_string());
    }
    let (diagnostics, nav_tx) = {
        let browser = state.browser_state.lock().await;
        if !browser.diagnostics.is_expected_unfinished(&agent_id) {
            return Err("Only the current unfinished setup agent can be confirmed".to_string());
        }
        (browser.diagnostics.clone(), browser.nav_tx.clone())
    };
    record_setup_completion(&diagnostics, &agent_id, "user_confirmed_manual");
    nav_tx
        .try_send(NavEvent::SetupManualConfirmed(agent_id))
        .map_err(|error| format!("Could not deliver manual setup confirmation: {error}"))?;
    Ok(())
}

/// User-confirmed active-turn recovery. The response is accepted only for the
/// exact agent and turn the autonomous loop is currently awaiting; it is
/// delivered as its own NavEvent rather than impersonating browser capture.
#[tauri::command(rename_all = "snake_case")]
pub async fn provide_manual_model_response(
    agent_id: String,
    turn_number: u32,
    response: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if !state.session_active.load(Ordering::SeqCst) {
        return Err("No active session".to_string());
    }
    if response.trim().is_empty() {
        return Err("Model response cannot be empty".to_string());
    }
    {
        let orchestrator = state.orchestrator.lock().await;
        if orchestrator.status != OrchestratorStatus::Running {
            return Err("Manual response is only available while a session is running".to_string());
        }
    }
    let nav_tx = {
        let browser = state.browser_state.lock().await;
        if browser.active_turn.as_ref() != Some(&(agent_id.clone(), turn_number)) {
            return Err("This model and turn are not currently awaiting a response".to_string());
        }
        browser.nav_tx.clone()
    };
    nav_tx
        .try_send(NavEvent::ManualResponse {
            agent_id,
            turn: turn_number,
            response,
        })
        .map_err(|error| format!("Could not deliver manual model response: {error}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn rate_limit_decision(
    agent_id: String,
    decision: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut orch = state.orchestrator.lock().await;
    orch.rate_limit_decisions.insert(agent_id.clone(), decision);
    app.emit(
        "rate-limit-reached",
        json!({ "agent_id": agent_id, "estimated_reset_mins": 5 }),
    )
    .map_err(|e| e.to_string())
}

// ── Settings & configuration ──────────────────────────────────────────────────

#[tauri::command]
pub async fn get_agent_brain_config(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let config = state
        .settings_store
        .lock()
        .await
        .get_agent_brain_config()
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// D-039: Save and activate the secondary (alternative) agent brain.
/// Follows the same ATOMIC pattern as save_agent_brain_config.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_secondary_brain_config(
    api_key: String,
    base_url: String,
    model: String,
    system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    validate_brain_fields(&base_url, &model)
        .map_err(|e| settings_command_error("Secondary brain validation failed", e))?;

    // STEP A: Validate by constructing brain2 first.
    let brain2 = AgentBrain::new(
        api_key.clone(),
        base_url.clone(),
        model.clone(),
        system_prompt.clone(),
    )
    .map_err(|e| settings_command_error("Secondary brain validation failed", e))?;

    // STEP B: Persist.
    {
        let mut store = state.settings_store.lock().await;
        store
            .set("brain2_api_key", &api_key)
            .map_err(|e| settings_command_error("Failed to save secondary brain settings", e))?;
        store
            .set("brain2_base_url", &base_url)
            .map_err(|e| settings_command_error("Failed to save secondary brain settings", e))?;
        store
            .set("brain2_model", &model)
            .map_err(|e| settings_command_error("Failed to save secondary brain settings", e))?;
        store
            .set("brain2_system_prompt", &system_prompt)
            .map_err(|e| settings_command_error("Failed to save secondary brain settings", e))?;
    }

    // STEP C: Update AppState.
    {
        let mut lock = state.agent_brain_2.lock().await;
        *lock = Some(brain2);
    }

    Ok(())
}

/// D-039: Return the secondary brain config as JSON.
#[tauri::command]
pub async fn get_secondary_brain_config(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let config = state
        .settings_store
        .lock()
        .await
        .get_secondary_brain_config()
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Task 5 (HIGH-3): D-038's fallback brain had storage keys and retry logic
/// fully implemented in settings_store.rs / agent_brain.rs, but no command
/// ever let the user write to those keys — this is the missing piece.
///
/// Same ATOMIC shape as save_agent_brain_config / save_secondary_brain_config:
/// persist to DB first, then — if a primary brain is already configured and
/// live — update it in place so the change takes effect immediately rather
/// than only on the next save_agent_brain_config call. Passing all three
/// fields empty clears the fallback (AgentBrain::without_fallback()).
#[tauri::command(rename_all = "snake_case")]
pub async fn save_fallback_brain_config(
    api_key: String,
    base_url: String,
    model: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let clears_fallback =
        api_key.trim().is_empty() && base_url.trim().is_empty() && model.trim().is_empty();
    if !clears_fallback {
        validate_brain_fields(&base_url, &model)
            .map_err(|e| settings_command_error("Fallback brain validation failed", e))?;
        if api_key.trim().is_empty() {
            return Err(settings_command_error(
                "Fallback brain validation failed",
                "API key is required",
            ));
        }
    }

    let config = crate::settings_store::FallbackBrainConfig {
        api_key: api_key.clone(),
        base_url: base_url.clone(),
        model: model.clone(),
    };

    // STEP A: Persist.
    {
        let mut store = state.settings_store.lock().await;
        store
            .save_fallback_brain_config(&config)
            .map_err(|e| settings_command_error("Failed to save fallback brain settings", e))?;
    }

    // STEP B: Keep a live primary brain in sync, if one is configured.
    {
        let mut brain_lock = state.agent_brain.lock().await;
        if let Some(existing) = brain_lock.take() {
            let updated = if !clears_fallback {
                existing.with_fallback(api_key, base_url, model)
            } else {
                existing.without_fallback()
            };
            *brain_lock = Some(updated);
        }
    }

    Ok(())
}

/// Task 5: return the fallback brain config as JSON, mirroring
/// get_secondary_brain_config's shape.
#[tauri::command]
pub async fn get_fallback_brain_config(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let config = state
        .settings_store
        .lock()
        .await
        .get_fallback_brain_config()
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// P1: return the persisted custom participants as a JSON array string.
/// Follows the IPC.json-string convention (callers JSON.parse the result).
#[tauri::command]
pub async fn get_custom_participants(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let participants = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| settings_command_error("Failed to read custom participants", e))?;
    serde_json::to_string(&participants).map_err(|e| e.to_string())
}

/// P3: return the UNIFIED participant registry (the seven immutable built-ins
/// in frozen order followed by persisted custom participants) as a JSON array
/// string, each entry tagged with `is_custom`. This is the single logical
/// participant list the frontend iterates for participant/leader selection,
/// connected accounts, sidebar model dots, and name resolution. Returns a
/// JSON string per the project's convention (callers JSON.parse the result).
#[tauri::command]
pub async fn get_participants(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| settings_command_error("Failed to read custom participants", e))?;
    serde_json::to_string(&crate::browser_backend::merged_participants(&custom))
        .map_err(|e| e.to_string())
}

/// P1: validate a single proposed custom participant. Built-in ids are
/// reserved; ids must be unique and URLs must be absolute HTTP(S).
fn validate_custom_participant(
    index: usize,
    participant: &CustomParticipant,
    reserved: &HashSet<String>,
) -> Result<(), String> {
    let label = format!("Custom participant {}", index + 1);
    let id = participant.agent_id.trim();
    if id.is_empty() || id.chars().any(|c| c.is_whitespace()) {
        return Err(settings_command_error(
            &label,
            "agent_id is required and must not contain whitespace",
        ));
    }
    let name = participant.display_name.trim();
    if name.is_empty() {
        return Err(settings_command_error(&label, "display_name is required"));
    }
    let base_url = participant.base_url.trim();
    let parsed = reqwest::Url::parse(base_url)
        .map_err(|e| settings_command_error(&label, format!("base_url is invalid: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(settings_command_error(
            &label,
            "base_url must be an absolute http:// or https:// URL",
        ));
    }
    // Built-in ids are always reserved; a custom save can never shadow or
    // redefine one (e.g. agent_id "deepseek" is refused at save time).
    if get_agent_config(id).is_some() {
        return Err(settings_command_error(
            &label,
            format!("agent_id '{id}' is reserved — it collides with a built-in participant"),
        ));
    }
    if reserved.contains(id) {
        return Err(settings_command_error(
            &label,
            format!("agent_id '{id}' is duplicated within the custom participant list"),
        ));
    }
    Ok(())
}

/// P1: persist the custom-participant list. Passing an empty list clears all
/// custom participants. Built-in ids may never be overridden.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_custom_participants(
    participants: Vec<CustomParticipant>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut reserved: HashSet<String> = HashSet::new();
    for (index, participant) in participants.iter().enumerate() {
        validate_custom_participant(index, participant, &reserved)?;
        reserved.insert(participant.agent_id.trim().to_string());
    }

    let mut store = state.settings_store.lock().await;
    store
        .save_custom_participants(&participants)
        .map_err(|e| settings_command_error("Failed to save custom participants", e))?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_prompt_template(
    template_name: String,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let key = match template_name.as_str() {
        "leader_priming" => "prompt_leader_priming",
        "participant_priming" => "prompt_participant_priming",
        "agent_system" => "brain_system_prompt",
        _ => return Err(format!("Unknown template name: {}", template_name)),
    };

    state
        .settings_store
        .lock()
        .await
        .set(key, &content)
        .map_err(|e| settings_command_error("Failed to save prompt template", e))?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_prompt_template(
    template_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let key = match template_name.as_str() {
        "leader_priming" => "prompt_leader_priming",
        "participant_priming" => "prompt_participant_priming",
        "agent_system" => "brain_system_prompt",
        _ => return Err(format!("Unknown template name: {}", template_name)),
    };

    Ok(state
        .settings_store
        .lock()
        .await
        .get(key)
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

// ── Data retrieval ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct DiagnosticSnapshot {
    app_data_dir: String,
    settings_db_exists: bool,
    memory_db_exists: bool,
    transcript_db_exists: bool,
    blueprint_db_exists: bool,
    session_active: bool,
    primary_agent_brain_configured: bool,
    fallback_brain_settings_present: bool,
    secondary_brain_configured: bool,
    memory_health: crate::memory_store::MemoryHealth,
    leader_window_exists: bool,
    nav_window_exists: bool,
    browser_diagnostics: Vec<crate::browser_backend::BrowserDiagnosticRecord>,
    browser_console_error_count: usize,
    browser_console_warning_count: usize,
    browser_console_last_error_at: Option<String>,
    // Harness extensions (spec 15)
    browser_timeline: Vec<crate::browser_harness::BrowserEvent>,
    browser_timeline_dropped: std::collections::HashMap<String, usize>,
    browser_timeline_count: usize,
    // Cross-platform forensics extensions (§4-9, §14)
    navigation_intents: Vec<crate::browser_harness::NavigationIntent>,
    lifecycle_events: Vec<crate::browser_harness::PageLifecycleEvent>,
    safe_dom_snapshots: Vec<crate::browser_harness::SafeDomForensics>,
    action_records: Vec<crate::browser_harness::ActionRecord>,
    // Recent failures + auth hints
    recent_failures: Vec<serde_json::Value>,
    command_timestamp: String,
}

/// Secret-free runtime snapshot for diagnosing configuration and persistence
/// failures. Configuration values are reduced to booleans before serialization.
#[tauri::command]
pub async fn get_diagnostic_snapshot(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| settings_command_error("Failed to resolve app data directory", e))?;

    let (primary_configured, fallback_present, secondary_configured) = {
        let store = state.settings_store.lock().await;
        let primary = store
            .get_agent_brain_config()
            .map_err(|e| settings_command_error("Failed to read diagnostic settings", e))?;
        let fallback = store
            .get_fallback_brain_config()
            .map_err(|e| settings_command_error("Failed to read diagnostic settings", e))?;
        let secondary = store
            .get_secondary_brain_config()
            .map_err(|e| settings_command_error("Failed to read diagnostic settings", e))?;

        (
            !primary.base_url.trim().is_empty() && !primary.model.trim().is_empty(),
            !fallback.api_key.trim().is_empty()
                && !fallback.base_url.trim().is_empty()
                && !fallback.model.trim().is_empty(),
            !secondary.base_url.trim().is_empty() && !secondary.model.trim().is_empty(),
        )
    };

    let memory_store = state.memory_store.clone();
    let memory_health = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .map_err(|_| AgentError::DatabaseError("memory store lock poisoned".to_string()))?;
        Ok(memory.check_health())
    })
    .await
    .map_err(|e| settings_command_error("Failed to read memory health", e))?;

    let (browser_diagnostics, browser_timeline, browser_timeline_dropped, browser_timeline_count, navigation_intents, lifecycle_events, safe_dom_snapshots, action_records, recent_failures) = {
        let browser = state.browser_state.lock().await;
        let diag = browser.diagnostics.snapshot();
        let tl = browser.diagnostics.timeline.all_events_sorted();
        let mut dropped: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in &diag {
            let d = browser.diagnostics.timeline.events_dropped(&r.agent_id);
            if d > 0 {
                dropped.insert(r.agent_id.clone(), d);
            }
        }
        let count = browser.diagnostics.timeline.total_events();
        let nav_intents = browser.diagnostics.navigation_intents.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let lifecycle = browser.diagnostics.lifecycle_events.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let dom = browser.diagnostics.safe_dom_snapshots.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let actions = browser.diagnostics.action_records.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let recent_failures = tl.iter().filter(|e| e.event_type.contains("failed") || e.event_type.contains("error") || e.event_type.contains("blocked") || e.event_type.contains("missing")).take(20).map(|e| serde_json::json!({ "timestamp": e.timestamp, "agent_id": e.agent_id, "event_type": e.event_type, "operation_id": e.operation_id, "url": e.url })).collect::<Vec<_>>();
        (diag, tl, dropped, count, nav_intents, lifecycle, dom, actions, recent_failures)
    };
    // Top-level console summary derived from per-agent vectors.
    let browser_console_error_count = browser_diagnostics
        .iter()
        .map(|r| r.browser_console_error_count as usize)
        .sum();
    let browser_console_warning_count = browser_diagnostics
        .iter()
        .map(|r| r.browser_console_warning_count as usize)
        .sum();
    let browser_console_last_error_at = browser_diagnostics
        .iter()
        .filter_map(|r| r.browser_console_last_error_at.clone())
        .max();

    let snapshot = DiagnosticSnapshot {
        app_data_dir: app_data_dir.to_string_lossy().into_owned(),
        settings_db_exists: app_data_dir.join("settings.db").is_file(),
        memory_db_exists: app_data_dir.join("memory.db").is_file(),
        transcript_db_exists: app_data_dir.join("transcript.db").is_file(),
        blueprint_db_exists: app_data_dir.join("blueprint.db").is_file(),
        session_active: state.session_active.load(Ordering::SeqCst),
        primary_agent_brain_configured: primary_configured,
        fallback_brain_settings_present: fallback_present,
        secondary_brain_configured: secondary_configured,
        memory_health,
        leader_window_exists: app
            .get_webview_window(crate::browser_backend::LEADER_WINDOW_LABEL)
            .is_some(),
        nav_window_exists: app
            .get_webview_window(crate::browser_backend::NAV_WINDOW_LABEL)
            .is_some(),
        browser_diagnostics,
        browser_console_error_count,
        browser_console_warning_count,
        browser_console_last_error_at,
        browser_timeline,
        browser_timeline_dropped,
        browser_timeline_count,
        navigation_intents,
        lifecycle_events,
        safe_dom_snapshots,
        action_records,
        recent_failures,
        command_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    serde_json::to_string(&snapshot)
        .map_err(|e| settings_command_error("Failed to serialize diagnostic snapshot", e))
}

/// Harness: return chronological timeline events as JSON string (spec 3-15)
#[tauri::command]
pub async fn get_browser_timeline(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let browser = state.browser_state.lock().await;
    let events = browser.diagnostics.timeline.all_events_sorted();
    serde_json::to_string(&events).map_err(|e| e.to_string())
}

/// Harness: human-readable reliability report markdown (spec 16)
#[tauri::command]
pub async fn get_browser_reliability_report(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let (timeline, diagnostics) = {
        let browser = state.browser_state.lock().await;
        (browser.diagnostics.timeline.clone(), browser.diagnostics.snapshot())
    };
    let md = crate::browser_harness::generate_reliability_report_markdown(&timeline, &diagnostics);
    Ok(md)
}

/// Harness: export bundle (spec 18) – writes BROWSER_RELIABILITY_REPORT.md + JSON files to app_data_dir/exports
#[tauri::command]
pub async fn export_browser_diagnostics(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let export_dir = app_data_dir.join(format!("diagnostics_export_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")));
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
    let (timeline, diagnostics, browser_diagnostics) = {
        let browser = state.browser_state.lock().await;
        (
            browser.diagnostics.timeline.all_events_sorted(),
            browser.diagnostics.snapshot(),
            browser.diagnostics.snapshot(),
        )
    };
    // events.json
    let events_path = export_dir.join("events.json");
    std::fs::write(&events_path, serde_json::to_string_pretty(&timeline).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // browser-diagnostics.json
    let diag_path = export_dir.join("browser-diagnostics.json");
    std::fs::write(&diag_path, serde_json::to_string_pretty(&browser_diagnostics).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // navigation-history.json
    let nav: Vec<_> = browser_diagnostics.iter().flat_map(|r| r.navigation_diagnostics.clone()).collect();
    let nav_path = export_dir.join("navigation-history.json");
    std::fs::write(&nav_path, serde_json::to_string_pretty(&nav).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // console-errors.json
    let console: Vec<_> = browser_diagnostics.iter().flat_map(|r| r.console_diagnostics.clone()).collect();
    let console_path = export_dir.join("console-errors.json");
    std::fs::write(&console_path, serde_json::to_string_pretty(&console).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // Cross-platform forensics: lifecycle, dom, actions, intents
    let (lifecycle, dom_snapshots, actions, intents, full_snapshot) = {
        let browser = state.browser_state.lock().await;
        let lc = browser.diagnostics.lifecycle_events.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let dom = browser.diagnostics.safe_dom_snapshots.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let acts = browser.diagnostics.action_records.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        let intents = browser.diagnostics.navigation_intents.lock().unwrap_or_else(|p| p.into_inner()).values().flat_map(|d| d.iter().cloned()).collect::<Vec<_>>();
        // Full diagnostic snapshot for forensic file
        let diag = browser.diagnostics.snapshot();
        let tl = browser.diagnostics.timeline.all_events_sorted();
        let snapshot = serde_json::json!({
            "browser_diagnostics": diag,
            "timeline": tl,
            "lifecycle_events": lc.clone(),
            "safe_dom_snapshots": dom.clone(),
            "action_records": acts.clone(),
            "navigation_intents": intents.clone(),
        });
        (lc, dom, acts, intents, snapshot)
    };
    let lifecycle_path = export_dir.join("lifecycle-events.json");
    std::fs::write(&lifecycle_path, serde_json::to_string_pretty(&lifecycle).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let dom_path = export_dir.join("safe-dom-snapshots.json");
    std::fs::write(&dom_path, serde_json::to_string_pretty(&dom_snapshots).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let actions_path = export_dir.join("action-records.json");
    std::fs::write(&actions_path, serde_json::to_string_pretty(&actions).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let intents_path = export_dir.join("navigation-intents.json");
    std::fs::write(&intents_path, serde_json::to_string_pretty(&intents).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let snapshot_path = export_dir.join("diagnostic-snapshot.json");
    std::fs::write(&snapshot_path, serde_json::to_string_pretty(&full_snapshot).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // BROWSER_RELIABILITY_REPORT.md
    let timeline_obj = {
        let browser = state.browser_state.lock().await;
        browser.diagnostics.timeline.clone()
    };
    let report = crate::browser_harness::generate_reliability_report_markdown(&timeline_obj, &diagnostics);
    let report_path = export_dir.join("BROWSER_RELIABILITY_REPORT.md");
    std::fs::write(&report_path, &report).map_err(|e| e.to_string())?;
    let result = serde_json::json!({
        "export_dir": export_dir.to_string_lossy(),
        "report": report_path.to_string_lossy(),
        "events": events_path.to_string_lossy(),
        "browser_diagnostics": diag_path.to_string_lossy(),
        "navigation_history": nav_path.to_string_lossy(),
        "console_errors": console_path.to_string_lossy(),
        "lifecycle_events": lifecycle_path.to_string_lossy(),
        "safe_dom_snapshots": dom_path.to_string_lossy(),
        "action_records": actions_path.to_string_lossy(),
        "navigation_intents": intents_path.to_string_lossy(),
        "diagnostic_snapshot": snapshot_path.to_string_lossy(),
    });
    Ok(serde_json::to_string(&result).map_err(|e| e.to_string())?)
}

/// Dev-only single-model diagnostic (spec 19) – probe one agent without full arena loop
#[tauri::command(rename_all = "snake_case")]
pub async fn run_single_model_diagnostic(
    agent_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    if state.session_active.load(Ordering::SeqCst) {
        return Err("Cannot run single-model diagnostic while a session is active. Stop the session first.".to_string());
    }
    let custom = state.settings_store.lock().await.get_custom_participants().map_err(|e| e.to_string())?;
    let participant = crate::browser_backend::resolve_participant(&agent_id, &custom).ok_or_else(|| format!("Unknown participant: {agent_id}"))?;
    let window = {
        let mut browser = state.browser_state.lock().await;
        match browser.nav_window.clone() {
            Some(w) => w,
            None => {
                if let Some(existing) = app.get_webview_window(crate::browser_backend::NAV_WINDOW_LABEL) {
                    browser.nav_window = Some(existing.clone());
                    existing
                } else {
                    crate::browser_backend::ensure_nav_window(&app, &mut browser).map_err(|e| e.to_string())?
                }
            }
        }
    };
    let diagnostics = {
        let browser = state.browser_state.lock().await;
        browser.diagnostics.clone()
    };
    let generation = diagnostics.setup_generation();
    let op = crate::browser_harness::operation_id_diagnostic_single(&agent_id, generation);
    diagnostics.set_operation(&agent_id, &op, "diagnostic");
    diagnostics.emit_harness_event(&agent_id, crate::browser_harness::EventType::WindowCreated, "diagnostic", &op, &participant.base_url, serde_json::json!({ "single_model": true }));

    // Navigate
    crate::browser_backend::navigate_agent_window(&app, &diagnostics, &window, &agent_id, "nav", &participant.base_url).map_err(|e| e.to_string())?;

    // Wait briefly for readiness probe (non-blocking, harness captures regardless)
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(15);
    let probe_snapshot = loop {
        if start.elapsed() > timeout {
            break serde_json::json!({ "timed_out": true, "elapsed_ms": start.elapsed().as_millis() });
        }
        // Check diagnostics for composer detection
        let diag = diagnostics.snapshot().into_iter().find(|r| r.agent_id == agent_id);
        if let Some(r) = diag {
            if r.input_found || r.send_button_found || r.last_ready_at.is_some() {
                break serde_json::json!({
                    "input_found": r.input_found,
                    "send_button_found": r.send_button_found,
                    "composer_candidate_count": r.composer_candidate_count,
                    "input_candidate_count": r.input_candidate_count,
                    "send_button_candidate_count": r.send_button_candidate_count,
                    "page_state_hint": r.page_state_hint,
                    "page_health_hint": r.page_health_hint,
                    "last_navigation_url": r.last_navigation_url,
                    "elapsed_ms": start.elapsed().as_millis()
                });
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    };

    diagnostics.emit_harness_event(&agent_id, crate::browser_harness::EventType::DomSnapshot, "diagnostic", &op, &participant.base_url, probe_snapshot.clone());

    let result = serde_json::json!({
        "agent_id": agent_id,
        "display_name": participant.display_name,
        "base_url": participant.base_url,
        "operation_id": op,
        "probe": probe_snapshot,
        "timeline_dropped": diagnostics.timeline.events_dropped(&agent_id),
        "note": "Diagnostic probe is dev-only and does not run the full arena loop; check timeline for detailed events"
    });
    Ok(serde_json::to_string(&result).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn get_transcript(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let session_id = {
        let orch = state.orchestrator.lock().await;
        orch.current_session
            .as_ref()
            .map(|s| s.session_id.clone())
            .unwrap_or_default()
    };
    if session_id.is_empty() {
        return Ok("[]".to_string());
    }

    // Task 9: transcript_store is now Arc<std::sync::Mutex<_>> — see
    // orchestrator.rs and db_helpers.rs for the full rationale.
    let store = state.transcript_store.clone();
    let sid = session_id.clone();
    let records = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        guard.get_transcript(&sid)
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(serde_json::to_string(&records).unwrap_or_default())
}

#[tauri::command]
pub async fn get_session_list(state: tauri::State<'_, AppState>) -> Result<String, String> {
    // Task 9: transcript_store is now Arc<std::sync::Mutex<_>>.
    let store = state.transcript_store.clone();
    let sessions = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        guard.list_sessions()
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(serde_json::to_string(&sessions).unwrap_or_default())
}

/// CRIT-6 (Task 8): Validate `format` against the IPC.md contract ('markdown'
/// | 'txt') before doing anything else — an unrecognised string now fails
/// loudly instead of silently falling through to plaintext.
///
/// HIGH-8 (Task 3): `session_id` is now an explicit, optional parameter.
/// Previously the backend silently ignored whatever `session_id` the
/// frontend sent and always derived it from the live orchestrator state —
/// so clicking "Export" on any *past* session in the sidebar history
/// actually exported whichever session was currently active, not the one
/// the user clicked. Sidebar.tsx already sends `session_id` on every export
/// call; this makes that value do something. Omitting it (or sending an
/// empty string) preserves the original "export the active session"
/// behaviour for any other caller (e.g. a download button inside an active
/// session view) that doesn't have a specific past session_id to give.
///
/// Task 9: blueprint_store is now Arc<std::sync::Mutex<_>>.
#[tauri::command(rename_all = "snake_case")]
pub async fn export_blueprint(
    format: String,
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    if format != "markdown" && format != "txt" {
        return Err(format!(
            "Invalid export format '{}'. Expected 'markdown' or 'txt'.",
            format
        ));
    }

    let resolved_session_id = match session_id {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            let orch = state.orchestrator.lock().await;
            orch.current_session
                .as_ref()
                .map(|s| s.session_id.clone())
                .unwrap_or_default()
        }
    };
    if resolved_session_id.is_empty() {
        return Err("No active session".to_string());
    }

    let store = state.blueprint_store.clone();
    let sid = resolved_session_id.clone();
    let fmt = format.clone();
    let content = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("blueprint store lock poisoned".to_string()))?;
        if fmt == "markdown" {
            guard.export_markdown(&sid)
        } else {
            guard.export_plaintext(&sid)
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let ext = if format == "markdown" { "md" } else { "txt" };
    let id_prefix_len = resolved_session_id.len().min(8);
    let filename = format!(
        "blueprint-{}.{}",
        &resolved_session_id[..id_prefix_len],
        ext
    );
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(&filename);

    std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// IMP-5: Return the real per-agent health map populated by run_agent_loop.
/// Serialises HashMap<String, ModelHealth> directly.
/// Returns empty JSON object ({}) before any session has run.
#[tauri::command]
pub async fn get_agent_health(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let health = state.model_health.lock().await;
    serde_json::to_string(&*health).map_err(|e| e.to_string())
}

// ── Task 3 (CRIT-3, CRIT-4): Session CRUD ─────────────────────────────────────
//
// get_session_list (above) is backed by TranscriptStore's `sessions` table —
// confirmed by direct read of transcript_store.rs, not assumed. These three
// commands are net-new; none existed anywhere in the backend before this
// batch, even though Sidebar.tsx already called all three.

/// Delete a session and cascade the deletion across every store that holds
/// session-scoped data: transcript turns + the session row itself
/// (TranscriptStore), blueprint sections (BlueprintStore), and saved
/// conversation URLs (SessionVault). Deliberately does NOT touch
/// SessionVault's `cookies` table — cookies are keyed by agent_id (the
/// user's login state with that model's website), not by session, and must
/// survive deleting any number of sessions.
///
/// Refuses to delete the session that is currently active (session_active
/// is true AND it's the orchestrator's current_session) — deleting state
/// out from under a running autonomous loop would corrupt that session's
/// in-flight writes, not just lose history.
#[tauri::command(rename_all = "snake_case")]
pub async fn delete_session(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if state.session_active.load(Ordering::SeqCst) {
        let orch = state.orchestrator.lock().await;
        if let Some(current) = orch.current_session.as_ref() {
            if current.session_id == session_id {
                return Err(
                    "Cannot delete the currently active session. Stop it first.".to_string()
                );
            }
        }
    }

    // Transcript store: turns + session row.
    {
        let store = state.transcript_store.clone();
        let sid = session_id.clone();
        crate::db_helpers::run_blocking(move || {
            let mut guard = store.lock().map_err(|_| {
                AgentError::DatabaseError("transcript store lock poisoned".to_string())
            })?;
            guard.delete_session(&sid)
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    // Blueprint store: sections. Zero sections is a normal outcome (e.g. a
    // session deleted before any section was agreed) — not an error.
    {
        let store = state.blueprint_store.clone();
        let sid = session_id.clone();
        crate::db_helpers::run_blocking(move || {
            let guard = store.lock().map_err(|_| {
                AgentError::DatabaseError("blueprint store lock poisoned".to_string())
            })?;
            guard.delete_session_sections(&sid)
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    // Session vault: saved conversation URLs only — cookies are untouched.
    {
        let vault = state.session_vault.clone();
        let sid = session_id.clone();
        crate::db_helpers::run_blocking(move || {
            let mut guard = vault.lock().map_err(|_| {
                AgentError::DatabaseError("session vault lock poisoned".to_string())
            })?;
            guard.delete_session_urls(&sid)
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Rename a session — updates `project_brief`, the same field Sidebar.tsx
/// already displays (truncated) as the session's title in the list. There is
/// no separate `title` column; adding one for text that would otherwise be
/// identical to `project_brief` would just create two sources of truth for
/// the same string.
#[tauri::command(rename_all = "snake_case")]
pub async fn rename_session(
    session_id: String,
    title: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Title cannot be empty".to_string());
    }
    let new_title = trimmed.to_string();

    let store = state.transcript_store.clone();
    let sid = session_id.clone();
    crate::db_helpers::run_blocking(move || {
        let mut guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        guard.rename_session(&sid, &new_title)
    })
    .await
    .map_err(|e| e.to_string())
}

/// JSON shape returned by get_session_details — strictly more than
/// get_session_list's SessionSummary: adds turn_count, section_count, and the
/// distinct set of agent_ids that actually participated, none of which
/// get_session_list computes (it returns the raw `sessions` table rows only).
#[derive(Serialize)]
struct SessionDetails {
    id: String,
    project_brief: String,
    session_type: String,
    status: String,
    created_at: i64,
    turn_count: usize,
    section_count: usize,
    agent_ids: Vec<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_session_details(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ts_store = state.transcript_store.clone();
    let sid = session_id.clone();
    let (summary, records) = crate::db_helpers::run_blocking(move || {
        let guard = ts_store
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let summary = guard
            .get_session(&sid)?
            .ok_or_else(|| AgentError::DatabaseError(format!("Session '{}' not found", sid)))?;
        let records = guard.get_transcript(&sid)?;
        Ok((summary, records))
    })
    .await
    .map_err(|e| e.to_string())?;

    let bp_store = state.blueprint_store.clone();
    let sid_for_bp = session_id.clone();
    let section_count = crate::db_helpers::run_blocking(move || {
        let guard = bp_store
            .lock()
            .map_err(|_| AgentError::DatabaseError("blueprint store lock poisoned".to_string()))?;
        Ok(guard.get_sections(&sid_for_bp)?.len())
    })
    .await
    .map_err(|e| e.to_string())?;

    let mut agent_ids: Vec<String> = records.iter().map(|r| r.agent_id.clone()).collect();
    agent_ids.sort();
    agent_ids.dedup();

    let details = SessionDetails {
        id: summary.id,
        project_brief: summary.project_brief,
        session_type: summary.session_type,
        status: summary.status,
        created_at: summary.created_at,
        turn_count: records.len(),
        section_count,
        agent_ids,
    };

    serde_json::to_string(&details).map_err(|e| e.to_string())
}

// ── IMP-7: Session recovery ───────────────────────────────────────────────────

/// IMP-7: Return whether an incomplete session exists that can be recovered.
/// The frontend calls this on startup to decide whether to offer recovery.
///
/// Returns JSON: { "available": bool, "session_id": string }
#[tauri::command]
pub async fn get_recovery_state(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = state.settings_store.lock().await;

    let session_id = store
        .get("last_session_id")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let session_complete = store
        .get("session_complete")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "true".to_string()); // default: nothing to recover

    // Available only if we have a session that did NOT reach Complete.
    let available = !session_id.is_empty() && session_complete == "false";

    serde_json::to_string(&json!({
        "available": available,
        "session_id": session_id
    }))
    .map_err(|e| e.to_string())
}

/// IMP-7: Re-emit blueprint-section-added for every section of the given
/// incomplete session.  Does NOT re-enter the autonomous loop — only replays
/// the already-agreed sections so the user can see the partial blueprint.
///
/// Task 9: blueprint_store is now Arc<std::sync::Mutex<_>>.
#[tauri::command(rename_all = "snake_case")]
pub async fn recover_session(
    session_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let store = state.blueprint_store.clone();
    let sid = session_id.clone();
    let sections = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("blueprint store lock poisoned".to_string()))?;
        guard.get_sections(&sid)
    })
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!(
        "[RECOVERY] Replaying {} sections for session {}",
        sections.len(),
        &session_id
    );

    for section in &sections {
        let _ = app.emit(
            "blueprint-section-added",
            json!({
                "section_id": &section.id,
                "title":      &section.title,
                "content":    &section.content
            }),
        );
    }

    Ok(())
}

// ── Launch Connected Account (reuses 2-WebView, no third window) ───────────

#[tauri::command(rename_all = "snake_case")]
pub async fn launch_connected_account(
    agent_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    if state.session_active.load(Ordering::SeqCst) {
        return Err("Cannot launch a model window while a session is active. Stop the session first.".to_string());
    }
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| format!("Failed to read custom participants: {e}"))?;
    let participant = resolve_participant(&agent_id, &custom)
        .ok_or_else(|| format!("Unknown participant: {agent_id}"))?;

    // Ensure nav window exists (creates one if no session has ever created windows).
    // This never creates a third window; it reuses the shared nav WebView.
    let window = {
        let mut browser = state.browser_state.lock().await;
        match browser.nav_window.clone() {
            Some(w) => w,
            None => {
                // Try to get from app if BrowserState lost it but window still exists
                if let Some(existing) = app.get_webview_window(crate::browser_backend::NAV_WINDOW_LABEL) {
                    browser.nav_window = Some(existing.clone());
                    existing
                } else {
                    // Create nav window anew via ensure helper
                    ensure_nav_window(&app, &mut browser)
                        .map_err(|e| format!("Failed to create model window: {e}"))?
                }
            }
        }
    };

    let diagnostics = {
        let browser = state.browser_state.lock().await;
        browser.diagnostics.clone()
    };

    navigate_agent_window(&app, &diagnostics, &window, &agent_id, "nav", &participant.base_url)
        .map_err(|e| e.to_string())?;

    // Make window visible so user can interact (login/inspect)
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn get_brain_status(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let status = state.active_brain.lock().await.clone();
    serde_json::to_string(&status).map_err(|e| e.to_string())
}

// ── Phase 1 memory ───────────────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub async fn get_project_memory(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let entries = crate::db_helpers::run_blocking(move || {
        let mut memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_project_memory(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_global_memory(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let entries = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_global_memory()
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn clear_project_memory(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.clear_project_memory(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_open_questions(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let questions = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_open_questions(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&questions).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_model_strengths(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let strengths = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_model_strengths(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&strengths).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_project_config(
    state: tauri::State<'_, AppState>,
    project_brief: String,
    content: String,
) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.save_project_config(&project_brief, &content)
    })
    .await
    .map_err(|e| settings_command_error("Failed to save Project Context", e))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_project_config(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_project_config(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_memory_health(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let health = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        Ok(memory.check_health())
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&health).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn repair_memory_index(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.repair_fts_index()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_patterns(
    state: tauri::State<'_, AppState>,
    project_brief: String,
) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let patterns = crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.get_patterns(&project_brief)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&patterns).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn export_memory(
    state: tauri::State<'_, AppState>,
    destination_path: String,
) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.export_to(&destination_path)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn restore_memory(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
    source_path: String,
) -> Result<(), String> {
    if state.session_active.load(Ordering::SeqCst) {
        return Err(
            "Cannot restore memory while a session is active. Stop the session first.".to_string(),
        );
    }

    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let backup_dir = app_data_dir.join("memory_backups");
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let pre_restore_path = backup_dir.join(format!("pre_restore_{timestamp}.db"));
    let memory_store = state.memory_store.clone();

    crate::db_helpers::run_blocking(move || {
        std::fs::create_dir_all(&backup_dir).map_err(|e| {
            AgentError::DatabaseError(format!("could not create memory backup directory: {e}"))
        })?;
        let pre_restore_path = pre_restore_path.to_string_lossy().into_owned();
        let mut memory = memory_store
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        memory.export_to(&pre_restore_path)?;
        memory.restore_from(&source_path)?;
        let health = memory.check_health();
        if !health.is_healthy {
            return Err(AgentError::DatabaseError(format!(
                "Restore completed but health check failed: {}. Pre-restore backup saved at {}",
                health.issues.join("; "),
                pre_restore_path
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_store::CustomParticipant;

    fn participant(agent_id: &str, name: &str, url: &str) -> CustomParticipant {
        CustomParticipant {
            agent_id: agent_id.to_string(),
            display_name: name.to_string(),
            base_url: url.to_string(),
        }
    }

    // P1: a custom participant may never reserve a built-in id.
    #[test]
    fn custom_save_rejects_builtin_id_collision() {
        let mut reserved = HashSet::new();
        let err = validate_custom_participant(
            0,
            &participant("deepseek", "Spoof", "https://spoof.example.com"),
            &reserved,
        )
        .unwrap_err();
        assert!(
            err.contains("reserved"),
            "expected a reserved-id error, got: {err}"
        );
        reserved.insert("deepseek".to_string());
    }

    // P1: duplicate ids within the same custom list are rejected.
    #[test]
    fn custom_save_rejects_duplicate_ids() {
        let mut reserved = HashSet::new();
        reserved.insert("acme".to_string());
        let err = validate_custom_participant(
            1,
            &participant("acme", "Acme", "https://acme.example.com"),
            &reserved,
        )
        .unwrap_err();
        assert!(
            err.contains("duplicated"),
            "expected a duplicate-id error, got: {err}"
        );
    }

    // P1: a valid absolute-URL custom id passes validation.
    #[test]
    fn custom_save_accepts_valid_participant() {
        let reserved = HashSet::new();
        let result = validate_custom_participant(
            0,
            &participant("acme", "Acme Bot", "https://acme.example.com"),
            &reserved,
        );
        assert!(result.is_ok(), "valid participant rejected: {result:?}");
    }

    // P1: session validation consumes the MERGED registry (built-in + custom).
    #[test]
    fn session_validation_accepts_merged_custom_participant() {
        let custom = vec![participant(
            "acme",
            "Acme Bot",
            "https://acme.example.com",
        )];
        let ids = vec!["chatgpt".to_string(), "acme".to_string()];
        let result = validate_session_agents(&ids, "chatgpt", &custom);
        assert!(result.is_ok(), "merged validation failed: {result:?}");
    }

    // P1: session validation still rejects a truly unknown id.
    #[test]
    fn session_validation_rejects_unknown_participant() {
        let custom: Vec<CustomParticipant> = vec![];
        let ids = vec!["chatgpt".to_string(), "does-not-exist".to_string()];
        let result = validate_session_agents(&ids, "chatgpt", &custom);
        assert!(
            result.is_err(),
            "unknown participant should be rejected"
        );
    }
}
