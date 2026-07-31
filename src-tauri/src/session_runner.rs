use crate::browser_backend::{
    NavEvent, READINESS_WAIT_TIMEOUT_SECS, display_name_for, get_agent_config,
    navigate_agent_window, record_browser_blocker, record_browser_error,
    record_prompt_injected, record_prompt_injection_error, record_prompt_injection_report,
    record_setup_completion,
    record_setup_expected_agent, record_setup_stale_signal,
};
use crate::errors::AgentError;
use crate::orchestrator::{AppState, SessionConfig};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::Receiver;

const ROLES: &[&str] = &[
    "Leader",
    "Critic",
    "Technical Realist",
    "Validator",
    "Precedent Analyst",
];

enum SetupCompletionProof {
    SendDetected(String),
    UserConfirmedManual,
}

fn is_strong_setup_event(event: &NavEvent, agent_id: &str) -> bool {
    matches!(event,
        NavEvent::SendDetected(id, _) | NavEvent::SetupManualConfirmed(id) if id == agent_id
    )
}

fn setup_send_reason(reason: Option<&str>) -> String {
    match reason {
        Some("trusted-click") | Some("trusted-enter") | Some("trusted-submit") => {
            "trusted_submit".to_string()
        }
        Some("automation-scoped-submit") => "automation_submit".to_string(),
        Some("mutation") | Some("poll") => "mutation_fallback".to_string(),
        _ => "send_detected".to_string(),
    }
}

fn drain_stale_nav_events(nav_rx: &mut Receiver<NavEvent>, context: &str) -> usize {
    let mut drained = 0usize;
    loop {
        match nav_rx.try_recv() {
            Ok(event) => {
                drained = drained.saturating_add(1);
                tracing::warn!(
                    "[SETUP] Drained stale nav event before {context}: {:?}",
                    event
                );
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return drained,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return drained,
        }
    }
}

fn assign_role(agent_id: &str, config: &SessionConfig) -> String {
    if agent_id == config.leader_agent_id {
        return "Leader".to_string();
    }
    let non_leaders: Vec<&String> = config
        .agent_ids
        .iter()
        .filter(|id| *id != &config.leader_agent_id)
        .collect();
    let pos = non_leaders
        .iter()
        .position(|id| *id == agent_id)
        .unwrap_or(0);
    ROLES.get(pos + 1).unwrap_or(&"Analyst").to_string()
}

fn build_setup_injection_script(priming: &str) -> Result<String, AgentError> {
    let priming_json = serde_json::to_string(priming).map_err(|error| {
        AgentError::InjectionFailed(format!("priming prompt serialization failed: {error}"))
    })?;
    Ok(format!(
        r#"(function() {{
            const text = {};
            const agentId = window.__ca_agentId || '';
            function report(result, fallbackReason) {{
                const input = result && result.input;
                const visibleText = input && (input.value || input.textContent || '') || '';
                const ok = !!(result && result.ok);
                const capability = window.__caAutomation && window.__caAutomation.getCapabilitySnapshot
                    ? window.__caAutomation.getCapabilitySnapshot()
                    : null;
                const sendFound = !!(capability && capability.send_found);
                const method = result && result.method || 'ca_automation_inject';
                const reason = ok ? '' : (result && result.reason || fallbackReason || 'prompt_integrity_failed');
                try {{
                    window.location.href = 'arena://prompt-injection/' +
                        encodeURIComponent(agentId) + '/' +
                        encodeURIComponent(method) + '/' +
                        (ok ? '1' : '0') + '/' +
                        (ok ? '1' : '0') + '/' +
                        visibleText.length + '/' +
                        (sendFound ? '1' : '0') + '/' +
                        encodeURIComponent(input ? input.tagName : 'none') + '/' +
                        encodeURIComponent(input && input.getAttribute('role') || '') + '/' +
                        encodeURIComponent(input && input.getAttribute('contenteditable') || '') + '/' +
                        encodeURIComponent(reason);
                }} catch (_) {{}}
            }}
            if (!window.__caAutomation) {{
                report(null, 'automation_missing');
                return;
            }}
            const result = window.__caAutomation.injectPrompt(text, {{
                agentId: agentId,
                setup: true
            }});
            if (!result || !result.ok) {{
                report(result, 'prompt_integrity_failed');
                return;
            }}
            if (!window.__caAutomation.submitPrompt({{
                agentId: agentId,
                turn: 0,
                setup: true
            }})) {{
                report({{
                    ok: false,
                    reason: 'submit_helper_missing',
                    method: result.method,
                    input: result.input
                }}, 'submit_helper_missing');
                return;
            }}
            report(result, 'prompt_integrity_failed');
        }})();"#,
        priming_json
    ))
}

async fn wait_for_setup_ready(
    agent_id: &str,
    base_url: &str,
    display_name: &str,
    app: &AppHandle,
    diagnostics: &crate::browser_backend::BrowserDiagnostics,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError> {
    let mut challenge_seen = false;
    loop {
        let agent_id_owned = agent_id.to_string();
        let ready = tokio::time::timeout(
            std::time::Duration::from_secs(READINESS_WAIT_TIMEOUT_SECS),
            async {
            loop {
                match nav_rx.recv().await {
                    Some(NavEvent::Ready(id)) if id == agent_id_owned => break Ok(()),
                    Some(NavEvent::Error(id)) if id == agent_id_owned => {
                        break Err(AgentError::NavigationFailed(
                            diagnostics.readiness_timeout_message(&id, display_name),
                        ));
                    }
                    Some(NavEvent::ChallengeDetected(id, indicator)) if id == agent_id_owned => {
                        break Err(AgentError::CaptchaRequired(indicator));
                    }
                    Some(NavEvent::UnshowableUrl(id, url)) if id == agent_id_owned => {
                        break Err(AgentError::NavigationFailed(format!(
                            "{} navigated to a URL this WebView cannot display: {}",
                            display_name_for(&id),
                            url
                        )));
                    }
                    Some(NavEvent::SessionAborted) => {
                        break Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    Some(_) => continue,
                    None => break Err(AgentError::NavigationFailed("channel closed".to_string())),
                }
            }
        },
        )
        .await;

        match ready {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(AgentError::CaptchaRequired(indicator))) => {
                challenge_seen = true;
                let _ = app.emit("captcha-detected", json!({ "agent_id": agent_id }));
                let _ = app.emit("boss-message", json!({
                    "text": format!("{display_name} needs verification ({indicator}). Complete the check in the model window, then click Resume."),
                    "message_type": "status"
                }));
                let resumed = tokio::time::timeout(std::time::Duration::from_secs(600), async {
                    loop {
                        match nav_rx.recv().await {
                            Some(NavEvent::ResumeRequested(id)) if id == agent_id => break Ok(()),
                            Some(NavEvent::Ready(id)) if id == agent_id => break Ok(()),
                            Some(NavEvent::ChallengeDetected(id, next_indicator)) if id == agent_id => {
                                record_browser_blocker(
                                    app,
                                    diagnostics,
                                    agent_id,
                                    "captcha_or_challenge",
                                    "captcha_or_challenge",
                                    None,
                                    "Verification challenge still present",
                                    Some(&next_indicator),
                                );
                                let _ = app.emit("captcha-detected", json!({ "agent_id": agent_id }));
                            }
                            Some(NavEvent::UnshowableUrl(id, url)) if id == agent_id => {
                                break Err(AgentError::NavigationFailed(format!(
                                    "{display_name} navigated to a URL this WebView cannot display: {url}"
                                )))
                            }
                            Some(NavEvent::SessionAborted) => {
                                break Err(AgentError::UnknownError("Session aborted".to_string()))
                            }
                            Some(_) => continue,
                            None => break Err(AgentError::NavigationFailed(
                                "channel closed while waiting for verification resume".to_string(),
                            )),
                        }
                    }
                })
                .await;
                match resumed {
                    Ok(Ok(())) => continue,
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {
                        let message = "timeout waiting for verification resume".to_string();
                        record_browser_blocker(
                            app,
                            diagnostics,
                            agent_id,
                            "timeout",
                            "error",
                            Some(base_url),
                            "Verification resume timed out",
                            Some(&message),
                        );
                        return Err(AgentError::Timeout(message));
                    }
                }
            }
            Ok(Err(error)) => return Err(error),
            Err(_) if challenge_seen => {
                let _ = app.emit("captcha-detected", json!({ "agent_id": agent_id }));
                let last_real_url = diagnostics
                    .last_real_navigation_url(agent_id)
                    .unwrap_or_else(|| "unknown".to_string());
                return Err(AgentError::Timeout(format!(
                    "{display_name} window timed out waiting for readiness from {base_url} after verification resume. Last real URL: {last_real_url}. See Settings → Diagnostics."
                )));
            }
            Err(_) => {
                let last_real_url = diagnostics
                    .last_real_navigation_url(agent_id)
                    .unwrap_or_else(|| "unknown".to_string());
                return Err(AgentError::Timeout(format!(
                    "{display_name} window timed out waiting for readiness from {base_url}. Last real URL: {last_real_url}. See Settings → Diagnostics."
                )));
            }
        }
    }
}

pub async fn run_setup(
    config: &SessionConfig,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError> {
    let setup_order = config.setup_order();

    for agent_id in &setup_order {
        let role = assign_role(agent_id, config);
        let is_leader = agent_id == &config.leader_agent_id;

        let agent_config = get_agent_config(agent_id)
            .ok_or_else(|| AgentError::NavigationFailed(format!("Unknown model id: {agent_id}")))?;
        let (window, diagnostics, window_kind) = {
            let browser = state.browser_state.lock().await;
            let window = if is_leader {
                browser.leader_window.clone()
            } else {
                browser.nav_window.clone()
            }
            .ok_or_else(|| {
                AgentError::NavigationFailed(format!(
                    "{} window is not initialized for {agent_id}",
                    if is_leader { "leader" } else { "nav" }
                ))
            })?;
            (
                window,
                browser.diagnostics.clone(),
                if is_leader { "leader" } else { "nav" },
            )
        };

        // A retry re-enters setup with the same session/generation. Completed
        // agents are deliberately left untouched so their prompt is never
        // duplicated and the persistent leader conversation remains usable.
        if diagnostics.setup_completed(agent_id) {
            continue;
        }

        record_setup_expected_agent(&diagnostics, agent_id);
        let drained = drain_stale_nav_events(nav_rx, &format!("setup for {agent_id}"));
        if drained > 0 {
            tracing::warn!("[SETUP] Drained {drained} stale nav events before {agent_id}");
        }

        let navigation_result = navigate_agent_window(
            app,
            &diagnostics,
            &window,
            agent_id,
            window_kind,
            agent_config.base_url,
        );
        if let Err(error) = navigation_result {
            record_browser_error(app, &diagnostics, agent_id, &error.to_string());
            let text = format!(
                "{} window failed to load {}: {}",
                agent_config.display_name, agent_config.base_url, error
            );
            let _ = app.emit(
                "boss-message",
                json!({ "text": text, "message_type": "status" }),
            );
            return Err(error);
        }

        match wait_for_setup_ready(
            agent_id,
            agent_config.base_url,
            agent_config.display_name,
            app,
            &diagnostics,
            nav_rx,
        )
        .await
        {
            Ok(()) => {}
            Err(AgentError::Timeout(message)) => {
                record_browser_error(app, &diagnostics, agent_id, &message);
                let _ = app.emit(
                    "boss-message",
                    json!({
                        "text": message.clone(),
                        "message_type": "status"
                    }),
                );
                return Err(AgentError::Timeout(message));
            }
            Err(error) => {
                let message = error.to_string();
                let _ = app.emit(
                    "boss-message",
                    json!({
                        "text": format!(
                            "{} window failed to load {}: {}",
                            agent_config.display_name, agent_config.base_url, message
                        ),
                        "message_type": "status"
                    }),
                );
                return Err(error);
            }
        }

        // Build and inject role-priming prompt into input field (not sent; user sends manually).
        // The setup response monitor captures a post-injection assistant baseline
        // and emits only a content-free signal when a new response appears.
        let priming = format!(
            "You are participating in a structured expert panel discussion.\n\
             Your role is {}. Respond thoughtfully, be concise, and signal\n\
             clearly when you agree or disagree with a proposal. When you have\n\
             nothing to improve on the current proposal, respond with CONSENSUS.",
            role
        );

        if !diagnostics.prompt_already_visible(agent_id) {
            let script = build_setup_injection_script(&priming)?;
            if let Err(error) = window.eval(&script) {
                let message = format!("priming prompt eval failed: {error}");
                record_prompt_injection_error(&diagnostics, agent_id, &message);
                record_browser_error(app, &diagnostics, agent_id, &message);
                return Err(AgentError::InjectionFailed(message));
            }
            record_prompt_injected(&diagnostics, agent_id);
            let report_agent_id = agent_id.clone();
            let report = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match nav_rx.recv().await {
                        Some(NavEvent::PromptInjectionReport {
                            agent_id,
                            method,
                            prefix_ok,
                            suffix_ok,
                            visible_length,
                            send_enabled,
                            target_tag,
                            target_role,
                            target_contenteditable,
                            error,
                        }) if agent_id == report_agent_id => {
                            break Some((
                                method, prefix_ok, suffix_ok, visible_length, send_enabled,
                                target_tag, target_role, target_contenteditable, error,
                            ));
                        }
                        Some(NavEvent::SessionAborted) => break None,
                        Some(event) => {
                            record_setup_stale_signal(&diagnostics, &report_agent_id, &event);
                        }
                        None => break None,
                    }
                }
            }).await;
            match report {
                Ok(Some((method, prefix_ok, suffix_ok, visible_length, send_enabled, target_tag, target_role, target_contenteditable, error))) => {
                    let failure = error.clone().or_else(|| {
                        (!prefix_ok || !suffix_ok)
                            .then(|| "prompt_integrity_failed".to_string())
                    });
                    record_prompt_injection_report(
                        &diagnostics, agent_id, method, prefix_ok, suffix_ok, visible_length,
                        send_enabled, target_tag, target_role, target_contenteditable, error,
                    );
                    if let Some(reason) = failure {
                        record_prompt_injection_error(&diagnostics, agent_id, &reason);
                        return Err(AgentError::InjectionFailed(format!(
                            "setup automation failed for {agent_id}: {reason}"
                        )));
                    }
                }
                Ok(None) | Err(_) => {
                    let reason =
                        "prompt injection was not confirmed by the composer diagnostics";
                    record_prompt_injection_error(&diagnostics, agent_id, reason);
                    return Err(AgentError::InjectionFailed(reason.to_string()));
                }
            }
        } else {
            let _ = app.emit("boss-message", json!({
                "text": format!("{} still has its verified priming prompt. Press Send in the model window when ready.", agent_config.display_name),
                "message_type": "status"
            }));
        }

        app.emit("setup-agent-ready", json!({ "agent_id": agent_id }))
            .ok();

        // Wait only for strong proof that the setup prompt was submitted:
        // same-agent send detection or explicit same-agent manual confirmation.
        let agent_id_clone = agent_id.clone();
        let sent = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            loop {
                match nav_rx.recv().await {
                    Some(NavEvent::SendDetected(id, reason)) if id == agent_id_clone => {
                        break Ok(SetupCompletionProof::SendDetected(setup_send_reason(
                            reason.as_deref(),
                        )));
                    }
                    Some(NavEvent::SetupManualConfirmed(id)) if id == agent_id_clone => {
                        break Ok(SetupCompletionProof::UserConfirmedManual);
                    }
                    Some(NavEvent::ChallengeDetected(id, indicator)) if id == agent_id_clone => {
                        break Err(AgentError::CaptchaRequired(indicator));
                    }
                    Some(NavEvent::UnshowableUrl(id, url)) if id == agent_id_clone => {
                        break Err(AgentError::NavigationFailed(format!(
                            "{} navigated to a URL this WebView cannot display: {}",
                            agent_config.display_name, url
                        )));
                    }
                    Some(ref event @ NavEvent::SetupResponseObserved(ref id))
                    | Some(ref event @ NavEvent::Response(ref id, _, _))
                    | Some(ref event @ NavEvent::Done(ref id, _)) if id == &agent_id_clone => {
                        // Weak evidence only: setup/meta output must never advance
                        // into debate or become a setup completion proof.
                        record_setup_stale_signal(&diagnostics, &agent_id_clone, &event);
                        continue;
                    }
                    Some(NavEvent::SessionAborted) => {
                        break Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    Some(event) => {
                        record_setup_stale_signal(&diagnostics, &agent_id_clone, &event);
                        continue;
                    }
                    None => {
                        break Err(AgentError::Timeout(
                            "send detection channel closed".to_string(),
                        ));
                    }
                }
            }
        })
        .await;

        match sent {
            Ok(Ok(SetupCompletionProof::SendDetected(reason))) => {
                record_setup_completion(&diagnostics, agent_id, &reason);
            }
            Ok(Ok(SetupCompletionProof::UserConfirmedManual)) => {
                record_setup_completion(&diagnostics, agent_id, "user_confirmed_manual");
            }
            Ok(Err(error)) => {
                let message = error.to_string();
                if matches!(error, AgentError::CaptchaRequired(_)) {
                    let _ = app.emit("captcha-detected", json!({ "agent_id": agent_id }));
                    let _ = app.emit("boss-message", json!({
                        "text": format!("{} needs verification. Complete the check in the model window, then click Resume.", agent_config.display_name),
                        "message_type": "status"
                    }));
                }
                record_browser_error(app, &diagnostics, agent_id, &message);
                let _ = app.emit(
                    "boss-message",
                    json!({
                        "text": format!("{} setup stopped: {}", agent_config.display_name, message),
                        "message_type": "status"
                    }),
                );
                return Err(error);
            }
            Err(_) => {
                let message =
                    diagnostics.send_detection_timeout_message(agent_id, agent_config.display_name);
                record_browser_error(app, &diagnostics, agent_id, &message);
                let _ = app.emit(
                    "boss-message",
                    json!({
                        "text": message.clone(),
                        "message_type": "status"
                    }),
                );
                return Err(AgentError::Timeout(message));
            }
        }

        // Capture conversation URL
        let conversation_url = window.url().map(|url| url.to_string()).map_err(|error| {
            let message = format!("failed to read current conversation URL: {error}");
            record_browser_error(app, &diagnostics, agent_id, &message);
            AgentError::NavigationFailed(message)
        })?;

        // Save URL to vault.
        //
        // Task 9 (HIGH-5/HIGH-6): session_vault is now Arc<std::sync::Mutex<_>>
        // (see orchestrator.rs) instead of Arc<tokio::sync::Mutex<_>>, so the
        // synchronous rusqlite write happens inside db_helpers::run_blocking —
        // off the async runtime thread, with retry/backoff on transient
        // failure — instead of directly on it via `.lock().await`.
        //
        // Best-effort, same as before this change: a vault write failure does
        // not abort setup. Previously swallowed silently via `.ok()`; now
        // logged via tracing::warn! so a persistent failure is at least
        // visible in the log file, without changing the non-fatal behaviour.
        {
            let vault = state.session_vault.clone();
            let session_id = config.session_id.clone();
            let agent_id_owned = agent_id.clone();
            let url_owned = conversation_url.clone();
            if let Err(e) = crate::db_helpers::run_blocking(move || {
                let mut guard = vault.lock().map_err(|_| {
                    AgentError::DatabaseError("session vault lock poisoned".to_string())
                })?;
                guard.save_conversation_url(&session_id, &agent_id_owned, &url_owned)
            })
            .await
            {
                tracing::warn!(
                    "[VAULT] Failed to save conversation URL for {}: {}",
                    agent_id,
                    e
                );
            }
        }

        // Save URL to browser state
        {
            let mut browser = state.browser_state.lock().await;
            browser
                .conversation_urls
                .insert(agent_id.clone(), Some(conversation_url.clone()));
        }

        app.emit(
            "setup-agent-complete",
            json!({
                "agent_id": agent_id,
                "conversation_url": conversation_url
            }),
        )
        .ok();
    }

    // Setup monitors observe priming only. Active turn 1 establishes its own
    // post-injection baseline so a setup response can never satisfy it.
    app.emit("setup-complete", json!({})).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_setup_injection_script, is_strong_setup_event};
    use crate::browser_backend::NavEvent;

    #[test]
    fn setup_only_accepts_strong_same_agent_proof() {
        assert!(!is_strong_setup_event(&NavEvent::SetupResponseObserved("chatgpt".into()), "chatgpt"));
        assert!(!is_strong_setup_event(&NavEvent::Done("chatgpt".into(), 1), "chatgpt"));
        assert!(!is_strong_setup_event(&NavEvent::Response("chatgpt".into(), 1, "meta".into()), "chatgpt"));
        assert!(is_strong_setup_event(&NavEvent::SendDetected("chatgpt".into(), Some("trusted-click".into())), "chatgpt"));
        assert!(is_strong_setup_event(&NavEvent::SetupManualConfirmed("chatgpt".into()), "chatgpt"));
        assert!(!is_strong_setup_event(&NavEvent::SendDetected("deepseek".into(), None), "chatgpt"));
    }

    #[test]
    fn setup_injection_uses_only_the_page_automation_contract() {
        let script = build_setup_injection_script("role prompt")
            .unwrap_or_else(|error| panic!("setup script should build: {error}"));
        assert!(script.contains("window.__caAutomation.injectPrompt"));
        assert!(script.contains("window.__caAutomation.submitPrompt"));
        assert!(script.contains("window.__caAutomation.getCapabilitySnapshot"));
        assert!(script.contains("automation_missing"));
        for legacy_marker in [
            "const selectors",
            "sendSelectors",
            "findInput",
            "sendButton",
            "latestResponse",
            "pollSetupResponse",
            "setup-response",
        ] {
            assert!(
                !script.contains(legacy_marker),
                "legacy setup automation marker remained: {legacy_marker}"
            );
        }
    }
}

pub async fn run_debate(
    config: SessionConfig,
    state: AppState,
    app: AppHandle,
    mut nav_rx: tokio::sync::mpsc::Receiver<NavEvent>,
) -> Result<(), AgentError> {
    // DEF-001: Clone the brain out of the lock before starting the session loop.
    //
    // Previously brain_guard was held alive for the entire duration of
    // run_agent_loop, keeping the agent_brain Mutex locked for the whole
    // session.  Any call to save_agent_brain_config during an active session
    // would try to acquire the same lock — deadlock.
    //
    // Fix: clone the AgentBrain value (all fields are cheap to clone —
    // reqwest::Client is Arc-backed, everything else is String) and drop
    // the guard immediately.  The session loop uses the local clone.
    // agent_brain is now free to be acquired by save_agent_brain_config at
    // any point during the session.
    let brain = {
        let guard = state.agent_brain.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| {
                AgentError::UnknownError(
                    "Agent brain is not configured. \
                     Please configure it in Settings before starting a session."
                        .to_string(),
                )
            })?
            .clone()
    }; // lock released here — agent_brain is now free

    crate::response_router::run_agent_loop(&config, &brain, &state, &app, &mut nav_rx).await
}
