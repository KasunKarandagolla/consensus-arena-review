use std::sync::atomic::Ordering;
use std::collections::HashMap;
use std::time::Duration;
use tauri::AppHandle;
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

// ── Constants ─────────────────────────────────────────────────────────────────

const RESPONSE_TIMEOUT_SECS: u64 = 300;

/// IMP-2: Maximum number of retry attempts for participant injection.
/// Attempt 0 is the initial try; attempts 1–3 are retries with backoff.
const MAX_RETRIES: u32 = 3;

/// IMP-2: Exponential backoff base in seconds.
/// Attempt 1 → 2 s, attempt 2 → 4 s, attempt 3 → 8 s (all < 60 s cap).
const BACKOFF_BASE_SECS: u64 = 2;
const MAX_UNCLASSIFIED_CONTINUES: u32 = 1;

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
    agent_id: &str,
    prompt: &str,
    turn: u32,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError> {
    {
        let mut browser = state.browser_state.lock().await;
        browser.begin_active_turn(agent_id, turn);
    }
    let _ = app.emit("active-turn-state", serde_json::json!({
        "event": "active_turn_started",
        "agent_id": agent_id,
        "turn_number": turn,
    }));
    crate::browser_backend::inject_to_window(window, agent_id, prompt, turn, nav_rx, false, true)
        .await
        .map_err(|error| {
            AgentError::InjectionFailed(format!("Failed to inject active prompt for {agent_id}: {error}"))
        })?;
    {
        let browser = state.browser_state.lock().await;
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
    Ok(())
}

async fn finish_active_turn(state: &AppState, agent_id: &str, turn: u32) {
    let mut browser = state.browser_state.lock().await;
    browser.clear_active_turn(agent_id, turn);
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
            match wait_for_response(&leader_id, active_turn, nav_rx).await {
                Ok(response) => break response,
                Err(AgentError::Timeout(error)) => {
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
                    finish_active_turn(state, &leader_id, active_turn).await;
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

        let context = format!(
            "Session iteration: {}\nSelected participant IDs: {}\nLeader ID: {}\nDeepSeek selected: {}\nDeepSeek consulted this session: {}\nProject brief requires DeepSeek once: {}",
            iteration, config.agent_ids.join(", "), leader_id, deepseek_selected,
            deepseek_consulted, consult_deepseek_once,
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
                    &leader_id,
                    "Please continue.",
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
        let (nav_window, diagnostics, target_url) = {
            let mut browser = state.browser_state.lock().await;
            let window = crate::browser_backend::ensure_nav_window(app, &mut browser)?;
            let target_url = browser
                .conversation_urls
                .get(target_model)
                .and_then(|url| url.clone())
                .or_else(|| {
                    crate::browser_backend::get_agent_config(target_model)
                        .map(|config| config.base_url.to_string())
                })
                .ok_or_else(|| {
                    AgentError::NavigationFailed(format!(
                        "unknown participant model: {target_model}"
                    ))
                })?;
            (window, browser.diagnostics.clone(), target_url)
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

        // Attempt injection.
        {
            let mut browser = state.browser_state.lock().await;
            browser.begin_active_turn(target_model, turn);
        }
        let _ = app.emit("active-turn-state", serde_json::json!({
            "event": "active_turn_started",
            "agent_id": target_model,
            "turn_number": turn,
        }));
        match crate::browser_backend::inject_to_window(
            nav_window,
            target_model,
            prompt,
            turn,
            nav_rx,
            true,
            true,
        )
        .await
        {
            Err(e) => {
                finish_active_turn(state, target_model, turn).await;
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

        // Wait for response.
        match wait_for_response(target_model, turn, nav_rx).await {
            Ok(response) => {
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
            Err(e) => {
                finish_active_turn(state, target_model, turn).await;
                let _ = app.emit("active-turn-state", serde_json::json!({
                    "event": "active_turn_timeout",
                    "agent_id": target_model,
                    "turn_number": turn,
                }));
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
                    "[RETRY] Wait for response from {} failed (attempt {}): {}",
                    target_model,
                    attempt,
                    e
                );
                last_err = Some(e);
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
    let deadline = Duration::from_secs(RESPONSE_TIMEOUT_SECS);
    let mut chunks: HashMap<String, (u32, HashMap<u32, String>)> = HashMap::new();

    loop {
        match timeout(deadline, nav_rx.recv()).await {
            Ok(Some(event)) => {
                // D-040 [NAV]
                tracing::debug!("[NAV] {:?}", event);
                match event {
                    NavEvent::ResponseStart { agent_id: ev_agent, turn: ev_turn, message_id, chunk_count }
                        if ev_agent == agent_id && ev_turn == turn => {
                        chunks.insert(message_id, (chunk_count, HashMap::new()));
                    }
                    NavEvent::ResponseChunk { agent_id: ev_agent, turn: ev_turn, message_id, index, text }
                        if ev_agent == agent_id && ev_turn == turn => {
                        if let Some((_, received)) = chunks.get_mut(&message_id) {
                            received.insert(index, text);
                        }
                    }
                    NavEvent::ResponseEnd { agent_id: ev_agent, turn: ev_turn, message_id }
                        if ev_agent == agent_id && ev_turn == turn => {
                        if let Some((expected, received)) = chunks.remove(&message_id) {
                            if received.len() == expected as usize {
                                let mut response = String::new();
                                for index in 0..expected {
                                    if let Some(chunk) = received.get(&index) { response.push_str(chunk); } else { break; }
                                }
                                if !response.is_empty() { return Ok(response); }
                            }
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
                return Err(AgentError::Timeout(format!(
                    "Agent {} did not respond within {} seconds",
                    agent_id, RESPONSE_TIMEOUT_SECS
                )));
            }
        }
    }
}
