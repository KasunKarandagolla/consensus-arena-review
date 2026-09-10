use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::agent_brain::AgentBrain;
use crate::browser_backend::{
    NavEvent, create_windows, ensure_nav_window, get_agent_config, navigate_agent_window,
    record_setup_completion, resolve_participant,
};
use crate::context_manager::ContextManager;
use crate::errors::AgentError;
use crate::orchestrator::{AppState, OrchestratorStatus, SessionConfig, SessionType};
use crate::session_runner::{run_debate, run_setup};
use crate::settings_store::CustomParticipant;
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

async fn require_maintenance_enabled(state: &tauri::State<'_, AppState>) -> Result<(), String> {
    let enabled = state
        .settings_store
        .lock()
        .await
        .get_maintenance_mode()
        .map_err(|e| settings_command_error("Failed to read maintenance mode", e))?;
    if !enabled {
        return Err(
            "Diagnostic capture disabled — enable Maintenance mode to collect diagnostics."
                .to_string(),
        );
    }
    Ok(())
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
    // SessionRuntime ownership: acquire STARTING permit before any fallible setup.
    // The permit's Drop provides owner-checked rollback if we return before handoff.
    let start_permit = state
        .session_runtime
        .try_acquire_start(config.session_id.clone())
        .map_err(|e| e.to_string())?;
    let start_owner = start_permit.owner();

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

    // Attach this session to the process-lifetime navigation ingress. Named
    // WebViews keep their original callback sender and can be reused without
    // destroying authenticated browser state.
    let tokio_rx = {
        let mut browser = state.browser_state.lock().await;
        browser.reset_for_session();
        let nav_rx = browser.attach_nav_receiver();
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
            {
                let mut orch = state.orchestrator.lock().await;
                orch.status = OrchestratorStatus::Ended;
            }
            return Err(error.to_string());
        }
        nav_rx
    };

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
    // SessionRuntime ownership handles concurrency; former session_active/resuming removed
    let rt_clone = state.session_runtime.clone();
    let mh_clone = state.model_health.clone();
    let bfc_clone = state.brain_fail_count.clone();
    let mem_clone = state.memory_store.clone();
    let memory_health = state.last_memory_health.clone();
    let setup_generation_clone = state.setup_generation.clone();
    let active_brain_clone = state.active_brain.clone();
    // Hackathon
    let hk_run_clone = state.hackathon_run.clone();
    let hk_run_id_clone = state.hackathon_run_id.clone();
    let hk_cancel_clone = state.hackathon_cancel.clone();
    let pause_req_clone = state.pause_requested.clone();
    let checkpoint_clone = state.checkpoint.clone();

    let owner_for_task = start_owner.clone();
    let handle = tokio::spawn(async move {
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
            session_runtime: rt_clone.clone(),
            model_health: mh_clone,
            brain_fail_count: bfc_clone,
            memory_store: mem_clone,
            last_memory_health: memory_health,
            setup_generation: setup_generation_clone,
            active_brain: active_brain_clone,
            hackathon_run: hk_run_clone,
            hackathon_run_id: hk_run_id_clone,
            hackathon_cancel: hk_cancel_clone,
            pause_requested: pause_req_clone,
            checkpoint: checkpoint_clone,
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
                    let message = format!(
                        "Complete login/loading/security check in the model window, then retry setup. {}",
                        e
                    );
                    app_clone
                        .emit(
                            "boss-message",
                            json!({ "text": message, "message_type": "status" }),
                        )
                        .ok();
                    app_clone
                        .emit(
                            "setup-agent-failed",
                            json!({
                                "agent_id": agent_id,
                                "recoverable": true
                            }),
                        )
                        .ok();
                    match nav_rx.recv().await {
                        Some(NavEvent::ResumeRequested(_)) => continue,
                        Some(NavEvent::SetupManualConfirmed(agent_id)) => {
                            app_clone
                                .emit(
                                    "setup-agent-complete",
                                    json!({
                                        "agent_id": agent_id,
                                        "conversation_url": ""
                                    }),
                                )
                                .ok();
                            continue;
                        }
                        Some(NavEvent::SessionAborted) | None => {
                            // Owner-checked terminal cleanup: only the live owner may mutate status/runtime.
                            let is_owner = state_ref
                                .session_runtime
                                .current_owner()
                                .map(|o| o == owner_for_task)
                                .unwrap_or(false);
                            if is_owner {
                                let mut orch = orch_clone.lock().await;
                                orch.status = OrchestratorStatus::Ended;
                                app_clone
                                    .emit("session-status", json!({ "status": "ended" }))
                                    .ok();
                                state_ref.session_runtime.mark_completed(&owner_for_task);
                            }
                            return;
                        }
                        Some(_) => continue,
                    }
                }
            }
        }

        // Transition to running (owner-checked: only if still live owner)
        {
            let is_owner = state_ref
                .session_runtime
                .current_owner()
                .map(|o| o == owner_for_task)
                .unwrap_or(false);
            if is_owner {
                let mut orch = state_ref.orchestrator.lock().await;
                orch.status = OrchestratorStatus::Running;
            }
        }

        // Debate / autonomous loop phase — preserve runtime owner for terminal checks
        let runtime_for_terminal = state_ref.session_runtime.clone();
        let orch_for_terminal = orch_clone.clone();
        let owner_for_terminal = owner_for_task.clone();
        let debate_result =
            run_debate(config_clone.clone(), state_ref, app_clone.clone(), nav_rx).await;
        if let Err(e) = &debate_result {
            app_clone
                .emit(
                    "boss-message",
                    json!({
                        "text": format!("Debate error: {}", e),
                        "message_type": "status"
                    }),
                )
                .ok();
            // Owner-checked: only live owner may set orchestrator to Ended
            let is_owner = runtime_for_terminal
                .current_owner()
                .map(|o| o == owner_for_terminal)
                .unwrap_or(false);
            if is_owner {
                let mut orch = orch_for_terminal.lock().await;
                orch.status = OrchestratorStatus::Ended;
                app_clone
                    .emit("session-status", json!({ "status": "ended" }))
                    .ok();
            }
        }

        // Owner-checked terminal cleanup: only live owner may mark Finished.
        // If paused, keep runtime active so resume can continue; do not mark completed.
        {
            let orch = orch_for_terminal.lock().await;
            if orch.status != OrchestratorStatus::Paused {
                let is_owner = runtime_for_terminal
                    .current_owner()
                    .map(|o| o == owner_for_terminal)
                    .unwrap_or(false);
                if is_owner {
                    runtime_for_terminal.mark_completed(&owner_for_terminal);
                }
            }
        }
    });
    // Handoff ownership to SessionRuntime with task handle.
    // This must be owner-checked: if Stop won the race during STARTING, handoff is rejected and task aborted.
    if let Err(e) = start_permit.commit(handle) {
        tracing::warn!("[RUNTIME] start handoff rejected: {}", e);
        // Ensure orchestrator reflects ended if we never reached Running
        let mut orch = state.orchestrator.lock().await;
        if orch
            .current_session
            .as_ref()
            .map(|c| c.session_id == config.session_id)
            .unwrap_or(false)
        {
            orch.status = OrchestratorStatus::Ended;
        }
        let _ = app.emit("session-status", json!({ "status": "ended" }));
    }

    Ok(())
}

#[tauri::command]
pub async fn pause_session(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // Graceful pause requested — backend owns transition to Paused after checkpoint.
    state.pause_requested.store(true, Ordering::SeqCst);
    // Keep orchestrator status as Running until checkpoint persisted; response_router will transition to Paused.
    // Emit intermediate status so frontend can show "Pausing..."
    let _ = app.emit(
        "session-status",
        json!({ "status": "paused", "reason": "pause_requested" }),
    );
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn resume_session(
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // Determine target session_id: explicit param wins, else current orchestrator session
    let target_sid = if let Some(s) = session_id {
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            trimmed
        } else {
            let orch = state.orchestrator.lock().await;
            orch.current_session
                .as_ref()
                .map(|c| c.session_id.clone())
                .unwrap_or_default()
        }
    } else {
        let orch = state.orchestrator.lock().await;
        orch.current_session
            .as_ref()
            .map(|c| c.session_id.clone())
            .unwrap_or_default()
    };
    if target_sid.is_empty() {
        return Err("No session to resume — open a paused session first".to_string());
    }
    let cp_key = crate::checkpoint::SessionCheckpoint::key_for(&target_sid);
    let cp_json = {
        let store = state.settings_store.lock().await;
        store
            .get(&cp_key)
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    if cp_json.is_empty() {
        return Err("No checkpoint found for this session".to_string());
    }
    let cp: crate::checkpoint::SessionCheckpoint =
        serde_json::from_str(&cp_json).map_err(|e| format!("Checkpoint parse failed: {}", e))?;
    if let Err(e) = cp.validate() {
        return Err(format!("Checkpoint invalid: {}", e));
    }
    if cp.session_id != target_sid {
        return Err("Checkpoint does not belong to this session".to_string());
    }
    if !cp.paused {
        return Err("Session is not paused".to_string());
    }
    // In-process case: a live task owns the runtime
    let is_in_process = state.session_runtime.is_active();
    if is_in_process {
        let live_owner = state
            .session_runtime
            .current_owner()
            .ok_or_else(|| "No live session owner".to_string())?;
        if live_owner.session_id != target_sid {
            return Err(format!(
                "Cannot resume session {} while session {} is still active — stop the current session first",
                target_sid, live_owner.session_id
            ));
        }
        // Same-session in-process resume: no second task, just wake the paused loop.
        // Do not reconstruct orchestrator to a different session (owner check already passed).
        {
            let mut orch = state.orchestrator.lock().await;
            orch.status = OrchestratorStatus::Running;
        }
        state.pause_requested.store(false, Ordering::SeqCst);
        // If runtime was Paused, transition to Running
        state.session_runtime.mark_running(&live_owner);
        app.emit(
            "session-status",
            json!({ "status": "running", "session_id": cp.session_id.clone(), "resume_from": format!("{:?}", cp.next_step) }),
        )
        .map_err(|e| e.to_string())?;
        let _ = app.emit(
            "session-checkpoint",
            json!({ "checkpoint_id": cp.session_id, "phase": "resumed", "next_step": format!("{:?}", cp.next_step) }),
        );
        return Ok(());
    }
    // Restart case: no live owner — acquire fresh runtime ownership with new generation.
    if cp.agent_ids.is_empty() {
        return Err("Checkpoint missing session config — cannot reconstruct after restart (old checkpoint version)".to_string());
    }
    // Acquire resume ownership before any fallible window creation.
    let resume_permit = state
        .session_runtime
        .try_acquire_resume(target_sid.clone())
        .map_err(|e| e.to_string())?;
    let resume_owner = resume_permit.owner();
    // Reconstruct SessionConfig from checkpoint
    let stype = match cp.session_type.as_str() {
        "Architecture" => crate::orchestrator::SessionType::Architecture,
        "Mvp" => crate::orchestrator::SessionType::Mvp,
        "Api" => crate::orchestrator::SessionType::Api,
        "Security" => crate::orchestrator::SessionType::Security,
        _ => crate::orchestrator::SessionType::Custom,
    };
    let config = crate::orchestrator::SessionConfig {
        session_id: cp.session_id.clone(),
        project_brief: cp.project_brief.clone(),
        session_type: stype.clone(),
        agent_ids: cp.agent_ids.clone(),
        leader_agent_id: cp.leader_id.clone(),
    };
    let setup_order = config.setup_order();
    let setup_generation = state
        .setup_generation
        .fetch_add(1, Ordering::SeqCst)
        .saturating_add(1);
    // Custom participants for window creation
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .unwrap_or_default();
    // Validate agent_ids still known
    for aid in &config.agent_ids {
        if crate::browser_backend::resolve_participant(aid, &custom).is_none() {
            return Err(format!(
                "Cannot resume — participant {} no longer configured",
                aid
            ));
        }
    }
    // Reattach the resumed session to the same process-lifetime ingress and
    // reuse healthy named WebViews. Session-only routing state is reset.
    let tokio_rx = {
        let mut browser = state.browser_state.lock().await;
        browser.reset_for_session();
        let nav_rx = browser.attach_nav_receiver();
        if let Err(e) = crate::browser_backend::create_windows(
            &app,
            &mut browser,
            &config.agent_ids,
            &config.leader_agent_id,
            &config.session_id,
            setup_generation,
            &setup_order,
            &custom,
        ) {
            return Err(format!("Failed to recreate windows for resume: {}", e));
        }
        nav_rx
    };
    // Restore AppState fields
    {
        let mut orch = state.orchestrator.lock().await;
        orch.current_session = Some(config.clone());
        orch.status = OrchestratorStatus::Running;
        orch.current_iteration = cp.turn_number;
    }
    {
        let mut ctx = state.context_manager.lock().await;
        *ctx = crate::context_manager::ContextManager::new(cp.project_brief.clone(), stype);
        for msg in &cp.pending_user_messages {
            ctx.set_pending_user_input_for_session(msg.clone(), cp.session_id.clone());
        }
    }
    state.pause_requested.store(false, Ordering::SeqCst);
    // Persist that we are resuming (keep checkpoint for audit, not deleted)
    {
        let mut cached = state.checkpoint.lock().await;
        *cached = Some(cp.clone());
    }
    app.emit(
        "session-status",
        json!({
            "status": "running",
            "session_id": cp.session_id,
            "setup_generation": setup_generation,
            "selected_leader_id": cp.leader_id,
            "selected_agent_ids": cp.agent_ids,
            "setup_order": setup_order,
            "resume_from": format!("{:?}", cp.next_step)
        }),
    )
    .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "session-checkpoint",
        json!({ "checkpoint_id": cp.session_id, "phase": "resumed", "next_step": format!("{:?}", cp.next_step) }),
    );
    // Clone Arcs for spawned task
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
    let rt_clone = state.session_runtime.clone();
    let mh_clone = state.model_health.clone();
    let bfc_clone = state.brain_fail_count.clone();
    let mem_clone = state.memory_store.clone();
    let memory_health = state.last_memory_health.clone();
    let setup_gen_clone = state.setup_generation.clone();
    let active_brain_clone = state.active_brain.clone();
    let hk_run_clone = state.hackathon_run.clone();
    let hk_run_id_clone = state.hackathon_run_id.clone();
    let hk_cancel_clone = state.hackathon_cancel.clone();
    let pause_req_clone = state.pause_requested.clone();
    let checkpoint_clone = state.checkpoint.clone();
    // Spawn resumed loop — skip setup, go directly to debate loop
    let handle = tokio::spawn(async move {
        let state_ref = crate::orchestrator::AppState {
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
            session_runtime: rt_clone.clone(),
            model_health: mh_clone,
            brain_fail_count: bfc_clone,
            memory_store: mem_clone,
            last_memory_health: memory_health,
            setup_generation: setup_gen_clone,
            active_brain: active_brain_clone,
            hackathon_run: hk_run_clone,
            hackathon_run_id: hk_run_id_clone,
            hackathon_cancel: hk_cancel_clone,
            pause_requested: pause_req_clone,
            checkpoint: checkpoint_clone,
        };
        let runtime_for_terminal = state_ref.session_runtime.clone();
        let owner_for_terminal = resume_owner.clone();
        let orch_for_terminal = orch_clone.clone();
        let mut nav_rx = tokio_rx;
        // Clone brain out of lock before loop (DEF-001)
        let brain = {
            let guard = state_ref.agent_brain.lock().await;
            guard.clone()
        };
        // If no brain configured, we cannot run loop — emit error and pause again
        if brain.is_none() {
            let _ = app_clone.emit(
                "boss-message",
                serde_json::json!({"text":"Cannot resume — agent brain not configured","message_type":"status"}),
            );
            let is_owner = runtime_for_terminal
                .current_owner()
                .map(|o| o == owner_for_terminal)
                .unwrap_or(false);
            if is_owner {
                let mut orch = orch_clone.lock().await;
                orch.status = OrchestratorStatus::Paused;
                let _ = app_clone.emit(
                    "session-status",
                    serde_json::json!({"status":"paused","session_id": config_clone.session_id}),
                );
                runtime_for_terminal.mark_completed(&owner_for_terminal);
            }
            return;
        }
        if let Err(e) = crate::response_router::run_agent_loop(
            &config_clone,
            &brain.unwrap(),
            &state_ref,
            &app_clone,
            &mut nav_rx,
        )
        .await
        {
            let _ = app_clone.emit("boss-message", serde_json::json!({"text": format!("Resumed debate error: {}", e),"message_type":"status"}));
            let is_owner = runtime_for_terminal
                .current_owner()
                .map(|o| o == owner_for_terminal)
                .unwrap_or(false);
            if is_owner {
                let mut orch = orch_clone.lock().await;
                if orch.status != OrchestratorStatus::Paused {
                    orch.status = OrchestratorStatus::Ended;
                    let _ = app_clone.emit(
                        "session-status",
                        serde_json::json!({"status":"ended","session_id": config_clone.session_id}),
                    );
                }
            }
        }
        // Terminal cleanup: only live owner may mark completed
        {
            let orch = orch_for_terminal.lock().await;
            if orch.status != OrchestratorStatus::Paused {
                let is_owner = runtime_for_terminal
                    .current_owner()
                    .map(|o| o == owner_for_terminal)
                    .unwrap_or(false);
                if is_owner {
                    runtime_for_terminal.mark_completed(&owner_for_terminal);
                }
            }
        }
    });
    if let Err(e) = resume_permit.commit(handle) {
        tracing::warn!("[RUNTIME] resume handoff rejected: {}", e);
        let mut orch = state.orchestrator.lock().await;
        orch.status = OrchestratorStatus::Ended;
        let _ = app.emit("session-status", json!({ "status": "ended" }));
        return Err(e);
    }
    Ok(())
}

#[tauri::command]
pub async fn abort_session(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // Cooperative cancellation flags before runtime stop (no async lock held across await)
    state.hackathon_cancel.store(true, Ordering::SeqCst);
    {
        let run = state.hackathon_run.lock().await;
        if let Some(r) = run.as_ref() {
            r.cancelled.store(true, Ordering::SeqCst);
        }
    }

    {
        let mut ask = state.ask_user_tx.lock().await;
        *ask = None;
    }

    {
        let browser = state.browser_state.lock().await;
        let _ = browser.nav_tx.try_send(NavEvent::SessionAborted);
    }

    // SessionRuntime stop: abort owned task, await proof of termination, then Idle.
    // Do not hold any async lock across this await.
    let rt = state.session_runtime.clone();
    rt.stop().await.map_err(|e| e.to_string())?;

    state.pause_requested.store(false, Ordering::SeqCst);

    {
        let mut orch = state.orchestrator.lock().await;
        orch.status = OrchestratorStatus::Ended;
    }

    app.emit("session-status", json!({ "status": "ended" }))
        .map_err(|e| e.to_string())
}

// ── User interaction + Checkpoint ─────────────────────────────────────────

#[tauri::command]
pub async fn user_input(text: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if !state.session_runtime.is_active() {
        return Err("No active session — start a session before sending context".to_string());
    }
    let session_id_check = {
        let orch = state.orchestrator.lock().await;
        orch.current_session
            .as_ref()
            .map(|c| c.session_id.clone())
            .unwrap_or_default()
    };
    if session_id_check.is_empty() {
        return Err("No active session".to_string());
    }
    let mut ctx = state.context_manager.lock().await;
    ctx.set_pending_user_input_for_session(text, session_id_check.clone());
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn request_pause(
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let sid = if let Some(s) = session_id {
        if !s.trim().is_empty() {
            s.trim().to_string()
        } else {
            {
                let orch = state.orchestrator.lock().await;
                orch.current_session
                    .as_ref()
                    .map(|c| c.session_id.clone())
                    .unwrap_or_default()
            }
        }
    } else {
        {
            let orch = state.orchestrator.lock().await;
            orch.current_session
                .as_ref()
                .map(|c| c.session_id.clone())
                .unwrap_or_default()
        }
    };
    if sid.is_empty() {
        return Err("No active session to pause".to_string());
    }
    state.pause_requested.store(true, Ordering::SeqCst);
    // Build checkpoint at safe boundary: include session config for post-restart reconstruction
    let (leader_id, pending, agent_ids, project_brief, session_type) = {
        let orch = state.orchestrator.lock().await;
        let cfg = orch.current_session.clone();
        let lid = cfg
            .as_ref()
            .map(|c| c.leader_agent_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let aids = cfg
            .as_ref()
            .map(|c| c.agent_ids.clone())
            .unwrap_or_default();
        let pbrief = cfg
            .as_ref()
            .map(|c| c.project_brief.clone())
            .unwrap_or_default();
        let stype = cfg
            .as_ref()
            .map(|c| format!("{:?}", c.session_type))
            .unwrap_or_default();
        let pending = {
            let ctx = state.context_manager.lock().await;
            ctx.pending_user_input
                .clone()
                .map(|s| vec![s])
                .unwrap_or_default()
        };
        let pbrief2 = if pbrief.is_empty() {
            let ctx = state.context_manager.lock().await;
            ctx.project_brief.clone()
        } else {
            pbrief
        };
        (lid, pending, aids, pbrief2, stype)
    };
    let (hackathon_run_id, hackathon_task_brief) = {
        let run = state.hackathon_run.lock().await;
        if let Some(r) = run.as_ref() {
            (Some(r.run_id.clone()), Some(r.task_brief.clone()))
        } else {
            let id = state.hackathon_run_id.lock().await.clone();
            (id, None)
        }
    };
    let cp = crate::checkpoint::SessionCheckpoint {
        checkpoint_version: crate::checkpoint::CHECKPOINT_VERSION,
        session_id: sid.clone(),
        run_id: format!("run-{}", &sid[..sid.len().min(8)]),
        turn_number: {
            let orch = state.orchestrator.lock().await;
            orch.current_iteration
        },
        phase: "leader_decision".to_string(),
        leader_id: leader_id.clone(),
        target_participant: None,
        next_step: crate::checkpoint::CheckpointNextStep::LeaderDecision,
        pending_user_messages: pending,
        pause_requested: true,
        paused: true,
        pause_reason: crate::checkpoint::PauseReason::UserRequested,
        created_at: chrono::Utc::now().to_rfc3339(),
        agent_ids,
        project_brief,
        session_type,
        hackathon_run_id,
        hackathon_task_brief,
        last_leader_response: None,
    };
    cp.validate()
        .map_err(|e| format!("Checkpoint invalid: {}", e))?;
    let key = crate::checkpoint::SessionCheckpoint::key_for(&sid);
    let json = serde_json::to_string(&cp).map_err(|e| e.to_string())?;
    {
        let mut store = state.settings_store.lock().await;
        store
            .set(&key, &json)
            .map_err(|e| settings_command_error("Failed to persist checkpoint", e))?;
    }
    {
        let mut cached = state.checkpoint.lock().await;
        *cached = Some(cp.clone());
    }
    {
        let mut orch = state.orchestrator.lock().await;
        orch.status = OrchestratorStatus::Paused;
    }
    let _ = app.emit(
        "session-status",
        json!({ "status": "paused", "session_id": sid, "checkpoint_id": sid }),
    );
    let _ = app.emit("session-checkpoint", json!({ "checkpoint_id": sid, "phase": "paused", "session_id": sid, "next_step": "leader_decision" }));
    Ok(json)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_session_checkpoint(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let trimmed = session_id.trim().to_string();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let key = crate::checkpoint::SessionCheckpoint::key_for(&trimmed);
    let val = {
        let store = state.settings_store.lock().await;
        store
            .get(&key)
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    if val.is_empty() {
        return Ok("null".to_string());
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&val).map_err(|e| format!("Checkpoint parse failed: {}", e))?;
    if let Some(v) = parsed.get("checkpoint_version") {
        if v.as_u64() != Some(crate::checkpoint::CHECKPOINT_VERSION as u64) {
            return Err(format!("Unsupported checkpoint version {}", v));
        }
    }
    Ok(val)
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
    }
    .ok_or_else(|| "No setup session is active".to_string())?;
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
    let agent =
        resolve_participant(&agent_id, &custom).ok_or_else(|| "Unknown setup agent".to_string())?;
    let (window, diagnostics, window_kind, nav_tx) = {
        let browser = state.browser_state.lock().await;
        let is_leader = agent_id == config.leader_agent_id;
        let window = browser
            .select_window(is_leader)
            .ok_or_else(|| "Model window is not available".to_string())?;
        (
            window,
            browser.diagnostics.clone(),
            if is_leader { "leader" } else { "nav" },
            browser.nav_tx.clone(),
        )
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
    nav_tx
        .try_send(NavEvent::ResumeRequested(agent_id))
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
    if !state.session_runtime.is_active() {
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
    if !state.session_runtime.is_active() {
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
pub async fn get_custom_participants(state: tauri::State<'_, AppState>) -> Result<String, String> {
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
        .get_prompt_template_with_default(key)
        .map_err(|e| e.to_string())?)
}

// ── Maintenance mode (Diagnostics gate) ─────────────────────────────────────

#[tauri::command]
pub async fn get_maintenance_mode(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let enabled = state
        .settings_store
        .lock()
        .await
        .get_maintenance_mode()
        .map_err(|e| e.to_string())?;
    // Return JSON boolean string per IPC convention (callers JSON.parse it)
    serde_json::to_string(&enabled).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_maintenance_mode(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings_store
        .lock()
        .await
        .set_maintenance_mode(enabled)
        .map_err(|e| settings_command_error("Failed to save maintenance mode", e))
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
    // W1-D: minimal Windows WebView2 environment
    webview_version: Option<String>,
    os: String,
    arch: String,
    tauri_version: String,
    command_timestamp: String,
}

/// Secret-free runtime snapshot for diagnosing configuration and persistence
/// failures. Configuration values are reduced to booleans before serialization.
#[tauri::command]
pub async fn get_diagnostic_snapshot(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    require_maintenance_enabled(&state).await?;

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

    let (
        browser_diagnostics,
        browser_timeline,
        browser_timeline_dropped,
        browser_timeline_count,
        navigation_intents,
        lifecycle_events,
        safe_dom_snapshots,
        action_records,
        recent_failures,
    ) = {
        let browser = state.browser_state.lock().await;
        let diag = browser.diagnostics.snapshot();
        let tl = browser.diagnostics.timeline.all_events_sorted();
        let mut dropped: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for r in &diag {
            let d = browser.diagnostics.timeline.events_dropped(&r.agent_id);
            if d > 0 {
                dropped.insert(r.agent_id.clone(), d);
            }
        }
        let count = browser.diagnostics.timeline.total_events();
        let nav_intents = browser
            .diagnostics
            .navigation_intents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let lifecycle = browser
            .diagnostics
            .lifecycle_events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let dom = browser
            .diagnostics
            .safe_dom_snapshots
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let actions = browser
            .diagnostics
            .action_records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let recent_failures = tl.iter().filter(|e| e.event_type.contains("failed") || e.event_type.contains("error") || e.event_type.contains("blocked") || e.event_type.contains("missing")).take(20).map(|e| serde_json::json!({ "timestamp": e.timestamp, "agent_id": e.agent_id, "event_type": e.event_type, "operation_id": e.operation_id, "url": e.url })).collect::<Vec<_>>();
        (
            diag,
            tl,
            dropped,
            count,
            nav_intents,
            lifecycle,
            dom,
            actions,
            recent_failures,
        )
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

    // W1-D: WebView version via Tauri/Wry (no new dep). On Windows this is
    // the Edge WebView2 runtime version; on Linux it is WebKitGTK version.
    let webview_version = tauri::webview_version().ok();
    let snapshot = DiagnosticSnapshot {
        app_data_dir: app_data_dir.to_string_lossy().into_owned(),
        settings_db_exists: app_data_dir.join("settings.db").is_file(),
        memory_db_exists: app_data_dir.join("memory.db").is_file(),
        transcript_db_exists: app_data_dir.join("transcript.db").is_file(),
        blueprint_db_exists: app_data_dir.join("blueprint.db").is_file(),
        session_active: state.session_runtime.is_active(),
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
        webview_version,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        tauri_version: env!("CARGO_PKG_VERSION").to_string(),
        command_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    serde_json::to_string(&snapshot)
        .map_err(|e| settings_command_error("Failed to serialize diagnostic snapshot", e))
}

/// Compact, secret-free human/AI-facing diagnostic artifact. This is plain
/// Markdown by design; callers must not JSON.parse it.
#[tauri::command]
pub async fn get_diagnostic_brief(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    // Gate before any database or retained-diagnostics collection.
    require_maintenance_enabled(&state).await?;
    build_diagnostic_brief(&state, &app).await
}

async fn build_diagnostic_brief(state: &AppState, app: &AppHandle) -> Result<String, String> {
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

    let leader_exists = app
        .get_webview_window(crate::browser_backend::LEADER_WINDOW_LABEL)
        .is_some();
    let nav_exists = app
        .get_webview_window(crate::browser_backend::NAV_WINDOW_LABEL)
        .is_some();
    // The nav window is the active shared model surface when present; fall
    // back to leader without creating either window for diagnostics.
    let active_model_window = app
        .get_webview_window(crate::browser_backend::NAV_WINDOW_LABEL)
        .map(|window| (window, crate::browser_backend::NAV_WINDOW_LABEL.to_string()))
        .or_else(|| {
            app.get_webview_window(crate::browser_backend::LEADER_WINDOW_LABEL)
                .map(|window| {
                    (
                        window,
                        crate::browser_backend::LEADER_WINDOW_LABEL.to_string(),
                    )
                })
        });
    let storage =
        crate::browser_backend::collect_model_webview_storage_diagnostics(active_model_window)
            .await;
    let diagnostic_bool = |value: Option<bool>| match value {
        Some(value) => value.to_string(),
        None => "unknown".to_string(),
    };
    let diagnostic_cookies = |cookies: &crate::browser_backend::CookieDiagnosticSummary| {
        if cookies.available {
            format!(
                "count: {}\nnames: [{}]\nany_secure: {}\nany_http_only: {}",
                cookies.count,
                cookies.names.join(", "),
                diagnostic_bool(cookies.any_secure),
                diagnostic_bool(cookies.any_http_only),
            )
        } else {
            "count: unknown\nnames: unknown\nany_secure: unknown\nany_http_only: unknown"
                .to_string()
        }
    };
    let mut writer = crate::browser_harness::DiagnosticBriefWriter::new();
    writer.push("# Consensus Arena Diagnostic Brief\n\n");
    writer.push(&format!(
        "timestamp: {}\nos_arch: {}/{}\napp_version: {}\nwebview_engine: {}\nsession_active: {}\nleader_registry_window: {}\nnav_registry_window: {}\n",
        chrono::Utc::now().to_rfc3339(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION"),
        tauri::webview_version().unwrap_or_else(|_| "unavailable".to_string()),
        state.session_runtime.is_active(),
        leader_exists,
        nav_exists,
    ));
    writer.push(&format!(
        "\n## Active model WebView storage\n\nactive_model_window: {}\nwebview_ephemeral: {}\nwebkit_itp_enabled: {}\ncookie_policy: {}\nclaude_cookie_{}\ncloudflare_cookie_{}\n",
        storage.window_label.as_deref().unwrap_or("none"),
        diagnostic_bool(storage.webview_ephemeral),
        diagnostic_bool(storage.webkit_itp_enabled),
        storage.cookie_policy.as_deref().unwrap_or("unknown"),
        diagnostic_cookies(&storage.claude).replace('\n', "\nclaude_cookie_"),
        diagnostic_cookies(&storage.cloudflare).replace('\n', "\ncloudflare_cookie_"),
    ));

    let (retention, timeline_retained, timeline_selected, dropped) = {
        let browser = state.browser_state.lock().await;
        let leader_cached = browser.leader_window.is_some();
        let nav_cached = browser.nav_window.is_some();
        writer.push(&format!(
            "leader_cached_handle: {}\nnav_cached_handle: {}\nbrain_configured: primary={}, fallback={}, secondary={}\nmemory_health: healthy={}, warnings={}, issues={}\n",
            leader_cached,
            nav_cached,
            primary_configured,
            fallback_present,
            secondary_configured,
            memory_health.is_healthy,
            memory_health.warnings.len(),
            memory_health.issues.len(),
        ));
        if leader_exists != leader_cached || nav_exists != nav_cached {
            writer.push("flags: registry/cache mismatch\n");
        }
        let mut dropped = Vec::new();
        let mut agents = browser.diagnostics.timeline.agent_ids();
        agents.sort();
        for agent in agents {
            dropped.push((
                agent.clone(),
                browser.diagnostics.timeline.events_dropped(&agent),
            ));
        }
        let retention = browser.diagnostics.append_diagnostic_brief(&mut writer);
        let (retained, selected) = browser
            .diagnostics
            .timeline
            .append_diagnostic_brief_events(&mut writer);
        (retention, retained, selected, dropped)
    };
    writer.push("\n## Evidence accounting\n\n");
    writer.push(&format!(
        "raw_timeline_retained: {}\nevents_included_in_brief: {}\nevents_omitted: {}\nlifecycle_retained: {}\nnavigation_intent_retained: {}\naction_retained: {}\ndom_snapshot_retained: {}\n",
        timeline_retained,
        timeline_selected,
        timeline_retained.saturating_sub(timeline_selected),
        retention.lifecycle,
        retention.navigation_intents,
        retention.actions,
        retention.dom_snapshots,
    ));
    if dropped.is_empty() {
        writer.push("per_agent_dropped_events: none\n");
    } else {
        for (agent, count) in dropped {
            writer.push(&format!("dropped_events.{}: {}\n", agent, count));
        }
    }
    Ok(writer.finish())
}

/// Harness: return chronological timeline events as JSON string (spec 3-15)
#[tauri::command]
pub async fn get_browser_timeline(state: tauri::State<'_, AppState>) -> Result<String, String> {
    require_maintenance_enabled(&state).await?;
    let browser = state.browser_state.lock().await;
    let events = browser.diagnostics.timeline.all_events_sorted();
    serde_json::to_string(&events).map_err(|e| e.to_string())
}

/// Harness: human-readable reliability report markdown (spec 16)
#[tauri::command]
pub async fn get_browser_reliability_report(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    require_maintenance_enabled(&state).await?;
    let (timeline, diagnostics) = {
        let browser = state.browser_state.lock().await;
        (
            browser.diagnostics.timeline.clone(),
            browser.diagnostics.snapshot(),
        )
    };
    let md = crate::browser_harness::generate_reliability_report_markdown(&timeline, &diagnostics);
    Ok(md)
}

/// Harness: export bundle (spec 18) – writes BROWSER_RELIABILITY_REPORT.md + JSON files to app_data_dir/exports
#[tauri::command]
pub async fn export_browser_diagnostics(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    require_maintenance_enabled(&state).await?;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let export_dir = app_data_dir.join(format!(
        "diagnostics_export_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
    let brief_path = export_dir.join("DIAGNOSTIC_BRIEF.md");
    let brief = build_diagnostic_brief(&state, &app).await?;
    std::fs::write(&brief_path, brief).map_err(|e| e.to_string())?;
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
    std::fs::write(
        &events_path,
        serde_json::to_string_pretty(&timeline).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // browser-diagnostics.json
    let diag_path = export_dir.join("browser-diagnostics.json");
    std::fs::write(
        &diag_path,
        serde_json::to_string_pretty(&browser_diagnostics).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // navigation-history.json
    let nav: Vec<_> = browser_diagnostics
        .iter()
        .flat_map(|r| r.navigation_diagnostics.clone())
        .collect();
    let nav_path = export_dir.join("navigation-history.json");
    std::fs::write(
        &nav_path,
        serde_json::to_string_pretty(&nav).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // console-errors.json
    let console: Vec<_> = browser_diagnostics
        .iter()
        .flat_map(|r| r.console_diagnostics.clone())
        .collect();
    let console_path = export_dir.join("console-errors.json");
    std::fs::write(
        &console_path,
        serde_json::to_string_pretty(&console).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Cross-platform forensics: lifecycle, dom, actions, intents
    let (lifecycle, dom_snapshots, actions, intents, full_snapshot) = {
        let browser = state.browser_state.lock().await;
        let lc = browser
            .diagnostics
            .lifecycle_events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let dom = browser
            .diagnostics
            .safe_dom_snapshots
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let acts = browser
            .diagnostics
            .action_records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
        let intents = browser
            .diagnostics
            .navigation_intents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|d| d.iter().cloned())
            .collect::<Vec<_>>();
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
    std::fs::write(
        &lifecycle_path,
        serde_json::to_string_pretty(&lifecycle).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let dom_path = export_dir.join("safe-dom-snapshots.json");
    std::fs::write(
        &dom_path,
        serde_json::to_string_pretty(&dom_snapshots).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let actions_path = export_dir.join("action-records.json");
    std::fs::write(
        &actions_path,
        serde_json::to_string_pretty(&actions).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let intents_path = export_dir.join("navigation-intents.json");
    std::fs::write(
        &intents_path,
        serde_json::to_string_pretty(&intents).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let snapshot_path = export_dir.join("diagnostic-snapshot.json");
    std::fs::write(
        &snapshot_path,
        serde_json::to_string_pretty(&full_snapshot).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // BROWSER_RELIABILITY_REPORT.md
    let timeline_obj = {
        let browser = state.browser_state.lock().await;
        browser.diagnostics.timeline.clone()
    };
    let report =
        crate::browser_harness::generate_reliability_report_markdown(&timeline_obj, &diagnostics);
    let report_path = export_dir.join("BROWSER_RELIABILITY_REPORT.md");
    std::fs::write(&report_path, &report).map_err(|e| e.to_string())?;
    let result = serde_json::json!({
        "export_dir": export_dir.to_string_lossy(),
        "diagnostic_brief": brief_path.to_string_lossy(),
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
    require_maintenance_enabled(&state).await?;
    if state.session_runtime.is_active() {
        return Err(
            "Cannot run single-model diagnostic while a session is active. Stop the session first."
                .to_string(),
        );
    }
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| e.to_string())?;
    let participant = crate::browser_backend::resolve_participant(&agent_id, &custom)
        .ok_or_else(|| format!("Unknown participant: {agent_id}"))?;
    let window = {
        let mut browser = state.browser_state.lock().await;
        // This externally initiated diagnostic shares the same registry-
        // authoritative lifecycle rule as Connected Accounts.
        crate::browser_backend::ensure_nav_window(&app, &mut browser).map_err(|e| e.to_string())?
    };
    let diagnostics = {
        let browser = state.browser_state.lock().await;
        browser.diagnostics.clone()
    };
    let generation = diagnostics.setup_generation();
    let op = crate::browser_harness::operation_id_diagnostic_single(&agent_id, generation);
    diagnostics.set_operation(&agent_id, &op, "diagnostic");
    diagnostics.emit_harness_event(
        &agent_id,
        crate::browser_harness::EventType::Unknown,
        "diagnostic",
        &op,
        &participant.base_url,
        serde_json::json!({ "single_model": true, "window": "available" }),
    );

    // Navigate
    crate::browser_backend::navigate_agent_window(
        &app,
        &diagnostics,
        &window,
        &agent_id,
        "nav",
        &participant.base_url,
    )
    .map_err(|e| e.to_string())?;

    // Wait briefly for readiness probe (non-blocking, harness captures regardless)
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(15);
    let probe_snapshot = loop {
        if start.elapsed() > timeout {
            break serde_json::json!({ "timed_out": true, "elapsed_ms": start.elapsed().as_millis() });
        }
        // Check diagnostics for composer detection
        let diag = diagnostics
            .snapshot()
            .into_iter()
            .find(|r| r.agent_id == agent_id);
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

    diagnostics.emit_harness_event(
        &agent_id,
        crate::browser_harness::EventType::DomSnapshot,
        "diagnostic",
        &op,
        &participant.base_url,
        probe_snapshot.clone(),
    );

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
    if state.session_runtime.is_active() {
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
// ── Recent session loading: explicit session_id variants ─────────────────────
// LOOP 1: Fix recent-session click that previously only setSelectedSessionId without
// loading transcript/blueprint. These commands accept an explicit session_id
// so Sidebar can load any past session, not just the active one. Stale
// protection is handled frontend via loadSeq guard and backend via session_id
// matching; no unwrap/expect, no secrets in payloads.

#[tauri::command(rename_all = "snake_case")]
pub async fn get_session_transcript(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let trimmed = session_id.trim().to_string();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let store = state.transcript_store.clone();
    let sid = trimmed.clone();
    let records = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        guard.get_transcript(&sid)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&records).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_blueprint_sections(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let trimmed = session_id.trim().to_string();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let store = state.blueprint_store.clone();
    let sid = trimmed.clone();
    let sections = crate::db_helpers::run_blocking(move || {
        let guard = store
            .lock()
            .map_err(|_| AgentError::DatabaseError("blueprint store lock poisoned".to_string()))?;
        guard.get_sections(&sid)
    })
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&sections).map_err(|e| e.to_string())
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectedAccountOutcome {
    ComposerReady,
    LoginRequired,
    EmptyShell,
}

fn connected_account_outcome(
    event: &NavEvent,
    expected_agent_id: &str,
) -> Option<ConnectedAccountOutcome> {
    match event {
        NavEvent::Ready(agent_id) if agent_id == expected_agent_id => {
            Some(ConnectedAccountOutcome::ComposerReady)
        }
        NavEvent::SendProbe {
            agent_id,
            page_state_hint: Some(hint),
            ..
        } if agent_id == expected_agent_id => match hint.as_str() {
            "possible_login_required" => Some(ConnectedAccountOutcome::LoginRequired),
            "empty_shell_or_hydration_stuck" => Some(ConnectedAccountOutcome::EmptyShell),
            _ => None,
        },
        _ => None,
    }
}

async fn wait_for_connected_account_readiness<F>(
    expected_agent_id: &str,
    display_name: &str,
    diagnostics: &crate::browser_backend::BrowserDiagnostics,
    nav_rx: &mut crate::browser_backend::AsyncNavReceiver<NavEvent>,
    mut emit_challenge: F,
) -> Result<ConnectedAccountOutcome, String>
where
    F: FnMut(),
{
    let mut verification_ui_emitted = false;
    loop {
        match nav_rx.recv().await {
            Some(event) => {
                if let Some(outcome) = connected_account_outcome(&event, expected_agent_id) {
                    return Ok(outcome);
                }
                match event {
                    // A challenge is a non-terminal state. The user may
                    // complete it in the existing page; do not reload,
                    // navigate, reinject, release ownership, or turn a
                    // resume request into fake composer readiness.
                    NavEvent::ChallengeDetected(id, _) if id == expected_agent_id => {
                        if !verification_ui_emitted {
                            emit_challenge();
                            verification_ui_emitted = true;
                        }
                    }
                    NavEvent::ResumeRequested(id) if id == expected_agent_id => {}
                    NavEvent::Error(id) if id == expected_agent_id => {
                        return Err(format!(
                            "Page did not become ready: {}",
                            diagnostics.readiness_timeout_message(&id, display_name)
                        ));
                    }
                    NavEvent::UnshowableUrl(id, url) if id == expected_agent_id => {
                        return Err(format!(
                            "{} navigated to an unshowable URL: {}",
                            display_name, url
                        ));
                    }
                    NavEvent::SessionAborted => return Err("Session aborted".to_string()),
                    _ => {}
                }
            }
            None => return Err("Navigation channel closed".to_string()),
        }
    }
}

#[cfg(test)]
fn announces_connected_account_ready(outcome: ConnectedAccountOutcome) -> bool {
    outcome == ConnectedAccountOutcome::ComposerReady
}

#[tauri::command(rename_all = "snake_case")]
pub async fn launch_connected_account(
    agent_id: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    if state.session_runtime.is_active() {
        return Err(
            "Cannot launch a model window while a session is active. Stop the session first."
                .to_string(),
        );
    }
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .map_err(|e| format!("Failed to read custom participants: {e}"))?;
    let participant = resolve_participant(&agent_id, &custom)
        .ok_or_else(|| format!("Unknown participant: {agent_id}"))?;

    // The named Tauri registry is authoritative. Do this before consulting the
    // Connected Accounts lease: a manually destroyed arena-nav can otherwise
    // leave a stale cached handle and 20–120 second lease that blocks its one
    // legitimate replacement.
    let window = {
        let mut browser = state.browser_state.lock().await;
        ensure_nav_window(&app, &mut browser)
            .map_err(|e| format!("Failed to create model window: {e}"))?
    };

    // R1.3: shared-window busy guard — prevents rapid successive launches
    // from yanking the same WebView between models while navigation is still
    // in flight. Uses the existing BrowserState lock so frontend and backend
    // stay synchronized; not a frontend-only flag.
    {
        let mut browser = state.browser_state.lock().await;
        if let Some(until) = browser.connected_account_busy_until {
            if std::time::Instant::now() < until {
                return Err(
                    "A model window launch is already in progress. Wait about 30s and try again."
                        .to_string(),
                );
            }
        }
        browser.connected_account_busy_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(10));
    }

    // Attach this command to the process-lifetime ingress. The sender captured
    // by the shared WebView never changes, so a healthy authenticated window
    // does not need to be destroyed merely to receive navigation signals.
    let (diagnostics, mut tokio_rx) = {
        let mut browser = state.browser_state.lock().await;
        let nav_rx = browser.attach_nav_receiver();
        (browser.diagnostics.clone(), nav_rx)
    };

    // A healthy same-agent/same-origin composer is already the desired account
    // page. Focus it without another browsing transition. Any ownership,
    // origin, blocker, or readiness mismatch still performs normal navigation.
    let current_url = window.url().ok().map(|url| url.to_string());
    let reuse_existing = current_url.as_deref().is_some_and(|url| {
        diagnostics.can_reuse_connected_page(
            &agent_id,
            crate::browser_backend::NAV_WINDOW_LABEL,
            url,
            &participant.base_url,
        )
    });
    let nav_result = if reuse_existing {
        tracing::info!(
            "[LAUNCH] reusing healthy connected account page for {} without navigation",
            agent_id
        );
        Ok(())
    } else {
        navigate_agent_window(
            &app,
            &diagnostics,
            &window,
            &agent_id,
            "nav",
            &participant.base_url,
        )
        .map_err(|e| e.to_string())
    };

    // Keep busy guard through the page-load tail so a second immediate click
    // at 11s does not steal the window while the first navigation is still
    // hydrating. Doubled readiness (90s/100s) means the tail must cover the
    // full WebKitGTK composer mount; 20s tail (total 30s) is the minimum, but
    // the readiness wait below holds the async function for up to 100s, so the
    // guard is extended to 100s+20s to prevent yank during slow load.
    {
        let mut browser = state.browser_state.lock().await;
        if nav_result.is_ok() {
            browser.connected_account_busy_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(100 + 20));
        } else {
            browser.connected_account_busy_until = None;
        }
    }
    nav_result?;

    // Wait for page readiness (composer detected) with the doubled timeout
    // (100s). This mirrors Priming's `wait_for_setup_ready` but without
    // requiring priming prompt injection — it only ensures the page actually
    // loaded and the generic composer was found, so the user does not see a
    // blank window. The outcome distinguishes a usable composer from login,
    // challenge, and empty-shell states so only genuine composer evidence is
    // announced as ready. The window stays visible for every non-fatal state.
    let wait_outcome = if reuse_existing {
        Ok(Ok(ConnectedAccountOutcome::ComposerReady))
    } else {
        let timeout =
            std::time::Duration::from_secs(crate::browser_backend::READINESS_WAIT_TIMEOUT_SECS);
        let agent_id_wait = agent_id.clone();
        let display_name_wait = participant.display_name.clone();
        let diagnostics_wait = diagnostics.clone();
        let app_wait = app.clone();
        tokio::time::timeout(timeout, async move {
            wait_for_connected_account_readiness(
                &agent_id_wait,
                &display_name_wait,
                &diagnostics_wait,
                &mut tokio_rx,
                || {
                    let _ = app_wait.emit(
                        "captcha-detected",
                        serde_json::json!({ "agent_id": agent_id_wait }),
                    );
                },
            )
            .await
        })
        .await
    };

    // Show/focus regardless of wait outcome — the page may be showing a login
    // screen even if readiness hasn't fired yet. P5: surface failures.
    if let Err(e) = window.show() {
        tracing::warn!("[LAUNCH] show nav window failed for {}: {}", agent_id, e);
    }
    if let Err(e) = window.set_focus() {
        tracing::warn!("[LAUNCH] focus nav window failed for {}: {}", agent_id, e);
    }

    match wait_outcome {
        Ok(Ok(ConnectedAccountOutcome::ComposerReady)) => {
            // Ready — clear extended guard to base tail and notify.
            {
                let mut browser = state.browser_state.lock().await;
                browser.connected_account_busy_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(20));
            }
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!(
                        "{} is ready — window showing {}.",
                        participant.display_name, participant.base_url
                    ),
                    "message_type": "status"
                }),
            );
        }
        Ok(Ok(ConnectedAccountOutcome::LoginRequired)) => {
            let mut browser = state.browser_state.lock().await;
            browser.connected_account_busy_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(20));
            drop(browser);
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!("{} is showing a login page at {}. Please log in in the window.", participant.display_name, participant.base_url),
                    "message_type": "status"
                }),
            );
        }
        Ok(Ok(ConnectedAccountOutcome::EmptyShell)) => {
            let mut browser = state.browser_state.lock().await;
            browser.connected_account_busy_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(20));
            drop(browser);
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!("{} loaded, but its application UI did not render or hydrate. The window remains open for inspection; Arena will not reload it automatically. Check Settings → Diagnostics.", participant.display_name),
                    "message_type": "status"
                }),
            );
        }
        Ok(Err(msg)) => {
            // Non-fatal readiness error (e.g., timeout with page_state_hint) —
            // window is still visible; surface diagnostics but don't fail the
            // command so user can still interact (login, retry).
            tracing::warn!("[LAUNCH] readiness wait for {}: {}", agent_id, msg);
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!("{} window loading — {} Complete any login if prompted. Diagnostics: {}", participant.display_name, participant.base_url, msg),
                    "message_type": "status"
                }),
            );
        }
        Err(_) => {
            // Timeout (100s) — page still loading. Keep window visible and
            // tell user to check diagnostics / retry. Not a hard error.
            tracing::warn!(
                "[LAUNCH] readiness timeout for {} after {}s",
                agent_id,
                crate::browser_backend::READINESS_WAIT_TIMEOUT_SECS
            );
            let hint = diagnostics
                .page_state_hint_for(&agent_id)
                .unwrap_or_else(|| "still_loading".to_string());
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!(
                        "{} is still loading ({}). Window remains open — complete any login there if prompted, or retry in 30s. Hint: {}",
                        participant.display_name, participant.base_url, hint
                    ),
                    "message_type": "status"
                }),
            );
        }
    }
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
    if state.session_runtime.is_active() {
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

// ── Hackathon Mode ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_hackathon_config(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let config = state
        .settings_store
        .lock()
        .await
        .get_hackathon_config()
        .map_err(|e| settings_command_error("Failed to read hackathon config", e))?;
    let safe = config.to_safe();
    serde_json::to_string(&safe).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_hackathon_config(
    mut config: crate::hackathon::HackathonConfig,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Preserve existing api_keys when frontend sends empty string (since frontend never receives keys back).
    // Frontend's safe config omits keys; on round-trip, empty means "keep previous".
    {
        let store = state.settings_store.lock().await;
        let existing = store
            .get_hackathon_config()
            .map_err(|e| settings_command_error("Failed to read existing hackathon config", e))?;
        let existing_map: std::collections::HashMap<String, String> = existing
            .models
            .into_iter()
            .map(|m| (m.id, m.api_key))
            .collect();
        for m in &mut config.models {
            if m.api_key.trim().is_empty() {
                if let Some(prev) = existing_map.get(&m.id) {
                    if !prev.trim().is_empty() {
                        m.api_key = prev.clone();
                    }
                }
            }
        }
    }
    config
        .validate()
        .map_err(|e| settings_command_error("Hackathon config validation failed", e))?;
    {
        let mut store = state.settings_store.lock().await;
        store
            .save_hackathon_config(&config)
            .map_err(|e| settings_command_error("Failed to save hackathon config", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_hackathon_run_state(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let run = state.hackathon_run.lock().await;
    match run.as_ref() {
        Some(r) => {
            let safe = r.to_safe();
            serde_json::to_string(&safe).map_err(|e| e.to_string())
        }
        None => Ok("null".to_string()),
    }
}

#[tauri::command]
pub async fn cancel_hackathon_run(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.hackathon_cancel.store(true, Ordering::SeqCst);
    // Also clear run_id after short delay? Keep run for display.
    Ok(())
}

/// Send parallel health-check invitations to all models in selected groups.
/// Returns run_id as JSON string. Emits hackathon-invitation-update per model and hackathon-group-status per group.
#[tauri::command]
pub async fn send_hackathon_invitations(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    // Prevent duplicate active runs
    {
        let existing = state.hackathon_run.lock().await;
        if let Some(r) = existing.as_ref() {
            // If previous run is still pending/running and not cancelled, reject
            let has_running = r
                .groups
                .iter()
                .any(|g| g.status == crate::hackathon::GroupRunStatus::Running);
            if has_running && !r.cancelled.load(Ordering::SeqCst) {
                return Err(
                    "A hackathon invitation/run is already in progress. Cancel it first."
                        .to_string(),
                );
            }
        }
    }

    let config = state
        .settings_store
        .lock()
        .await
        .get_hackathon_config()
        .map_err(|e| settings_command_error("Failed to read hackathon config", e))?;

    if config.groups.is_empty() || config.models.is_empty() {
        return Err("No hackathon groups or models configured".to_string());
    }

    let selected_groups: Vec<_> = config
        .groups
        .iter()
        .filter(|g| g.selected)
        .cloned()
        .collect();
    if selected_groups.is_empty() {
        return Err("No groups selected for invitation".to_string());
    }

    // Build GroupRunState list for selected groups
    let run_id = Uuid::new_v4().to_string();
    let mut groups: Vec<crate::hackathon::GroupRunState> = Vec::new();
    let mut model_creds: std::collections::HashMap<String, (String, String, String)> =
        std::collections::HashMap::new();
    for m in &config.models {
        model_creds.insert(
            m.id.clone(),
            (m.base_url.clone(), m.api_key.clone(), m.model_name.clone()),
        );
    }
    let task_brief = {
        // Use context_manager project_brief if available, else generic
        let ctx = state.context_manager.lock().await;
        if !ctx.project_brief.trim().is_empty() {
            ctx.project_brief.clone()
        } else {
            // Fallback: use orchestrator current_session brief if any
            let orch = state.orchestrator.lock().await;
            orch.current_session
                .as_ref()
                .map(|c| c.project_brief.clone())
                .unwrap_or_else(|| "Hackathon task".to_string())
        }
    };

    for g in selected_groups {
        let participants: Vec<crate::hackathon::ParticipantRunState> = g
            .model_ids
            .iter()
            .filter_map(|mid| {
                config.models.iter().find(|m| &m.id == mid).map(|m| {
                    crate::hackathon::ParticipantRunState {
                        model_id: m.id.clone(),
                        model_name: m.model_name.clone(),
                        base_url: m.base_url.clone(),
                        group_id: g.id.clone(),
                        status: crate::hackathon::ParticipantRunStatus::Pending,
                        consultation_count: 0,
                        last_error: None,
                    }
                })
            })
            .collect();
        groups.push(crate::hackathon::GroupRunState {
            group_id: g.id.clone(),
            group_name: g.name.clone(),
            model_ids_ordered: g.model_ids.clone(),
            participants,
            leader_id: None,
            history: Vec::new(),
            status: crate::hackathon::GroupRunStatus::Pending,
            final_output: None,
            consultation_counts: std::collections::HashMap::new(),
        });
    }

    let max_questions = config.max_questions_per_teammate;
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let run_state = crate::hackathon::HackathonRunState {
        run_id: run_id.clone(),
        task_brief: task_brief.clone(),
        max_questions,
        groups: groups.clone(),
        cancelled: cancel_flag.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Store run state and run_id
    {
        let mut r = state.hackathon_run.lock().await;
        *r = Some(run_state);
    }
    {
        let mut id = state.hackathon_run_id.lock().await;
        *id = Some(run_id.clone());
    }
    state.hackathon_cancel.store(false, Ordering::SeqCst);

    // Emit run started
    let _ = app.emit(
        "hackathon-run-started",
        json!({
            "run_id": run_id,
            "task_brief": task_brief,
            "group_ids": groups.iter().map(|g| &g.group_id).collect::<Vec<_>>(),
            "max_questions": max_questions,
        }),
    );

    // Clone state arcs for tasks
    let hackathon_run_clone = state.hackathon_run.clone();
    let app_clone = app.clone();
    let run_id_clone = run_id.clone();

    // Fan-out invitations concurrently
    tokio::spawn(async move {
        let mut join_set = tokio::task::JoinSet::new();

        // Snapshot participants for tasks
        let mut tasks_info: Vec<(String, String, String, String, String, String)> = Vec::new(); // group_id, model_id, base_url, api_key, model_name, run_id
        for group in &groups {
            for p in &group.participants {
                if let Some((base_url, api_key, model_name)) = model_creds.get(&p.model_id) {
                    tasks_info.push((
                        group.group_id.clone(),
                        p.model_id.clone(),
                        base_url.clone(),
                        api_key.clone(),
                        model_name.clone(),
                        run_id_clone.clone(),
                    ));
                }
            }
        }

        for (group_id, model_id, base_url, api_key, model_name, run_id_task) in tasks_info {
            let app_task = app_clone.clone();
            let run_clone = hackathon_run_clone.clone();
            let run_id_task_clone = run_id_task.clone();
            join_set.spawn(async move {
                let prompt = crate::hackathon::build_invitation_prompt();
                let messages = vec![crate::hackathon::HackathonMessage {
                    role: "user".to_string(),
                    content: prompt,
                }];
                let result = crate::hackathon::call_hackathon_model(
                    &base_url,
                    &api_key,
                    &model_name,
                    &messages,
                    crate::hackathon::HACKATHON_INVITE_TIMEOUT_SECS,
                )
                .await;

                let (status_str, error_opt) = match &result {
                    Ok(_) => ("confirmed", None),
                    Err(e) => ("failed", Some(e.clone())),
                };

                // Update run state under lock — clone, modify, drop lock before emit
                let should_emit = {
                    let mut run_lock = run_clone.lock().await;
                    if let Some(run) = run_lock.as_mut() {
                        // Stale run check
                        if run.run_id != run_id_task_clone {
                            return;
                        }
                        for group in &mut run.groups {
                            if group.group_id == group_id {
                                for p in &mut group.participants {
                                    if p.model_id == model_id {
                                        p.status = if status_str == "confirmed" {
                                            crate::hackathon::ParticipantRunStatus::Confirmed
                                        } else {
                                            crate::hackathon::ParticipantRunStatus::Failed
                                        };
                                        p.last_error = error_opt.clone();
                                        break;
                                    }
                                }
                                break;
                            }
                        }
                    }
                    true
                };

                if should_emit {
                    let _ = app_task.emit(
                        "hackathon-invitation-update",
                        json!({
                            "run_id": run_id_task,
                            "group_id": group_id,
                            "model_id": model_id,
                            "status": status_str,
                            "error": error_opt,
                        }),
                    );
                }
            });
        }

        // Wait for all invitations
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                tracing::warn!("[HACKATHON] invitation task join error: {}", e);
            }
        }

        // After all, compute group status and emit group-status, handle zero-responders locking
        let final_groups: Vec<(String, String)> = {
            // group_id, status
            let mut run_lock = hackathon_run_clone.lock().await;
            if let Some(run) = run_lock.as_mut() {
                if run.run_id != run_id_clone {
                    return;
                }
                let mut out = Vec::new();
                for group in &mut run.groups {
                    let live_count = group
                        .participants
                        .iter()
                        .filter(|p| p.status == crate::hackathon::ParticipantRunStatus::Confirmed)
                        .count();
                    if live_count == 0 {
                        group.status = crate::hackathon::GroupRunStatus::Locked;
                    } else {
                        // Sort participants so responders float top preserving order
                        // Build status map
                        let mut status_map = std::collections::HashMap::new();
                        for p in &group.participants {
                            status_map.insert(p.model_id.clone(), p.status.clone());
                        }
                        let sorted_ids = crate::hackathon::sort_by_responder_status(
                            &group.model_ids_ordered,
                            &status_map,
                        );
                        group.model_ids_ordered = sorted_ids.clone();
                        // Also reorder participants vec to match sorted order for UI consistency
                        let mut sorted_parts = Vec::new();
                        for id in &sorted_ids {
                            if let Some(p) = group
                                .participants
                                .iter()
                                .find(|pp| &pp.model_id == id)
                                .cloned()
                            {
                                sorted_parts.push(p);
                            }
                        }
                        group.participants = sorted_parts;
                        // Select leader = first confirmed
                        let live_set: std::collections::HashSet<String> = group
                            .participants
                            .iter()
                            .filter(|p| {
                                p.status == crate::hackathon::ParticipantRunStatus::Confirmed
                            })
                            .map(|p| p.model_id.clone())
                            .collect();
                        group.leader_id =
                            crate::hackathon::select_leader(&group.model_ids_ordered, &live_set);
                        group.status = crate::hackathon::GroupRunStatus::Pending;
                    }
                    let status_str = match group.status {
                        crate::hackathon::GroupRunStatus::Locked => "locked",
                        crate::hackathon::GroupRunStatus::Pending => "pending",
                        _ => "pending",
                    };
                    out.push((group.group_id.clone(), status_str.to_string()));
                }
                out
            } else {
                Vec::new()
            }
        };

        for (group_id, status) in final_groups {
            let _ = app_clone.emit(
                "hackathon-group-status",
                json!({
                    "run_id": run_id_clone,
                    "group_id": group_id,
                    "status": status,
                }),
            );
        }

        // Also update hackathon_run_id cleanup? Keep it.
        let _ = app_clone.emit(
            "hackathon-invitations-complete",
            json!({
                "run_id": run_id_clone,
            }),
        );
    });

    serde_json::to_string(&json!({ "run_id": run_id })).map_err(|e| e.to_string())
}

/// Run full hackathon execution for all selected groups concurrently.
/// Uses task_brief verbatim for every group. Groups execute via run_single_group concurrently.
/// Returns combined report as JSON string.
#[tauri::command(rename_all = "snake_case")]
pub async fn run_hackathon(
    task_brief: String,
    selected_participant_ids: Option<Vec<String>>,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    if task_brief.trim().is_empty() {
        return Err("Task brief cannot be empty".to_string());
    }

    // Must have an existing run state from invitations, or create one if groups exist but no prior invitations
    let (run_id, max_questions, config_groups) = {
        let run_lock = state.hackathon_run.lock().await;
        if let Some(run) = run_lock.as_ref() {
            // If cancelled, reject
            if run.cancelled.load(Ordering::SeqCst) {
                return Err(
                    "Previous hackathon run was cancelled — send invitations again".to_string(),
                );
            }
            // If groups exist but some are still Pending without confirmed status, we need confirmed participants
            let has_confirmed = run.groups.iter().any(|g| {
                g.participants
                    .iter()
                    .any(|p| p.status == crate::hackathon::ParticipantRunStatus::Confirmed)
            });
            if !has_confirmed {
                return Err(
                    "No confirmed participants — send invitations and wait for responders"
                        .to_string(),
                );
            }
            (run.run_id.clone(), run.max_questions, run.groups.clone())
        } else {
            // No prior run — try to build from config's selected groups with live=Pending (allow execution without invitation phase)
            let config = state
                .settings_store
                .lock()
                .await
                .get_hackathon_config()
                .map_err(|e| settings_command_error("Failed to read hackathon config", e))?;
            let selected: Vec<_> = config
                .groups
                .iter()
                .filter(|g| g.selected)
                .cloned()
                .collect();
            if selected.is_empty() {
                return Err("No hackathon run found — send invitations first".to_string());
            }
            let new_run_id = Uuid::new_v4().to_string();
            let mut groups = Vec::new();
            for g in selected {
                let participants: Vec<crate::hackathon::ParticipantRunState> = g
                    .model_ids
                    .iter()
                    .filter_map(|mid| {
                        config.models.iter().find(|m| &m.id == mid).map(|m| {
                            crate::hackathon::ParticipantRunState {
                                model_id: m.id.clone(),
                                model_name: m.model_name.clone(),
                                base_url: m.base_url.clone(),
                                group_id: g.id.clone(),
                                status: crate::hackathon::ParticipantRunStatus::Pending,
                                consultation_count: 0,
                                last_error: None,
                            }
                        })
                    })
                    .collect();
                groups.push(crate::hackathon::GroupRunState {
                    group_id: g.id.clone(),
                    group_name: g.name.clone(),
                    model_ids_ordered: g.model_ids.clone(),
                    participants,
                    leader_id: None,
                    history: Vec::new(),
                    status: crate::hackathon::GroupRunStatus::Pending,
                    final_output: None,
                    consultation_counts: std::collections::HashMap::new(),
                });
            }
            // Create run state now; need to store it
            // We cannot store inside this read lock — will do after drop
            (new_run_id, config.max_questions_per_teammate, groups)
        }
    };

    // If we created a new run_id above because no prior run existed, store it
    {
        let mut run_lock = state.hackathon_run.lock().await;
        if run_lock.is_none() {
            // Build new run_state from config_groups
            // We already have run_id/max_questions/config_groups
            // Need to reconstruct task_brief? Use passed task_brief
            let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let new_state = crate::hackathon::HackathonRunState {
                run_id: run_id.clone(),
                task_brief: task_brief.clone(),
                max_questions,
                groups: config_groups.clone(),
                cancelled: cancel_flag,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            *run_lock = Some(new_state);
            let mut id_lock = state.hackathon_run_id.lock().await;
            *id_lock = Some(run_id.clone());
            state.hackathon_cancel.store(false, Ordering::SeqCst);
        } else {
            // Update task_brief for existing run for this execution
            if let Some(run) = run_lock.as_mut() {
                run.task_brief = task_brief.clone();
                run.cancelled.store(false, Ordering::SeqCst);
            }
            state.hackathon_cancel.store(false, Ordering::SeqCst);
        }
    }

    // Build credentials map from config
    let config = state
        .settings_store
        .lock()
        .await
        .get_hackathon_config()
        .map_err(|e| settings_command_error("Failed to read hackathon config", e))?;
    let mut model_creds: std::collections::HashMap<String, (String, String, String)> =
        std::collections::HashMap::new();
    for m in &config.models {
        model_creds.insert(
            m.id.clone(),
            (m.base_url.clone(), m.api_key.clone(), m.model_name.clone()),
        );
    }

    // Server-trusted participant selection: if caller supplied selected ids, validate strictly.
    // Rejects: unknown model, non-selected group, non-confirmed/failed/pending, deleted, stale run, cross-group.
    if let Some(selected) = &selected_participant_ids {
        let run_lock = state.hackathon_run.lock().await;
        let run = run_lock
            .as_ref()
            .ok_or_else(|| "No hackathon run to validate selection against".to_string())?;
        let current_run_id = run.run_id.clone();
        // Build lookup: model_id -> (group_id, status)
        let mut id_to_group: std::collections::HashMap<
            String,
            (String, crate::hackathon::ParticipantRunStatus),
        > = std::collections::HashMap::new();
        for g in &run.groups {
            for p in &g.participants {
                id_to_group.insert(p.model_id.clone(), (g.group_id.clone(), p.status.clone()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for mid in selected {
            if !seen.insert(mid.clone()) {
                return Err(format!("Duplicate selected participant: {}", mid));
            }
            let (group_id, status) = id_to_group
                .get(mid)
                .ok_or_else(|| {
                    format!(
                        "Selected model {} not in current run (deleted or unknown)",
                        mid
                    )
                })?
                .clone();
            if status != crate::hackathon::ParticipantRunStatus::Confirmed {
                return Err(format!(
                    "Selected model {} is not a confirmed responder (status {:?}) — cannot participate",
                    mid, status
                ));
            }
            // Ensure its group is still selected in persisted config
            let cfg = config_groups.iter().find(|g| &g.group_id == &group_id);
            if cfg.is_none() {
                return Err(format!(
                    "Selected model {} belongs to group {} not in current run",
                    mid, group_id
                ));
            }
            // Stale run check is implicit via run_id match above; caller must have fresh run.
            let _ = current_run_id.clone();
        }
        // Apply selection: filter each group's participants to only selected confirmed ones; zero-selected groups become Locked (inactive)
        {
            let mut run_mut = state.hackathon_run.lock().await;
            if let Some(r) = run_mut.as_mut() {
                let sel_set: std::collections::HashSet<String> = selected.iter().cloned().collect();
                for g in &mut r.groups {
                    let is_selected_group =
                        config_groups.iter().any(|cg| cg.group_id == g.group_id);
                    if !is_selected_group || g.status == crate::hackathon::GroupRunStatus::Locked {
                        continue;
                    }
                    // Retain only selected confirmed participants as Confirmed; others marked as Failed? Actually keep but mark not selected as Failed for execution exclusion
                    // Instead we filter model_ids_ordered to selected only, and keep participants but execution will check live_set = Confirmed only
                    // So we keep participants but mark non-selected Confirmed as still Confirmed? We need to downgrade non-selected to not live.
                    // Easiest: keep status but filter live_set later via selected set. So we store selection as separate filtered view:
                    // For now, mark non-selected confirmed as Pending so they become non-live.
                    for p in &mut g.participants {
                        if p.status == crate::hackathon::ParticipantRunStatus::Confirmed
                            && !sel_set.contains(&p.model_id)
                        {
                            p.status = crate::hackathon::ParticipantRunStatus::Failed;
                            p.last_error = Some("Deselected by user before Go".to_string());
                        }
                    }
                    // Re-sort and recompute leader after deselection
                    let live_set: std::collections::HashSet<String> = g
                        .participants
                        .iter()
                        .filter(|p| p.status == crate::hackathon::ParticipantRunStatus::Confirmed)
                        .map(|p| p.model_id.clone())
                        .collect();
                    if live_set.is_empty() {
                        g.status = crate::hackathon::GroupRunStatus::Locked;
                        g.leader_id = None;
                    } else {
                        g.leader_id =
                            crate::hackathon::select_leader(&g.model_ids_ordered, &live_set);
                        // Keep status Pending to be executable
                        if g.status != crate::hackathon::GroupRunStatus::Locked {
                            g.status = crate::hackathon::GroupRunStatus::Pending;
                        }
                    }
                }
            }
        }
    }

    // Snapshot groups for execution (clone)
    let groups_snapshot: Vec<crate::hackathon::GroupRunState> = {
        let run_lock = state.hackathon_run.lock().await;
        run_lock
            .as_ref()
            .map(|r| r.groups.clone())
            .unwrap_or_default()
    };

    // Filter to only groups that are not Locked and have at least one live participant
    let executable_groups: Vec<crate::hackathon::GroupRunState> = groups_snapshot
        .into_iter()
        .filter(|g| g.status != crate::hackathon::GroupRunStatus::Locked)
        .filter(|g| {
            g.participants
                .iter()
                .any(|p| p.status == crate::hackathon::ParticipantRunStatus::Confirmed)
        })
        .collect();

    if executable_groups.is_empty() {
        return Err(
            "No executable groups — all are locked or have zero selected live members".to_string(),
        );
    }

    let run_id_clone = run_id.clone();
    let app_clone = app.clone();
    let _ = app_clone.emit(
        "hackathon-run-started",
        json!({
            "run_id": run_id_clone,
            "task_brief": task_brief,
            "group_count": executable_groups.len(),
        }),
    );

    // Prepare shared cancel flag from AppState
    let cancel_flag = {
        let run_lock = state.hackathon_run.lock().await;
        run_lock
            .as_ref()
            .map(|r| r.cancelled.clone())
            .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(false)))
    };
    // Also respect global hackathon_cancel
    let global_cancel = state.hackathon_cancel.clone();

    // Concurrent execution via JoinSet
    let mut join_set = tokio::task::JoinSet::new();
    for group in executable_groups {
        let task_brief_clone = task_brief.clone();
        let max_q = max_questions;
        let creds = model_creds.clone();
        let run_id_task = run_id.clone();
        let cancel_clone = cancel_flag.clone();
        let global_cancel_clone = global_cancel.clone();
        let app_task = app_clone.clone();
        join_set.spawn(async move {
            // Combine cancel flags
            let combined_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            // We'll check both flags inside run_single_group via closure that checks either
            // For simplicity, make run_single_group check cancel_clone; we also spawn a watcher
            let watcher_cancel = combined_cancel.clone();
            let c1 = cancel_clone.clone();
            let c2 = global_cancel_clone.clone();
            tokio::spawn(async move {
                loop {
                    if c1.load(Ordering::SeqCst) || c2.load(Ordering::SeqCst) {
                        watcher_cancel.store(true, Ordering::SeqCst);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            });

            let result = crate::hackathon::run_single_group(
                group,
                task_brief_clone,
                max_q,
                creds,
                run_id_task.clone(),
                combined_cancel,
            )
            .await;
            let _ = app_task.emit(
                "hackathon-group-output",
                json!({
                    "run_id": run_id_task,
                    "group_id": result.group_id,
                    "group_name": result.group_name,
                    "status": result.status,
                    "final_output": result.final_output,
                }),
            );
            result
        });
    }

    let mut completed_groups: Vec<crate::hackathon::GroupRunState> = Vec::new();
    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(group) => completed_groups.push(group),
            Err(e) => {
                tracing::warn!("[HACKATHON] group task join error: {}", e);
            }
        }
    }

    // Check for stale run: if current active run_id differs, discard
    {
        let active_id = state
            .hackathon_run_id
            .lock()
            .await
            .clone()
            .unwrap_or_default();
        if active_id != run_id {
            return Err("Hackathon run was superseded by a newer run".to_string());
        }
    }

    // Respect cancellation
    if cancel_flag.load(Ordering::SeqCst) || global_cancel.load(Ordering::SeqCst) {
        return Err("Hackathon run was cancelled".to_string());
    }

    // Include locked groups for report from stored run
    {
        let mut run_lock = state.hackathon_run.lock().await;
        if let Some(run) = run_lock.as_mut() {
            if run.run_id == run_id {
                // Update stored groups with completed results
                for completed in &completed_groups {
                    if let Some(stored) = run
                        .groups
                        .iter_mut()
                        .find(|g| g.group_id == completed.group_id)
                    {
                        *stored = completed.clone();
                    }
                }
                // For report, use stored groups (includes locked)
                completed_groups = run.groups.clone();
            }
        }
    }

    let report = crate::hackathon::format_report(&completed_groups, &run_id);

    // Emit complete event — NEVER include api keys
    let _ = app.emit(
        "hackathon-complete",
        json!({
            "run_id": run_id,
            "report": report,
            "group_count": completed_groups.len(),
        }),
    );

    // Optionally inject into leader context if session is running — store for report-up later
    // For now just return report; caller (frontend or session_runner) can inject into context_manager

    serde_json::to_string(&json!({ "run_id": run_id, "report": report })).map_err(|e| e.to_string())
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
        let custom = vec![participant("acme", "Acme Bot", "https://acme.example.com")];
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
        assert!(result.is_err(), "unknown participant should be rejected");
    }

    fn connected_send_probe(agent_id: &str, page_state_hint: &str) -> NavEvent {
        NavEvent::SendProbe {
            agent_id: agent_id.to_string(),
            input_found: false,
            send_button_found: false,
            user_submit_seen: false,
            message_count_seen: None,
            sent_signal_emitted: false,
            readiness_probe_count: Some(1),
            input_candidate_count: Some(0),
            composer_candidate_count: Some(0),
            send_button_candidate_count: Some(0),
            readiness_timeout_ms: Some(crate::browser_backend::READINESS_TIMEOUT_MS),
            page_state_hint: Some(page_state_hint.to_string()),
            page_health_hint: None,
        }
    }

    #[test]
    fn connected_account_outcomes_preserve_ready_semantics() {
        assert_eq!(
            connected_account_outcome(&NavEvent::Ready("chatgpt".to_string()), "chatgpt"),
            Some(ConnectedAccountOutcome::ComposerReady)
        );
        assert_eq!(
            connected_account_outcome(
                &connected_send_probe("chatgpt", "possible_login_required"),
                "chatgpt"
            ),
            Some(ConnectedAccountOutcome::LoginRequired)
        );
        assert_eq!(
            connected_account_outcome(
                &NavEvent::ChallengeDetected("chatgpt".to_string(), "captcha".to_string()),
                "chatgpt"
            ),
            None,
            "a challenge must not be classified as a terminal outcome"
        );
        assert_eq!(
            connected_account_outcome(
                &connected_send_probe("chatgpt", "empty_shell_or_hydration_stuck"),
                "chatgpt"
            ),
            Some(ConnectedAccountOutcome::EmptyShell)
        );

        assert!(announces_connected_account_ready(
            ConnectedAccountOutcome::ComposerReady
        ));
        for outcome in [
            ConnectedAccountOutcome::LoginRequired,
            ConnectedAccountOutcome::EmptyShell,
        ] {
            assert!(
                !announces_connected_account_ready(outcome),
                "only ComposerReady may announce ready"
            );
        }
    }

    fn readiness_waiter(
        rx: crate::browser_backend::AsyncNavReceiver<NavEvent>,
    ) -> tokio::task::JoinHandle<(Result<ConnectedAccountOutcome, String>, usize)> {
        tokio::spawn(async move {
            let diagnostics = crate::browser_backend::BrowserDiagnostics::new();
            let mut rx = rx;
            let mut challenge_emits = 0usize;
            let result = wait_for_connected_account_readiness(
                "claude",
                "Claude",
                &diagnostics,
                &mut rx,
                || challenge_emits += 1,
            )
            .await;
            (result, challenge_emits)
        })
    }

    #[tokio::test]
    async fn connected_account_challenge_resume_stays_waiting() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let mut handle = readiness_waiter(rx);
        tx.send(NavEvent::ChallengeDetected(
            "claude".to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tx.send(NavEvent::ResumeRequested("claude".to_string()))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut handle)
                .await
                .is_err()
        );
        handle.abort();
    }

    #[tokio::test]
    async fn connected_account_repeated_challenge_stays_waiting_and_emits_once() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let mut handle = readiness_waiter(rx);
        for indicator in ["captcha", "still-captcha"] {
            tx.send(NavEvent::ChallengeDetected(
                "claude".to_string(),
                indicator.to_string(),
            ))
            .await
            .unwrap();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut handle)
                .await
                .is_err()
        );
        tx.send(NavEvent::Ready("claude".to_string()))
            .await
            .unwrap();
        assert_eq!(
            handle.await.unwrap(),
            (Ok(ConnectedAccountOutcome::ComposerReady), 1)
        );
    }

    #[tokio::test]
    async fn connected_account_resume_then_ready_succeeds_and_wrong_agent_ready_is_ignored() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let mut handle = readiness_waiter(rx);
        tx.send(NavEvent::ChallengeDetected(
            "claude".to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tx.send(NavEvent::ResumeRequested("claude".to_string()))
            .await
            .unwrap();
        tx.send(NavEvent::Ready("other".to_string())).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut handle)
                .await
                .is_err()
        );
        tx.send(NavEvent::Ready("claude".to_string()))
            .await
            .unwrap();
        assert_eq!(
            handle.await.unwrap(),
            (Ok(ConnectedAccountOutcome::ComposerReady), 1)
        );
    }

    #[tokio::test]
    async fn connected_account_challenge_then_error_fails() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let handle = readiness_waiter(rx);
        tx.send(NavEvent::ChallengeDetected(
            "claude".to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tx.send(NavEvent::Error("claude".to_string()))
            .await
            .unwrap();
        assert!(handle.await.unwrap().0.is_err());
    }

    #[tokio::test]
    async fn connected_account_challenge_then_unshowable_fails() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let handle = readiness_waiter(rx);
        tx.send(NavEvent::ChallengeDetected(
            "claude".to_string(),
            "captcha".to_string(),
        ))
        .await
        .unwrap();
        tx.send(NavEvent::UnshowableUrl(
            "claude".to_string(),
            "intent://blocked".to_string(),
        ))
        .await
        .unwrap();
        assert!(handle.await.unwrap().0.is_err());
    }
}
