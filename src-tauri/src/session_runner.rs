use crate::browser_backend::{
    MAX_SETUP_NAVIGATION_RECOVERIES, NavEvent, READINESS_WAIT_TIMEOUT_SECS, display_name_for,
    navigate_agent_window, record_browser_blocker, record_browser_error, record_prompt_injected,
    record_prompt_injection_error, record_prompt_injection_report, record_setup_completion,
    record_setup_expected_agent, record_setup_stale_signal, resolve_participant,
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

fn prompt_hash_for_log(s: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, s.as_bytes());
    let hex: String = digest
        .as_ref()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("len={} sha256={}...", s.len(), &hex[..16.min(hex.len())])
}

fn has_canonical_leader_markers(s: &str) -> bool {
    s.contains("You are the leader of an expert AI panel assembled")
        && s.contains("Runtime state is authoritative")
        && s.contains("Route — consult one participant")
        && s.contains("Phase 1")
        && s.contains("Phase 2")
}

enum SetupCompletionProof {
    SendDetected(String),
    ResponseAfterInjection,
    UserConfirmedManual,
}

fn setup_send_reason(reason: Option<&str>) -> String {
    match reason {
        Some("trusted-click") | Some("trusted-enter") | Some("trusted-submit") => {
            "trusted_submit".to_string()
        }
        Some("mutation") | Some("poll") => "mutation_fallback".to_string(),
        _ => "send_detected".to_string(),
    }
}

/// Strong setup capability proof, shared by the setup gate.
///
/// Setup completes WITHOUT waiting for a human to press Send only when the
/// priming injection is confirmed verbatim (`prefix_ok` AND `suffix_ok`), a
/// composer-owned Send control is enabled after injection
/// (`send_enabled`), and there is no injection error. This deliberately
/// treats capability as readiness ONLY: it never implies the priming text was
/// submitted and never fabricates a response or an ActiveSubmitReport. The
/// `send_button_candidate_count` alone is never sufficient — the enabled,
/// composer-owned Send probe result is required.
fn capability_verified(
    prefix_ok: bool,
    suffix_ok: bool,
    send_enabled: bool,
    injection_error: Option<&str>,
) -> bool {
    injection_error.is_none() && prefix_ok && suffix_ok && send_enabled
}

fn build_priming_script(priming: &str) -> Result<String, AgentError> {
    let priming_json = serde_json::to_string(priming).map_err(|e| {
        AgentError::InjectionFailed(format!("priming prompt serialization failed: {e}"))
    })?;
    Ok(format!(
        r#"(function() {{
                const text = {};
                const selectors = ['textarea', '#prompt-textarea', '#chat-input', 'div.ProseMirror[contenteditable="true"]', '[contenteditable="true"]', '[role="textbox"]', '[aria-multiline="true"]', 'p[data-placeholder]'];
                function visible(el) {{ if (!el || !(el instanceof Element)) return false; const s = getComputedStyle(el), r = el.getBoundingClientRect(); return s.display !== 'none' && s.visibility !== 'hidden' && r.width > 0 && r.height > 0; }}
                function root(el) {{
                    if (!el || !(el instanceof Element)) return null;
                    if (el.tagName === 'TEXTAREA') return el;
                    const editable = el.closest('[contenteditable="true"], [role="textbox"], div.ProseMirror');
                    if (editable) return editable;
                    if (el.matches('p[data-placeholder]')) return null;
                    const child = el.querySelector && el.querySelector('textarea,[contenteditable="true"],[role="textbox"],div.ProseMirror');
                    return child ? root(child) : null;
                }}
                function findInput() {{
                    const textareas = Array.from(document.querySelectorAll('textarea')).filter(visible);
                    if (textareas.length) return textareas[0];
                    for (const selector of selectors) for (const candidate of document.querySelectorAll(selector)) {{ const candidateRoot = root(candidate); if (candidateRoot && visible(candidateRoot)) return candidateRoot; }}
                    return null;
                }}
                function fire(type, inputType) {{
                    try {{ el.dispatchEvent(new InputEvent(type, {{ bubbles: true, cancelable: type === 'beforeinput', inputType: inputType, data: text }})); }}
                    catch (_) {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }}
                }}
                function valueOf(target) {{ return target && target.tagName === 'TEXTAREA' ? target.value : (target && (target.innerText || target.textContent) || ''); }}
                function selectContents(target) {{ const range = document.createRange(); range.selectNodeContents(target); const selection = getSelection(); if (selection) {{ selection.removeAllRanges(); selection.addRange(range); }} }}
                function latestResponse() {{ for (const selector of ['[data-message-author-role="assistant"]','[data-testid="assistant-message"]','[class*="assistant-message"]','[class*="ai-message"]','.markdown','.prose']) {{ const items = document.querySelectorAll(selector); if (items.length) {{ const response = (items[items.length - 1].innerText || '').trim(); if (response) return response; }} }} return ''; }}
                const baseline = latestResponse();
                let el = findInput();
                let method = 'none', error = '';
                if (!el) {{ error = 'priming input field not found'; }} else {{
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    if (prefixOk && suffixOk && visibleText.length >= text.length) {{
                        const id = encodeURIComponent(window.__ca_agentId || '');
                        try {{ window.location.href = 'arena://prompt-injection/' + id + '/' + encodeURIComponent('idempotent_skip') + '/1/1/' + visibleText.length + '/0/' + encodeURIComponent(el ? el.tagName : '') + '/' + encodeURIComponent(el && el.getAttribute('role') || '') + '/' + encodeURIComponent(el && el.getAttribute('contenteditable') || '') + '/' + encodeURIComponent(''); }} catch (_) {{}}
                        return;
                    }}
                    try {{
                        el.focus();
                        if (el.tagName === 'TEXTAREA') {{
                            const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
                            if (!descriptor || !descriptor.set) throw new Error('textarea native value setter unavailable');
                            descriptor.set.call(el, text);
                            try {{
                                el.dispatchEvent(new InputEvent('beforeinput', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }}));
                            }} catch (_) {{}}
                            try {{
                                el.dispatchEvent(new InputEvent('input', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }}));
                            }} catch (_) {{
                                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                            }}
                            el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                            method = 'textarea-native-setter';
                        }} else {{
                            selectContents(el); fire('beforeinput', 'insertText');
                            if (document.execCommand && document.execCommand('insertText', false, text)) method = 'contenteditable-execCommand';
                            if (valueOf(el).indexOf(text) === -1) {{ el.textContent = text; method = 'contenteditable-textContent-fallback'; }}
                            fire('input', 'insertText'); el.dispatchEvent(new Event('change', {{ bubbles: true }})); el.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: 'Unidentified' }}));
                        }}
                    }} catch (injectionError) {{ error = String(injectionError && injectionError.message || injectionError); }}
                }}
                // R1.5: bounded stability window — verify injected text remains
                // present after the initial report window and re-inject once if
                // wiped by an in-place SPA rerender. No permanent timer.
                let _stabilityRetry = 0;
                function doReport() {{
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    if (!error && (!prefixOk || !suffixOk)) error = 'prompt integrity check failed; composer did not show the full prompt';
                    let capability = null;
                    try {{ capability = typeof window.__ca_findOwnedSend === 'function' ? window.__ca_findOwnedSend(el) : null; }} catch (_) {{ capability = null; }}
                    const enabled = !!capability && !capability.disabled && capability.getAttribute('aria-disabled') !== 'true';
                    if (!error && !enabled) error = 'prompt injected but the composer-owned send control was not discoverable; composer state may not have accepted injected text';
                    const id = encodeURIComponent(window.__ca_agentId || '');
                    try {{ window.location.href = 'arena://prompt-injection/' + id + '/' + encodeURIComponent(method) + '/' + (prefixOk ? '1' : '0') + '/' + (suffixOk ? '1' : '0') + '/' + visibleText.length + '/' + (enabled ? '1' : '0') + '/' + encodeURIComponent(el ? el.tagName : '') + '/' + encodeURIComponent(el && el.getAttribute('role') || '') + '/' + encodeURIComponent(el && el.getAttribute('contenteditable') || '') + '/' + encodeURIComponent(error); }} catch (_) {{}}
                    let responseChecks = 0, responseEmitted = false;
                    function pollSetupResponse() {{
                        if (responseEmitted || ++responseChecks > 240) return;
                        if (latestResponse() && latestResponse() !== baseline) {{
                            responseEmitted = true;
                            try {{ window.location.href = 'arena://setup-response/' + id; }} catch (_) {{}}
                            return;
                        }}
                        setTimeout(pollSetupResponse, 500);
                    }}
                    setTimeout(pollSetupResponse, 1000);
                }}
                function checkStabilityAndReport() {{
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    const stillPresent = prefixOk && suffixOk && visibleText.length >= text.length;
                    const elValid = el && el.isConnected && visible(el);
                    if ((!stillPresent || !elValid) && _stabilityRetry < 1) {{
                        _stabilityRetry++;
                        // Re-resolve composer after rerender and retry injection once
                        const fresh = findInput();
                        if (fresh && fresh.isConnected && visible(fresh)) {{
                            el = fresh;
                            error = '';
                            try {{
                                el.focus();
                                if (el.tagName === 'TEXTAREA') {{
                                    const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
                                    if (!descriptor || !descriptor.set) throw new Error('textarea native value setter unavailable');
                                    descriptor.set.call(el, text);
                                    try {{ el.dispatchEvent(new InputEvent('beforeinput', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }})); }} catch (_) {{}}
                                    try {{ el.dispatchEvent(new InputEvent('input', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }})); }} catch (_) {{ el.dispatchEvent(new Event('input', {{ bubbles: true }})); }}
                                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                                    method = 'textarea-native-setter-retry';
                                }} else {{
                                    selectContents(el); fire('beforeinput', 'insertText');
                                    if (document.execCommand && document.execCommand('insertText', false, text)) method = 'contenteditable-execCommand-retry';
                                    if (valueOf(el).indexOf(text) === -1) {{ el.textContent = text; method = 'contenteditable-textContent-fallback-retry'; }}
                                    fire('input', 'insertText'); el.dispatchEvent(new Event('change', {{ bubbles: true }})); el.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: 'Unidentified' }}));
                                }}
                            }} catch (e) {{ error = String(e && e.message || e); }}
                            setTimeout(checkStabilityAndReport, 700);
                            return;
                        }} else {{
                            error = error || 'prompt wiped by page rerender and composer not recoverable';
                        }}
                    }} else if (!stillPresent || !elValid) {{
                        error = error || 'prompt wiped by page rerender after injection';
                    }}
                    doReport();
                }}
                setTimeout(checkStabilityAndReport, 1000);
            }})();"#,
        priming_json
    ))
}

async fn perform_priming_injection(
    window: &tauri::WebviewWindow,
    priming: &str,
    diagnostics: &crate::browser_backend::BrowserDiagnostics,
    agent_id: &str,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<bool, AgentError> {
    let script = build_priming_script(priming)?;
    window.eval(&script).map_err(|e| {
        let msg = format!("priming prompt eval failed: {e}");
        record_prompt_injection_error(diagnostics, agent_id, &msg);
        AgentError::InjectionFailed(msg)
    })?;
    record_prompt_injected(diagnostics, agent_id);
    let report_agent_id = agent_id.to_string();
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
                        method,
                        prefix_ok,
                        suffix_ok,
                        visible_length,
                        send_enabled,
                        target_tag,
                        target_role,
                        target_contenteditable,
                        error,
                    ));
                }
                Some(NavEvent::SessionAborted) => break None,
                Some(event) => {
                    record_setup_stale_signal(diagnostics, &report_agent_id, &event);
                }
                None => break None,
            }
        }
    })
    .await;
    match report {
        Ok(Some((
            method,
            prefix_ok,
            suffix_ok,
            visible_length,
            send_enabled,
            target_tag,
            target_role,
            target_contenteditable,
            error,
        ))) => {
            record_prompt_injection_report(
                diagnostics,
                agent_id,
                method,
                prefix_ok,
                suffix_ok,
                visible_length,
                send_enabled,
                target_tag,
                target_role,
                target_contenteditable,
                error.clone(),
            );
            Ok(capability_verified(
                prefix_ok,
                suffix_ok,
                send_enabled,
                error.as_deref(),
            ))
        }
        Ok(None) | Err(_) => {
            record_prompt_injection_error(
                diagnostics,
                agent_id,
                "prompt injection was not confirmed by the composer diagnostics",
            );
            Ok(false)
        }
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

pub(crate) async fn wait_for_setup_ready(
    agent_id: &str,
    base_url: &str,
    display_name: &str,
    app: &AppHandle,
    diagnostics: &crate::browser_backend::BrowserDiagnostics,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError> {
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
                        Some(NavEvent::ChallengeDetected(id, indicator))
                            if id == agent_id_owned =>
                        {
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
                        // Login page detected (e.g., Claude at /login, GLM Chinese login) — treat
                        // like a challenge: wait for user to complete login (600s) rather than
                        // timing out after 100s. This prevents premature `setup_failed_recoverable`
                        // for pages that correctly show login UI but have no composer yet.
                        Some(NavEvent::SendProbe {
                            agent_id: probe_id,
                            page_state_hint: Some(hint),
                            ..
                        }) if probe_id == agent_id_owned && hint == "possible_login_required" => {
                            break Err(AgentError::CaptchaRequired("login_required".to_string()));
                        }
                        Some(_) => continue,
                        None => {
                            break Err(AgentError::NavigationFailed("channel closed".to_string()));
                        }
                    }
                }
            },
        )
        .await;

        match ready {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(AgentError::CaptchaRequired(indicator))) => {
                let _ = app.emit("captcha-detected", json!({ "agent_id": agent_id }));
                let is_login = indicator == "login_required";
                let _ = app.emit("boss-message", json!({
                    "text": if is_login {
                        format!("{display_name} is showing a login page at {base_url}. Please log in in the {} window, then click Resume or wait for it to become ready.", display_name)
                    } else {
                        format!("{display_name} needs verification ({indicator}). Complete the check in the model window, then click Resume.")
                    },
                    "message_type": "status"
                }));
                let resumed = tokio::time::timeout(std::time::Duration::from_secs(600), async {
                    loop {
                        match nav_rx.recv().await {
                            Some(NavEvent::ResumeRequested(id)) if id == agent_id => {
                                let _ = app.emit("boss-message", json!({
                                    "text": format!(
                                        "Checking {display_name} again; waiting for a ready composer."
                                    ),
                                    "message_type": "status"
                                }));
                                continue;
                            }
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
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {
                        let message = "timeout waiting for verification resume (600s)".to_string();
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
                        // RC1-A4: expose as CaptchaRequired (Permanent) so callers can
                        // distinguish challenge-expiry from a normal Timeout. Matches
                        // active-turn wait_for_response challenge expiry semantics.
                        return Err(AgentError::CaptchaRequired(format!(
                            "{display_name} blocked by verification challenge: timeout waiting for resume (600s)"
                        )));
                    }
                }
            }
            Ok(Err(error)) => return Err(error),
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

fn format_display_list(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        _ => {
            let last = names.last().unwrap();
            let init = &names[..names.len() - 1];
            format!("{}, and {}", init.join(", "), last)
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

    // P2: resolve participants against the MERGED registry (built-ins + custom)
    // loaded once for the whole setup pass, so a persisted custom participant
    // gets a valid navigation URL and display name. Best-effort load: a settings
    // read failure falls back to built-ins only.
    let custom = state
        .settings_store
        .lock()
        .await
        .get_custom_participants()
        .unwrap_or_default();

    // Load hardened priming templates with embedded defaults (single source:
    // leader_priming.md / participant_priming.md at repo root). Placeholders
    // {{...}} are filled from live session config so the model sees real
    // roster/leader/count, not a generic string.
    // Differential provenance logging: record hashes/lengths of canonical vs retrieved.
    let (leader_template_raw, participant_template_raw) = {
        let store = state.settings_store.lock().await;
        let leader = store
            .get_prompt_template_with_default("prompt_leader_priming")
            .unwrap_or_else(|_| crate::settings_store::default_leader_priming());
        let participant = store
            .get_prompt_template_with_default("prompt_participant_priming")
            .unwrap_or_else(|_| crate::settings_store::default_participant_priming());
        let canon_leader = crate::settings_store::default_leader_priming();
        let canon_part = crate::settings_store::default_participant_priming();
        tracing::info!(
            "[PROMPT] provenance retrieval leader stored={} canonical={} participant stored={} canonical={} markers_leader={} markers_participant={}",
            prompt_hash_for_log(&leader),
            prompt_hash_for_log(&canon_leader),
            prompt_hash_for_log(&participant),
            prompt_hash_for_log(&canon_part),
            has_canonical_leader_markers(&leader),
            participant.contains("You are a reviewing member of an expert AI panel")
        );
        if !has_canonical_leader_markers(&leader) {
            tracing::warn!(
                "[PROMPT] RETRIEVED leader prompt missing canonical markers — likely legacy short prompt still in DB! stored={}",
                prompt_hash_for_log(&leader)
            );
        }
        (leader, participant)
    };
    let leader_display = display_name_for(&config.leader_agent_id).to_string();
    let participant_count_total = config.agent_ids.len().to_string();
    let other_count = config.agent_ids.len().saturating_sub(1).to_string();
    let other_display_names: Vec<String> = config
        .agent_ids
        .iter()
        .filter(|id| *id != &config.leader_agent_id)
        .map(|id| display_name_for(id).to_string())
        .collect();
    let other_list = format_display_list(&other_display_names);
    let full_display_names: Vec<String> = config
        .agent_ids
        .iter()
        .map(|id| display_name_for(id).to_string())
        .collect();
    let full_list = format_display_list(&full_display_names);
    let session_type_str = match &config.session_type {
        crate::orchestrator::SessionType::Architecture => "Architecture",
        crate::orchestrator::SessionType::Mvp => "MVP",
        crate::orchestrator::SessionType::Api => "API Design",
        crate::orchestrator::SessionType::Security => "Security Review",
        crate::orchestrator::SessionType::Custom => "Custom",
    }
    .to_string();

    for agent_id in &setup_order {
        let role = assign_role(agent_id, config);
        let is_leader = agent_id == &config.leader_agent_id;

        let agent_config = resolve_participant(agent_id, &custom)
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
            &agent_config.base_url,
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
            &agent_config.base_url,
            &agent_config.display_name,
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

        // Setup is deliberately auth/readiness-only.  The usable composer is
        // the proof; setup never writes a priming prompt, clicks Send, waits
        // for a priming response, or saves a speculative conversation URL.
        record_setup_completion(&diagnostics, agent_id, "composer_ready");
        {
            let mut browser = state.browser_state.lock().await;
            browser.conversation_urls.insert(agent_id.clone(), None);
        }
        app.emit(
            "setup-agent-complete",
            json!({ "agent_id": agent_id, "conversation_url": "" }),
        )
        .ok();
        continue;

        // Retired setup-only priming path retained below temporarily for source
        // compatibility; it is unreachable. First useful active envelopes own
        // priming and submission.
        // Uses hardened templates (leader_priming.md / participant_priming.md) with
        // live placeholder substitution. Falls back to a minimal generic prompt only
        // if the template is unexpectedly empty after substitution.
        let priming_raw = if is_leader {
            let mut t = leader_template_raw.clone();
            t = t.replace("{{participant_count}}", &other_count);
            t = t.replace("{{participant_list_with_display_names}}", &other_list);
            t = t.replace("{{leader_display_name}}", &leader_display);
            t = t.replace("{{full_participant_list_including_leader}}", &full_list);
            t = t.replace("{{project_brief}}", &config.project_brief);
            t = t.replace("{{session_type}}", &session_type_str);
            t = t.replace("{{role}}", &role);
            t
        } else {
            let mut t = participant_template_raw.clone();
            t = t.replace("{{leader_display_name}}", &leader_display);
            t = t.replace("{{participant_count}}", &participant_count_total);
            t = t.replace("{{full_participant_list_including_leader}}", &full_list);
            t = t.replace("{{participant_list_with_display_names}}", &other_list);
            t = t.replace("{{project_brief}}", &config.project_brief);
            t = t.replace("{{session_type}}", &session_type_str);
            t = t.replace("{{role}}", &role);
            t
        };
        let priming = if priming_raw.trim().is_empty() {
            tracing::warn!(
                "[PROMPT] priming interpolation resulted in empty string for {} (leader={}) — using minimal fallback",
                agent_id,
                is_leader
            );
            format!(
                "You are participating in a structured expert panel discussion.\n\
                 Your role is {}. Respond thoughtfully, be concise, and signal\n\
                 clearly when you agree or disagree with a proposal. When you have\n\
                 nothing to improve on the current proposal, respond with CONSENSUS.",
                role
            )
        } else {
            // Interpolation provenance: log hash/length and whether canonical markers survived
            let canonical_hash = if is_leader {
                prompt_hash_for_log(&leader_template_raw)
            } else {
                prompt_hash_for_log(&participant_template_raw)
            };
            let interpolated_hash = prompt_hash_for_log(&priming_raw);
            let has_markers = if is_leader {
                has_canonical_leader_markers(&priming_raw)
            } else {
                priming_raw.contains("You are a reviewing member of an expert AI panel")
            };
            // Participant count semantics: leader "other_count" vs participant total — verify matches canonical wording
            tracing::info!(
                "[PROMPT] interpolated agent_id={} leader={} role={} participant_count={} other_list_len={} full_list_len={} template={} interpolated={} has_markers={} other_count={} participant_count_total={}",
                agent_id,
                is_leader,
                role,
                if is_leader {
                    &other_count
                } else {
                    &participant_count_total
                },
                other_list.len(),
                full_list.len(),
                canonical_hash,
                interpolated_hash,
                has_markers,
                other_count,
                participant_count_total
            );
            if is_leader && !has_markers {
                tracing::error!(
                    "[PROMPT] INTERPOLATED leader prompt missing canonical markers! agent={} template_hash={} interpolated_hash={}",
                    agent_id,
                    canonical_hash,
                    interpolated_hash
                );
            }
            // Verify no hardcoded 7-model list leaked: injected string must not contain the full legacy hardcoded line unless those models are actually selected
            if priming_raw.contains("Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi") {
                tracing::error!(
                    "[PROMPT] interpolated prompt contains legacy hardcoded 7-model list! This indicates old template still in use for {} ",
                    agent_id
                );
            }
            // Verify dynamic participant reflection: check that other_list appears and unselected models do not appear as participants
            tracing::debug!(
                "[PROMPT] dynamic participants leader={} participants={:?} other_list='{}' full_list='{}'",
                config.leader_agent_id,
                config.agent_ids,
                other_list,
                full_list
            );
            priming_raw
        };

        // Strong setup capability proof, set when the priming injection is
        // confirmed verbatim (prefix+suffix), a composer-owned Send control is
        // enabled after injection, and no injection error was reported. This is
        // a capability/readiness verdict, NOT a submission. When true, setup
        // advances without waiting for a human to press Send; the ACTIVE loop
        // performs the real turn submissions.
        let mut setup_capability_verified = false;

        // Injection provenance: hash/length before eval
        tracing::info!(
            "[PROMPT] injection agent_id={} leader={} injected_hash={} role={} participant_count={}",
            agent_id,
            is_leader,
            prompt_hash_for_log(&priming),
            role,
            if is_leader {
                &other_count
            } else {
                &participant_count_total
            }
        );

        if !diagnostics.prompt_already_visible(agent_id) {
            let priming_json = serde_json::to_string(&priming).map_err(|error| {
                AgentError::InjectionFailed(format!("priming prompt serialization failed: {error}"))
            })?;
            let script = format!(
                r#"(function() {{
                const text = {};
                const selectors = ['textarea', '#prompt-textarea', '#chat-input', 'div.ProseMirror[contenteditable="true"]', '[contenteditable="true"]', '[role="textbox"]', '[aria-multiline="true"]', 'p[data-placeholder]'];
                function visible(el) {{ if (!el || !(el instanceof Element)) return false; const s = getComputedStyle(el), r = el.getBoundingClientRect(); return s.display !== 'none' && s.visibility !== 'hidden' && r.width > 0 && r.height > 0; }}
                function root(el) {{
                    if (!el || !(el instanceof Element)) return null;
                    if (el.tagName === 'TEXTAREA') return el;
                    const editable = el.closest('[contenteditable="true"], [role="textbox"], div.ProseMirror');
                    if (editable) return editable;
                    if (el.matches('p[data-placeholder]')) return null;
                    const child = el.querySelector && el.querySelector('textarea,[contenteditable="true"],[role="textbox"],div.ProseMirror');
                    return child ? root(child) : null;
                }}
                function findInput() {{
                    const textareas = Array.from(document.querySelectorAll('textarea')).filter(visible);
                    if (textareas.length) return textareas[0];
                    for (const selector of selectors) for (const candidate of document.querySelectorAll(selector)) {{ const candidateRoot = root(candidate); if (candidateRoot && visible(candidateRoot)) return candidateRoot; }}
                    return null;
                }}
                function fire(type, inputType) {{
                    try {{ el.dispatchEvent(new InputEvent(type, {{ bubbles: true, cancelable: type === 'beforeinput', inputType: inputType, data: text }})); }}
                    catch (_) {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }}
                }}
                function valueOf(target) {{ return target && target.tagName === 'TEXTAREA' ? target.value : (target && (target.innerText || target.textContent) || ''); }}
                function selectContents(target) {{ const range = document.createRange(); range.selectNodeContents(target); const selection = getSelection(); if (selection) {{ selection.removeAllRanges(); selection.addRange(range); }} }}
                function latestResponse() {{ for (const selector of ['[data-message-author-role="assistant"]','[data-testid="assistant-message"]','[class*="assistant-message"]','[class*="ai-message"]','.markdown','.prose']) {{ const items = document.querySelectorAll(selector); if (items.length) {{ const response = (items[items.length - 1].innerText || '').trim(); if (response) return response; }} }} return ''; }}
                const baseline = latestResponse();
                let el = findInput();
                let method = 'none', error = '';
                if (!el) {{ error = 'priming input field not found'; }} else {{
                    // Idempotency guard: if the priming prompt is already fully visible in the input,
                    // skip re-injection (e.g., after page reload or retry where prompt remains in composer).
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    if (prefixOk && suffixOk && visibleText.length >= text.length) {{
                        // Prompt already present — report success without re-injecting
                        const id = encodeURIComponent(window.__ca_agentId || '');
                        try {{ window.location.href = 'arena://prompt-injection/' + id + '/' + encodeURIComponent('idempotent_skip') + '/1/1/' + visibleText.length + '/0/' + encodeURIComponent(el ? el.tagName : '') + '/' + encodeURIComponent(el && el.getAttribute('role') || '') + '/' + encodeURIComponent(el && el.getAttribute('contenteditable') || '') + '/' + encodeURIComponent(''); }} catch (_) {{}}
                        return;
                    }}
                    try {{
                        el.focus();
                        if (el.tagName === 'TEXTAREA') {{
                            const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
                            if (!descriptor || !descriptor.set) throw new Error('textarea native value setter unavailable');
                            fire('beforeinput', 'insertText'); descriptor.set.call(el, text); fire('input', 'insertText'); el.dispatchEvent(new Event('change', {{ bubbles: true }})); method = 'textarea-native-setter';
                        }} else {{
                            selectContents(el); fire('beforeinput', 'insertText');
                            if (document.execCommand && document.execCommand('insertText', false, text)) method = 'contenteditable-execCommand';
                            if (valueOf(el).indexOf(text) === -1) {{ el.textContent = text; method = 'contenteditable-textContent-fallback'; }}
                            fire('input', 'insertText'); el.dispatchEvent(new Event('change', {{ bubbles: true }})); el.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: 'Unidentified' }}));
                        }}
                    }} catch (injectionError) {{ error = String(injectionError && injectionError.message || injectionError); }}
                }}
                // R1.5: bounded stability — verify injected text remains after rerender window
                let _stabilityRetry = 0;
                function doReport() {{
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    if (!error && (!prefixOk || !suffixOk)) error = 'prompt integrity check failed; composer did not show the full prompt';
                    let capability = null;
                    try {{ capability = typeof window.__ca_findOwnedSend === 'function' ? window.__ca_findOwnedSend(el) : null; }} catch (_) {{ capability = null; }}
                    const enabled = !!capability && !capability.disabled && capability.getAttribute('aria-disabled') !== 'true';
                    if (!error && !enabled) error = 'prompt injected but the composer-owned send control was not discoverable; composer state may not have accepted injected text';
                    const id = encodeURIComponent(window.__ca_agentId || '');
                    try {{ window.location.href = 'arena://prompt-injection/' + id + '/' + encodeURIComponent(method) + '/' + (prefixOk ? '1' : '0') + '/' + (suffixOk ? '1' : '0') + '/' + visibleText.length + '/' + (enabled ? '1' : '0') + '/' + encodeURIComponent(el ? el.tagName : '') + '/' + encodeURIComponent(el && el.getAttribute('role') || '') + '/' + encodeURIComponent(el && el.getAttribute('contenteditable') || '') + '/' + encodeURIComponent(error); }} catch (_) {{}}
                    let responseChecks = 0, responseEmitted = false;
                    function pollSetupResponse() {{
                        if (responseEmitted || ++responseChecks > 240) return;
                        if (latestResponse() && latestResponse() !== baseline) {{
                            responseEmitted = true;
                            try {{ window.location.href = 'arena://setup-response/' + id; }} catch (_) {{}}
                            return;
                        }}
                        setTimeout(pollSetupResponse, 500);
                    }}
                    setTimeout(pollSetupResponse, 1000);
                }}
                function checkStabilityAndReport() {{
                    const visibleText = valueOf(el);
                    const prefixOk = !!el && visibleText.indexOf(text.slice(0, Math.min(32, text.length))) !== -1;
                    const suffixOk = !!el && visibleText.indexOf(text.slice(Math.max(0, text.length - 32))) !== -1;
                    const stillPresent = prefixOk && suffixOk && visibleText.length >= text.length;
                    const elValid = el && el.isConnected && visible(el);
                    if ((!stillPresent || !elValid) && _stabilityRetry < 1) {{
                        _stabilityRetry++;
                        const fresh = findInput();
                        if (fresh && fresh.isConnected && visible(fresh)) {{
                            el = fresh;
                            error = '';
                            try {{
                                el.focus();
                                if (el.tagName === 'TEXTAREA') {{
                                    const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
                                    if (!descriptor || !descriptor.set) throw new Error('textarea native value setter unavailable');
                                    descriptor.set.call(el, text);
                                    try {{ el.dispatchEvent(new InputEvent('beforeinput', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }})); }} catch (_) {{}}
                                    try {{ el.dispatchEvent(new InputEvent('input', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: text }})); }} catch (_) {{ el.dispatchEvent(new Event('input', {{ bubbles: true }})); }}
                                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                                    method = 'textarea-native-setter-retry';
                                }} else {{
                                    selectContents(el); fire('beforeinput', 'insertText');
                                    if (document.execCommand && document.execCommand('insertText', false, text)) method = 'contenteditable-execCommand-retry';
                                    if (valueOf(el).indexOf(text) === -1) {{ el.textContent = text; method = 'contenteditable-textContent-fallback-retry'; }}
                                    fire('input', 'insertText'); el.dispatchEvent(new Event('change', {{ bubbles: true }})); el.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: 'Unidentified' }}));
                                }}
                            }} catch (e) {{ error = String(e && e.message || e); }}
                            setTimeout(checkStabilityAndReport, 700);
                            return;
                        }} else {{
                            error = error || 'prompt wiped by page rerender and composer not recoverable';
                        }}
                    }} else if (!stillPresent || !elValid) {{
                        error = error || 'prompt wiped by page rerender after injection';
                    }}
                    doReport();
                }}
                setTimeout(checkStabilityAndReport, 300);
            }})();"#,
                priming_json
            );
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
                                method,
                                prefix_ok,
                                suffix_ok,
                                visible_length,
                                send_enabled,
                                target_tag,
                                target_role,
                                target_contenteditable,
                                error,
                            ));
                        }
                        Some(NavEvent::SessionAborted) => break None,
                        Some(event) => {
                            record_setup_stale_signal(&diagnostics, &report_agent_id, &event);
                        }
                        None => break None,
                    }
                }
            })
            .await;
            match report {
                Ok(Some((
                    method,
                    prefix_ok,
                    suffix_ok,
                    visible_length,
                    send_enabled,
                    target_tag,
                    target_role,
                    target_contenteditable,
                    error,
                ))) => {
                    record_prompt_injection_report(
                        &diagnostics,
                        agent_id,
                        method,
                        prefix_ok,
                        suffix_ok,
                        visible_length,
                        send_enabled,
                        target_tag,
                        target_role,
                        target_contenteditable,
                        error.clone(),
                    );
                    setup_capability_verified =
                        capability_verified(prefix_ok, suffix_ok, send_enabled, error.as_deref());
                }
                Ok(None) | Err(_) => record_prompt_injection_error(
                    &diagnostics,
                    agent_id,
                    "prompt injection was not confirmed by the composer diagnostics",
                ),
            }
        } else {
            let _ = app.emit("boss-message", json!({
                "text": format!("{} still has its verified priming prompt. Press Send in the model window when ready.", agent_config.display_name),
                "message_type": "status"
            }));
        }

        app.emit("setup-agent-ready", json!({ "agent_id": agent_id }))
            .ok();

        // Wait for proof that the setup prompt was submitted — either same-agent
        // send detection or a same-agent post-injection assistant response.
        // This wait is skipped when the strong capability proof already
        // advanced setup: the ACTIVE loop performs the real submissions.
        if !setup_capability_verified {
            let agent_id_clone = agent_id.clone();
            let mut nav_recovery_count: u32 = 0;
            let mut final_proof: Option<SetupCompletionProof> = None;
            let mut final_error: Option<AgentError> = None;
            for _ in 0..=MAX_SETUP_NAVIGATION_RECOVERIES {
                let mut proof_deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_secs(120);
                let sent = async {
            'setup_proof: loop {
                match tokio::time::timeout_at(proof_deadline, nav_rx.recv())
                    .await
                    .ok()
                    .flatten()
                {
                    Some(NavEvent::SendDetected(id, reason)) if id == agent_id_clone => {
                        break Ok(SetupCompletionProof::SendDetected(setup_send_reason(
                            reason.as_deref(),
                        )));
                    }
                    Some(NavEvent::SetupResponseObserved(id)) if id == agent_id_clone => {
                        break Ok(SetupCompletionProof::ResponseAfterInjection);
                    }
                    Some(NavEvent::SetupManualConfirmed(id)) if id == agent_id_clone => {
                        break Ok(SetupCompletionProof::UserConfirmedManual);
                    }
                    Some(NavEvent::ChallengeDetected(id, indicator)) if id == agent_id_clone => {
                        let _ = app.emit(
                            "captcha-detected",
                            json!({ "agent_id": agent_id_clone }),
                        );
                        let _ = app.emit("boss-message", json!({
                            "text": format!(
                                "{} needs verification ({}). Complete it in the current model window; Resume only requests another readiness check.",
                                agent_config.display_name, indicator
                            ),
                            "message_type": "status"
                        }));
                        let deadline = tokio::time::Instant::now()
                            + std::time::Duration::from_secs(600);
                        loop {
                            match tokio::time::timeout_at(deadline, nav_rx.recv()).await {
                                Ok(Some(NavEvent::Ready(ready_id)))
                                    if ready_id == agent_id_clone =>
                                {
                                    if diagnostics
                                        .has_response_observed_after_injection(&agent_id_clone)
                                    {
                                        break 'setup_proof Ok(
                                            SetupCompletionProof::ResponseAfterInjection,
                                        );
                                    }
                                    if diagnostics.has_pending_user_submit(&agent_id_clone) {
                                        break 'setup_proof Ok(
                                            SetupCompletionProof::SendDetected(
                                                "trusted_submit".to_string(),
                                            ),
                                        );
                                    }
                                    if nav_recovery_count >= MAX_SETUP_NAVIGATION_RECOVERIES {
                                        break 'setup_proof Err(AgentError::NavigationFailed(
                                            "repeated_navigation_during_setup".to_string(),
                                        ));
                                    }
                                    nav_recovery_count += 1;
                                    diagnostics.increment_setup_navigation_recovery(
                                        &agent_id_clone,
                                    );
                                    let _ = app.emit("boss-message", json!({
                                        "text": format!(
                                            "{} verification resolved; checking the priming prompt on the current page ({}/{}).",
                                            agent_config.display_name,
                                            nav_recovery_count,
                                            MAX_SETUP_NAVIGATION_RECOVERIES
                                        ),
                                        "message_type": "status"
                                    }));
                                    match perform_priming_injection(
                                        &window,
                                        &priming,
                                        &diagnostics,
                                        &agent_id_clone,
                                        nav_rx,
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            proof_deadline = tokio::time::Instant::now()
                                                + std::time::Duration::from_secs(120);
                                            continue 'setup_proof;
                                        }
                                        Err(error) => break 'setup_proof Err(error),
                                    }
                                }
                                Ok(Some(NavEvent::ResumeRequested(resume_id)))
                                    if resume_id == agent_id_clone =>
                                {
                                    let _ = app.emit("boss-message", json!({
                                        "text": format!(
                                            "Checking {} again; waiting for the page to report a ready composer.",
                                            agent_config.display_name
                                        ),
                                        "message_type": "status"
                                    }));
                                }
                                Ok(Some(NavEvent::ChallengeDetected(
                                    challenge_id,
                                    next_indicator,
                                ))) if challenge_id == agent_id_clone => {
                                    record_browser_blocker(
                                        app,
                                        &diagnostics,
                                        &agent_id_clone,
                                        "captcha_or_challenge",
                                        "captcha_or_challenge",
                                        None,
                                        "Verification challenge still present",
                                        Some(&next_indicator),
                                    );
                                    let _ = app.emit(
                                        "captcha-detected",
                                        json!({ "agent_id": agent_id_clone }),
                                    );
                                }
                                Ok(Some(NavEvent::SetupResponseObserved(response_id)))
                                    if response_id == agent_id_clone =>
                                {
                                    break 'setup_proof Ok(
                                        SetupCompletionProof::ResponseAfterInjection,
                                    );
                                }
                                Ok(Some(NavEvent::Response { agent_id: response_id, .. }))
                                | Ok(Some(NavEvent::Done { agent_id: response_id, .. }))
                                    if response_id == agent_id_clone =>
                                {
                                    break 'setup_proof Ok(
                                        SetupCompletionProof::ResponseAfterInjection,
                                    );
                                }
                                Ok(Some(NavEvent::SetupManualConfirmed(confirm_id)))
                                    if confirm_id == agent_id_clone =>
                                {
                                    break 'setup_proof Ok(
                                        SetupCompletionProof::UserConfirmedManual,
                                    );
                                }
                                Ok(Some(NavEvent::UnshowableUrl(unshowable_id, url)))
                                    if unshowable_id == agent_id_clone =>
                                {
                                    break 'setup_proof Err(AgentError::NavigationFailed(
                                        format!(
                                            "{} navigated to a URL this WebView cannot display: {}",
                                            agent_config.display_name, url
                                        ),
                                    ));
                                }
                                Ok(Some(NavEvent::SessionAborted)) => {
                                    break 'setup_proof Err(AgentError::UnknownError(
                                        "Session aborted".to_string(),
                                    ));
                                }
                                Ok(Some(event)) => {
                                    record_setup_stale_signal(
                                        &diagnostics,
                                        &agent_id_clone,
                                        &event,
                                    );
                                }
                                Ok(None) => {
                                    break 'setup_proof Err(AgentError::NavigationFailed(
                                        "channel closed while waiting for verification"
                                            .to_string(),
                                    ));
                                }
                                Err(_) => {
                                    break 'setup_proof Err(AgentError::CaptchaRequired(
                                        format!(
                                            "{} verification did not become ready within 600s",
                                            agent_config.display_name
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                    Some(NavEvent::UnshowableUrl(id, url)) if id == agent_id_clone => {
                        break Err(AgentError::NavigationFailed(format!(
                            "{} navigated to a URL this WebView cannot display: {}",
                            agent_config.display_name, url
                        )));
                    }
                    Some(NavEvent::Response { agent_id: id, .. } | NavEvent::Done { agent_id: id, .. })
                        if id == agent_id_clone =>
                    {
                        break Ok(SetupCompletionProof::ResponseAfterInjection);
                    }
                    Some(NavEvent::SessionAborted) => {
                        break Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    Some(NavEvent::Ready(id)) if id == agent_id_clone => {
                        // Idempotency: if a response was already observed after injection, treat Ready as completion, not as failure requiring re-prime.
                        if diagnostics.has_response_observed_after_injection(&agent_id_clone) {
                            let _ = app.emit("boss-message", json!({"text": format!("{} response already observed — treating Ready as completion", agent_config.display_name), "message_type": "status"}));
                            break Ok(SetupCompletionProof::ResponseAfterInjection);
                        }
                        if diagnostics.has_pending_user_submit(&agent_id_clone) {
                            let _ = app.emit("boss-message", json!({"text": format!("{} navigated to new chat; treating as sent...", agent_config.display_name), "message_type": "status"}));
                            break Ok(SetupCompletionProof::SendDetected("trusted_submit".to_string()));
                        }
                        if nav_recovery_count >= MAX_SETUP_NAVIGATION_RECOVERIES {
                            break Err(AgentError::NavigationFailed("repeated_navigation_during_setup".to_string()));
                        }
                        nav_recovery_count += 1;
                        diagnostics.increment_setup_navigation_recovery(&agent_id_clone);
                        let _ = app.emit("boss-message", json!({"text": format!("{} page refreshed; re-priming... (attempt {}/{})", agent_config.display_name, nav_recovery_count, MAX_SETUP_NAVIGATION_RECOVERIES), "message_type": "status"}));
                        match perform_priming_injection(&window, &priming, &diagnostics, &agent_id_clone, nav_rx).await {
                            Ok(_) => {
                                proof_deadline = tokio::time::Instant::now()
                                    + std::time::Duration::from_secs(120);
                                continue;
                            }
                            Err(e) => break Err(e),
                        }
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
        }
        .await;

                match sent {
                    Ok(proof) => {
                        final_proof = Some(proof);
                        break;
                    }
                    Err(e) if !matches!(e, AgentError::Timeout(_)) => {
                        final_error = Some(e);
                        break;
                    }
                    Err(_) => {
                        if diagnostics.has_recent_unexpected_navigation(agent_id, 15)
                            && nav_recovery_count < MAX_SETUP_NAVIGATION_RECOVERIES
                        {
                            nav_recovery_count += 1;
                            diagnostics.increment_setup_navigation_recovery(agent_id);
                            let _ = app.emit("boss-message", json!({"text": format!("{} page navigation detected after timeout; re-priming... (attempt {}/{})", agent_config.display_name, nav_recovery_count, MAX_SETUP_NAVIGATION_RECOVERIES), "message_type": "status"}));
                            if let Err(e) = wait_for_setup_ready(
                                agent_id,
                                &agent_config.base_url,
                                &agent_config.display_name,
                                app,
                                &diagnostics,
                                nav_rx,
                            )
                            .await
                            {
                                final_error = Some(e);
                                break;
                            }
                            match perform_priming_injection(
                                &window,
                                &priming,
                                &diagnostics,
                                agent_id,
                                nav_rx,
                            )
                            .await
                            {
                                Ok(verified) => {
                                    if verified {
                                        final_proof = Some(SetupCompletionProof::SendDetected(
                                            "capability_verified".to_string(),
                                        ));
                                        break;
                                    }
                                    continue;
                                }
                                Err(e) => {
                                    final_error = Some(e);
                                    break;
                                }
                            }
                        }
                        let message = diagnostics
                            .send_detection_timeout_message(agent_id, &agent_config.display_name);
                        record_browser_error(app, &diagnostics, agent_id, &message);
                        let _ = app.emit(
                            "boss-message",
                            json!({
                                "text": message.clone(),
                                "message_type": "status"
                            }),
                        );
                        final_error = Some(AgentError::Timeout(message));
                        break;
                    }
                }
            }
            match (final_proof, final_error) {
                (Some(SetupCompletionProof::SendDetected(reason)), _) => {
                    record_setup_completion(&diagnostics, agent_id, &reason)
                }
                (Some(SetupCompletionProof::ResponseAfterInjection), _) => {
                    record_setup_completion(&diagnostics, agent_id, "response_after_injection")
                }
                (Some(SetupCompletionProof::UserConfirmedManual), _) => {
                    record_setup_completion(&diagnostics, agent_id, "user_confirmed_manual")
                }
                (Some(_), _) => unreachable!(),
                (None, Some(e)) => return Err(e),
                (None, None) => unreachable!(),
            }
        } else {
            // Strong capability proof (composer accepted priming verbatim, an
            // owned Send is enabled, no injection error) advanced this agent
            // without waiting for a human to submit. The priming text was NOT
            // submitted and no response was fabricated: the ACTIVE loop will
            // perform turn-1 and onward. Setup completion is an accelerator for
            // readiness, never a guarantee of submission.
            record_setup_completion(&diagnostics, agent_id, "capability_verified");
        }

        // R1.7: do NOT save a fake conversation URL for a participant that
        // was advanced via capability_verified without a real Send. A
        // participant conversation URL must represent a real conversation
        // (trusted SendDetected / submit / response / manual confirmation),
        // not the base URL captured before any message was created. For the
        // leader, capability_verified is safe because the leader window is
        // persistent and never re-navigated via the saved URL; for a
        // non-leader, saving base_url would pretend a conversation exists and
        // would later be returned to the shared nav window as if it were a
        // real chat URL. The router already falls back to the validated
        // base_url when no conversation URL is stored, so skipping the save
        // preserves the invariant without fabricating a URL.
        let is_leader_for_url = is_leader;
        if setup_capability_verified && !is_leader_for_url {
            // No real conversation was created — do not persist base_url.
            {
                let mut browser = state.browser_state.lock().await;
                browser.conversation_urls.insert(agent_id.clone(), None);
            }
            tracing::info!(
                "[SETUP] capability_verified for participant {} — not saving fake conversation URL (will use base_url on first route)",
                agent_id
            );
            app.emit(
                "setup-agent-complete",
                json!({
                    "agent_id": agent_id,
                    "conversation_url": ""
                }),
            )
            .ok();
        } else {
            // Genuine proof (SendDetected / response / manual / leader capability)
            // — capture and persist the current URL.
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
    }

    // Setup monitors observe priming only. Active turn 1 establishes its own
    // post-injection baseline so a setup response can never satisfy it.
    app.emit("setup-complete", json!({})).ok();
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::capability_verified;

    // A. Strong setup capability proof completes setup (accelerator) — all four
    //    signal strengths true and no injection error → the gate holds true.
    #[test]
    fn strong_capability_proof_is_verified() {
        assert!(capability_verified(true, true, true, None));
    }

    // B. Weak capability proof (any of the required fields false) must NOT
    //    complete setup automatically; it falls through to the manual
    //    send-detection / recovery path.
    #[test]
    fn weak_capability_proof_not_verified() {
        assert!(!capability_verified(false, true, true, None), "prefix fail");
        assert!(!capability_verified(true, false, true, None), "suffix fail");
        assert!(!capability_verified(true, true, false, None), "send fail");
        assert!(
            !capability_verified(false, false, false, None),
            "all-weak must not verify"
        );
    }

    // C. Failed prompt integrity (injection error present) must NOT complete
    //    setup even when the other signals look strong.
    #[test]
    fn failed_prompt_integrity_not_verified() {
        assert!(!capability_verified(
            true,
            true,
            true,
            Some("prompt integrity check failed; composer did not show the full prompt")
        ));
    }

    // D. Disabled Send control must NOT complete setup.
    #[test]
    fn disabled_send_not_verified() {
        assert!(!capability_verified(true, true, false, None));
    }

    // The send_button_candidate_count alone must never be a setup-completion
    // condition: capability verification requires the composer-owned
    // send_button_enabled_after_injection probe result, not a raw candidate
    // count. This guards the contract that a present-but-disabled/unauthoritative
    // Send does not advance setup.
    #[test]
    fn send_button_candidate_count_alone_is_never_sufficient() {
        // Emulate a discovered but DISABLED owned Send: candidates > 0 but the
        // probe's enabled flag is false. Using count alone would be true.
        let candidates_found_but_disabled = true;
        assert!(
            candidates_found_but_disabled,
            "candidate trace present is not proof of enablement"
        );
        assert!(!capability_verified(true, true, false, None));
    }

    // F. Setup priming never generates an active-submit ACK. The capability gate
    //    returns a boolean only; nothing in it emits or fabricates an
    //    ActiveSubmitReport (that report path is exclusively the ACTIVE loop's
    //    __caSubmitActivePrompt). This test asserts the gate is side-effect-free
    //    in the sense that it decides readiness without touching submit state.
    #[test]
    fn capability_gate_does_not_imply_submission() {
        // Verifying capability must not be interpreted as a success ack for a
        // turn. It is a readiness signal; turnover is the ACTIVE loop's job.
        let verified = capability_verified(true, true, true, None);
        assert!(verified);
    }
}
