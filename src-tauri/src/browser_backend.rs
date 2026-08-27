use crate::errors::AgentError;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync;
pub type AsyncNavReceiver<T> = sync::mpsc::Receiver<T>;

pub const LEADER_WINDOW_LABEL: &str = "arena-leader";
pub const NAV_WINDOW_LABEL: &str = "arena-nav";
pub const READINESS_TIMEOUT_MS: u32 = 45_000;
pub const READINESS_WAIT_TIMEOUT_SECS: u64 = 50;

#[derive(Debug, Clone, Serialize)]
pub struct BrowserDiagnosticRecord {
    pub agent_id: String,
    pub display_name: String,
    pub setup_generation: u32,
    pub session_id: String,
    pub selected_leader_id: String,
    pub selected_agent_ids: Vec<String>,
    pub setup_order: Vec<String>,
    pub intended_url: String,
    pub window_label: String,
    pub window_kind: String,
    pub assigned_window_label: String,
    pub assigned_window_kind: String,
    pub is_selected_leader: bool,
    pub created_at: String,
    pub last_navigation_url: Option<String>,
    pub last_ready_at: Option<String>,
    pub last_send_detected_at: Option<String>,
    pub last_response_at: Option<String>,
    pub last_error: Option<String>,
    pub current_phase: String,
    pub last_blocker: String,
    pub last_blocker_url_redacted: Option<String>,
    pub last_challenge_detected_at: Option<String>,
    pub resume_attempt_count: u32,
    pub last_resume_at: Option<String>,
    pub input_found: bool,
    pub send_button_found: bool,
    pub last_send_probe_at: Option<String>,
    pub last_user_submit_event_at: Option<String>,
    pub last_message_count_seen: Option<u32>,
    pub sent_signal_emitted: bool,
    pub expected_agent_id: Option<String>,
    pub last_signal_agent_id: Option<String>,
    pub last_signal_type: Option<String>,
    pub last_signal_at: Option<String>,
    pub stale_signal_count: u32,
    pub response_observed_before_send: bool,
    pub response_observed_after_injection: bool,
    pub setup_completion_reason: Option<String>,
    pub prompt_injected_at: Option<String>,
    pub prompt_injection_error: Option<String>,
    pub prompt_injection_method: Option<String>,
    pub prompt_visible_prefix_ok: Option<bool>,
    pub prompt_visible_suffix_ok: Option<bool>,
    pub prompt_visible_length: Option<u32>,
    pub send_button_enabled_after_injection: Option<bool>,
    pub injection_target_tag: Option<String>,
    pub injection_target_role: Option<String>,
    pub injection_target_contenteditable: Option<String>,
    pub readiness_timeout_ms: Option<u32>,
    pub readiness_probe_count: Option<u32>,
    pub input_candidate_count: Option<u32>,
    pub composer_candidate_count: Option<u32>,
    pub send_button_candidate_count: Option<u32>,
    pub page_state_hint: Option<String>,
    pub page_health_hint: Option<String>,
    pub active_expected_agent_id: Option<String>,
    pub active_turn_number: Option<u32>,
    pub last_active_prompt_injected_at: Option<String>,
    pub last_active_response_at: Option<String>,
    pub active_auto_submit_attempted: bool,
    pub active_auto_submit_succeeded: Option<bool>,
    pub active_auto_submit_method: Option<String>,
    pub active_send_button_enabled_before_submit: Option<bool>,
    pub active_submit_error: Option<String>,
    pub active_submit_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserSetupMetadata {
    pub setup_generation: u32,
    pub session_id: String,
    pub selected_leader_id: String,
    pub selected_agent_ids: Vec<String>,
    pub setup_order: Vec<String>,
}

#[derive(Clone)]
pub struct BrowserDiagnostics {
    records: Arc<Mutex<HashMap<String, BrowserDiagnosticRecord>>>,
    active_by_window: Arc<Mutex<HashMap<String, String>>>,
    metadata: Arc<Mutex<BrowserSetupMetadata>>,
}

impl BrowserDiagnostics {
    fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            active_by_window: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(BrowserSetupMetadata::default())),
        }
    }

    pub fn snapshot(&self) -> Vec<BrowserDiagnosticRecord> {
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        records
    }

    pub fn begin_setup_run(&self, metadata: BrowserSetupMetadata) {
        self.records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
        self.active_by_window
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
        *self
            .metadata
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = metadata;
    }

    fn register(&self, agent_id: &str, window_label: &str, window_kind: &str) {
        let intended_url = get_agent_config(agent_id)
            .map(|config| config.base_url)
            .unwrap_or_default();
        let metadata = self
            .metadata
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        let is_selected_leader = metadata.selected_leader_id == agent_id;
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let record = records
            .entry(agent_id.to_string())
            .or_insert_with(|| BrowserDiagnosticRecord {
                agent_id: agent_id.to_string(),
                display_name: display_name_for(agent_id).to_string(),
                setup_generation: metadata.setup_generation,
                session_id: metadata.session_id.clone(),
                selected_leader_id: metadata.selected_leader_id.clone(),
                selected_agent_ids: metadata.selected_agent_ids.clone(),
                setup_order: metadata.setup_order.clone(),
                intended_url: intended_url.to_string(),
                window_label: window_label.to_string(),
                window_kind: window_kind.to_string(),
                assigned_window_label: window_label.to_string(),
                assigned_window_kind: window_kind.to_string(),
                is_selected_leader,
                created_at: now_timestamp(),
                last_navigation_url: None,
                last_ready_at: None,
                last_send_detected_at: None,
                last_response_at: None,
                last_error: None,
                current_phase: "unknown".to_string(),
                last_blocker: "none".to_string(),
                last_blocker_url_redacted: None,
                last_challenge_detected_at: None,
                resume_attempt_count: 0,
                last_resume_at: None,
                input_found: false,
                send_button_found: false,
                last_send_probe_at: None,
                last_user_submit_event_at: None,
                last_message_count_seen: None,
                sent_signal_emitted: false,
                expected_agent_id: None,
                last_signal_agent_id: None,
                last_signal_type: None,
                last_signal_at: None,
                stale_signal_count: 0,
                response_observed_before_send: false,
                response_observed_after_injection: false,
                setup_completion_reason: None,
                prompt_injected_at: None,
                prompt_injection_error: None,
                prompt_injection_method: None,
                prompt_visible_prefix_ok: None,
                prompt_visible_suffix_ok: None,
                prompt_visible_length: None,
                send_button_enabled_after_injection: None,
                injection_target_tag: None,
                injection_target_role: None,
                injection_target_contenteditable: None,
                readiness_timeout_ms: None,
                readiness_probe_count: None,
                input_candidate_count: None,
                composer_candidate_count: None,
                send_button_candidate_count: None,
                page_state_hint: None,
                page_health_hint: None,
                active_expected_agent_id: None,
                active_turn_number: None,
                last_active_prompt_injected_at: None,
                last_active_response_at: None,
                active_auto_submit_attempted: false,
                active_auto_submit_succeeded: None,
                active_auto_submit_method: None,
                active_send_button_enabled_before_submit: None,
                active_submit_error: None,
                active_submit_at: None,
            });
        record.display_name = display_name_for(agent_id).to_string();
        record.setup_generation = metadata.setup_generation;
        record.session_id = metadata.session_id;
        record.selected_leader_id = metadata.selected_leader_id;
        record.selected_agent_ids = metadata.selected_agent_ids;
        record.setup_order = metadata.setup_order;
        record.intended_url = intended_url.to_string();
        record.window_label = window_label.to_string();
        record.window_kind = window_kind.to_string();
        record.assigned_window_label = window_label.to_string();
        record.assigned_window_kind = window_kind.to_string();
        record.is_selected_leader = is_selected_leader;
    }

    fn set_active(&self, window_label: &str, agent_id: &str) {
        self.active_by_window
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(window_label.to_string(), agent_id.to_string());
    }

    fn active_agent(&self, window_label: &str) -> Option<String> {
        self.active_by_window
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(window_label)
            .cloned()
    }

    pub fn is_active(&self, window_label: &str, agent_id: &str) -> bool {
        self.active_agent(window_label).as_deref() == Some(agent_id)
    }

    pub fn last_real_navigation_url(&self, agent_id: &str) -> Option<String> {
        self.records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(agent_id)
            .and_then(|record| record.last_navigation_url.clone())
    }

    pub fn prompt_already_visible(&self, agent_id: &str) -> bool {
        self.records.lock().unwrap_or_else(|poison| poison.into_inner())
            .get(agent_id)
            .map(|record| record.prompt_injected_at.is_some()
                && record.prompt_visible_prefix_ok == Some(true)
                && record.prompt_visible_suffix_ok == Some(true))
            .unwrap_or(false)
    }

    pub fn setup_completed(&self, agent_id: &str) -> bool {
        self.records.lock().unwrap_or_else(|poison| poison.into_inner())
            .get(agent_id)
            .and_then(|record| record.setup_completion_reason.as_ref())
            .is_some()
    }

    pub fn is_expected_unfinished(&self, agent_id: &str) -> bool {
        self.records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(agent_id)
            .map(|record| {
                record.expected_agent_id.as_deref() == Some(agent_id)
                    && record.setup_completion_reason.is_none()
            })
            .unwrap_or(false)
    }

    pub fn mark_setup_failed_recoverable(&self) -> Option<String> {
        let mut records = self.records.lock().unwrap_or_else(|poison| poison.into_inner());
        let record = records.values_mut().find(|record| {
            record.expected_agent_id.as_deref() == Some(record.agent_id.as_str())
                && record.setup_completion_reason.is_none()
        })?;
        record.current_phase = "setup_failed_recoverable".to_string();
        Some(record.agent_id.clone())
    }

    pub fn send_detection_timeout_message(&self, agent_id: &str, display_name: &str) -> String {
        let records = self
            .records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(record) = records.get(agent_id) else {
            return format!(
                "{display_name} is ready and prompt appears sent, but no browser send signal was detected. Check Settings → Diagnostics."
            );
        };
        if record.prompt_injected_at.is_some()
            && record.send_button_enabled_after_injection == Some(false)
        {
            return format!(
                "{display_name} prompt was inserted, but the send button stayed disabled. Click the composer and type one character/delete it, or retry. Check Settings → Diagnostics."
            );
        }
        let mut details = Vec::new();
        if !record.input_found {
            details.push("input/composer was not found by the send detector");
        }
        if !record.send_button_found {
            details.push("send button was not found by the send detector");
        }
        if record.last_user_submit_event_at.is_some() && !record.sent_signal_emitted {
            details.push("a trusted send action was seen, but the sent signal was not emitted");
        }
        if record.response_observed_before_send {
            details.push("response observed but send signal missing");
        }
        if record.response_observed_after_injection {
            details.push("response observed after prompt injection");
        }
        if details.is_empty() {
            format!(
                "{display_name} is ready and prompt appears sent, but no browser send signal was detected. Check Settings → Diagnostics."
            )
        } else {
            format!(
                "{display_name} is ready and prompt appears sent, but no browser send signal was detected. {}. Check Settings → Diagnostics.",
                details.join("; ")
            )
        }
    }

    pub fn readiness_timeout_message(&self, agent_id: &str, display_name: &str) -> String {
        let records = self
            .records
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(record) = records.get(agent_id) else {
            return format!(
                "{display_name} did not become ready before the readiness timeout. Check Settings → Diagnostics."
            );
        };
        let readiness_timeout_ms = record.readiness_timeout_ms.unwrap_or(READINESS_TIMEOUT_MS);
        let readiness_probe_count = record.readiness_probe_count.unwrap_or(0);
        let input_candidate_count = record.input_candidate_count.unwrap_or(0);
        let composer_candidate_count = record.composer_candidate_count.unwrap_or(0);
        let send_button_candidate_count = record.send_button_candidate_count.unwrap_or(0);
        let page_state_hint = record
            .page_state_hint
            .as_deref()
            .unwrap_or("composer_selector_miss");
        if record.last_navigation_url.is_some() {
            return format!(
                "{display_name} loaded but no composer was detected. Complete login/security checks or open a new chat, then retry. Classification: page_loaded_but_no_composer ({page_state_hint}). readiness_probe_count={readiness_probe_count}, input_candidate_count={input_candidate_count}, composer_candidate_count={composer_candidate_count}, send_button_candidate_count={send_button_candidate_count}, readiness_timeout_ms={readiness_timeout_ms}. Check Settings → Diagnostics."
            );
        }
        format!(
            "{display_name} did not become ready before the readiness timeout. Classification: {page_state_hint}. readiness_probe_count={readiness_probe_count}, input_candidate_count={input_candidate_count}, composer_candidate_count={composer_candidate_count}, send_button_candidate_count={send_button_candidate_count}, readiness_timeout_ms={readiness_timeout_ms}. Check Settings → Diagnostics."
        )
    }
}

#[derive(Clone, Serialize)]
struct BrowserDiagnosticPayload<'a> {
    agent_id: &'a str,
    window_label: &'a str,
    phase: &'a str,
    url: &'a str,
    message: &'a str,
    error: Option<&'a str>,
}

fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn is_sensitive_query_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key == "code"
        || key == "state"
        || key.contains("auth")
        || key.contains("key")
        || key.contains("session")
        || key.contains("cf_clearance")
}

fn redacted_url(value: &str) -> String {
    match value.parse::<tauri::Url>() {
        Ok(mut url) => {
            if url.query().is_some() {
                let pairs = url
                    .query_pairs()
                    .map(|(key, value)| {
                        if is_sensitive_query_key(&key) {
                            (key.into_owned(), "[REDACTED]".to_string())
                        } else {
                            (key.into_owned(), value.into_owned())
                        }
                    })
                    .collect::<Vec<_>>();
                url.set_query(None);
                if !pairs.is_empty() {
                    let mut serializer = url.query_pairs_mut();
                    for (key, value) in pairs {
                        serializer.append_pair(&key, &value);
                    }
                }
            }
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => value
            .split_once('?')
            .map(|(prefix, _)| format!("{prefix}?[REDACTED]"))
            .unwrap_or_else(|| value.to_string()),
    }
}

fn sanitized_url(value: &str) -> String {
    redacted_url(value)
}

fn is_real_external_url(value: &str) -> bool {
    value
        .parse::<tauri::Url>()
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

fn update_diagnostic<F>(
    diagnostics: &BrowserDiagnostics,
    agent_id: &str,
    update: F,
) -> Option<BrowserDiagnosticRecord>
where
    F: FnOnce(&mut BrowserDiagnosticRecord),
{
    let mut records = diagnostics
        .records
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let record = records.get_mut(agent_id)?;
    update(record);
    Some(record.clone())
}

fn emit_browser_diagnostic(app: &AppHandle, record: &BrowserDiagnosticRecord, message: &str) {
    let url = record
        .last_navigation_url
        .as_deref()
        .unwrap_or(record.intended_url.as_str());
    if let Err(error) = app.emit(
        "browser-diagnostic",
        BrowserDiagnosticPayload {
            agent_id: &record.agent_id,
            window_label: &record.window_label,
            phase: &record.current_phase,
            url,
            message,
            error: record.last_error.as_deref(),
        },
    ) {
        tracing::warn!("[BROWSER] Failed to emit browser-diagnostic: {error}");
    }
}

pub fn record_browser_error(
    app: &AppHandle,
    diagnostics: &BrowserDiagnostics,
    agent_id: &str,
    message: &str,
) {
    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = "error".to_string();
        record.last_error = Some(message.to_string());
    }) {
        tracing::error!("[BROWSER] {}: {}", agent_id, message);
        emit_browser_diagnostic(app, &record, "Model window failed");
    }
}

pub fn record_browser_blocker(
    app: &AppHandle,
    diagnostics: &BrowserDiagnostics,
    agent_id: &str,
    blocker: &str,
    phase: &str,
    url: Option<&str>,
    message: &str,
    error: Option<&str>,
) {
    let timestamp = now_timestamp();
    let url_redacted = url.map(redacted_url);
    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = phase.to_string();
        record.last_blocker = blocker.to_string();
        record.last_blocker_url_redacted = url_redacted.clone();
        record.last_error = error.map(|value| value.to_string());
        if blocker == "captcha_or_challenge" {
            record.last_challenge_detected_at = Some(timestamp.clone());
        }
        if let Some(url) = url_redacted
            .clone()
            .filter(|value| is_real_external_url(value))
        {
            record.last_navigation_url = Some(url);
        }
    }) {
        emit_browser_diagnostic(app, &record, message);
    }
}

pub fn record_browser_resume(app: &AppHandle, diagnostics: &BrowserDiagnostics, agent_id: &str) {
    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.resume_attempt_count = record.resume_attempt_count.saturating_add(1);
        record.last_resume_at = Some(now_timestamp());
        record.current_phase = "navigation_started".to_string();
        record.last_error = None;
    }) {
        emit_browser_diagnostic(app, &record, "Resume requested; re-checking model window");
    }
}

pub fn record_setup_expected_agent(diagnostics: &BrowserDiagnostics, agent_id: &str) {
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.expected_agent_id = Some(agent_id.to_string());
        record.last_signal_agent_id = None;
        record.last_signal_type = None;
        record.last_signal_at = None;
        record.stale_signal_count = 0;
        record.response_observed_before_send = false;
        record.response_observed_after_injection = false;
        record.setup_completion_reason = None;
    });
}

pub fn record_prompt_injected(diagnostics: &BrowserDiagnostics, agent_id: &str) {
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = "prompt_injected".to_string();
        record.prompt_injected_at = Some(now_timestamp());
        record.prompt_injection_error = None;
        record.response_observed_after_injection = false;
        record.setup_completion_reason = None;
    });
}

pub fn record_prompt_injection_report(
    diagnostics: &BrowserDiagnostics,
    agent_id: &str,
    method: String,
    prefix_ok: bool,
    suffix_ok: bool,
    visible_length: Option<u32>,
    send_enabled: bool,
    target_tag: String,
    target_role: String,
    target_contenteditable: String,
    error: Option<String>,
) {
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.prompt_injection_method = Some(method);
        record.prompt_visible_prefix_ok = Some(prefix_ok);
        record.prompt_visible_suffix_ok = Some(suffix_ok);
        record.prompt_visible_length = visible_length;
        record.send_button_enabled_after_injection = Some(send_enabled);
        record.injection_target_tag = Some(target_tag);
        record.injection_target_role = Some(target_role);
        record.injection_target_contenteditable = Some(target_contenteditable);
        record.prompt_injection_error = error;
    });
}

pub fn record_prompt_injection_error(
    diagnostics: &BrowserDiagnostics,
    agent_id: &str,
    error: &str,
) {
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.prompt_injection_error = Some(error.to_string());
    });
}

fn nav_event_signal(event: &NavEvent) -> Option<(&str, &'static str)> {
    match event {
        NavEvent::Ready(agent_id) => Some((agent_id.as_str(), "ready")),
        NavEvent::Error(agent_id) => Some((agent_id.as_str(), "error")),
        NavEvent::Response(agent_id, _, _) => Some((agent_id.as_str(), "response")),
        NavEvent::Done(agent_id, _) => Some((agent_id.as_str(), "done")),
        NavEvent::SetupResponseObserved(agent_id) => Some((agent_id.as_str(), "setup-response")),
        NavEvent::SendDetected(agent_id, _) => Some((agent_id.as_str(), "sent")),
        NavEvent::SetupManualConfirmed(agent_id) => Some((agent_id.as_str(), "manual_confirm")),
        NavEvent::PromptInjectionReport { agent_id, .. } => Some((agent_id.as_str(), "prompt-injection")),
        NavEvent::ActiveSubmitReport { agent_id, .. } => Some((agent_id.as_str(), "active-submit")),
        NavEvent::SendProbe { agent_id, .. } => Some((agent_id.as_str(), "send-probe")),
        NavEvent::ChallengeDetected(agent_id, _) => Some((agent_id.as_str(), "challenge")),
        NavEvent::UnshowableUrl(agent_id, _) => Some((agent_id.as_str(), "unshowable")),
        NavEvent::ResumeRequested(agent_id) => Some((agent_id.as_str(), "resume")),
        NavEvent::ManualResponse { agent_id, .. } => Some((agent_id.as_str(), "manual_response")),
        NavEvent::UnsupportedNavigation { .. } | NavEvent::SessionAborted => None,
    }
}

fn record_signal_metadata(diagnostics: &BrowserDiagnostics, event: &NavEvent) {
    let Some((agent_id, signal_type)) = nav_event_signal(event) else {
        return;
    };
    let timestamp = now_timestamp();
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.last_signal_agent_id = Some(agent_id.to_string());
        record.last_signal_type = Some(signal_type.to_string());
        record.last_signal_at = Some(timestamp.clone());
        if record.expected_agent_id.is_some()
            && record.expected_agent_id.as_deref() != Some(agent_id)
        {
            record.stale_signal_count = record.stale_signal_count.saturating_add(1);
        }
        if matches!(
            event,
            NavEvent::Response(_, _, _) | NavEvent::Done(_, _) | NavEvent::SetupResponseObserved(_)
        ) && record.last_send_detected_at.is_none()
        {
            record.response_observed_before_send = true;
            if record.prompt_injected_at.is_some() {
                record.response_observed_after_injection = true;
            }
            record.last_error = Some("response observed but send signal missing".to_string());
        }
    });
}

pub fn record_setup_stale_signal(
    diagnostics: &BrowserDiagnostics,
    expected_agent_id: &str,
    event: &NavEvent,
) {
    let Some((signal_agent_id, signal_type)) = nav_event_signal(event) else {
        return;
    };
    if signal_agent_id == expected_agent_id {
        return;
    }
    let timestamp = now_timestamp();
    let _ = update_diagnostic(diagnostics, expected_agent_id, |record| {
        record.expected_agent_id = Some(expected_agent_id.to_string());
        record.last_signal_agent_id = Some(signal_agent_id.to_string());
        record.last_signal_type = Some(signal_type.to_string());
        record.last_signal_at = Some(timestamp);
        record.stale_signal_count = record.stale_signal_count.saturating_add(1);
    });
}

pub fn record_setup_completion(diagnostics: &BrowserDiagnostics, agent_id: &str, reason: &str) {
    let _ = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = if reason == "user_confirmed_manual" {
            "primed".to_string()
        } else {
            "setup_agent_complete".to_string()
        };
        record.setup_completion_reason = Some(reason.to_string());
        if reason == "user_confirmed_manual" {
            record.last_signal_agent_id = Some(agent_id.to_string());
            record.last_signal_type = Some("manual_confirm".to_string());
            record.last_signal_at = Some(now_timestamp());
        }
        if reason == "response_after_injection" {
            record.response_observed_after_injection = true;
            record.response_observed_before_send = record.last_send_detected_at.is_none();
        }
        record.last_error = None;
    });
}

pub fn record_nav_event(app: &AppHandle, diagnostics: &BrowserDiagnostics, event: &NavEvent) {
    record_signal_metadata(diagnostics, event);

    if let NavEvent::ActiveSubmitReport {
        agent_id,
        turn,
        succeeded,
        ..
    } = event
    {
        let _ = app.emit(
            "active-turn-state",
            serde_json::json!({
                "event": if *succeeded { "active_prompt_submitted" } else { "active_submit_failed" },
                "agent_id": agent_id,
                "turn_number": turn,
            }),
        );
    }

    match event {
        NavEvent::ChallengeDetected(agent_id, indicator) => {
            record_browser_blocker(
                app,
                diagnostics,
                agent_id,
                "captcha_or_challenge",
                "captcha_or_challenge",
                None,
                "Verification challenge detected",
                Some(indicator),
            );
            let _ = app.emit(
                "captcha-detected",
                serde_json::json!({ "agent_id": agent_id }),
            );
            let name = display_name_for(agent_id);
            let _ = app.emit(
                "boss-message",
                serde_json::json!({
                    "text": format!("{name} needs verification. Complete the check in the model window, then click Resume."),
                    "message_type": "status"
                }),
            );
            return;
        }
        NavEvent::UnshowableUrl(agent_id, url) => {
            record_browser_blocker(
                app,
                diagnostics,
                agent_id,
                "unsupported_url",
                "unshowable_url",
                Some(url),
                "WebView reached an unshowable URL page",
                Some("The URL can't be shown"),
            );
            return;
        }
        NavEvent::UnsupportedNavigation {
            window_label,
            url,
            reason,
        } => {
            if let Some(agent_id) = diagnostics.active_agent(window_label) {
                let message = if reason == "Unknown arena diagnostic signal ignored" {
                    "Unknown arena diagnostic signal ignored"
                } else {
                    "Unsupported or unshowable URL"
                };
                record_browser_blocker(
                    app,
                    diagnostics,
                    &agent_id,
                    "navigation_error",
                    "navigation_error",
                    Some(url),
                    message,
                    Some(reason),
                );
            }
            return;
        }
        NavEvent::ResumeRequested(agent_id) => {
            record_browser_resume(app, diagnostics, agent_id);
            return;
        }
        NavEvent::SendProbe {
            agent_id,
            input_found,
            send_button_found,
            user_submit_seen,
            message_count_seen,
            sent_signal_emitted,
            readiness_probe_count,
            input_candidate_count,
            composer_candidate_count,
            send_button_candidate_count,
            readiness_timeout_ms,
            page_state_hint,
            page_health_hint,
        } => {
            let timestamp = now_timestamp();
            let _ = update_diagnostic(diagnostics, agent_id, |record| {
                record.input_found = *input_found;
                record.send_button_found = *send_button_found;
                record.last_send_probe_at = Some(timestamp.clone());
                if *user_submit_seen {
                    record.last_user_submit_event_at = Some(timestamp.clone());
                }
                record.last_message_count_seen = *message_count_seen;
                record.sent_signal_emitted = *sent_signal_emitted;
                record.readiness_probe_count = *readiness_probe_count;
                record.input_candidate_count = *input_candidate_count;
                record.composer_candidate_count = *composer_candidate_count;
                record.send_button_candidate_count = *send_button_candidate_count;
                record.readiness_timeout_ms = *readiness_timeout_ms;
                record.page_state_hint = page_state_hint.clone();
                record.page_health_hint = page_health_hint.clone();
                if record.current_phase == "real_url_loaded" || record.current_phase == "navigation_started" {
                    record.current_phase = if *input_found {
                        "composer_detected".to_string()
                    } else {
                        "page_script_active".to_string()
                    };
                }
            });
            return;
        }
        NavEvent::SessionAborted => return,
        _ => {}
    }

    let (agent_id, phase, message) = match event {
        NavEvent::Ready(agent_id) => (agent_id, "composer_detected", "Composer detected"),
        NavEvent::Error(agent_id) => {
            let _ = update_diagnostic(diagnostics, agent_id, |record| {
                if record.readiness_timeout_ms.is_none() {
                    record.readiness_timeout_ms = Some(READINESS_TIMEOUT_MS);
                }
            });
            let message =
                diagnostics.readiness_timeout_message(agent_id, display_name_for(agent_id));
            record_browser_error(app, diagnostics, agent_id, &message);
            return;
        }
        NavEvent::SetupResponseObserved(agent_id) => {
            (agent_id, "consulting", "Setup response detected")
        }
        NavEvent::SendDetected(agent_id, _) => (agent_id, "consulting", "User send detected"),
        NavEvent::SetupManualConfirmed(agent_id) => (
            agent_id,
            "primed",
            "User confirmed setup completion",
        ),
        NavEvent::ManualResponse { agent_id, .. } => (
            agent_id,
            "active_response_captured",
            "User-provided active response received",
        ),
        NavEvent::PromptInjectionReport { agent_id, .. } => (
            agent_id,
            "prompt_injection_report",
            "Prompt injection report received; method, verification state, and error status recorded",
        ),
        NavEvent::ActiveSubmitReport { agent_id, succeeded, .. } => (
            agent_id,
            if *succeeded { "active_prompt_submitted" } else { "active_submit_failed" },
            if *succeeded { "Active prompt submitted automatically" } else { "Active prompt inserted but was not submitted" },
        ),
        NavEvent::Response(agent_id, _, _) => (agent_id, "ready", "Model response detected"),
        NavEvent::Done(agent_id, _) => (agent_id, "ready", "Model response completed"),
        NavEvent::ChallengeDetected(_, _)
        | NavEvent::UnshowableUrl(_, _)
        | NavEvent::UnsupportedNavigation { .. }
        | NavEvent::ResumeRequested(_)
        | NavEvent::SendProbe { .. }
        | NavEvent::SessionAborted => return,
    };
    let timestamp = now_timestamp();
    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = phase.to_string();
        record.last_error = None;
        if matches!(
            event,
            NavEvent::Ready(_)
                | NavEvent::SendDetected(_, _)
                | NavEvent::Response(_, _, _)
                | NavEvent::Done(_, _)
                | NavEvent::SetupResponseObserved(_)
        ) {
            record.last_blocker = "none".to_string();
            record.last_blocker_url_redacted = None;
        }
        match event {
            NavEvent::Ready(_) => {
                record.last_ready_at = Some(timestamp.clone());
                record.input_found = true;
                record.page_state_hint = Some("composer_detected".to_string());
                record.page_health_hint = Some("interactive".to_string());
            }
            NavEvent::SendDetected(_, _) => {
                record.last_send_detected_at = Some(timestamp.clone());
                record.last_user_submit_event_at = Some(timestamp.clone());
                record.sent_signal_emitted = true;
            }
            NavEvent::SetupManualConfirmed(_) => {
                record.setup_completion_reason = Some("user_confirmed_manual".to_string());
                record.last_signal_type = Some("manual_confirm".to_string());
                record.last_signal_agent_id = Some(record.agent_id.clone());
            }
            NavEvent::PromptInjectionReport {
                method,
                prefix_ok,
                suffix_ok,
                visible_length,
                send_enabled,
                target_tag,
                target_role,
                target_contenteditable,
                error,
                ..
            } => {
                record.prompt_injection_method = Some(method.clone());
                record.prompt_visible_prefix_ok = Some(*prefix_ok);
                record.prompt_visible_suffix_ok = Some(*suffix_ok);
                record.prompt_visible_length = *visible_length;
                record.send_button_enabled_after_injection = Some(*send_enabled);
                record.injection_target_tag = Some(target_tag.clone());
                record.injection_target_role = Some(target_role.clone());
                record.injection_target_contenteditable = Some(target_contenteditable.clone());
                record.prompt_injection_error = error.clone();
            }
            NavEvent::ActiveSubmitReport {
                turn,
                succeeded,
                method,
                send_enabled,
                error,
                ..
            } => {
                record.active_expected_agent_id = Some(record.agent_id.clone());
                record.active_turn_number = Some(*turn);
                record.active_auto_submit_attempted = true;
                record.active_auto_submit_succeeded = Some(*succeeded);
                record.active_auto_submit_method = Some(method.clone());
                record.active_send_button_enabled_before_submit = Some(*send_enabled);
                record.active_submit_error = error.clone();
                record.active_submit_at = Some(timestamp.clone());
            }
            NavEvent::Response(_, _, _)
            | NavEvent::Done(_, _)
            | NavEvent::SetupResponseObserved(_) => {
                record.last_response_at = Some(timestamp.clone());
                if record.active_expected_agent_id.as_deref() == Some(record.agent_id.as_str()) {
                    record.last_active_response_at = Some(timestamp.clone());
                }
            }
            NavEvent::ManualResponse { .. } => {
                record.last_response_at = Some(timestamp.clone());
                record.last_active_response_at = Some(timestamp.clone());
            }
            NavEvent::Error(_)
            | NavEvent::SendProbe { .. }
            | NavEvent::ChallengeDetected(_, _)
            | NavEvent::UnshowableUrl(_, _)
            | NavEvent::UnsupportedNavigation { .. }
            | NavEvent::ResumeRequested(_)
            | NavEvent::SessionAborted => {}
        }
    }) {
        emit_browser_diagnostic(app, &record, message);
    }
}

pub struct AgentConfig {
    pub agent_id: &'static str,
    pub display_name: &'static str,
    pub base_url: &'static str,
}

pub const AGENTS: &[AgentConfig] = &[
    AgentConfig {
        agent_id: "chatgpt",
        display_name: "ChatGPT",
        base_url: "https://chatgpt.com",
    },
    AgentConfig {
        agent_id: "claude",
        display_name: "Claude",
        base_url: "https://claude.ai",
    },
    AgentConfig {
        agent_id: "gemini",
        display_name: "Gemini",
        base_url: "https://gemini.google.com",
    },
    AgentConfig {
        agent_id: "deepseek",
        display_name: "DeepSeek",
        base_url: "https://chat.deepseek.com",
    },
    AgentConfig {
        agent_id: "qwen",
        display_name: "Qwen",
        base_url: "https://chat.qwen.ai",
    },
    // D-036: GLM via Z.ai
    AgentConfig {
        agent_id: "glm",
        display_name: "GLM",
        base_url: "https://chat.z.ai/",
    },
    // D-042: Kimi via Kimi.com (Lexical contenteditable editor)
    AgentConfig {
        agent_id: "kimi",
        display_name: "Kimi",
        base_url: "https://www.kimi.com/",
    },
];

pub fn get_agent_config(agent_id: &str) -> Option<&'static AgentConfig> {
    AGENTS.iter().find(|a| a.agent_id == agent_id)
}

pub fn display_name_for(agent_id: &str) -> &'static str {
    match agent_id {
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        "gemini" => "Gemini",
        "deepseek" => "DeepSeek",
        "qwen" => "Qwen",
        "glm" => "GLM",
        "kimi" => "Kimi",
        other => {
            eprintln!("[MEMORY] display_name_for: unknown agent_id '{other}'");
            "Unknown Model"
        }
    }
}

/// P1: an owned, merged view of a participant. `get_agent_config` returns a
/// `&'static` borrow limited to built-ins; this carries the same three fields
/// plus an `is_custom` flag so the unified runtime registry (and the frontend)
/// can distinguish the seven immutable built-ins from persisted custom entries.
/// It is serializable so the backend can expose the unified list via IPC.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ParticipantInfo {
    pub agent_id: String,
    pub display_name: String,
    pub base_url: String,
    pub is_custom: bool,
}

/// P1: merged registry lookup — the single source both the session validator
/// and the navigation-URL resolver use so they consume built-ins AND persisted
/// custom participants. Built-ins always win: if `agent_id` matches a built-in,
/// the built-in definition is authoritative and any same-id custom entry is
/// ignored. Custom entries only fill ids not covered by the built-in set.
pub fn resolve_participant(
    agent_id: &str,
    custom: &[crate::settings_store::CustomParticipant],
) -> Option<ParticipantInfo> {
    if let Some(builtin) = get_agent_config(agent_id) {
        return Some(ParticipantInfo {
            agent_id: builtin.agent_id.to_string(),
            display_name: builtin.display_name.to_string(),
            base_url: builtin.base_url.to_string(),
            is_custom: false,
        });
    }
    custom
        .iter()
        .find(|p| p.agent_id == agent_id)
        .map(|p| ParticipantInfo {
            agent_id: p.agent_id.clone(),
            display_name: p.display_name.clone(),
            base_url: p.base_url.clone(),
            is_custom: true,
        })
}

/// P3: the unified runtime registry — the single logical participant list the
/// UI/runtime consume. Built-ins are emitted first (in the frozen `AGENTS`
/// order, `is_custom: false`), then persisted custom participants in saved
/// order (`is_custom: true`). Custom entries can never alias a built-in id
/// (save-time validation rejects that, and this function masks any that leak
/// through), so built-ins always precede and shadow same-id customs.
pub fn merged_participants(
    custom: &[crate::settings_store::CustomParticipant],
) -> Vec<ParticipantInfo> {
    let mut merged: Vec<ParticipantInfo> = AGENTS
        .iter()
        .map(|builtin| ParticipantInfo {
            agent_id: builtin.agent_id.to_string(),
            display_name: builtin.display_name.to_string(),
            base_url: builtin.base_url.to_string(),
            is_custom: false,
        })
        .collect();
    for p in custom {
        if get_agent_config(&p.agent_id).is_none() {
            merged.push(ParticipantInfo {
                agent_id: p.agent_id.clone(),
                display_name: p.display_name.clone(),
                base_url: p.base_url.clone(),
                is_custom: true,
            });
        }
    }
    merged
}

/// P1: merged display-name resolution. Falls back to the built-in display name
/// when known; otherwise checks custom participants; else "Unknown Model".
pub fn resolve_display_name(
    agent_id: &str,
    custom: &[crate::settings_store::CustomParticipant],
) -> String {
    if let Some(info) = resolve_participant(agent_id, custom) {
        return info.display_name;
    }
    "Unknown Model".to_string()
}



#[derive(Debug)]
pub enum NavEvent {
    Ready(String),
    Error(String),
    Response(String, u32, String),
    Done(String, u32),
    SetupResponseObserved(String),
    SendDetected(String, Option<String>),
    SetupManualConfirmed(String),
    /// Explicit user-entered content for the one active turn currently being
    /// awaited. This is deliberately distinct from a browser response event.
    ManualResponse {
        agent_id: String,
        turn: u32,
        response: String,
    },
    PromptInjectionReport {
        agent_id: String,
        method: String,
        prefix_ok: bool,
        suffix_ok: bool,
        visible_length: Option<u32>,
        send_enabled: bool,
        target_tag: String,
        target_role: String,
        target_contenteditable: String,
        error: Option<String>,
    },
    ActiveSubmitReport {
        agent_id: String,
        turn: u32,
        succeeded: bool,
        method: String,
        send_enabled: bool,
        error: Option<String>,
    },
    SendProbe {
        agent_id: String,
        input_found: bool,
        send_button_found: bool,
        user_submit_seen: bool,
        message_count_seen: Option<u32>,
        sent_signal_emitted: bool,
        readiness_probe_count: Option<u32>,
        input_candidate_count: Option<u32>,
        composer_candidate_count: Option<u32>,
        send_button_candidate_count: Option<u32>,
        readiness_timeout_ms: Option<u32>,
        page_state_hint: Option<String>,
        page_health_hint: Option<String>,
    },
    ChallengeDetected(String, String),
    UnshowableUrl(String, String),
    UnsupportedNavigation {
        window_label: String,
        url: String,
        reason: String,
    },
    ResumeRequested(String),
    SessionAborted,
}

// ── BrowserState ──────────────────────────────────────────────────────────────

pub struct BrowserState {
    pub leader_window: Option<WebviewWindow>,
    pub leader_agent_id: String,
    pub nav_window: Option<WebviewWindow>,
    pub conversation_urls: HashMap<String, Option<String>>,
    pub nav_tx: std::sync::mpsc::SyncSender<NavEvent>,
    pub diagnostics: BrowserDiagnostics,
    pub pending_sends: HashSet<String>,
    pub captcha_resolved: HashSet<String>,
    /// IMP-4: per-agent cooldown map.
    /// Key = agent_id, Value = Instant when the cooldown expires.
    pub cooldowns: HashMap<String, std::time::Instant>,
    pub active_turn: Option<(String, u32)>,
}

impl BrowserState {
    pub fn new(nav_tx: std::sync::mpsc::SyncSender<NavEvent>) -> Self {
        BrowserState {
            leader_window: None,
            leader_agent_id: String::new(),
            nav_window: None,
            conversation_urls: HashMap::new(),
            nav_tx,
            diagnostics: BrowserDiagnostics::new(),
            pending_sends: HashSet::new(),
            captcha_resolved: HashSet::new(),
            cooldowns: HashMap::new(),
            active_turn: None,
        }
    }

    pub fn select_window(&self, is_leader: bool) -> Option<WebviewWindow> {
        if is_leader {
            self.leader_window.clone()
        } else {
            self.nav_window.clone()
        }
    }

    /// IMP-4: Mark agent as rate-limited for `duration_secs` seconds.
    pub fn set_cooldown(&mut self, agent_id: &str, duration_secs: u64) {
        let expires = std::time::Instant::now() + std::time::Duration::from_secs(duration_secs);
        self.cooldowns.insert(agent_id.to_string(), expires);
        tracing::warn!(
            "[COOLDOWN] {} placed in cooldown for {}s",
            agent_id,
            duration_secs
        );
    }

    /// IMP-4: Returns true if the agent is still within its cooldown window.
    pub fn is_in_cooldown(&self, agent_id: &str) -> bool {
        match self.cooldowns.get(agent_id) {
            Some(expires) => std::time::Instant::now() < *expires,
            None => false,
        }
    }

    pub fn begin_active_turn(&mut self, agent_id: &str, turn: u32) {
        self.active_turn = Some((agent_id.to_string(), turn));
        let _ = update_diagnostic(&self.diagnostics, agent_id, |record| {
            record.active_expected_agent_id = Some(agent_id.to_string());
            record.active_turn_number = Some(turn);
            record.last_active_prompt_injected_at = Some(now_timestamp());
            record.current_phase = "active_prompt_injected".to_string();
            record.last_error = None;
        });
    }

    pub fn mark_active_waiting(&self, agent_id: &str, turn: u32) {
        let _ = update_diagnostic(&self.diagnostics, agent_id, |record| {
            record.active_expected_agent_id = Some(agent_id.to_string());
            record.active_turn_number = Some(turn);
            record.current_phase = "active_waiting_for_response".to_string();
        });
    }

    pub fn clear_active_turn(&mut self, agent_id: &str, turn: u32) {
        if self.active_turn.as_ref() == Some(&(agent_id.to_string(), turn)) {
            self.active_turn = None;
        }
        let _ = update_diagnostic(&self.diagnostics, agent_id, |record| {
            if record.active_turn_number == Some(turn) {
                record.current_phase = "active_response_captured".to_string();
                record.last_active_response_at = Some(now_timestamp());
            }
        });
    }
}

fn identity_script(agent_id: &str) -> Result<String, AgentError> {
    let agent_json = serde_json::to_string(agent_id).map_err(|error| {
        AgentError::InjectionFailed(format!("agent identity serialization failed: {error}"))
    })?;
    Ok(format!(
        "window.name = '__consensus_arena_agent__:' + {agent_json}; window.__ca_agentId = {agent_json};"
    ))
}

fn set_window_identity(window: &WebviewWindow, agent_id: &str) -> Result<(), AgentError> {
    let script = identity_script(agent_id)?;
    window.eval(&script).map_err(|error| {
        AgentError::InjectionFailed(format!("agent identity eval failed: {error}"))
    })
}

pub fn navigate_agent_window(
    app: &AppHandle,
    diagnostics: &BrowserDiagnostics,
    window: &WebviewWindow,
    agent_id: &str,
    window_kind: &str,
    target_url: &str,
) -> Result<(), AgentError> {
    let window_label = window.label().to_string();
    diagnostics.register(agent_id, &window_label, window_kind);
    diagnostics.set_active(&window_label, agent_id);

    let sanitized = sanitized_url(target_url);
    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.window_label = window_label.clone();
        record.window_kind = window_kind.to_string();
        record.last_error = None;
        record.current_phase = "creating".to_string();
    }) {
        emit_browser_diagnostic(app, &record, "Preparing model window");
    }

    let parsed_url = target_url.parse::<tauri::Url>().map_err(|error| {
        let message = format!("invalid external model URL {sanitized}: {error}");
        record_browser_error(app, diagnostics, agent_id, &message);
        AgentError::NavigationFailed(message)
    })?;
    if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
        let message = format!("external model URL is not absolute HTTP(S): {sanitized}");
        record_browser_error(app, diagnostics, agent_id, &message);
        return Err(AgentError::NavigationFailed(message));
    }

    set_window_identity(window, agent_id).map_err(|error| {
        record_browser_error(app, diagnostics, agent_id, &error.to_string());
        error
    })?;
    window.navigate(parsed_url).map_err(|error| {
        let message = format!("navigation request to {sanitized} failed: {error}");
        record_browser_error(app, diagnostics, agent_id, &message);
        AgentError::NavigationFailed(message)
    })?;
    window.show().map_err(|error| {
        let message = format!("showing {window_label} failed: {error}");
        record_browser_error(app, diagnostics, agent_id, &message);
        AgentError::NavigationFailed(message)
    })?;
    window.set_focus().map_err(|error| {
        let message = format!("focusing {window_label} failed: {error}");
        record_browser_error(app, diagnostics, agent_id, &message);
        AgentError::NavigationFailed(message)
    })?;

    if let Some(record) = update_diagnostic(diagnostics, agent_id, |record| {
        record.current_phase = "navigation_started".to_string();
    }) {
        emit_browser_diagnostic(
            app,
            &record,
            "Navigation requested; waiting for page readiness",
        );
    }
    Ok(())
}

fn handle_page_load(
    window: WebviewWindow,
    payload: tauri::webview::PageLoadPayload<'_>,
    diagnostics: &BrowserDiagnostics,
) {
    let window_label = window.label().to_string();
    let Some(agent_id) = diagnostics.active_agent(&window_label) else {
        return;
    };
    let app = window.app_handle().clone();

    if let Err(error) = set_window_identity(&window, &agent_id) {
        record_browser_error(&app, diagnostics, &agent_id, &error.to_string());
        return;
    }

    let url = sanitized_url(payload.url().as_str());
    let event = payload.event();
    if let Some(record) = update_diagnostic(diagnostics, &agent_id, |record| {
        if is_real_external_url(&url) {
            record.last_navigation_url = Some(url.clone());
        }
        record.current_phase = if event == PageLoadEvent::Finished {
            "real_url_loaded".to_string()
        } else {
            "navigation_started".to_string()
        };
    }) {
        tracing::debug!(
            "[BROWSER] {} {} page load {:?}: {}",
            window_label,
            agent_id,
            event,
            url
        );
        if event == PageLoadEvent::Finished {
            emit_browser_diagnostic(
                &app,
                &record,
                "Page load finished; waiting for ready signal",
            );
        }
    }
}

fn make_new_window_handler(
    tx: std::sync::mpsc::SyncSender<NavEvent>,
    window_label: &'static str,
) -> impl Fn(tauri::Url, tauri::webview::NewWindowFeatures) -> NewWindowResponse<tauri::Wry>
+ Send
+ 'static {
    move |url, _features| {
        send_nav_event(
            &tx,
            NavEvent::UnsupportedNavigation {
                window_label: window_label.to_string(),
                url: redacted_url(url.as_str()),
                reason: "new window request denied to preserve the two-WebView architecture"
                    .to_string(),
            },
        );
        NewWindowResponse::Deny
    }
}

// ── inject_to_window (lock-safe — caller drops BrowserState lock first) ───────

/// Inject a prompt using a pre-extracted window handle.
/// Used by response_router.rs — caller extracts window from BrowserState
/// and drops the lock BEFORE calling this function.
///
/// wait_ready: true  = wait for arena://ready signal (window just navigated)
/// wait_ready: false = inject immediately (leader window already loaded)
pub async fn inject_to_window(
    window: WebviewWindow,
    agent_id: &str,
    prompt: &str,
    turn: u32,
    nav_rx: &mut AsyncNavReceiver<NavEvent>,
    wait_ready: bool,
    auto_submit: bool,
) -> Result<(), AgentError> {
    if wait_ready {
        let agent_id_owned = agent_id.to_string();
        match tokio::time::timeout(
            std::time::Duration::from_secs(READINESS_WAIT_TIMEOUT_SECS),
            wait_for_ready(agent_id_owned, nav_rx),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(AgentError::Timeout(format!(
                    "Agent {} timed out waiting for ready signal",
                    agent_id
                )));
            }
        }
    }

    let js = build_inject_js(prompt, agent_id, turn, auto_submit);
    window
        .eval(&js)
        .map_err(|e| AgentError::InjectionFailed(format!("inject eval failed: {}", e)))?;

    Ok(())
}

/// Inject a prompt using BrowserState directly.
/// Used by session_runner.rs for setup phase.
pub async fn inject_to_agent(
    state: &BrowserState,
    agent_id: &str,
    is_leader: bool,
    prompt: &str,
    turn: u32,
    nav_rx: &mut AsyncNavReceiver<NavEvent>,
) -> Result<(), AgentError> {
    if let Some(win) = state.select_window(is_leader) {
        set_window_identity(&win, agent_id)?;
        if let Some(config) = get_agent_config(agent_id) {
            let url = config.base_url.parse::<tauri::Url>().map_err(|error| {
                AgentError::NavigationFailed(format!("url parse failed: {error}"))
            })?;
            win.navigate(url).map_err(|error| {
                AgentError::NavigationFailed(format!("navigation request failed: {error}"))
            })?;
        }
    }

    let window = state
        .select_window(is_leader)
        .ok_or_else(|| AgentError::NavigationFailed("window not initialised".to_string()))?;

    inject_to_window(window, agent_id, prompt, turn, nav_rx, true, false).await
}

// ── wait_for_ready ────────────────────────────────────────────────────────────

async fn wait_for_ready(
    agent_id: String,
    nav_rx: &mut AsyncNavReceiver<NavEvent>,
) -> Result<(), AgentError> {
    loop {
        match nav_rx.recv().await {
            Some(NavEvent::Ready(id)) if id == agent_id => return Ok(()),
            Some(NavEvent::Error(id)) if id == agent_id => {
                return Err(AgentError::NavigationFailed(format!(
                    "Agent {} reported error during ready wait (input field not found or timed out)",
                    id
                )));
            }
            Some(NavEvent::ChallengeDetected(id, indicator)) if id == agent_id => loop {
                match nav_rx.recv().await {
                    Some(NavEvent::ResumeRequested(resume_id)) if resume_id == agent_id => break,
                    Some(NavEvent::ChallengeDetected(challenge_id, _))
                        if challenge_id == agent_id =>
                    {
                        return Err(AgentError::CaptchaRequired(format!(
                            "Agent {} is still blocked by verification challenge: {}",
                            agent_id, indicator
                        )));
                    }
                    Some(NavEvent::UnshowableUrl(unshowable_id, url))
                        if unshowable_id == agent_id =>
                    {
                        return Err(AgentError::NavigationFailed(format!(
                            "Agent {} navigated to an unshowable URL: {}",
                            agent_id,
                            redacted_url(&url)
                        )));
                    }
                    Some(NavEvent::SessionAborted) => {
                        return Err(AgentError::UnknownError("Session aborted".to_string()));
                    }
                    Some(_) => continue,
                    None => {
                        return Err(AgentError::NavigationFailed(
                            "Navigation channel closed while waiting for challenge resume"
                                .to_string(),
                        ));
                    }
                }
            },
            Some(NavEvent::UnshowableUrl(id, url)) if id == agent_id => {
                return Err(AgentError::NavigationFailed(format!(
                    "Agent {} navigated to an unshowable URL: {}",
                    agent_id,
                    redacted_url(&url)
                )));
            }
            Some(NavEvent::SessionAborted) => {
                return Err(AgentError::UnknownError("Session aborted".to_string()));
            }
            None => {
                return Err(AgentError::NavigationFailed(
                    "Navigation channel closed while waiting for ready".to_string(),
                ));
            }
            _ => continue,
        }
    }
}

// ── on_navigation closure factory ────────────────────────────────────────────

fn make_nav_closure(
    tx: std::sync::mpsc::SyncSender<NavEvent>,
    window_label: &'static str,
) -> impl Fn(&tauri::Url) -> bool + Send + 'static {
    move |url| match url.scheme() {
        "arena" => {
            handle_arena_url(tx.clone(), window_label, url);
            false
        }
        "http" | "https" | "about" | "blob" | "data" => true,
        scheme => {
            send_nav_event(
                &tx,
                NavEvent::UnsupportedNavigation {
                    window_label: window_label.to_string(),
                    url: redacted_url(url.as_str()),
                    reason: format!("unsupported scheme: {scheme}"),
                },
            );
            false
        }
    }
}

fn handle_arena_url(
    tx: std::sync::mpsc::SyncSender<NavEvent>,
    window_label: &'static str,
    url: &tauri::Url,
) {
    let Some(signal) = parse_arena_signal(url) else {
        send_unknown_arena_signal(&tx, window_label, url);
        return;
    };

    match (signal.action.as_str(), signal.args.as_slice()) {
        ("ready", [agent_id]) => {
            let event = if agent_id.starts_with("error-") {
                NavEvent::Error(agent_id.trim_start_matches("error-").to_string())
            } else {
                NavEvent::Ready(agent_id.to_string())
            };
            send_nav_event(&tx, event);
        }
        ("error", [agent_id]) | ("error", [agent_id, _]) => {
            send_nav_event(&tx, NavEvent::Error(agent_id.to_string()));
        }
        ("response", [agent_id, turn_str, encoded]) => {
            if let Ok(turn) = turn_str.parse::<u32>() {
                let text = urlencoding::decode(encoded)
                    .unwrap_or_default()
                    .into_owned();
                send_nav_event(&tx, NavEvent::Response(agent_id.to_string(), turn, text));
            }
        }
        ("done", [agent_id, turn_str]) => {
            if let Ok(turn) = turn_str.parse::<u32>() {
                send_nav_event(&tx, NavEvent::Done(agent_id.to_string(), turn));
            }
        }
        ("setup-response", [agent_id]) => {
            send_nav_event(&tx, NavEvent::SetupResponseObserved(agent_id.to_string()));
        }
        ("sent", [agent_id]) => {
            send_nav_event(&tx, NavEvent::SendDetected(agent_id.to_string(), None));
        }
        ("sent", [agent_id, reason]) => {
            send_nav_event(
                &tx,
                NavEvent::SendDetected(agent_id.to_string(), Some(reason.to_string())),
            );
        }
        (
            "prompt-injection",
            [agent_id, method, prefix, suffix, length, enabled, tag, role, contenteditable, encoded_error],
        ) => {
            let error = urlencoding::decode(encoded_error)
                .unwrap_or_default()
                .into_owned();
            send_nav_event(
                &tx,
                NavEvent::PromptInjectionReport {
                    agent_id: agent_id.to_string(),
                    method: method.to_string(),
                    prefix_ok: prefix == "1",
                    suffix_ok: suffix == "1",
                    visible_length: length.parse::<u32>().ok(),
                    send_enabled: enabled == "1",
                    target_tag: tag.to_string(),
                    target_role: role.to_string(),
                    target_contenteditable: contenteditable.to_string(),
                    error: if error.is_empty() { None } else { Some(error) },
                },
            );
        }
        ("active-submit", [agent_id, turn, succeeded, method, enabled, encoded_error]) => {
            let error = urlencoding::decode(encoded_error)
                .unwrap_or_default()
                .into_owned();
            send_nav_event(
                &tx,
                NavEvent::ActiveSubmitReport {
                    agent_id: agent_id.to_string(),
                    turn: turn.parse::<u32>().unwrap_or_default(),
                    succeeded: succeeded == "1",
                    method: method.to_string(),
                    send_enabled: enabled == "1",
                    error: if error.is_empty() { None } else { Some(error) },
                },
            );
        }
        ("send-probe", args) => {
            let event = match args {
                [agent_id, input, button, submit, count, emitted] => NavEvent::SendProbe {
                    agent_id: agent_id.to_string(),
                    input_found: input == "1",
                    send_button_found: button == "1",
                    user_submit_seen: submit == "1",
                    message_count_seen: count.parse::<u32>().ok(),
                    sent_signal_emitted: emitted == "1",
                    readiness_probe_count: None,
                    input_candidate_count: None,
                    composer_candidate_count: None,
                    send_button_candidate_count: None,
                    readiness_timeout_ms: None,
                    page_state_hint: None,
                    page_health_hint: None,
                },
                [
                    agent_id,
                    input,
                    button,
                    submit,
                    count,
                    emitted,
                    input_candidates,
                    composer_candidates,
                    send_button_candidates,
                    readiness_probes,
                    timeout_ms,
                    encoded_hint,
                    encoded_health,
                ] => NavEvent::SendProbe {
                    agent_id: agent_id.to_string(),
                    input_found: input == "1",
                    send_button_found: button == "1",
                    user_submit_seen: submit == "1",
                    message_count_seen: count.parse::<u32>().ok(),
                    sent_signal_emitted: emitted == "1",
                    readiness_probe_count: readiness_probes.parse::<u32>().ok(),
                    input_candidate_count: input_candidates.parse::<u32>().ok(),
                    composer_candidate_count: composer_candidates.parse::<u32>().ok(),
                    send_button_candidate_count: send_button_candidates.parse::<u32>().ok(),
                    readiness_timeout_ms: timeout_ms.parse::<u32>().ok(),
                    page_state_hint: Some(
                        urlencoding::decode(encoded_hint)
                            .unwrap_or_default()
                            .into_owned(),
                    ),
                    page_health_hint: Some(
                        urlencoding::decode(encoded_health)
                            .unwrap_or_default()
                            .into_owned(),
                    ),
                },
                _ => {
                    send_unknown_arena_signal(&tx, window_label, url);
                    return;
                }
            };
            send_nav_event(&tx, event);
        }
        ("challenge", [agent_id]) | ("captcha", [agent_id]) => {
            send_nav_event(
                &tx,
                NavEvent::ChallengeDetected(agent_id.to_string(), "challenge".to_string()),
            );
        }
        ("challenge", [agent_id, encoded_indicator])
        | ("captcha", [agent_id, encoded_indicator]) => {
            let indicator = urlencoding::decode(encoded_indicator)
                .unwrap_or_default()
                .into_owned();
            send_nav_event(
                &tx,
                NavEvent::ChallengeDetected(agent_id.to_string(), indicator),
            );
        }
        ("unshowable", [agent_id, encoded_url]) => {
            let url = urlencoding::decode(encoded_url)
                .unwrap_or_default()
                .into_owned();
            send_nav_event(&tx, NavEvent::UnshowableUrl(agent_id.to_string(), url));
        }
        // D-040 Tier 2: WebView JS errors forwarded via arena://log/{level}/{msg}
        // No async, no lock, no nav_tx capture — tracing macros only per spec.
        ("log", [level, encoded_msg]) => {
            let msg = urlencoding::decode(encoded_msg)
                .unwrap_or_default()
                .into_owned();
            match level.as_str() {
                "error" => tracing::error!("[WEBVIEW] {}", msg),
                "warn" => tracing::warn!("[WEBVIEW] {}", msg),
                _ => tracing::info!("[WEBVIEW {}] {}", level.to_uppercase(), msg),
            }
        }
        _ => send_unknown_arena_signal(&tx, window_label, url),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ArenaSignal {
    action: String,
    args: Vec<String>,
}

fn parse_arena_signal(url: &tauri::Url) -> Option<ArenaSignal> {
    let host = url.host_str().unwrap_or_default();
    let path_segments = url
        .path()
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if !host.is_empty() {
        return Some(ArenaSignal {
            action: host.to_string(),
            args: path_segments,
        });
    }

    let mut segments = path_segments.into_iter();
    let action = segments.next()?;
    Some(ArenaSignal {
        action,
        args: segments.collect(),
    })
}

fn send_unknown_arena_signal(
    tx: &std::sync::mpsc::SyncSender<NavEvent>,
    window_label: &'static str,
    url: &tauri::Url,
) {
    send_nav_event(
        tx,
        NavEvent::UnsupportedNavigation {
            window_label: window_label.to_string(),
            url: redacted_url(url.as_str()),
            reason: "Unknown arena diagnostic signal ignored".to_string(),
        },
    );
}

fn send_nav_event(tx: &std::sync::mpsc::SyncSender<NavEvent>, event: NavEvent) {
    if let Err(e) = tx.try_send(event) {
        tracing::warn!(
            "[NAV] NavEvent dropped — channel full or disconnected: {:?}",
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AGENTS, ArenaSignal, GENERIC_INIT_SCRIPT, merged_participants, parse_arena_signal,
        resolve_display_name, resolve_participant, validate_window_registry,
    };
    use crate::settings_store::CustomParticipant;

    fn parse(value: &str) -> ArenaSignal {
        let url = match value.parse::<tauri::Url>() {
            Ok(url) => url,
            Err(error) => panic!("test URL should parse: {error}"),
        };
        match parse_arena_signal(&url) {
            Some(signal) => signal,
            None => panic!("arena signal should parse"),
        }
    }

    #[test]
    fn parses_host_based_arena_signals() {
        assert_eq!(
            parse("arena://ready/chatgpt"),
            ArenaSignal {
                action: "ready".to_string(),
                args: vec!["chatgpt".to_string()],
            }
        );
        assert_eq!(
            parse("arena://response/chatgpt/3/hello"),
            ArenaSignal {
                action: "response".to_string(),
                args: vec!["chatgpt".to_string(), "3".to_string(), "hello".to_string(),],
            }
        );
        assert_eq!(
            parse("arena://done/chatgpt/3"),
            ArenaSignal {
                action: "done".to_string(),
                args: vec!["chatgpt".to_string(), "3".to_string()],
            }
        );
        assert_eq!(
            parse("arena://sent/chatgpt"),
            ArenaSignal {
                action: "sent".to_string(),
                args: vec!["chatgpt".to_string()],
            }
        );
        assert_eq!(
            parse("arena://sent/chatgpt/trusted-click"),
            ArenaSignal {
                action: "sent".to_string(),
                args: vec!["chatgpt".to_string(), "trusted-click".to_string()],
            }
        );
        assert_eq!(
            parse("arena://setup-response/chatgpt"),
            ArenaSignal {
                action: "setup-response".to_string(),
                args: vec!["chatgpt".to_string()],
            }
        );
        assert_eq!(
            parse("arena://send-probe/chatgpt/1/1/1/2/0"),
            ArenaSignal {
                action: "send-probe".to_string(),
                args: vec![
                    "chatgpt".to_string(),
                    "1".to_string(),
                    "1".to_string(),
                    "1".to_string(),
                    "2".to_string(),
                    "0".to_string(),
                ],
            }
        );
    }

    #[test]
    fn generic_init_keeps_fixture_critical_detection_paths() {
        for required in [
            "div.ProseMirror[contenteditable=\"true\"]",
            "p[data-placeholder]",
            "send-probe",
            "empty_shell_or_hydration_stuck",
            "possible_login_required",
            "possible_challenge_or_security",
            "composer_selector_miss",
            "READY_TIMEOUT_MS = 45000",
            "__caSubmitActivePrompt",
            "active-submit",
            "MAX_SUBMIT_ATTEMPTS",
            "button.click()",
            "__ca_findOwnedSend",
        ] {
            assert!(GENERIC_INIT_SCRIPT.contains(required), "missing {required}");
        }
    }

    #[test]
    fn generic_init_has_no_document_wide_send_discovery() {
        // Regression guard (GO/NO-GO item #3): Send discovery must be rooted in
        // the ACTIVE composer. Any document-wide Send scan is forbidden.
        for forbidden in [
            "document.querySelectorAll(SEND_SELECTORS",
            "document.querySelectorAll('button,[role=\"button\"],input[type=\"submit\"]')",
            "document.querySelector(SEND_SELECTORS",
            "document.querySelector('button",
            "document.body.querySelector",
            "getElementsByTagName",
            "getElementsByClassName",
        ] {
            assert!(
                !GENERIC_INIT_SCRIPT.contains(forbidden),
                "forbidden document-wide Send discovery pattern leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn generic_init_has_no_loose_ownership_fallback() {
        // Regression guard (GO/NO-GO item #4): root === document.body must never
        // grant Send ownership, and the loose ownContainerCheck fallback that
        // accepted any global candidate must not return.
        for forbidden in [
            "ownContainerCheck",
            "root === document.body",
            "inputEl.closest('form,[role=\"form\"],[class*=\"composer\" i],[class*=\"prompt\" i],[class*=\"input\" i],footer,main')",
        ] {
            assert!(
                !GENERIC_INIT_SCRIPT.contains(forbidden),
                "loose ownership pattern leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn generic_init_send_discovery_is_composer_rooted() {
        // Required ownership chain present in the emitted script:
        // composer root -> descendants -> enabled Send -> click.
        for required in [
            "function composerRootFromInput(input)",
            "root.querySelectorAll(SEND_SELECTORS[i])",
            "root.querySelectorAll('button,[role=\"button\"],input[type=\"submit\"]')",
            "function findOwnedSend(input)",
            "window.__ca_findOwnedSend",
            "composer_not_found",
            "input.isConnected",
            "function findEnabledButton()",
            "function currentComposerRoot()",
        ] {
            assert!(
                GENERIC_INIT_SCRIPT.contains(required),
                "composer-rooted ownership marker missing: {required}"
            );
        }
        // The document must never be the Send-search boundary. A page-state
        // heuristic (classifyPageState) still counts interactive elements
        // document-wide, but a Send candidate list must never be built from a
        // document-wide button/role/input scan.
        assert!(
            !GENERIC_INIT_SCRIPT.contains("document.querySelectorAll('button,[role=\"button\"]"),
            "document-wide Send-capable button scan must not exist in GENERIC_INIT_SCRIPT"
        );
    }

    #[test]
    fn inject_script_send_discovery_is_composer_rooted() {
        // The per-turn injector's diagnostic send probe must not scan the
        // document for a Send control; it reuses the composer-rooted helper.
        let inject_js = super::build_inject_js("test prompt", "chatgpt", 1, true);
        assert!(
            !inject_js.contains("document.querySelector(SEND_SELECTORS"),
            "inject script must not do document-wide Send discovery"
        );
        assert!(
            inject_js.contains("window.__ca_findOwnedSend"),
            "inject script must reuse composer-rooted Send discovery"
        );
    }

    #[test]
    fn generic_init_composer_root_is_narrow() {
        // Regression guard (GO/NO-GO item #5): the composer boundary must stay
        // NARROW. Broad class selectors ([class*="chat" i], [class*="input" i])
        // can climb to a whole-chat/transcript wrapper and would then own
        // unrelated Send controls in the message history.
        for forbidden in [
            "[class*=\"prompt\" i],[class*=\"input\" i],[class*=\"chat\" i]",
            "[class*=\"input\" i],[class*=\"chat\" i]",
        ] {
            assert!(
                !GENERIC_INIT_SCRIPT.contains(forbidden),
                "broad composer-boundary selector leaked: {forbidden}"
            );
        }
        // The narrow boundary (semantic composer ancestors only) is declared
        // once in COMPOSER_ROOT_SELECTORS and shared by the ownership probe.
        // It must contain the semantic composer selectors and NEVER the broad
        // [class*="input"] / [data-testid*="input"] / [class*="chat"] matches
        // that climb to a text-input wrapper or whole-chat wrapper — a
        // [data-testid*="input"] wrapper excludes the Send sibling, which is
        // the live ChatGPT failure (send_button_candidate_count=0) this guard
        // exists to prevent. The injected-prompt re-verification marker must
        // also exist for current-composer proof.
        let boundary_start = GENERIC_INIT_SCRIPT
            .find("const COMPOSER_ROOT_SELECTORS = [")
            .expect("COMPOSER_ROOT_SELECTORS declaration missing");
        let boundary_end = GENERIC_INIT_SCRIPT[boundary_start..]
            .find("];")
            .expect("COMPOSER_ROOT_SELECTORS terminator missing");
        let boundary = &GENERIC_INIT_SCRIPT[boundary_start..boundary_start + boundary_end];
        for required in [
            "'form',",
            "'[role=\"form\"]',",
            "'[class*=\"composer\" i]',",
            "'[class*=\"prompt\" i]',",
            "'[data-testid*=\"composer\" i]',",
            "'[data-testid*=\"prompt\" i]'",
        ] {
            assert!(
                boundary.contains(required),
                "narrow composer-boundary selector missing from COMPOSER_ROOT_SELECTORS: {required}"
            );
        }
        for forbidden in [
            "'[class*=\"input\" i]'",
            "'[data-testid*=\"input\" i]'",
            "'[class*=\"chat\" i]'",
        ] {
            assert!(
                !boundary.contains(forbidden),
                "broad composer-boundary selector leaked into COMPOSER_ROOT_SELECTORS: {forbidden}"
            );
        }
        for required in [
            "window.__ca_lastInjectedText",
            "inputValue(liveInput).indexOf(window.__ca_lastInjectedText.slice(0, 40)) !== 0",
        ] {
            assert!(
                GENERIC_INIT_SCRIPT.contains(required),
                "narrow composer-boundary marker missing: {required}"
            );
        }
    }

    #[test]
    fn inject_script_stamps_injected_text_for_ownership() {
        // The per-turn injector must stamp the injected prompt so the submit
        // helper and retries can prove they act on the CURRENT composer.
        let inject_js = super::build_inject_js("test prompt", "chatgpt", 1, true);
        assert!(
            inject_js.contains("window.__ca_lastInjectedText = text"),
            "inject script must stamp the injected text for current-composer proof"
        );
    }

    #[test]
    fn browser_ownership_fixtures_behavioral() {
        // Behaviorally evaluates the REAL emitted GENERIC_INIT_SCRIPT against
        // fixture DOMs (sidebar/transcript vs composer-owned Send, composer
        // replacement between retries, no-composer -> composer_not_found).
        // Skips gracefully when node is unavailable.
        let fixture_script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/browser-ownership-fixtures.mjs"
        );
        let node_check = std::process::Command::new("node")
            .arg("--version")
            .output();
        let Ok(node_status) = node_check else {
            eprintln!("node unavailable; skipping behavioral fixture test");
            return;
        };
        if !node_status.status.success() {
            eprintln!("node unavailable; skipping behavioral fixture test");
            return;
        }
        let output = std::process::Command::new("node")
            .arg(fixture_script)
            .output()
            .expect("failed to execute behavioral fixture harness");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "composer ownership fixtures failed:\n{stdout}\n{stderr}"
        );
    }

    // ── P1: merged participant registry ───────────────────────────────────────

    #[test]
    fn builtin_registry_has_exactly_seven_participants_unchanged() {
        let ids: Vec<&str> = AGENTS.iter().map(|a| a.agent_id).collect();
        assert_eq!(
            ids,
            vec![
                "chatgpt", "claude", "gemini", "deepseek", "qwen", "glm", "kimi"
            ]
        );
        let names: Vec<&str> = AGENTS.iter().map(|a| a.display_name).collect();
        assert_eq!(
            names,
            vec![
                "ChatGPT", "Claude", "Gemini", "DeepSeek", "Qwen", "GLM", "Kimi"
            ]
        );
        let urls: Vec<&str> = AGENTS.iter().map(|a| a.base_url).collect();
        assert_eq!(
            urls,
            vec![
                "https://chatgpt.com",
                "https://claude.ai",
                "https://gemini.google.com",
                "https://chat.deepseek.com",
                "https://chat.qwen.ai",
                "https://chat.z.ai/",
                "https://www.kimi.com/"
            ]
        );
    }

    #[test]
    fn resolve_participant_returns_builtin_even_with_matching_custom() {
        // A custom entry colliding with a built-in id must NEVER override it —
        // built-ins are authoritative.
        let custom = vec![CustomParticipant {
            agent_id: "deepseek".to_string(),
            display_name: "Not DeepSeek".to_string(),
            base_url: "https://evil.example.com".to_string(),
        }];
        let resolved = resolve_participant("deepseek", &custom).expect("built-in resolves");
        assert_eq!(resolved.display_name, "DeepSeek");
        assert_eq!(resolved.base_url, "https://chat.deepseek.com");
    }

    #[test]
    fn resolve_participant_returns_custom_when_no_builtin_overrides() {
        let custom = vec![CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }];
        let resolved = resolve_participant("acme", &custom).expect("custom resolves");
        assert_eq!(resolved.display_name, "Acme Bot");
        assert_eq!(resolved.base_url, "https://acme.example.com");
    }

    #[test]
    fn resolve_participant_returns_none_for_unknown_id() {
        let custom: Vec<CustomParticipant> = vec![];
        assert!(resolve_participant("does-not-exist", &custom).is_none());
        // Unknown id is not resolved even with unrelated custom entries present.
        let custom = vec![CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }];
        assert!(resolve_participant("does-not-exist", &custom).is_none());
    }

    #[test]
    fn resolve_display_name_merges_custom_and_builtin() {
        let custom = vec![CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }];
        assert_eq!(resolve_display_name("chatgpt", &custom), "ChatGPT");
        assert_eq!(resolve_display_name("acme", &custom), "Acme Bot");
        assert_eq!(resolve_display_name("nope", &custom), "Unknown Model");
    }

    #[test]
    fn custom_does_not_alias_builtin_ids() {
        // The merged resolver MUST return the built-in for any built-in id,
        // masking a bogus custom entry on a reserved id.
        let custom = vec![
            CustomParticipant {
                agent_id: "glm".to_string(),
                display_name: "Spoof".to_string(),
                base_url: "https://spoof.example.com".to_string(),
            },
            CustomParticipant {
                agent_id: "acme".to_string(),
                display_name: "Acme Bot".to_string(),
                base_url: "https://acme.example.com".to_string(),
            },
        ];
        for builtin in AGENTS {
            assert_eq!(
                resolve_participant(builtin.agent_id, &custom).map(|i| i.display_name),
                Some(builtin.display_name.to_string()),
                "builtin {} must not be shadowed by a custom entry",
                builtin.agent_id
            );
        }
    }

    // ── P2: create_windows registry gate ──────────────────────────────────────

    fn custom_acme() -> CustomParticipant {
        CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }
    }

    // A custom participant clears the shared-window registry gate (both as the
    // leader and as the shared-nav participant).
    #[test]
    fn custom_participant_passes_window_registry_gate() {
        let custom = vec![custom_acme()];
        assert!(
            validate_window_registry("chatgpt", "acme", &custom).is_ok(),
            "built-in leader + custom nav must pass"
        );
        assert!(
            validate_window_registry("acme", "chatgpt", &custom).is_ok(),
            "custom leader + built-in nav must pass"
        );
        assert!(
            validate_window_registry("acme", "deeper", &custom).is_err(),
            "custom leader + unknown nav must fail"
        );
        assert!(
            validate_window_registry("unknown", "acme", &custom).is_err(),
            "unknown leader + custom nav must fail"
        );
    }

    // A custom participant resolves during setup (the predicate `run_setup`
    // and `retry_setup_agent` now use) — established resolver, custom granted.
    #[test]
    fn custom_resolves_for_setup_and_retry() {
        let custom = vec![custom_acme()];
        assert!(resolve_participant("acme", &custom).is_some());
        assert_eq!(
            resolve_participant("acme", &custom).map(|i| i.base_url),
            Some("https://acme.example.com".to_string())
        );
    }

    // Built-in resolution stays equivalent to the pre-P2 built-in-only path.
    #[test]
    fn builtin_resolution_unchanged_by_merge() {
        let custom = vec![custom_acme()];
        for builtin in AGENTS {
            let via_builtin = super::get_agent_config(builtin.agent_id)
                .map(|c| c.base_url)
                .expect("builtin exists in AGENTS");
            let via_merged = resolve_participant(builtin.agent_id, &custom)
                .map(|i| i.base_url)
                .expect("builtin resolves through merged registry");
            assert_eq!(
                via_merged, via_builtin,
                "builtin {} base_url must be identical after the registry merge",
                builtin.agent_id
            );
        }
    }

    // ── P3: unified runtime registry ─────────────────────────────────────────

    // The unified registry emits exactly the 7 built-ins first, in frozen order,
    // each tagged is_custom=false, then customs in saved order.
    #[test]
    fn unified_registry_returns_builtins_then_customs() {
        let custom = vec![custom_acme()];
        let merged = merged_participants(&custom);
        assert_eq!(merged.len(), AGENTS.len() + custom.len());
        for (i, builtin) in AGENTS.iter().enumerate() {
            assert_eq!(merged[i].agent_id, builtin.agent_id);
            assert_eq!(merged[i].display_name, builtin.display_name);
            assert_eq!(merged[i].base_url, builtin.base_url);
            assert!(!merged[i].is_custom, "built-in must not be flagged custom");
        }
        let tail = &merged[AGENTS.len()];
        assert_eq!(tail.agent_id, "acme");
        assert_eq!(tail.display_name, "Acme Bot");
        assert!(tail.is_custom, "custom must be flagged is_custom");
    }

    // A custom entry can never alias a built-in id in the merged registry.
    #[test]
    fn unified_registry_never_aliases_builtin_ids() {
        let custom = vec![CustomParticipant {
            agent_id: "deepseek".to_string(),
            display_name: "Spoof".to_string(),
            base_url: "https://spoof.example.com".to_string(),
        }];
        let merged = merged_participants(&custom);
        // deepseek remains the built-in and the spoof custom is dropped.
        assert_eq!(merged.len(), AGENTS.len());
        let ds = merged.iter().find(|p| p.agent_id == "deepseek").unwrap();
        assert!(!ds.is_custom);
        assert_eq!(ds.display_name, "DeepSeek");
        assert_eq!(ds.base_url, "https://chat.deepseek.com");
    }

    // Build-in display names remain exactly unchanged when merged.
    #[test]
    fn unified_registry_preserves_builtin_display_names() {
        let custom = vec![custom_acme()];
        let merged = merged_participants(&custom);
        for (i, builtin) in AGENTS.iter().enumerate() {
            assert_eq!(merged[i].display_name, builtin.display_name);
        }
    }

    // A custom participant's display name resolves through the merged resolver.
    #[test]
    fn unified_registry_resolves_custom_display_name() {
        let custom = vec![custom_acme()];
        assert_eq!(resolve_display_name("acme", &custom), "Acme Bot");
    }
}

// ── GENERIC_INIT_SCRIPT ───────────────────────────────────────────────────────
//
// Rules (NEVER VIOLATE):
// - Static &str constant — never modified at runtime, never agent-specific.
// - Agent identity always read from window.__ca_agentId — never captured.
// - Detects input field at runtime — generic across ALL agents.
//
// D-040 Tier 2: console.error override and window.onerror → arena://log/error
//   with re-entrancy guards to prevent infinite recursion.
//
// D-036/D-042: SELECTORS includes #chat-input (GLM) and
//   div.chat-input-editor[contenteditable="true"] (Kimi Lexical) first.
//
// IMP-11B: checkReady() now allows up to 45 seconds for the composer to
//   appear, while continuing to emit secret-free readiness/send probes.

pub const GENERIC_INIT_SCRIPT: &str = r#"
// D-040 Tier 2: Forward JS errors to Rust via arena://log
(function() {
    var _ce = console.error;
    var _ceGuard = false;
    console.error = function() {
        _ce.apply(console, arguments);
        if (_ceGuard) return;
        _ceGuard = true;
        try {
            var msg = Array.prototype.slice.call(arguments).join(' ');
            window.location.href = 'arena://log/error/' + encodeURIComponent(msg);
        } catch (x) {}
        _ceGuard = false;
    };
    var _oeGuard = false;
    window.onerror = function(msg, src, line) {
        if (_oeGuard) return false;
        _oeGuard = true;
        try {
            var m = (msg || '') + ' (' + (src || '') + ':' + (line || 0) + ')';
            window.location.href = 'arena://log/error/' + encodeURIComponent(m);
        } catch (x) {}
        _oeGuard = false;
        return false;
    };
})();

// Main agent init
(function() {
    // window.name survives full cross-origin navigations. Rust writes only a
    // generic marker plus the current agent id before navigation; every new
    // document restores the runtime identity into window.__ca_agentId.
    var _identityPrefix = '__consensus_arena_agent__:';
    if (!window.__ca_agentId && typeof window.name === 'string' && window.name.indexOf(_identityPrefix) === 0) {
        window.__ca_agentId = window.name.substring(_identityPrefix.length);
    }
    window.__ca_ready = false;
    window.__ca_lastResponse = '';
    window.__ca_lastTurn = 0;

    // D-036: #chat-input first (GLM textarea)
    // D-042: div.chat-input-editor first (Kimi Lexical contenteditable)
    // #prompt-textarea is retained as a generic direct candidate because some
    // composer UIs expose a stable textarea id instead of a richer wrapper.
    const SELECTORS = [
        '#chat-input',
        'div.chat-input-editor[contenteditable="true"]',
        '#prompt-textarea',
        'div.ProseMirror[contenteditable="true"]',
        'div.ProseMirror',
        'rich-textarea div[contenteditable="true"]',
        'textarea[placeholder*="Message"]',
        'textarea',
        '[data-testid*="composer" i]',
        '[data-testid*="textbox" i]',
        '[data-testid*="input" i]',
        '[role="textbox"]',
        '[aria-multiline="true"]',
        'p[data-placeholder]',
        '[contenteditable="true"]'
    ];
    const COMPOSER_CONTAINER_SELECTORS = [
        'form',
        'footer',
        'main',
        '[role="form"]',
        '[class*="composer" i]',
        '[class*="prompt" i]',
        '[class*="input" i]',
        '[class*="chat" i]',
        '[class*="textbox" i]',
        '[data-testid*="composer" i]',
        '[data-testid*="textbox" i]',
        '[data-testid*="input" i]'
    ];
    const EDITABLE_DESCENDANT_SELECTORS = [
        'textarea',
        '[contenteditable="true"]',
        '[role="textbox"]',
        '[aria-multiline="true"]',
        'div.ProseMirror',
        'p[data-placeholder]',
        '[data-testid*="composer" i]',
        '[data-testid*="textbox" i]',
        '[data-testid*="input" i]'
    ];
    const READY_TIMEOUT_MS = 45000;
    const READY_CHECK_INTERVAL_MS = 500;

    function getAgentId() {
        return window.__ca_agentId || 'unknown';
    }

    function addUniqueElement(target, el) {
        if (el && target.indexOf(el) === -1) target.push(el);
    }

    function isEditableSurface(el) {
        if (!el || !(el instanceof Element)) return false;
        if (el.tagName === 'TEXTAREA') return true;
        if (el.getAttribute('contenteditable') === 'true') return true;
        if (el.getAttribute('role') === 'textbox') return true;
        if (el.getAttribute('aria-multiline') === 'true') return true;
        if (el.matches && (el.matches('div.ProseMirror') || el.matches('p[data-placeholder]'))) return true;
        return false;
    }

    function normalizeComposerCandidate(el) {
        if (!el || !(el instanceof Element)) return null;
        if (el.matches && el.matches('p[data-placeholder]')) {
            const placeholderAncestor = el.closest('[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror');
            if (placeholderAncestor) return normalizeComposerCandidate(placeholderAncestor);
        }
        if (el.matches && el.matches('[data-testid*="composer" i],[data-testid*="textbox" i],[data-testid*="input" i],form,footer,main,[role="form"]')) {
            const nestedEditable = el.querySelector('textarea,[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror,p[data-placeholder]');
            if (nestedEditable) return normalizeComposerCandidate(nestedEditable);
        }
        if (el.matches && el.matches('div.ProseMirror') && el.getAttribute('contenteditable') !== 'true') {
            const proseEditable = el.querySelector('[contenteditable="true"],[role="textbox"],[aria-multiline="true"],textarea');
            if (proseEditable) return normalizeComposerCandidate(proseEditable);
        }
        if (isEditableSurface(el)) {
            const editableAncestor = el.closest('[contenteditable="true"]');
            if (editableAncestor && editableAncestor !== el && el.getAttribute('contenteditable') !== 'true' && el.tagName !== 'TEXTAREA') {
                return editableAncestor;
            }
            return el;
        }
        const nested = el.querySelector && el.querySelector('textarea,[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror,p[data-placeholder]');
        return nested ? normalizeComposerCandidate(nested) : null;
    }

    function collectComposerSnapshot() {
        const composerContainers = [];
        const inputCandidates = [];

        for (let i = 0; i < COMPOSER_CONTAINER_SELECTORS.length; i++) {
            let nodes = [];
            try {
                nodes = Array.prototype.slice.call(document.querySelectorAll(COMPOSER_CONTAINER_SELECTORS[i]));
            } catch (e) {}
            for (let j = 0; j < nodes.length; j++) {
                const container = nodes[j];
                if (!isVisible(container)) continue;
                addUniqueElement(composerContainers, container);
                for (let k = 0; k < EDITABLE_DESCENDANT_SELECTORS.length; k++) {
                    let descendants = [];
                    try {
                        descendants = Array.prototype.slice.call(container.querySelectorAll(EDITABLE_DESCENDANT_SELECTORS[k]));
                    } catch (e) {}
                    for (let m = 0; m < descendants.length; m++) {
                        const candidate = normalizeComposerCandidate(descendants[m]);
                        if (candidate && isVisible(candidate)) addUniqueElement(inputCandidates, candidate);
                    }
                }
            }
        }

        for (let i = 0; i < SELECTORS.length; i++) {
            let nodes = [];
            try {
                nodes = Array.prototype.slice.call(document.querySelectorAll(SELECTORS[i]));
            } catch (e) {}
            for (let j = 0; j < nodes.length; j++) {
                const raw = nodes[j];
                const candidate = normalizeComposerCandidate(raw);
                if (candidate && isVisible(candidate)) addUniqueElement(inputCandidates, candidate);
                if (raw && isVisible(raw) && raw !== candidate && raw instanceof Element) {
                    if (raw.matches('form,footer,main,[role="form"],[data-testid*="composer" i],[data-testid*="textbox" i],[data-testid*="input" i]')) {
                        addUniqueElement(composerContainers, raw);
                    }
                }
            }
        }

        // Current-composer proof: once a prompt has been injected, the ACTIVE
        // composer is the editable holding that text. Prefer it over any
        // DOM-order candidate (a previous-message edit box, a hidden editor, a
        // regenerated ProseMirror node) so retries and re-resolution never
        // target the transcript. Setup never sets __ca_lastInjectedText, so the
        // readiness/send-detection paths are unaffected.
        if (inputCandidates.length > 0 && window.__ca_lastInjectedText) {
            var expectedPrefix = window.__ca_lastInjectedText.slice(0, 40);
            for (var p = 0; p < inputCandidates.length; p++) {
                var preferred = inputCandidates[p];
                if (preferred && inputValue(preferred).indexOf(expectedPrefix) === 0) {
                    inputCandidates.splice(p, 1);
                    inputCandidates.unshift(preferred);
                    break;
                }
            }
        }

        return {
            input: inputCandidates.length > 0 ? inputCandidates[0] : null,
            inputCandidateCount: inputCandidates.length,
            composerCandidateCount: composerContainers.length
        };
    }

    function textContainsAny(text, values) {
        for (let i = 0; i < values.length; i++) {
            if (text.indexOf(values[i]) !== -1) return true;
        }
        return false;
    }

    function hasVisibleProgressIndicators() {
        const selectors = [
            '[role="progressbar"]',
            '[aria-busy="true"]',
            '[class*="spinner" i]',
            '[class*="loading" i]',
            '[class*="progress" i]'
        ];
        for (let i = 0; i < selectors.length; i++) {
            let nodes = [];
            try {
                nodes = Array.prototype.slice.call(document.querySelectorAll(selectors[i]));
            } catch (e) {}
            for (let j = 0; j < nodes.length; j++) {
                if (isVisible(nodes[j])) return true;
            }
        }
        return false;
    }

    function classifyPageState(snapshot) {
        const text = safeVisibleText();
        let path = '';
        try {
            path = (window.location && window.location.pathname || '').toLowerCase();
        } catch (e) {}

        if (textContainsAny(text, [
            'cloudflare',
            'checking your browser',
            'verify you are human',
            'just a moment',
            'cf-challenge',
            'challenge-platform',
            'turnstile',
            'captcha',
            'security check',
            'security verification'
        ])) {
            return 'possible_challenge_or_security';
        }

        if (
            path.indexOf('login') !== -1 ||
            path.indexOf('signin') !== -1 ||
            path.indexOf('auth') !== -1 ||
            textContainsAny(text, [
                'log in',
                'login',
                'sign in',
                'sign into',
                'welcome back',
                'continue with google',
                'continue with email',
                'enter your password',
                'create your account',
                'verify your email'
            ])
        ) {
            return 'possible_login_required';
        }

        if (snapshot.inputCandidateCount > 0) {
            return 'composer_detected';
        }

        if (document.readyState !== 'complete' || hasVisibleProgressIndicators() || textContainsAny(text, ['loading', 'please wait', 'starting'])) {
            return 'still_loading';
        }

        var bodyLength = 0;
        try { bodyLength = (document.body && (document.body.innerText || '').trim().length) || 0; } catch (e) {}
        var interactive = 0;
        try { interactive = document.querySelectorAll('button,a,input,textarea,select,[role="button"],[role="textbox"],[contenteditable="true"]').length; } catch (e) {}
        if (textContainsAny(text, ['something went wrong', 'application error', 'page not found', 'access denied', 'temporarily unavailable'])) return 'error_page';
        if (bodyLength < 40 && interactive < 2) return 'empty_shell_or_hydration_stuck';
        // A selector miss is meaningful only after a stable, interactive page.
        if (document.readyState === 'complete' && !hasVisibleProgressIndicators() && interactive >= 2) return 'composer_selector_miss';
        return 'still_loading';
    }

    function findInput() {
        return collectComposerSnapshot().input;
    }

    function signalReady() {
        const agentId = getAgentId();
        try { window.location.href = 'arena://ready/' + agentId; } catch (e) {}
    }

    var _lastChallengeSignal = '';
    var _lastUnshowableSignal = '';

    function safeVisibleText() {
        var title = document.title || '';
        var body = '';
        try {
            body = (document.body && document.body.innerText || '').slice(0, 5000);
        } catch (e) {}
        return (title + '\n' + body + '\n' + (window.location && window.location.href || '')).toLowerCase();
    }

    function redactedCurrentUrl() {
        try {
            return window.location.origin + window.location.pathname;
        } catch (e) {
            return '';
        }
    }

    function detectChallengeOrUnshowable() {
        try {
            if (window.location && window.location.protocol === 'arena:') return false;
        } catch (e) {}
        var text = safeVisibleText();
        var challengeIndicators = [
            'cloudflare',
            'checking your browser',
            'verify you are human',
            'just a moment',
            'cf-challenge',
            'challenge-platform',
            'turnstile',
            'captcha',
            'security check'
        ];
        for (var i = 0; i < challengeIndicators.length; i++) {
            if (text.indexOf(challengeIndicators[i]) !== -1) {
                var challengeKey = getAgentId() + ':' + challengeIndicators[i];
                if (_lastChallengeSignal !== challengeKey) {
                    _lastChallengeSignal = challengeKey;
                    try {
                        window.location.href = 'arena://challenge/' + getAgentId() + '/' + encodeURIComponent(challengeIndicators[i]);
                    } catch (e) {}
                }
                return true;
            }
        }
        if (text.indexOf("the url can't be shown") !== -1 || text.indexOf('the url can’t be shown') !== -1) {
            var url = redactedCurrentUrl();
            var unshowableKey = getAgentId() + ':' + url;
            if (_lastUnshowableSignal !== unshowableKey) {
                _lastUnshowableSignal = unshowableKey;
                try {
                    window.location.href = 'arena://unshowable/' + getAgentId() + '/' + encodeURIComponent(url);
                } catch (e) {}
            }
            return true;
        }
        return false;
    }

    // IMP-11B: checkReady now probes for up to 45 seconds before declaring
    // page_loaded_but_no_composer. This is long enough for slower WebKit/dev
    // paths without bypassing login, security, or challenge pages.
    var _checkReadyStart = null;
    window.__ca_readinessProbeCount = 0;
    window.__ca_pageStateHint = 'still_loading';

    function checkReady() {
        var now = Date.now();
        if (_checkReadyStart === null) { _checkReadyStart = now; }
        window.__ca_readinessProbeCount = (window.__ca_readinessProbeCount || 0) + 1;
        const snapshot = collectComposerSnapshot();
        window.__ca_pageStateHint = classifyPageState(snapshot);
        emitSendProbe(true);

        if (detectChallengeOrUnshowable()) {
            window.__ca_pageStateHint = 'possible_challenge_or_security';
            emitSendProbe(true);
            setTimeout(checkReady, 1000);
        } else if (snapshot.input) {
            window.__ca_ready = true;
            window.__ca_pageStateHint = 'composer_detected';
            emitSendProbe(true);
            signalReady();
        } else if (now - _checkReadyStart >= READY_TIMEOUT_MS) {
            // Timeout — signal error so the backend can surface it
            var agentId = getAgentId();
            emitSendProbe(true);
            try { window.location.href = 'arena://ready/error-' + agentId; } catch (e) {}
        } else {
            setTimeout(checkReady, READY_CHECK_INTERVAL_MS);
        }
    }

    if (document.readyState === 'complete' || document.readyState === 'interactive') {
        setTimeout(checkReady, 100);
    } else {
        document.addEventListener('DOMContentLoaded', function() {
            setTimeout(checkReady, 100);
        });
    }

    const SEND_SELECTORS = [
        '#send-message-button',
        'div.send-button-container',
        'button[data-testid*="send" i]',
        'button[data-testid*="submit" i]',
        'button[data-testid*="chat-send" i]',
        'button[data-testid*="composer" i]',
        '[role="button"][data-testid*="send" i]',
        '[role="button"][aria-label*="send" i]',
        '[role="button"][title*="send" i]',
        'button[aria-label*="send" i]',
        'button[aria-label*="arrow" i]',
        'button[title*="send" i]',
        'button[type="submit"]',
        'button[class*="send" i]'
    ];
    const MESSAGE_SELECTORS = [
        '[data-message-author-role="user"]',
        '[data-testid*="user" i]',
        'article',
        '[role="article"]',
        '[data-message-id]',
        'main div, main p, main article, main section'
    ];
    let pendingSend = null;
    let sentSignalEmitted = false;
    let lastProbeAt = 0;
    let userSubmitSeen = false;
    let lastMessageCountSeen = 0;
    let observedPrompt = { text: '', input: null, messageCount: 0 };

    function inputValue(input) {
        return (input && (input.value || input.textContent || '') || '').trim();
    }

    function isVisible(el) {
        if (!el || !(el instanceof Element)) return false;
        const style = window.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    }

    function isEnabled(el) {
        return !!el && !el.disabled && el.getAttribute('aria-disabled') !== 'true';
    }

    function candidateText(el) {
        if (!el) return '';
        return [
            el.getAttribute('aria-label') || '',
            el.getAttribute('title') || '',
            el.getAttribute('data-testid') || '',
            el.getAttribute('name') || '',
            el.getAttribute('class') || '',
            el.textContent || ''
        ].join(' ').toLowerCase();
    }

    function isSendCandidate(el) {
        if (!el || !(el instanceof Element)) return false;
        if (!isVisible(el) || !isEnabled(el)) return false;
        const text = candidateText(el);
        // Explicit negative filters: attachment, file, upload, paperclip, voice, microphone
        if (text.indexOf('attach') !== -1 || text.indexOf('file') !== -1 || text.indexOf('upload') !== -1 ||
            text.indexOf('paperclip') !== -1 || text.indexOf('voice') !== -1 || text.indexOf('microphone') !== -1) {
            return false;
        }
        if (text.indexOf('send') !== -1 || text.indexOf('submit') !== -1 || text.indexOf('arrow-up') !== -1 || text.indexOf('arrow up') !== -1) return true;
        return el.matches && SEND_SELECTORS.some(function(selector) {
            try { return el.matches(selector); } catch (e) { return false; }
        });
    }

    function looksIconOnlySend(button, input) {
        if (!button || !(button instanceof Element) || !isVisible(button) || !isEnabled(button)) return false;
        const rect = button.getBoundingClientRect();
        if (rect.width < 20 || rect.width > 80 || rect.height < 20 || rect.height > 80) return false;
        const inputRect = input && input.getBoundingClientRect ? input.getBoundingClientRect() : null;
        if (inputRect && Math.abs(rect.top - inputRect.top) > 140 && Math.abs(rect.bottom - inputRect.bottom) > 140) return false;
        const text = candidateText(button);
        if (text.indexOf('stop') !== -1 || text.indexOf('voice') !== -1 || text.indexOf('attach') !== -1 || text.indexOf('file') !== -1) return false;
        if (button.querySelector('svg')) return true;
        return !!button.querySelector('path[d]');
    }

    // Provider-neutral composer-ownership resolution (shared by setup probes,
    // readiness/send probes, and active submission).
    //
    // The composer boundary is resolved from the ACTIVE input and is the ONLY
    // boundary for Send discovery. It must never resolve to document.body /
    // documentElement, and the document is never searched for a Send control.
    //
    // Why a single closest() is not enough: real composer DOMs differ in
    // structure. ChatGPT nests the textarea inside a narrow text-input wrapper
    // (e.g. a [data-testid*="input"] div) whose SIBLING is the Send control,
    // while DeepSeek keeps an icon-only Send inside a non-form toolbar. A
    // one-shot closest() with a wide selector list stops at whichever wrapper
    // matches first — often the narrow text-input wrapper that EXCLUDES Send,
    // which is exactly the live failure (input_found=true, prompt injected,
    // send_button_candidate_count=0). Resolution therefore walks UP from the
    // active input, anchored to it, and never past body/html:
    //   1. nearest semantic composer ancestor (form / [role="form"] /
    //      [class*="composer"|"prompt"] / composer|prompt testid) that contains
    //      an owned Send control;
    //   2. else the NARROWEST ancestor (excluding body/html) that contains an
    //      owned Send control — covers Send-as-sibling-of-the-text-wrapper and
    //      non-form composers;
    //   3. else the nearest semantic composer ancestor;
    //   4. else the direct composer wrapper (input.parentElement).
    // Unprovable ownership yields null -> callers report composer_not_found or
    // a false send-capability probe; they never broaden to the document.
    const COMPOSER_ROOT_SELECTORS = [
        'form',
        '[role="form"]',
        '[class*="composer" i]',
        '[class*="prompt" i]',
        '[data-testid*="composer" i]',
        '[data-testid*="prompt" i]'
    ];

    function matchesAny(el, selectors) {
        if (!el || !el.matches) return false;
        for (let i = 0; i < selectors.length; i++) {
            try { if (el.matches(selectors[i])) return true; } catch (e) {}
        }
        return false;
    }

    function isOwnershipStop(node) {
        return !node || node === document.body || node === document.documentElement;
    }

    function composerRootFromInput(input) {
        if (!input || !(input instanceof Element)) return null;
        // 1) Nearest semantic composer ancestor containing an owned Send.
        let node = input.parentElement;
        while (!isOwnershipStop(node)) {
            if (matchesAny(node, COMPOSER_ROOT_SELECTORS) &&
                collectSendCandidatesIn(node, input).length > 0) {
                return node;
            }
            node = node.parentElement;
        }
        // 2) Narrowest ancestor (excluding body/html) containing an owned Send.
        // Also check the parent of each ancestor (i.e., siblings of the ancestor)
        // to handle cases where the Send button is a sibling of the ancestor
        // rather than a descendant (e.g., ChatGPT's text-input-wrapper + Send sibling).
        node = input.parentElement;
        while (!isOwnershipStop(node)) {
            if (collectSendCandidatesIn(node, input).length > 0) return node;
            const parent = node.parentElement;
            if (parent && !isOwnershipStop(parent) && collectSendCandidatesIn(parent, input).length > 0) {
                return parent;
            }
            node = node.parentElement;
        }
        // 3) Nearest semantic composer ancestor (ownerless capability check).
        const container = input.closest(COMPOSER_ROOT_SELECTORS.join(','));
        if (container && container instanceof Element && !isOwnershipStop(container)) {
            return container;
        }
        // 4) Direct composer wrapper. Never document.body / documentElement.
        const parent = input.parentElement;
        if (parent && parent instanceof Element && !isOwnershipStop(parent)) {
            return parent;
        }
        return null;
    }

    // Pure owned-Send scan over a given root. Never resolves a root itself and
    // never touches the document, so it can be used both as the boundary probe
    // (composerRootFromInput) and as the final candidate list. Geometry
    // (looksIconOnlySend) is only a secondary check on candidates already owned
    // by the composer region, never a substitute for ownership.
    function collectSendCandidatesIn(root, input) {
        const candidates = [];
        if (!root || !(root instanceof Element)) return candidates;
        function consider(button) {
            if ((isSendCandidate(button) || looksIconOnlySend(button, input)) && candidates.indexOf(button) === -1) {
                candidates.push(button);
            }
        }
        for (let i = 0; i < SEND_SELECTORS.length; i++) {
            try {
                const elements = Array.prototype.slice.call(root.querySelectorAll(SEND_SELECTORS[i]));
                for (let j = 0; j < elements.length; j++) {
                    consider(elements[j]);
                }
            } catch (e) {}
        }
        const owned = Array.prototype.slice.call(
            root.querySelectorAll('button,[role="button"],input[type="submit"]')
        );
        for (let i = 0; i < owned.length; i++) {
            consider(owned[i]);
        }
        return candidates;
    }

    // STRICTLY composer-owned Send discovery: every candidate lives inside the
    // resolved ACTIVE composer root. The document is never searched.
    function collectSendButtonCandidates(input) {
        const root = composerRootFromInput(input);
        if (!root) return [];
        return collectSendCandidatesIn(root, input);
    }

    function findOwnedSend(input) {
        const el = input || findInput();
        if (!el) return null;
        const root = composerRootFromInput(el);
        if (!root) return null;
        const candidates = collectSendButtonCandidates(el);
        for (let i = 0; i < candidates.length; i++) {
            const candidate = candidates[i];
            if (candidate && !candidate.disabled && candidate.getAttribute('aria-disabled') !== 'true') {
                return candidate;
            }
        }
        return candidates.length > 0 ? candidates[0] : null;
    }

    // Exposed for per-turn injection diagnostics and readiness probes.
    window.__ca_findOwnedSend = function(input) {
        return findOwnedSend(input);
    };

    function findSendButton(input) {
        const candidates = collectSendButtonCandidates(input || findInput());
        return candidates.length > 0 ? candidates[0] : null;
    }

    function renderedMessageCount(text, input) {
        if (!text) return 0;
        const normalized = text.replace(/\s+/g, ' ').trim();
        let count = 0;
        for (let i = 0; i < MESSAGE_SELECTORS.length; i++) {
            let nodes = [];
            try {
                nodes = Array.prototype.slice.call(document.querySelectorAll(MESSAGE_SELECTORS[i]));
            } catch (e) {}
            for (let j = 0; j < nodes.length; j++) {
                const el = nodes[j];
                if (!el || el === input || (input && el.contains(input))) continue;
                const value = ((el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim());
                if (value === normalized || value.indexOf(normalized) !== -1) {
                    count++;
                }
            }
        }
        lastMessageCountSeen = count;
        return count;
    }

    function emitSendProbe(force) {
        const now = Date.now();
        if (!force && now - lastProbeAt < 2000) return;
        lastProbeAt = now;
        const snapshot = collectComposerSnapshot();
        const input = snapshot.input;
        const sendCandidates = collectSendButtonCandidates(input);
        const send = sendCandidates.length > 0 ? sendCandidates[0] : null;
        const pageStateHint = classifyPageState(snapshot);
        window.__ca_pageStateHint = pageStateHint;
        const text = inputValue(input);
        if (text && !sentSignalEmitted) {
            observedPrompt.text = text;
            observedPrompt.input = input;
            observedPrompt.messageCount = renderedMessageCount(text, input);
        }
        const count = lastMessageCountSeen || 0;
        try {
            window.location.href = 'arena://send-probe/' + getAgentId() + '/' +
                (input ? '1' : '0') + '/' +
                (send ? '1' : '0') + '/' +
                (userSubmitSeen ? '1' : '0') + '/' +
                count + '/' +
                (sentSignalEmitted ? '1' : '0') + '/' +
                snapshot.inputCandidateCount + '/' +
                snapshot.composerCandidateCount + '/' +
                sendCandidates.length + '/' +
                (window.__ca_readinessProbeCount || 0) + '/' +
                READY_TIMEOUT_MS + '/' +
                encodeURIComponent(pageStateHint || 'still_loading') + '/' +
                encodeURIComponent(pageStateHint || 'still_loading');
        } catch (e) {}
    }

    function emitSent(reason) {
        if (sentSignalEmitted) return;
        sentSignalEmitted = true;
        userSubmitSeen = true;
        emitSendProbe(true);
        try { window.location.href = 'arena://sent/' + getAgentId() + '/' + encodeURIComponent(reason || 'unknown'); } catch (e) {}
    }

    function beginSendCheck(input, source) {
        const currentInput = input || findInput();
        const text = inputValue(currentInput);
        if (!text) {
            emitSendProbe(true);
            return;
        }
        observedPrompt.text = text;
        observedPrompt.input = currentInput;
        observedPrompt.messageCount = renderedMessageCount(text, currentInput);
        userSubmitSeen = true;
        pendingSend = {
            text: text,
            input: currentInput,
            messageCount: observedPrompt.messageCount,
            timeOrigin: performance.timeOrigin,
            startedAt: Date.now(),
            source: source
        };
        if (source === 'trusted-click' || source === 'trusted-enter' || source === 'trusted-submit') {
            setTimeout(function() { emitSent(source); }, 150);
        }
        emitSendProbe(true);
    }

    function trusted(event) {
        return !event || event.isTrusted !== false;
    }

    function handleCandidateClick(event) {
        if (!trusted(event)) return;
        const target = event && event.target instanceof Element ? event.target : null;
        const button = target && target.closest('button,[role="button"],input[type="submit"],[aria-label],[title],[data-testid]');
        if (button && isSendCandidate(button)) {
            beginSendCheck(findInput(), 'trusted-click');
        }
    }

    function handleSubmit(event) {
        if (!trusted(event)) return;
        const input = findInput();
        if (input) beginSendCheck(input, 'trusted-submit');
    }

    function attachSendListeners() {
        const input = findInput();
        const send = findSendButton(input);
        if (send && !send.__caSendListenerAttached) {
            send.__caSendListenerAttached = true;
            send.addEventListener('click', handleCandidateClick, true);
        }
        const forms = Array.prototype.slice.call(document.querySelectorAll('form'));
        for (let i = 0; i < forms.length; i++) {
            const form = forms[i];
            if (!form.__caSubmitListenerAttached) {
                form.__caSubmitListenerAttached = true;
                form.addEventListener('submit', handleSubmit, true);
            }
        }
        if (input && !input.__caKeyListenerAttached) {
            input.__caKeyListenerAttached = true;
            input.addEventListener('keydown', function(event) {
                if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
                if (!trusted(event)) return;
                beginSendCheck(input, 'trusted-enter');
            }, true);
        }
        emitSendProbe(false);
    }

    document.addEventListener('click', handleCandidateClick, true);
    document.addEventListener('submit', handleSubmit, true);
    document.addEventListener('keydown', function(event) {
        if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
        if (!trusted(event)) return;
        const input = findInput();
        if (input && (event.target === input || input.contains(event.target))) {
            beginSendCheck(input, 'trusted-enter');
        }
    }, true);

    function detectSend() {
        attachSendListeners();
        if (!pendingSend) return;
        const currentCount = renderedMessageCount(pendingSend.text, pendingSend.input);
        const documentUnchanged = performance.timeOrigin === pendingSend.timeOrigin;
        const ready = document.readyState === 'complete' || document.readyState === 'interactive';
        const inputCleared = !pendingSend.input || inputValue(pendingSend.input) === '' || inputValue(findInput()) === '';
        const messageAdded = currentCount === pendingSend.messageCount + 1;
        if (documentUnchanged && ready && (inputCleared || messageAdded) && userSubmitSeen) {
            emitSent('poll');
            pendingSend = null;
        } else if (Date.now() - pendingSend.startedAt > 15000) {
            emitSendProbe(true);
            pendingSend = null;
        }
    }

    try {
        const observer = new MutationObserver(function() {
            if (pendingSend && userSubmitSeen) {
                const currentCount = renderedMessageCount(pendingSend.text, pendingSend.input);
                const documentUnchanged = performance.timeOrigin === pendingSend.timeOrigin;
                if (documentUnchanged && currentCount === pendingSend.messageCount + 1) {
                    emitSent('mutation');
                    pendingSend = null;
                }
                return;
            }
            if (!observedPrompt.text || sentSignalEmitted || !userSubmitSeen) return;
            const currentCount = renderedMessageCount(observedPrompt.text, observedPrompt.input);
            const documentUnchanged = performance.timeOrigin === observedPrompt.timeOrigin || performance.timeOrigin === pendingSend?.timeOrigin;
            const inputCleared = inputValue(observedPrompt.input) === '' || inputValue(findInput()) === '';
            if (documentUnchanged && inputCleared && currentCount === observedPrompt.messageCount + 1) {
                emitSent('mutation');
            }
        });
        observer.observe(document.documentElement || document.body, { childList: true, subtree: true, characterData: true });
    } catch (e) {}

    setInterval(detectSend, 250);
    setInterval(attachSendListeners, 1000);
    setInterval(detectChallengeOrUnshowable, 1500);

    // Active orchestration only calls this helper after the per-turn injector
    // has verified the inserted prompt. Setup never invokes it.
    window.__caSubmitActivePrompt = function(input, expectedAgentId, expectedTurn) {
        var error = '';
        var method = 'none';
        var enabled = false;
        var attempts = 0;
        var MAX_SUBMIT_ATTEMPTS = 40;
        function report(success) {
            try {
                window.location.href = 'arena://active-submit/' + expectedAgentId + '/' + expectedTurn + '/' + (success ? '1' : '0') + '/' + encodeURIComponent(method) + '/' + (enabled ? '1' : '0') + '/' + encodeURIComponent(error);
            } catch (e) {}
        }
        // Resolve the CURRENT composer each attempt. The injected input is
        // re-validated against the live DOM: it must be connected AND still
        // hold the injected prompt. A stale node (React/Vue replaced the
        // composer, or the prompt was cleared) is discarded and the composer is
        // re-resolved fresh so ownership always tracks the ACTIVE composer.
        function currentComposerRoot() {
            var liveInput = (input && input.isConnected) ? input : findInput();
            if (!liveInput || !liveInput.isConnected) return { input: null, root: null };
            if (window.__ca_lastInjectedText &&
                inputValue(liveInput).indexOf(window.__ca_lastInjectedText.slice(0, 40)) !== 0) {
                liveInput = findInput();
            }
            if (!liveInput || !liveInput.isConnected) return { input: null, root: null };
            return { input: liveInput, root: composerRootFromInput(liveInput) };
        }
        function findEnabledButton() {
            var found = currentComposerRoot();
            if (!found.input || !found.root) return null;
            var composite = collectSendButtonCandidates(found.input);
            for (var i = 0; i < composite.length; i++) {
                var candidate = composite[i];
                if (candidate && !candidate.disabled && candidate.getAttribute('aria-disabled') !== 'true') {
                    return candidate;
                }
            }
            return null;
        }
        function submitWhenReady() {
            try {
                if (getAgentId() !== expectedAgentId) { error = 'agent_mismatch'; report(false); return; }
                var page = safeVisibleText();
                if (textContainsAny(page, ['cloudflare', 'captcha', 'challenge', 'verify you are human', 'log in', 'sign in'])) { error = 'page_health_blocked'; report(false); return; }
                var found = currentComposerRoot();
                if (!found.input || !found.root) {
                    attempts++;
                    if (attempts < MAX_SUBMIT_ATTEMPTS) { setTimeout(submitWhenReady, 300); return; }
                    error = 'composer_not_found';
                    report(false);
                    return;
                }
                // Nudge the composer so React/ProseMirror frameworks register
                // the injected text before we look for an enabled Send button.
                var inputEl = found.input;
                if (inputEl) {
                    try {
                        inputEl.dispatchEvent(new Event('input', { bubbles: true }));
                        inputEl.dispatchEvent(new KeyboardEvent('keyup', { bubbles: true, key: 'Unidentified' }));
                    } catch (e) {}
                }
                var button = findEnabledButton();
                enabled = !!button;
                if (!button) {
                    attempts++;
                    if (attempts < MAX_SUBMIT_ATTEMPTS) { setTimeout(submitWhenReady, 300); return; }
                    error = 'enabled_send_button_not_found_after_retry';
                    report(false);
                    return;
                }
                button.click();
                method = 'button_click';
                report(true);
            } catch (e) {
                error = 'submit_exception';
                report(false);
            }
        }
        setTimeout(submitWhenReady, 250);
    };
    // Re-runs the submit action from an external eval (used by the backend to
    // retry a failed auto-submit as a fresh action rather than just observing).
    // Re-resolves the live composer and owned Send control on each invocation.
    window.__caRetrySubmit = function(expectedAgentId, expectedTurn) {
        try {
            var input = findInput();
            if (input && typeof window.__caSubmitActivePrompt === 'function') {
                window.__caSubmitActivePrompt(input, expectedAgentId, expectedTurn);
            }
        } catch (e) {}
    };
})();
"#;

// ── create_windows ────────────────────────────────────────────────────────────

/// P2: registry existence gate for the leader and shared-nav participants,
/// using the MERGED registry (built-ins + persisted custom). Extracted from
/// `create_windows` so it is unit-testable without a full Tauri runtime. A
/// custom participant resolves like a built-in; an unknown id is rejected.
fn validate_window_registry(
    leader_agent_id: &str,
    nav_agent_id: &str,
    custom: &[crate::settings_store::CustomParticipant],
) -> Result<(), AgentError> {
    if resolve_participant(leader_agent_id, custom).is_none() {
        return Err(AgentError::NavigationFailed(format!(
            "unknown leader model: {leader_agent_id}"
        )));
    }
    if resolve_participant(nav_agent_id, custom).is_none() {
        return Err(AgentError::NavigationFailed(format!(
            "unknown participant model: {nav_agent_id}"
        )));
    }
    Ok(())
}

/// Two-WebView window setup. Resolves the leader and shared-nav participant
/// through the merged registry so a persisted custom participant can create
/// the windows; the seven built-ins behave identically to the pre-P2 gate.
pub fn create_windows(
    app: &AppHandle,
    state: &mut BrowserState,
    agent_ids: &[String],
    leader_agent_id: &str,
    session_id: &str,
    setup_generation: u32,
    setup_order: &[String],
    custom: &[crate::settings_store::CustomParticipant],
) -> Result<(), AgentError> {
    let nav_agent_id = agent_ids
        .iter()
        .find(|agent_id| agent_id.as_str() != leader_agent_id)
        .ok_or_else(|| {
            AgentError::NavigationFailed(
                "at least one non-leader model is required for the shared nav window".to_string(),
            )
        })?
        .clone();
    validate_window_registry(leader_agent_id, &nav_agent_id, custom)?;
    state.diagnostics.begin_setup_run(BrowserSetupMetadata {
        setup_generation,
        session_id: session_id.to_string(),
        selected_leader_id: leader_agent_id.to_string(),
        selected_agent_ids: agent_ids.to_vec(),
        setup_order: setup_order.to_vec(),
    });
    state.leader_agent_id = leader_agent_id.to_string();

    for label in [LEADER_WINDOW_LABEL, NAV_WINDOW_LABEL] {
        if let Some(existing) = app.get_webview_window(label) {
            existing.destroy().map_err(|error| {
                AgentError::NavigationFailed(format!(
                    "failed to destroy stale {label} window before session start: {error}"
                ))
            })?;
        }
    }

    for agent_id in agent_ids {
        let (label, kind) = if agent_id == leader_agent_id {
            (LEADER_WINDOW_LABEL, "leader")
        } else {
            (NAV_WINDOW_LABEL, "nav")
        };
        state.diagnostics.register(agent_id, label, kind);
        let _ = update_diagnostic(&state.diagnostics, agent_id, |record| {
            record.current_phase = "queued".to_string();
            record.last_error = None;
        });
        let intended_url = resolve_participant(agent_id, custom)
            .map(|info| info.base_url)
            .unwrap_or_default();
        tracing::info!(
            "[SETUP] generation={} session_id={} agent_id={} selected_leader_id={} selected_agent_ids={:?} setup_order={:?} assigned_window_label={} assigned_window_kind={} intended_url={} is_selected_leader={}",
            setup_generation,
            session_id,
            agent_id,
            leader_agent_id,
            agent_ids,
            setup_order,
            label,
            kind,
            intended_url,
            agent_id == leader_agent_id
        );
    }

    let leader_tx = state.nav_tx.clone();
    let leader_popup_tx = state.nav_tx.clone();
    let nav_tx = state.nav_tx.clone();
    let nav_popup_tx = state.nav_tx.clone();
    let leader_diagnostics = state.diagnostics.clone();
    let nav_diagnostics = state.diagnostics.clone();

    let leader_win = WebviewWindowBuilder::new(
        app,
        LEADER_WINDOW_LABEL,
        WebviewUrl::External(
            "about:blank"
                .parse()
                .map_err(|e| AgentError::NavigationFailed(format!("url parse: {}", e)))?,
        ),
    )
    .title("Consensus Arena — Leader")
    .inner_size(1200.0, 800.0)
    .visible(false)
    .initialization_script(GENERIC_INIT_SCRIPT)
    .on_navigation(make_nav_closure(leader_tx, LEADER_WINDOW_LABEL))
    .on_new_window(make_new_window_handler(
        leader_popup_tx,
        LEADER_WINDOW_LABEL,
    ))
    .on_page_load(move |window, payload| {
        handle_page_load(window, payload, &leader_diagnostics);
    })
    .build()
    .map_err(|e| AgentError::NavigationFailed(format!("leader window build failed: {}", e)))?;

    let nav_win = WebviewWindowBuilder::new(
        app,
        NAV_WINDOW_LABEL,
        WebviewUrl::External(
            "about:blank"
                .parse()
                .map_err(|e| AgentError::NavigationFailed(format!("url parse: {}", e)))?,
        ),
    )
    .title("Consensus Arena — Agent")
    .inner_size(1200.0, 800.0)
    .visible(false)
    .initialization_script(GENERIC_INIT_SCRIPT)
    .on_navigation(make_nav_closure(nav_tx, NAV_WINDOW_LABEL))
    .on_new_window(make_new_window_handler(nav_popup_tx, NAV_WINDOW_LABEL))
    .on_page_load(move |window, payload| {
        handle_page_load(window, payload, &nav_diagnostics);
    })
    .build()
    .map_err(|e| AgentError::NavigationFailed(format!("nav window build failed: {}", e)))?;

    state.leader_window = Some(leader_win.clone());
    state.nav_window = Some(nav_win.clone());
    Ok(())
}

/// Restore the one shared participant WebView if it was closed after setup.
/// This never creates a second nav window and leaves the persistent leader
/// WebView intact.
pub fn ensure_nav_window(app: &AppHandle, state: &mut BrowserState) -> Result<WebviewWindow, AgentError> {
    if let Some(window) = state.nav_window.clone() {
        return Ok(window);
    }
    if let Some(window) = app.get_webview_window(NAV_WINDOW_LABEL) {
        state.nav_window = Some(window.clone());
        return Ok(window);
    }

    let nav_tx = state.nav_tx.clone();
    let nav_popup_tx = state.nav_tx.clone();
    let nav_diagnostics = state.diagnostics.clone();
    let window = WebviewWindowBuilder::new(
        app,
        NAV_WINDOW_LABEL,
        WebviewUrl::External(
            "about:blank"
                .parse()
                .map_err(|e| AgentError::NavigationFailed(format!("url parse: {e}")) )?,
        ),
    )
    .title("Consensus Arena — Agent")
    .inner_size(1200.0, 800.0)
    .visible(false)
    .initialization_script(GENERIC_INIT_SCRIPT)
    .on_navigation(make_nav_closure(nav_tx, NAV_WINDOW_LABEL))
    .on_new_window(make_new_window_handler(nav_popup_tx, NAV_WINDOW_LABEL))
    .on_page_load(move |window, payload| {
        handle_page_load(window, payload, &nav_diagnostics);
    })
    .build()
    .map_err(|e| AgentError::NavigationFailed(format!("nav window recreate failed: {e}")))?;
    state.nav_window = Some(window.clone());
    Ok(window)
}

/// Re-run the submit ACTION on a page whose active-turn auto-submit did not get
/// confirmed. Evals the generic `__caRetrySubmit` helper (defined by
/// GENERIC_INIT_SCRIPT), which rediscoveries the composer and re-invokes
/// `__caSubmitActivePrompt` so the exact expected agent/turn is preserved.
pub fn retry_active_submit(
    window: &WebviewWindow,
    agent_id: &str,
    turn: u32,
) -> Result<(), AgentError> {
    let agent_json = serde_json::to_string(agent_id).map_err(|error| {
        AgentError::InjectionFailed(format!(
            "retry submit identity serialization failed: {error}"
        ))
    })?;
    let js = format!(
        "try {{ if (typeof window.__caRetrySubmit === 'function') {{ window.__caRetrySubmit({agent_json}, {turn}); }} }} catch (e) {{}}"
    );
    window.eval(&js).map_err(|error| {
        AgentError::InjectionFailed(format!("retry submit eval failed: {error}"))
    })
}

/// Start response capture for a message the user sent manually during setup.
/// Unlike `build_inject_js`, this does not modify the input or click Send.
pub fn monitor_existing_response(
    window: &WebviewWindow,
    agent_id: &str,
    turn: u32,
) -> Result<(), AgentError> {
    let agent_json = serde_json::to_string(agent_id).map_err(|error| {
        AgentError::ExtractionFailed(format!(
            "response monitor identity serialization failed: {error}"
        ))
    })?;
    let script = format!(
        r#"(function() {{
  var RESP_SELECTORS = [
    '[data-message-author-role="assistant"]',
    '[data-testid="assistant-message"]',
    '[class*="assistant-message"]',
    '[class*="ai-message"]',
    '[class*="bot-message"]',
    '[class*="model-response"]',
    '.markdown',
    '.prose'
  ];
  var AGENT_ID = {};
  var TURN = {};
  var _last = '';
  var _stable = 0;
  var _seen = false;
  var _checks = 0;

  function latestResponse() {{
    for (var i = 0; i < RESP_SELECTORS.length; i++) {{
      var els = document.querySelectorAll(RESP_SELECTORS[i]);
      if (els.length > 0) {{
        var text = (els[els.length - 1].innerText || '').trim();
        if (text.length > 0) return text;
      }}
    }}
    return '';
  }}

  function poll() {{
    _checks++;
    if (_checks > 720) return;
    var text = latestResponse();
    if (text.length > 0) {{
      if (!_seen || text !== _last) {{
        _seen = true;
        _last = text;
        _stable = 0;
      }} else {{
        _stable++;
        if (_stable >= 4) {{
          window.__ca_lastResponse = text;
          var encoded = encodeURIComponent(text.substring(0, 8000));
          try {{ window.location.href = 'arena://response/' + AGENT_ID + '/' + TURN + '/' + encoded; }} catch (e) {{}}
          setTimeout(function() {{
            try {{ window.location.href = 'arena://done/' + AGENT_ID + '/' + TURN; }} catch (e) {{}}
          }}, 200);
          return;
        }}
      }}
    }}
    setTimeout(poll, 500);
  }}
  poll();
}})();"#,
        agent_json, turn
    );
    window.eval(&script).map_err(|error| {
        AgentError::ExtractionFailed(format!("leader response monitor eval failed: {error}"))
    })
}

// ── build_inject_js ───────────────────────────────────────────────────────────
//
// Per-turn injection JS — eval'd into the target window for each prompt.
//
// D-036/D-042: #chat-input and div.chat-input-editor added to SELECTORS;
//              #send-message-button (GLM) and div.send-button-container (Kimi)
//              added to SEND_SELECTORS.
// D-042: contenteditable injection uses document.execCommand (Kimi Lexical + Claude.ai).
//
// Response monitoring polls for a new assistant response after the user sends.
// Once stable for ~2 seconds, fires:
//   arena://response/{AGENT_ID}/{TURN}/{url-encoded-text}  (text capped 8000 chars)
//   arena://done/{AGENT_ID}/{TURN}                         (200 ms later)
// A _baseline is captured before injection to avoid re-reporting old responses.

fn build_inject_js(prompt: &str, agent_id: &str, turn: u32, auto_submit: bool) -> String {
    let prompt_json = serde_json::to_string(prompt).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"(function() {{
  // ── Selectors ────────────────────────────────────────────────────────────
  var SELECTORS = [
    '#chat-input',
    'div.chat-input-editor[contenteditable="true"]',
    '#prompt-textarea',
    'div.ProseMirror[contenteditable="true"]',
    'div.ProseMirror',
    'rich-textarea div[contenteditable="true"]',
    'textarea[placeholder*="Message"]',
    'textarea',
    '[data-testid*="composer" i]',
    '[data-testid*="textbox" i]',
    '[data-testid*="input" i]',
    '[role="textbox"]',
    '[aria-multiline="true"]',
    'p[data-placeholder]',
    '[contenteditable="true"]'
  ];
  var CONTAINER_SELECTORS = [
    'form',
    'footer',
    'main',
    '[role="form"]',
    '[class*="composer" i]',
    '[class*="prompt" i]',
    '[class*="input" i]',
    '[class*="chat" i]',
    '[class*="textbox" i]',
    '[data-testid*="composer" i]',
    '[data-testid*="textbox" i]',
    '[data-testid*="input" i]'
  ];
  var DESCENDANT_SELECTORS = [
    'textarea',
    '[contenteditable="true"]',
    '[role="textbox"]',
    '[aria-multiline="true"]',
    'div.ProseMirror',
    'p[data-placeholder]',
    '[data-testid*="composer" i]',
    '[data-testid*="textbox" i]',
    '[data-testid*="input" i]'
  ];
  // Assistant-message selectors for response capture (most specific first).
  var RESP_SELECTORS = [
    '[data-message-author-role="assistant"]',
    '[data-testid="assistant-message"]',
    '[class*="assistant-message"]',
    '[class*="ai-message"]',
    '[class*="bot-message"]',
    '[class*="model-response"]',
    '.markdown',
    '.prose'
  ];

  var AGENT_ID = '{}';
  var TURN = {};
  var text = {};
  var AUTO_SUBMIT = {};

  // ── Helpers ───────────────────────────────────────────────────────────────
  function addUnique(target, el) {{
    if (el && target.indexOf(el) === -1) target.push(el);
  }}

  function isVisible(el) {{
    if (!el || !(el instanceof Element)) return false;
    var style = window.getComputedStyle(el);
    var rect = el.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  }}

  function normalizeInput(el) {{
    if (!el || !(el instanceof Element)) return null;
    if (el.matches && el.matches('p[data-placeholder]')) {{
      var placeholderAncestor = el.closest('[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror');
      if (placeholderAncestor) return normalizeInput(placeholderAncestor);
    }}
    if (el.matches && el.matches('[data-testid*="composer" i],[data-testid*="textbox" i],[data-testid*="input" i],form,footer,main,[role="form"]')) {{
      var nestedEditable = el.querySelector('textarea,[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror,p[data-placeholder]');
      if (nestedEditable) return normalizeInput(nestedEditable);
    }}
    if (el.matches && el.matches('div.ProseMirror') && el.getAttribute('contenteditable') !== 'true') {{
      var proseEditable = el.querySelector('[contenteditable="true"],[role="textbox"],[aria-multiline="true"],textarea');
      if (proseEditable) return normalizeInput(proseEditable);
    }}
    if (el.tagName === 'TEXTAREA' || el.getAttribute('contenteditable') === 'true' || el.getAttribute('role') === 'textbox' || el.getAttribute('aria-multiline') === 'true' || (el.matches && el.matches('div.ProseMirror'))) {{
      var editableAncestor = el.closest('[contenteditable="true"]');
      if (editableAncestor && editableAncestor !== el && el.getAttribute('contenteditable') !== 'true' && el.tagName !== 'TEXTAREA') return editableAncestor;
      return el;
    }}
    var nested = el.querySelector && el.querySelector('textarea,[contenteditable="true"],[role="textbox"],[aria-multiline="true"],div.ProseMirror,p[data-placeholder]');
    return nested ? normalizeInput(nested) : null;
  }}

  function findInput() {{
    var candidates = [];
    for (var i = 0; i < SELECTORS.length; i++) {{
      var nodes = [];
      try {{
        nodes = Array.prototype.slice.call(document.querySelectorAll(SELECTORS[i]));
      }} catch (e) {{}}
      for (var j = 0; j < nodes.length; j++) {{
        var directCandidate = normalizeInput(nodes[j]);
        if (directCandidate && isVisible(directCandidate)) addUnique(candidates, directCandidate);
      }}
    }}
    for (var c = 0; c < CONTAINER_SELECTORS.length; c++) {{
      var containers = [];
      try {{
        containers = Array.prototype.slice.call(document.querySelectorAll(CONTAINER_SELECTORS[c]));
      }} catch (e) {{}}
      for (var k = 0; k < containers.length; k++) {{
        var container = containers[k];
        if (!isVisible(container)) continue;
        for (var d = 0; d < DESCENDANT_SELECTORS.length; d++) {{
          var descendants = [];
          try {{
            descendants = Array.prototype.slice.call(container.querySelectorAll(DESCENDANT_SELECTORS[d]));
          }} catch (e) {{}}
          for (var m = 0; m < descendants.length; m++) {{
            var nestedCandidate = normalizeInput(descendants[m]);
            if (nestedCandidate && isVisible(nestedCandidate)) addUnique(candidates, nestedCandidate);
          }}
        }}
      }}
    }}
    return candidates.length > 0 ? candidates[0] : null;
  }}

  function findSend(input) {{
    // Composer-rooted only: reuse the ownership-aware discovery installed by
    // GENERIC_INIT_SCRIPT. Never search the document for a Send control.
    try {{
      if (typeof window.__ca_findOwnedSend === 'function') {{
        return window.__ca_findOwnedSend(input);
      }}
    }} catch (e) {{}}
    return null;
  }}

  function getLatestResponse() {{
    for (var i = 0; i < RESP_SELECTORS.length; i++) {{
      var els = document.querySelectorAll(RESP_SELECTORS[i]);
      if (els.length > 0) {{
        var t = (els[els.length - 1].innerText || '').trim();
        if (t.length > 0) return t;
      }}
    }}
    return '';
  }}

  // ── Response monitoring ───────────────────────────────────────────────────
  var _baseline = getLatestResponse();
  var _gotNew   = false;
  var _last     = '';
  var _stable   = 0;
  var _done     = false;
  var _checks   = 0;

  function pollResponse() {{
    if (_done) return;
    _checks++;
    if (_checks > 720) return; // ~6 minute hard cap

    var txt = getLatestResponse();

    if (!_gotNew) {{
      if (txt !== _baseline) {{
        _gotNew = true;
        _last   = txt;
      }}
    }} else {{
      if (txt === _last) {{
        _stable++;
        if (_stable >= 4) {{ // 4 × 500 ms = 2 s stable → response complete
          _done = true;
          window.__ca_lastResponse = txt;
          var enc = encodeURIComponent(txt.substring(0, 8000));
          try {{ window.location.href = 'arena://response/' + AGENT_ID + '/' + TURN + '/' + enc; }} catch (e) {{}}
          setTimeout(function() {{
            try {{ window.location.href = 'arena://done/' + AGENT_ID + '/' + TURN; }} catch (e) {{}}
          }}, 200);
          return;
        }}
      }} else {{
        _stable = 0;
        _last   = txt;
      }}
    }}
    setTimeout(pollResponse, 500);
  }}

  // ── Injection ─────────────────────────────────────────────────────────────
  function reportInjection(input, method, error) {{
    var visible = input ? ((input.value || input.textContent || '').trim()) : '';
    var prefix = visible.indexOf(text.slice(0, 32)) === 0;
    var suffix = text.length < 32 || visible.slice(-32) === text.slice(-32);
    var send = findSend(input);
    try {{ window.location.href = 'arena://prompt-injection/' + AGENT_ID + '/' + method + '/' + (prefix ? '1' : '0') + '/' + (suffix ? '1' : '0') + '/' + visible.length + '/' + (send ? '1' : '0') + '/' + (input ? input.tagName.toLowerCase() : 'none') + '/' + encodeURIComponent(input && input.getAttribute('role') || '') + '/' + encodeURIComponent(input && input.getAttribute('contenteditable') || '') + '/' + encodeURIComponent(error || ''); }} catch (e) {{}}
    return prefix && suffix;
  }}

  function reportSubmitOutcome(ok, methodName, err) {{
    try {{ window.location.href = 'arena://active-submit/' + AGENT_ID + '/' + TURN + '/' + (ok ? '1' : '0') + '/' + encodeURIComponent(methodName) + '/0/' + encodeURIComponent(err || ''); }} catch (e) {{}}
  }}

var _injectAttempts = 0;
  var MAX_INJECT_ATTEMPTS = 50; // 50 × 200 ms ≈ 10 s before reporting failure
  function inject() {{
    var input = findInput();
    if (!input) {{
      _injectAttempts++;
      if (_injectAttempts < MAX_INJECT_ATTEMPTS) {{ setTimeout(inject, 200); return; }}
      reportInjection(null, 'none', 'input_not_found_after_retry');
      if (AUTO_SUBMIT) reportSubmitOutcome(false, 'none', 'input_not_found_after_retry');
      return;
    }}

    // Idempotency guard: if the prompt is already fully visible in the input,
    // skip re-injection (e.g., after page reload where __ca_lastInjectedText was lost).
    var visible = input ? ((input.value || input.textContent || '').trim()) : '';
    var prefixOk = visible.indexOf(text.slice(0, Math.min(32, text.length))) === 0;
    var suffixOk = text.length < 32 || visible.slice(-32) === text.slice(-32);
    if (prefixOk && suffixOk && visible.length >= text.length) {{
      // Prompt already present — report success and proceed to submit if AUTO_SUBMIT
      reportInjection(input, 'idempotent_skip', '');
      if (AUTO_SUBMIT) {{
        try {{ window.__ca_lastInjectedText = text; }} catch (e) {{}}
        if (typeof window.__caSubmitActivePrompt === 'function') {{
          window.__caSubmitActivePrompt(input, AGENT_ID, TURN);
        }} else {{
          reportSubmitOutcome(false, 'none', 'submit_helper_missing');
        }}
      }}
      return;
    }}

    var method = 'unsupported';
    var methodError = '';
    if (input.tagName === 'TEXTAREA') {{
      try {{
        var setter = Object.getOwnPropertyDescriptor(
          window.HTMLTextAreaElement.prototype, 'value'
        ).set;
        setter.call(input, text);
        input.dispatchEvent(new Event('input', {{ bubbles: true }}));
        method = 'textarea_value';
      }} catch (e) {{ methodError = String((e && e.message) || e); }}
    }} else if (input.contentEditable === 'true') {{
      // D-042: execCommand works for Kimi Lexical and all other contenteditable
      // editors (Claude.ai, etc.).
      try {{
        input.focus();
        document.execCommand('selectAll', false, null);
        document.execCommand('insertText', false, text);
        method = 'contenteditable_exec_command';
      }} catch (e) {{
        methodError = String((e && e.message) || e);
        // Fallback: set text directly and dispatch input so frameworks
        // register the change even when execCommand is blocked.
        try {{
          input.textContent = text;
          method = 'contenteditable_text_content';
          methodError = '';
        }} catch (e2) {{ methodError = methodError || String((e2 && e2.message) || e2); }}
      }}
      input.dispatchEvent(new Event('input', {{ bubbles: true }}));
      input.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: 'Unidentified' }}));
    }}
    var integrityOk = reportInjection(input, method, methodError || (method === 'unsupported' ? 'unsupported_input' : ''));
    if (AUTO_SUBMIT) {{
      // Stamp the injected prompt so the submit helper and any retry can prove
      // they are acting on the CURRENT composer (see currentComposerRoot /
      // collectComposerSnapshot). Best-effort only; a sealed/non-writable
      // window must not break the normal path.
      try {{ window.__ca_lastInjectedText = text; }} catch (e) {{}}
      if (!integrityOk || methodError) {{
        reportSubmitOutcome(false, method, methodError || 'prompt_integrity_failed');
      }} else if (typeof window.__caSubmitActivePrompt === 'function') {{
        window.__caSubmitActivePrompt(input, AGENT_ID, TURN);
      }} else {{
        reportSubmitOutcome(false, 'none', 'submit_helper_missing');
      }}
    }}
  }}

  inject();
  // Active injection submits only through the phase-gated static helper above;
  // setup injection remains observation-only.
  setTimeout(pollResponse, 1500);
}})();"#,
        agent_id, turn, prompt_json, if auto_submit { "true" } else { "false" }
    )
}
