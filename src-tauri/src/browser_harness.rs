//! Browser Reliability Observability / Diagnostic Harness
//! Temporary, development-focused. Passive instrumentation only.
//! Satisfies spec sections 3-17, 20-22.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

// ── Constants ────────────────────────────────────────────────────────────────

/// Bounded per-agent timeline. Spec section 14: 500 events per agent default.
pub const BROWSER_EVENT_RING_BUFFER_LIMIT: usize = 500;
pub const MAX_HARNESS_DETAILS_BYTES: usize = 4096;

// ── Phase ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPhase {
    Idle,
    NavigationStarted,
    Login,
    Setup,
    ComposerDetection,
    Priming,
    WaitingForSend,
    Submitting,
    WaitingForResponse,
    ResponseCapture,
    Completed,
    Failed,
    Unknown,
    // Extended internal phases that map to spec
    Queued,
    Creating,
    Loading,
    Ready,
    Consulting,
    CaptchaOrChallenge,
    NavigationError,
    UnshowableUrl,
    Error,
}

impl HarnessPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::NavigationStarted => "navigation_started",
            Self::Login => "login",
            Self::Setup => "setup",
            Self::ComposerDetection => "composer_detection",
            Self::Priming => "priming",
            Self::WaitingForSend => "waiting_for_send",
            Self::Submitting => "submitting",
            Self::WaitingForResponse => "waiting_for_response",
            Self::ResponseCapture => "response_capture",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Queued => "queued",
            Self::Creating => "creating",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Consulting => "consulting",
            Self::CaptchaOrChallenge => "captcha_or_challenge",
            Self::NavigationError => "navigation_error",
            Self::UnshowableUrl => "unshowable_url",
            Self::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "idle" => Self::Idle,
            "navigation_started" => Self::NavigationStarted,
            "login" => Self::Login,
            "setup" => Self::Setup,
            "composer_detection" => Self::ComposerDetection,
            "priming" => Self::Priming,
            "waiting_for_send" => Self::WaitingForSend,
            "submitting" => Self::Submitting,
            "waiting_for_response" => Self::WaitingForResponse,
            "response_capture" => Self::ResponseCapture,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "unknown" => Self::Unknown,
            "queued" => Self::Queued,
            "creating" => Self::Creating,
            "loading" => Self::Loading,
            "ready" => Self::Ready,
            "consulting" => Self::Consulting,
            "captcha_or_challenge" => Self::CaptchaOrChallenge,
            "navigation_error" => Self::NavigationError,
            "unshowable_url" => Self::UnshowableUrl,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for HarnessPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Navigation Reason ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NavigationReason {
    Unknown,
    NormalNavigation,
    Reload,
    LoginRedirect,
    LogoutRedirect,
    Challenge,
    Authentication,
    ApplicationRedirect,
    ErrorPage,
    BrowserRecovery,
    UserNavigation,
}

impl NavigationReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NormalNavigation => "normal_navigation",
            Self::Reload => "reload",
            Self::LoginRedirect => "login_redirect",
            Self::LogoutRedirect => "logout_redirect",
            Self::Challenge => "challenge",
            Self::Authentication => "authentication",
            Self::ApplicationRedirect => "application_redirect",
            Self::ErrorPage => "error_page",
            Self::BrowserRecovery => "browser_recovery",
            Self::UserNavigation => "user_navigation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationForensics {
    pub from_url: String,
    pub to_url: String,
    pub timestamp: String,
    pub operation_id: String,
    pub phase: String,
    pub same_document: Option<bool>,
    pub navigation_reason: String,
    pub confidence: String,
    pub cause: String,
    pub arena_requested: bool,
}

// ── Event Type Taxonomy (spec section 5) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // WebView lifecycle
    WindowCreated,
    WindowDestroyed,
    NavigationStarted,
    NavigationCommitted,
    NavigationFinished,
    DocumentLoaded,
    DomContentLoaded,
    UrlChanged,
    PageReloaded,
    DocumentReplaced,
    // Authentication
    LoginStateUnknown,
    LoginRequired,
    LoginPageDetected,
    LoginInteractionStarted,
    LoginInteractionCompleted,
    LoginStateAuthenticated,
    LogoutDetected,
    AuthenticationRedirect,
    AuthenticationFailure,
    // Browser blockers
    ChallengeDetected,
    CaptchaDetected,
    CloudflareDetected,
    SecurityBlocked,
    NetworkBlocked,
    PageHealthBlocked,
    // Composer
    ComposerProbeStarted,
    ComposerDetected,
    ComposerLost,
    InputDetected,
    InputLost,
    SendProbeStarted,
    SendDetected,
    SendLost,
    AttachmentDetected,
    // Priming
    PrimingStarted,
    PrimingInjectionStarted,
    PrimingInjectionCompleted,
    PrimingInjectionFailed,
    PrimingPromptVisible,
    PrimingSendEnabled,
    PrimingSendDisabled,
    PrimingReinjectionStarted,
    PrimingReinjectionCompleted,
    PrimingCompleted,
    PrimingFailed,
    // Active operation
    ActivePromptInjectionStarted,
    ActivePromptInjectionCompleted,
    ActivePromptInjectionFailed,
    ActiveSubmitStarted,
    ActiveSubmitAttempt,
    ActiveSubmitCompleted,
    ActiveSubmitFailed,
    ResponseStarted,
    ResponseObserved,
    ResponseCompleted,
    // Diagnostics
    DomSnapshot,
    ComposerSnapshot,
    SendSnapshot,
    ConsoleError,
    ConsoleWarning,
    JavascriptError,
    UnhandledRejection,
    ResourceLoadError,
    AutomationError,
    ArenaProtocolError,
    // State machine
    PhaseChanged,
    StateChanged,
    RetryStarted,
    RetryExhausted,
    StaleSignalDetected,
    OperationCancelled,
    // Generic fallback
    Unknown,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WindowCreated => "window_created",
            Self::WindowDestroyed => "window_destroyed",
            Self::NavigationStarted => "navigation_started",
            Self::NavigationCommitted => "navigation_committed",
            Self::NavigationFinished => "navigation_finished",
            Self::DocumentLoaded => "document_loaded",
            Self::DomContentLoaded => "dom_content_loaded",
            Self::UrlChanged => "url_changed",
            Self::PageReloaded => "page_reloaded",
            Self::DocumentReplaced => "document_replaced",
            Self::LoginStateUnknown => "login_state_unknown",
            Self::LoginRequired => "login_required",
            Self::LoginPageDetected => "login_page_detected",
            Self::LoginInteractionStarted => "login_interaction_started",
            Self::LoginInteractionCompleted => "login_interaction_completed",
            Self::LoginStateAuthenticated => "login_state_authenticated",
            Self::LogoutDetected => "logout_detected",
            Self::AuthenticationRedirect => "authentication_redirect",
            Self::AuthenticationFailure => "authentication_failure",
            Self::ChallengeDetected => "challenge_detected",
            Self::CaptchaDetected => "captcha_detected",
            Self::CloudflareDetected => "cloudflare_detected",
            Self::SecurityBlocked => "security_blocked",
            Self::NetworkBlocked => "network_blocked",
            Self::PageHealthBlocked => "page_health_blocked",
            Self::ComposerProbeStarted => "composer_probe_started",
            Self::ComposerDetected => "composer_detected",
            Self::ComposerLost => "composer_lost",
            Self::InputDetected => "input_detected",
            Self::InputLost => "input_lost",
            Self::SendProbeStarted => "send_probe_started",
            Self::SendDetected => "send_detected",
            Self::SendLost => "send_lost",
            Self::AttachmentDetected => "attachment_detected",
            Self::PrimingStarted => "priming_started",
            Self::PrimingInjectionStarted => "priming_injection_started",
            Self::PrimingInjectionCompleted => "priming_injection_completed",
            Self::PrimingInjectionFailed => "priming_injection_failed",
            Self::PrimingPromptVisible => "priming_prompt_visible",
            Self::PrimingSendEnabled => "priming_send_enabled",
            Self::PrimingSendDisabled => "priming_send_disabled",
            Self::PrimingReinjectionStarted => "priming_reinjection_started",
            Self::PrimingReinjectionCompleted => "priming_reinjection_completed",
            Self::PrimingCompleted => "priming_completed",
            Self::PrimingFailed => "priming_failed",
            Self::ActivePromptInjectionStarted => "active_prompt_injection_started",
            Self::ActivePromptInjectionCompleted => "active_prompt_injection_completed",
            Self::ActivePromptInjectionFailed => "active_prompt_injection_failed",
            Self::ActiveSubmitStarted => "active_submit_started",
            Self::ActiveSubmitAttempt => "active_submit_attempt",
            Self::ActiveSubmitCompleted => "active_submit_completed",
            Self::ActiveSubmitFailed => "active_submit_failed",
            Self::ResponseStarted => "response_started",
            Self::ResponseObserved => "response_observed",
            Self::ResponseCompleted => "response_completed",
            Self::DomSnapshot => "dom_snapshot",
            Self::ComposerSnapshot => "composer_snapshot",
            Self::SendSnapshot => "send_snapshot",
            Self::ConsoleError => "console_error",
            Self::ConsoleWarning => "console_warning",
            Self::JavascriptError => "javascript_error",
            Self::UnhandledRejection => "unhandled_rejection",
            Self::ResourceLoadError => "resource_load_error",
            Self::AutomationError => "automation_error",
            Self::ArenaProtocolError => "arena_protocol_error",
            Self::PhaseChanged => "phase_changed",
            Self::StateChanged => "state_changed",
            Self::RetryStarted => "retry_started",
            Self::RetryExhausted => "retry_exhausted",
            Self::StaleSignalDetected => "stale_signal_detected",
            Self::OperationCancelled => "operation_cancelled",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "window_created" => Self::WindowCreated,
            "window_destroyed" => Self::WindowDestroyed,
            "navigation_started" => Self::NavigationStarted,
            "navigation_committed" => Self::NavigationCommitted,
            "navigation_finished" => Self::NavigationFinished,
            "document_loaded" => Self::DocumentLoaded,
            "dom_content_loaded" => Self::DomContentLoaded,
            "url_changed" => Self::UrlChanged,
            "page_reloaded" => Self::PageReloaded,
            "document_replaced" => Self::DocumentReplaced,
            "login_state_unknown" => Self::LoginStateUnknown,
            "login_required" => Self::LoginRequired,
            "login_page_detected" => Self::LoginPageDetected,
            "login_interaction_started" => Self::LoginInteractionStarted,
            "login_interaction_completed" => Self::LoginInteractionCompleted,
            "login_state_authenticated" => Self::LoginStateAuthenticated,
            "logout_detected" => Self::LogoutDetected,
            "authentication_redirect" => Self::AuthenticationRedirect,
            "authentication_failure" => Self::AuthenticationFailure,
            "challenge_detected" => Self::ChallengeDetected,
            "captcha_detected" => Self::CaptchaDetected,
            "cloudflare_detected" => Self::CloudflareDetected,
            "security_blocked" => Self::SecurityBlocked,
            "network_blocked" => Self::NetworkBlocked,
            "page_health_blocked" => Self::PageHealthBlocked,
            "composer_probe_started" => Self::ComposerProbeStarted,
            "composer_detected" => Self::ComposerDetected,
            "composer_lost" => Self::ComposerLost,
            "input_detected" => Self::InputDetected,
            "input_lost" => Self::InputLost,
            "send_probe_started" => Self::SendProbeStarted,
            "send_detected" => Self::SendDetected,
            "send_lost" => Self::SendLost,
            "attachment_detected" => Self::AttachmentDetected,
            "priming_started" => Self::PrimingStarted,
            "priming_injection_started" => Self::PrimingInjectionStarted,
            "priming_injection_completed" => Self::PrimingInjectionCompleted,
            "priming_injection_failed" => Self::PrimingInjectionFailed,
            "priming_prompt_visible" => Self::PrimingPromptVisible,
            "priming_send_enabled" => Self::PrimingSendEnabled,
            "priming_send_disabled" => Self::PrimingSendDisabled,
            "priming_reinjection_started" => Self::PrimingReinjectionStarted,
            "priming_reinjection_completed" => Self::PrimingReinjectionCompleted,
            "priming_completed" => Self::PrimingCompleted,
            "priming_failed" => Self::PrimingFailed,
            "active_prompt_injection_started" => Self::ActivePromptInjectionStarted,
            "active_prompt_injection_completed" => Self::ActivePromptInjectionCompleted,
            "active_prompt_injection_failed" => Self::ActivePromptInjectionFailed,
            "active_submit_started" => Self::ActiveSubmitStarted,
            "active_submit_attempt" => Self::ActiveSubmitAttempt,
            "active_submit_completed" => Self::ActiveSubmitCompleted,
            "active_submit_failed" => Self::ActiveSubmitFailed,
            "response_started" => Self::ResponseStarted,
            "response_observed" => Self::ResponseObserved,
            "response_completed" => Self::ResponseCompleted,
            "dom_snapshot" => Self::DomSnapshot,
            "composer_snapshot" => Self::ComposerSnapshot,
            "send_snapshot" => Self::SendSnapshot,
            "console_error" => Self::ConsoleError,
            "console_warning" => Self::ConsoleWarning,
            "javascript_error" => Self::JavascriptError,
            "unhandled_rejection" => Self::UnhandledRejection,
            "resource_load_error" => Self::ResourceLoadError,
            "automation_error" => Self::AutomationError,
            "arena_protocol_error" => Self::ArenaProtocolError,
            "phase_changed" => Self::PhaseChanged,
            "state_changed" => Self::StateChanged,
            "retry_started" => Self::RetryStarted,
            "retry_exhausted" => Self::RetryExhausted,
            "stale_signal_detected" => Self::StaleSignalDetected,
            "operation_cancelled" => Self::OperationCancelled,
            _ => Self::Unknown,
        }
    }
}

// ── DOM Snapshot ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomSnapshot {
    pub input: DomInputSnapshot,
    pub composer: DomComposerSnapshot,
    pub send: DomSendSnapshot,
    pub attachment: DomAttachmentSnapshot,
    // Identity lifecycle (ephemeral diagnostic only)
    pub input_identity: Option<String>,
    pub composer_identity: Option<String>,
    pub send_identity: Option<String>,
    pub attachment_identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomInputSnapshot {
    pub tag: String,
    pub exists: bool,
    pub visible: bool,
    pub value_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomComposerSnapshot {
    pub exists: bool,
    pub candidate_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomSendSnapshot {
    pub exists: bool,
    pub candidate_count: usize,
    pub enabled: bool,
    pub text: String,
    pub aria_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomAttachmentSnapshot {
    pub exists: bool,
    pub candidate_count: usize,
}

// ── Error Classification (spec 11) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorOrigin {
    Website,
    Arena,
    Environment,
    Authentication,
    Security,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedError {
    pub origin: String,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub automation_related: bool,
    pub source: String,
}

pub fn classify_console_error(category: &str, message: &str, source: &str) -> ClassifiedError {
    let lower = message.to_ascii_lowercase();
    let automation_markers = [
        "arena://",
        "__ca_",
        "prompt_injection",
        "active-submit",
        "send-probe",
    ];
    let automation_related = automation_markers.iter().any(|m| lower.contains(m) || source.contains(m));
    // Website telemetry failures that are NOT arena automation
    let website_noise = [
        "segment",
        "performance metrics",
        "load failed",
        "sentry",
        "analytics",
        "telemetry",
    ];
    let is_website_noise = website_noise.iter().any(|m| lower.contains(m)) && !automation_related;

    let (origin, category_out) = match category {
        "javascript_exception" | "unhandled_rejection" => {
            if automation_related {
                (ErrorOrigin::Arena, category.to_string())
            } else if is_website_noise {
                (ErrorOrigin::Website, "website_console_error".to_string())
            } else {
                (ErrorOrigin::Website, "website_console_error".to_string())
            }
        }
        "console_error" => {
            if automation_related {
                (ErrorOrigin::Arena, "arena_injection_error".to_string())
            } else if is_website_noise {
                (ErrorOrigin::Website, "website_console_error".to_string())
            } else {
                (ErrorOrigin::Website, "website_console_error".to_string())
            }
        }
        "console_warning" => (ErrorOrigin::Website, "website_console_warning".to_string()),
        "navigation_error" => (ErrorOrigin::Arena, "arena_navigation_error".to_string()),
        "automation_error" | "injection_error" | "submission_error" => {
            (ErrorOrigin::Arena, format!("arena_{category}"))
        }
        "challenge_blocker" | "captcha_detected" | "cloudflare_detected" => {
            (ErrorOrigin::Security, category.to_string())
        }
        "login_blocker" => (ErrorOrigin::Authentication, category.to_string()),
        "diagnostic_bridge_error" => (ErrorOrigin::Environment, category.to_string()),
        _ => (ErrorOrigin::Unknown, category.to_string()),
    };

    let severity = match category {
        "console_warning" => "warning",
        _ => "error",
    };

    ClassifiedError {
        origin: match origin {
            ErrorOrigin::Website => "website",
            ErrorOrigin::Arena => "arena",
            ErrorOrigin::Environment => "environment",
            ErrorOrigin::Authentication => "authentication",
            ErrorOrigin::Security => "security",
            ErrorOrigin::Unknown => "unknown",
        }
        .to_string(),
        category: category_out,
        severity: severity.to_string(),
        message: sanitize_details_value(message),
        automation_related,
        source: source.to_string(),
    }
}

// ── Browser Event (spec 3) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserEvent {
    pub timestamp: String,
    pub session_id: String,
    pub agent_id: String,
    pub display_name: String,
    pub window_label: String,
    pub window_kind: String,
    pub setup_generation: u32,
    pub phase: String,
    pub operation_id: String,
    pub event_type: String,
    pub url: String,
    pub details: serde_json::Value,
    // Identity propagation per spec 12
    pub expected_agent_id: Option<String>,
}

// ── Redaction ────────────────────────────────────────────────────────────────

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

pub fn redact_url(value: &str) -> String {
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

pub fn sanitize_details_value(raw: &str) -> String {
    let mut redacted = String::new();
    let mut redact_next = false;
    for part in raw.split_whitespace() {
        if redact_next {
            redact_next = false;
            redacted.push_str("[REDACTED] ");
            continue;
        }
        let lower = part.to_ascii_lowercase();
        if lower == "bearer" {
            redact_next = true;
            redacted.push_str(part);
            redacted.push(' ');
            continue;
        }
        if lower.contains("api_key")
            || lower.contains("apikey")
            || lower.starts_with("sk-")
            || lower.starts_with("token-")
        {
            redacted.push_str("[REDACTED] ");
            continue;
        }
        let is_long_secret = part.len() >= 32
            && part.chars().any(|c| c.is_ascii_alphabetic())
            && part.chars().any(|c| c.is_ascii_digit())
            && !part.contains('/')
            && !part.contains(':');
        if is_long_secret {
            redacted.push_str("[REDACTED] ");
        } else {
            redacted.push_str(part);
            redacted.push(' ');
        }
    }
    let mut msg = redacted.trim().to_string();
    if msg.len() > MAX_HARNESS_DETAILS_BYTES {
        msg.truncate(MAX_HARNESS_DETAILS_BYTES);
        msg.push_str(" [truncated]");
    }
    msg.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn sanitize_url(value: &str) -> String {
    redact_url(value)
}

// ── Operation IDs (spec 4) ───────────────────────────────────────────────────

pub fn operation_id_setup(agent_id: &str, generation: u32) -> String {
    format!("setup-{}-g{}", sanitize_id(agent_id), generation)
}
pub fn operation_id_login(agent_id: &str, generation: u32) -> String {
    format!("login-{}-g{}", sanitize_id(agent_id), generation)
}
pub fn operation_id_priming(agent_id: &str, generation: u32) -> String {
    format!("priming-{}-g{}", sanitize_id(agent_id), generation)
}
pub fn operation_id_active_turn(agent_id: &str, generation: u32, turn: u32) -> String {
    format!("active-turn-{}-g{}-t{}", sanitize_id(agent_id), generation, turn)
}
pub fn operation_id_submit(agent_id: &str, generation: u32, turn: u32) -> String {
    format!("submit-{}-g{}-t{}", sanitize_id(agent_id), generation, turn)
}
pub fn operation_id_diagnostic_single(agent_id: &str, generation: u32) -> String {
    format!("diagnostic-{}-g{}", sanitize_id(agent_id), generation)
}

fn sanitize_id(v: &str) -> String {
    v.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

// ── Navigation Reason Classification (spec 6) ────────────────────────────────

pub fn classify_navigation_reason(
    from_url: &str,
    to_url: &str,
    same_document: Option<bool>,
    page_health_hint: Option<&str>,
) -> (NavigationReason, Confidence) {
    // Never guess: default unknown low
    if from_url.is_empty() || to_url.is_empty() {
        return (NavigationReason::Unknown, Confidence::Low);
    }
    if same_document == Some(true) {
        return (NavigationReason::Unknown, Confidence::Low);
    }
    let lower_to = to_url.to_ascii_lowercase();
    let lower_from = from_url.to_ascii_lowercase();
    if lower_to == lower_from {
        return (NavigationReason::Reload, Confidence::Medium);
    }
    // Heuristics with confidence
    if lower_to.contains("login") || lower_to.contains("signin") || lower_to.contains("auth") {
        if lower_from.contains("chatgpt.com") || lower_from.contains("claude.ai") {
            return (NavigationReason::LoginRedirect, Confidence::Medium);
        }
        return (NavigationReason::Authentication, Confidence::Low);
    }
    if lower_to.contains("cloudflare") || lower_to.contains("challenge") || lower_to.contains("captcha") {
        return (NavigationReason::Challenge, Confidence::Medium);
    }
    if let Some(hint) = page_health_hint {
        if hint.contains("error_page") || hint.contains("application error") {
            return (NavigationReason::ErrorPage, Confidence::Medium);
        }
    }
    if lower_to.contains("error") || lower_to.contains("404") || lower_to.contains("500") {
        return (NavigationReason::ErrorPage, Confidence::Low);
    }
    // Default unknown – evidence insufficient
    (NavigationReason::Unknown, Confidence::Low)
}

// ── Ring Buffer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct TimelineRing {
    events: VecDeque<BrowserEvent>,
    dropped: usize,
}

impl Default for TimelineRing {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            dropped: 0,
        }
    }
}

impl TimelineRing {
    fn push(&mut self, event: BrowserEvent) {
        if self.events.len() >= BROWSER_EVENT_RING_BUFFER_LIMIT {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.events.push_back(event);
    }

    fn snapshot(&self) -> Vec<BrowserEvent> {
        self.events.iter().cloned().collect()
    }
}

// ── BrowserTimeline Store ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BrowserTimeline {
    per_agent: Arc<Mutex<HashMap<String, TimelineRing>>>,
    // Global view for cross-agent ordering (optional, but we store per-agent and sort on snapshot)
    dropped_total: Arc<Mutex<HashMap<String, usize>>>,
}

impl Default for BrowserTimeline {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserTimeline {
    pub fn new() -> Self {
        Self {
            per_agent: Arc::new(Mutex::new(HashMap::new())),
            dropped_total: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.dropped_total
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    pub fn record(&self, event: BrowserEvent) {
        let agent = event.agent_id.clone();
        let dropped = {
            let mut map = self.per_agent.lock().unwrap_or_else(|p| p.into_inner());
            let ring = map.entry(agent.clone()).or_default();
            let before = ring.dropped;
            ring.push(event);
            ring.dropped.saturating_sub(before)
        };
        if dropped > 0 {
            let mut d = self.dropped_total.lock().unwrap_or_else(|p| p.into_inner());
            let entry = d.entry(agent).or_insert(0);
            *entry = entry.saturating_add(dropped);
        }
    }

    pub fn events_for(&self, agent_id: &str) -> Vec<BrowserEvent> {
        self.per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(agent_id)
            .map(|r| r.snapshot())
            .unwrap_or_default()
    }

    pub fn all_events_sorted(&self) -> Vec<BrowserEvent> {
        let mut all: Vec<BrowserEvent> = self
            .per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .flat_map(|r| r.snapshot())
            .collect();
        all.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        all
    }

    pub fn events_dropped(&self, agent_id: &str) -> usize {
        self.per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(agent_id)
            .map(|r| r.dropped)
            .unwrap_or(0)
    }

    pub fn total_events(&self) -> usize {
        self.per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|r| r.events.len())
            .sum()
    }

    pub fn agent_ids(&self) -> Vec<String> {
        self.per_agent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

// ── Helper to build BrowserEvent ────────────────────────────────────────────

pub fn build_browser_event(
    session_id: &str,
    agent_id: &str,
    display_name: &str,
    window_label: &str,
    window_kind: &str,
    setup_generation: u32,
    phase: &str,
    operation_id: &str,
    event_type: &str,
    url: &str,
    details: serde_json::Value,
    expected_agent_id: Option<String>,
) -> BrowserEvent {
    BrowserEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        display_name: display_name.to_string(),
        window_label: window_label.to_string(),
        window_kind: window_kind.to_string(),
        setup_generation,
        phase: if phase.is_empty() { "unknown".to_string() } else { phase.to_string() },
        operation_id: operation_id.to_string(),
        event_type: event_type.to_string(),
        url: redact_url(url),
        details,
        expected_agent_id,
    }
}

// ── Report Generation (spec 16) ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AgentReportView<'a> {
    pub agent_id: &'a str,
    pub display_name: &'a str,
    pub window_label: &'a str,
    pub window_kind: &'a str,
    pub session_id: &'a str,
    pub generation: u32,
    pub phase: &'a str,
    pub current_url: &'a str,
    pub operation_id: &'a str,
    pub events: Vec<BrowserEvent>,
    pub navigation: Vec<NavigationForensics>,
    pub console_count: usize,
    pub dropped: usize,
}

pub fn generate_reliability_report_markdown(
    timeline: &BrowserTimeline,
    diagnostics_snapshot: &[crate::browser_backend::BrowserDiagnosticRecord],
) -> String {
    let mut md = String::new();
    md.push_str("# BROWSER_RELIABILITY_REPORT\n\n");
    md.push_str(&format!("Generated: {}\n\n", chrono::Utc::now().to_rfc3339()));
    md.push_str(&format!("Agents with timeline: {:?}\n\n", timeline.agent_ids()));
    md.push_str(&format!("Total timeline events: {}\n\n", timeline.total_events()));

    let mut all = timeline.all_events_sorted();
    // Limit report preview to last 200 events to keep lightweight
    if all.len() > 200 {
        all = all[all.len() - 200..].to_vec();
    }

    for record in diagnostics_snapshot {
        md.push_str(&format!("## Model: {} ({})\n\n", record.display_name, record.agent_id));
        md.push_str(&format!("- Window: {} ({})\n", record.window_label, record.window_kind));
        md.push_str(&format!("- Session: {}\n", record.session_id));
        md.push_str(&format!("- Generation: {}\n", record.setup_generation));
        md.push_str(&format!("- Current phase: {}\n", record.current_phase));
        md.push_str(&format!("- Current URL: {}\n", record.last_navigation_url.as_deref().unwrap_or(&record.intended_url)));
        md.push_str(&format!("- Authentication / blocker: {}\n", record.last_blocker));
        md.push_str(&format!("- Timeline events dropped: {}\n", timeline.events_dropped(&record.agent_id)));
        md.push_str(&format!("- Console errors: {} warnings: {}\n", record.browser_console_error_count, record.browser_console_warning_count));
        md.push_str(&format!("- Navigation diagnostics: {} entries\n", record.navigation_diagnostics.len()));
        if let Some(nav) = record.last_navigation.as_ref() {
            md.push_str(&format!("- Last navigation: {} -> {} cause={} gen={}\n", nav.from_url, nav.to_url, nav.cause, nav.setup_generation));
        }
        md.push_str("\n### Timeline (chronological, last 50 for this agent)\n\n");
        let agent_events: Vec<&BrowserEvent> = all.iter().filter(|e| e.agent_id == record.agent_id).collect();
        let slice = if agent_events.len() > 50 { &agent_events[agent_events.len()-50..] } else { &agent_events[..] };
        if slice.is_empty() {
            md.push_str("_No timeline events recorded for this agent._\n\n");
        } else {
            for ev in slice {
                md.push_str(&format!("- {} [{}] {} op={} phase={} url={}\n", ev.timestamp, ev.event_type, ev.agent_id, ev.operation_id, ev.phase, ev.url));
                if !ev.details.is_null() && ev.details != serde_json::Value::Null {
                    let details_str = ev.details.to_string();
                    let truncated = if details_str.len() > 300 { format!("{} [truncated]", &details_str[..300]) } else { details_str };
                    md.push_str(&format!("  details: {}\n", truncated));
                }
            }
            md.push_str("\n");
        }
        md.push_str("### Navigation events\n\n");
        for nav in &record.navigation_diagnostics {
            md.push_str(&format!("- {} {} -> {} cause={} arena_requested={} phase={}\n", nav.timestamp, nav.from_url, nav.to_url, nav.cause, nav.arena_requested, nav.phase));
        }
        md.push_str("\n### Console diagnostics\n\n");
        for c in &record.console_diagnostics {
            md.push_str(&format!("- {} [{}] {} source={} url={} msg={}\n", c.timestamp, c.category, c.severity, c.source, c.url, truncate(&c.message, 200)));
        }
        md.push_str("\n### DOM / Composer state (latest)\n\n");
        md.push_str(&format!("- input_found={} send_button_found={} input_candidate_count={:?} composer_candidate_count={:?} send_button_candidate_count={:?}\n", record.input_found, record.send_button_found, record.input_candidate_count, record.composer_candidate_count, record.send_button_candidate_count));
        md.push_str(&format!("- send_enabled_after_injection={:?} prompt_visible_prefix_ok={:?} suffix_ok={:?}\n", record.send_button_enabled_after_injection, record.prompt_visible_prefix_ok, record.prompt_visible_suffix_ok));
        md.push_str(&format!("- injection_target_tag={:?} role={:?} contenteditable={:?} method={:?}\n", record.injection_target_tag, record.injection_target_role, record.injection_target_contenteditable, record.prompt_injection_method));
        md.push_str(&format!("- page_state_hint={:?} page_health_hint={:?} last_error={:?}\n", record.page_state_hint, record.page_health_hint, record.last_error));
        md.push_str("\n### Priming / Submission result\n\n");
        md.push_str(&format!("- setup_completion_reason={:?}\n", record.setup_completion_reason));
        md.push_str(&format!("- prompt_injected_at={:?} error={:?}\n", record.prompt_injected_at, record.prompt_injection_error));
        md.push_str(&format!("- active_turn={:?} active_submit_succeeded={:?} method={:?} error={:?}\n", record.active_turn_number, record.active_auto_submit_succeeded, record.active_auto_submit_method, record.active_submit_error));
        md.push_str("\n### Final diagnosis (evidence-based)\n\n");
        // Evidence-based diagnosis: look at last navigation after injection
        let last_nav_after_injection = record.last_navigation.as_ref().filter(|n| {
            if let Some(injected) = record.prompt_injected_at.as_deref() {
                n.timestamp > injected.to_string()
            } else { false }
        });
        if let Some(nav) = last_nav_after_injection {
            if nav.cause == "page_initiated" && !nav.arena_requested {
                md.push_str("Observed: page-initiated navigation after prompt injection. Composer likely recreated; prompt may have been destroyed before send became enabled. Subsystem: POST-INJECTION NAVIGATION / COMPOSER LIFECYCLE Confidence: HIGH\n\n");
            } else {
                md.push_str("No post-injection page navigation detected in last snapshot. If pipeline stuck, check send_probe and composer lifecycle events above.\n\n");
            }
        } else if record.prompt_injection_error.is_some() {
            md.push_str(&format!("Observed injection error: {:?}. Likely failure subsystem: INJECTION / COMPOSER DETECTION Confidence: MEDIUM\n\n", record.prompt_injection_error));
        } else if record.send_button_enabled_after_injection == Some(false) {
            md.push_str("Observed: prompt visible but send disabled. Likely waiting for send enablement; check waiting probes and navigation history.\n\n");
        } else {
            md.push_str("No definitive failure pattern in last snapshot. Review full timeline above for WHAT/WHEN/WHERE per operation.\n\n");
        }
    }

    md.push_str("---\n");
    md.push_str("Report is evidence-based; do not generate speculative root causes beyond observed sequences.\n");
    md
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{} [truncated]", &s[..n]) }
}

// ── Cross-Platform Forensic Extensions (spec §4-12, §17-19) ─────────────────

pub const MAX_LIFECYCLE_RECORDS_PER_AGENT: usize = 100;
pub const MAX_ACTION_RECORDS_PER_AGENT: usize = 100;
pub const MAX_NAVIGATION_INTENT_RECORDS_PER_AGENT: usize = 100;
pub const MAX_SAFE_DOM_SNAPSHOTS_PER_AGENT: usize = 20;
pub const MAX_BUTTON_LABELS: usize = 10;
pub const MAX_LABEL_LENGTH: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationIntent {
    pub intent_id: String,
    pub agent_id: String,
    pub window_label: String,
    pub window_kind: String,
    pub url: String,
    pub timestamp: String,
    pub reason: String,
    pub setup_generation: u32,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLifecycleEvent {
    pub event_type: String,
    pub timestamp: String,
    pub url: String,
    pub title: String,
    pub agent_id: String,
    pub window_label: String,
    pub window_kind: String,
    pub setup_generation: u32,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeElement {
    pub tag: String,
    pub role: String,
    pub aria_label: String,
    pub name: String,
    pub enabled: bool,
    pub visible: bool,
    pub bounding_rect: Option<BoundingRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeDomForensics {
    pub url: String,
    pub title: String,
    pub active_element: SafeElement,
    pub button_labels: Vec<String>,
    pub input_types: Vec<String>,
    pub input_placeholders: Vec<String>,
    pub link_labels: Vec<String>,
    pub candidate_login_buttons: Vec<SafeElement>,
    pub candidate_next_buttons: Vec<SafeElement>,
    pub candidate_send_buttons: Vec<SafeElement>,
    pub candidate_attachment_buttons: Vec<SafeElement>,
    pub timestamp: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTarget {
    pub tag: String,
    pub role: String,
    pub aria_label: String,
    pub placeholder: String,
    pub method: String,
    pub text_length: usize,
    pub text_hash: String,
    pub classification: String,
    pub enabled: bool,
    pub visible: bool,
    pub coordinates: Option<(f64, f64)>,
    pub bounding_rect: Option<BoundingRect>,
    pub selection_logic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub action: String,
    pub actor: String,
    pub agent_id: String,
    pub window_label: String,
    pub window_kind: String,
    pub timestamp: String,
    pub reason: String,
    pub target: ActionTarget,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_id: String,
    pub timestamp: String,
    pub url_before: String,
    pub url_after: String,
    pub title_before: String,
    pub title_after: String,
    pub navigation_detected: bool,
    pub lifecycle_events: Vec<String>,
    pub console_errors_delta: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureClassification {
    NavigationStarted,
    NavigationCompleted,
    NavigationFailed,
    UnexpectedRedirect,
    UnexpectedReload,
    NavigationTimeout,
    LoginButtonMissing,
    LoginClickFailed,
    GoogleLoginDetected,
    GoogleLoginActionFailed,
    AuthRedirectDetected,
    AuthCompletionDetected,
    AuthStateUnknown,
    PageNotLoaded,
    PageHealthBlocked,
    ChallengeDetected,
    CaptchaDetected,
    LoginRequired,
    ComposerDetected,
    ComposerMissing,
    TargetNotFound,
    WrongTargetRejected,
    TargetDisabled,
    TargetNotVisible,
    TargetDetached,
    ClickFailed,
    InjectionFailed,
    SubmissionFailed,
    UnknownBrowserFailure,
}

impl FailureClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NavigationStarted => "navigation_started",
            Self::NavigationCompleted => "navigation_completed",
            Self::NavigationFailed => "navigation_failed",
            Self::UnexpectedRedirect => "unexpected_redirect",
            Self::UnexpectedReload => "unexpected_reload",
            Self::NavigationTimeout => "navigation_timeout",
            Self::LoginButtonMissing => "login_button_missing",
            Self::LoginClickFailed => "login_click_failed",
            Self::GoogleLoginDetected => "google_login_detected",
            Self::GoogleLoginActionFailed => "google_login_action_failed",
            Self::AuthRedirectDetected => "auth_redirect_detected",
            Self::AuthCompletionDetected => "auth_completion_detected",
            Self::AuthStateUnknown => "auth_state_unknown",
            Self::PageNotLoaded => "page_not_loaded",
            Self::PageHealthBlocked => "page_health_blocked",
            Self::ChallengeDetected => "challenge_detected",
            Self::CaptchaDetected => "captcha_detected",
            Self::LoginRequired => "login_required",
            Self::ComposerDetected => "composer_detected",
            Self::ComposerMissing => "composer_missing",
            Self::TargetNotFound => "target_not_found",
            Self::WrongTargetRejected => "wrong_target_rejected",
            Self::TargetDisabled => "target_disabled",
            Self::TargetNotVisible => "target_not_visible",
            Self::TargetDetached => "target_detached",
            Self::ClickFailed => "click_failed",
            Self::InjectionFailed => "injection_failed",
            Self::SubmissionFailed => "submission_failed",
            Self::UnknownBrowserFailure => "unknown_browser_failure",
        }
    }
}

pub fn new_navigation_intent_id() -> String {
    format!("nav-{}-{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0"))
}

pub fn sanitize_button_label(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s.len() > MAX_LABEL_LENGTH {
        s.truncate(MAX_LABEL_LENGTH);
        s.push_str("…");
    }
    sanitize_details_value(&s)
}

pub fn classify_failure_from_timeline(events: &[BrowserEvent]) -> FailureClassification {
    if events.is_empty() {
        return FailureClassification::UnknownBrowserFailure;
    }
    let last = events.last().expect("non-empty");
    match last.event_type.as_str() {
        "navigation_failed" => FailureClassification::NavigationFailed,
        "navigation_started" if last.details.get("unexpected").and_then(|v| v.as_bool()) == Some(true) => FailureClassification::UnexpectedReload,
        "challenge_detected" | "captcha_detected" => FailureClassification::ChallengeDetected,
        "composer_lost" | "input_lost" => FailureClassification::ComposerMissing,
        "login_page_detected" => FailureClassification::LoginRequired,
        "target_not_found" => FailureClassification::TargetNotFound,
        "wrong_button_rejected_pre_click" => FailureClassification::WrongTargetRejected,
        _ => FailureClassification::UnknownBrowserFailure,
    }
}

// ── Helpers for JS snapshots ─────────────────────────────────────────────────

pub fn empty_dom_snapshot() -> DomSnapshot {
    DomSnapshot {
        input: DomInputSnapshot { tag: "".to_string(), exists: false, visible: false, value_length: 0 },
        composer: DomComposerSnapshot { exists: false, candidate_count: 0 },
        send: DomSendSnapshot { exists: false, candidate_count: 0, enabled: false, text: "".to_string(), aria_label: "".to_string() },
        attachment: DomAttachmentSnapshot { exists: false, candidate_count: 0 },
        input_identity: None,
        composer_identity: None,
        send_identity: None,
        attachment_identity: None,
    }
}

pub fn empty_safe_dom_forensics(operation_id: &str, url: &str) -> SafeDomForensics {
    SafeDomForensics {
        url: redact_url(url),
        title: "".to_string(),
        active_element: SafeElement { tag: "".to_string(), role: "".to_string(), aria_label: "".to_string(), name: "".to_string(), enabled: false, visible: false, bounding_rect: None },
        button_labels: Vec::new(),
        input_types: Vec::new(),
        input_placeholders: Vec::new(),
        link_labels: Vec::new(),
        candidate_login_buttons: Vec::new(),
        candidate_next_buttons: Vec::new(),
        candidate_send_buttons: Vec::new(),
        candidate_attachment_buttons: Vec::new(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        operation_id: operation_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_url_params() {
        let url = "https://example.com/login?token=ABC&code=123&next=/chat";
        let redacted = redact_url(url);
        assert!(redacted.contains("token=%5BREDACTED%5D") || redacted.contains("token=[REDACTED]"));
        assert!(redacted.contains("code=%5BREDACTED%5D") || redacted.contains("code=[REDACTED]"));
        // non-sensitive param preserved
        assert!(redacted.contains("next="));
        // fragment removed
        let url2 = "https://example.com/page#section";
        assert!(!redact_url(url2).contains("#section"));
    }

    #[test]
    fn sanitizes_long_secret() {
        let msg = "Bearer sk-1234567890abcdef1234567890abcdef token and normal";
        let out = sanitize_details_value(msg);
        assert!(!out.contains("sk-1234"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn operation_ids_format() {
        assert_eq!(operation_id_setup("chatgpt", 1), "setup-chatgpt-g1");
        assert_eq!(operation_id_priming("chatgpt", 2), "priming-chatgpt-g2");
        assert_eq!(operation_id_active_turn("deepseek", 1, 3), "active-turn-deepseek-g1-t3");
        assert_eq!(operation_id_submit("deepseek", 1, 2), "submit-deepseek-g1-t2");
    }

    #[test]
    fn ring_buffer_wraps_and_tracks_dropped() {
        let tl = BrowserTimeline::new();
        for i in 0..(BROWSER_EVENT_RING_BUFFER_LIMIT + 5) {
            let ev = build_browser_event(
                "sess", "chatgpt", "ChatGPT", "arena-nav", "nav", 1, "priming",
                &operation_id_priming("chatgpt", 1),
                "dom_snapshot",
                "https://chatgpt.com/",
                serde_json::json!({ "i": i }),
                None,
            );
            tl.record(ev);
        }
        assert_eq!(tl.events_for("chatgpt").len(), BROWSER_EVENT_RING_BUFFER_LIMIT);
        assert_eq!(tl.events_dropped("chatgpt"), 5);
        // Oldest dropped
        let events = tl.events_for("chatgpt");
        assert!(events.iter().all(|e| e.details["i"].as_u64().unwrap_or(0) >= 5));
    }

    #[test]
    fn event_serialization_roundtrip() {
        let ev = build_browser_event(
            "sess123", "kimi", "Kimi", "arena-nav", "nav", 2, "submitting",
            &operation_id_submit("kimi", 2, 1),
            "active_submit_attempt",
            "https://www.kimi.com/chat/abc?token=SECRET",
            serde_json::json!({ "send_enabled": true }),
            Some("kimi".to_string()),
        );
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains("kimi"));
        // URL should be redacted
        assert!(!json.contains("SECRET"));
        assert!(json.contains("[REDACTED]"));
        let de: BrowserEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(de.agent_id, "kimi");
        assert_eq!(de.operation_id, "submit-kimi-g2-t1");
    }

    #[test]
    fn navigation_classification_unknown_low_by_default() {
        let (reason, conf) = classify_navigation_reason("https://chatgpt.com/", "https://chatgpt.com/c/xyz", None, None);
        assert_eq!(reason, NavigationReason::Unknown);
        assert_eq!(conf, Confidence::Low);
        let (r2, c2) = classify_navigation_reason("", "https://chatgpt.com/", None, None);
        assert_eq!(r2, NavigationReason::Unknown);
        assert_eq!(c2, Confidence::Low);
    }

    #[test]
    fn navigation_classification_reload_medium() {
        let (r, c) = classify_navigation_reason("https://chatgpt.com/", "https://chatgpt.com/", None, None);
        assert_eq!(r, NavigationReason::Reload);
        assert_eq!(c, Confidence::Medium);
    }

    #[test]
    fn phase_unknown_fallback() {
        assert_eq!(HarnessPhase::from_str(""), HarnessPhase::Unknown);
        assert_eq!(HarnessPhase::from_str("priming").as_str(), "priming");
        assert_eq!(HarnessPhase::from_str("nonexistent"), HarnessPhase::Unknown);
    }

    #[test]
    fn console_classification_separates_website_from_arena() {
        let web = classify_console_error("console_error", "Error sending segment performance metrics TypeError: Load failed", "console.error");
        assert_eq!(web.origin, "website");
        assert!(!web.automation_related);
        assert_eq!(web.category, "website_console_error");

        let arena = classify_console_error("console_error", "arena://prompt-injection failed __ca_ injection", "console.error");
        assert_eq!(arena.origin, "arena");
        assert!(arena.automation_related);
    }

    #[test]
    fn empty_unknown_values_handled() {
        let ev = build_browser_event("", "", "", "", "", 0, "", "", "", "", serde_json::Value::Null, None);
        assert_eq!(ev.phase, "unknown");
        assert_eq!(ev.url, "");
        assert_eq!(ev.session_id, "");
    }

    #[test]
    fn report_generation_contains_agent_sections() {
        let tl = BrowserTimeline::new();
        let ev = build_browser_event("s1", "chatgpt", "ChatGPT", "arena-leader", "leader", 1, "priming", "priming-chatgpt-g1", "priming_injection_started", "https://chatgpt.com/", serde_json::json!({}), None);
        tl.record(ev);
        let diag = crate::browser_backend::BrowserDiagnosticRecord {
            agent_id: "chatgpt".to_string(),
            display_name: "ChatGPT".to_string(),
            setup_generation: 1,
            session_id: "s1".to_string(),
            selected_leader_id: "chatgpt".to_string(),
            selected_agent_ids: vec!["chatgpt".to_string()],
            setup_order: vec!["chatgpt".to_string()],
            intended_url: "https://chatgpt.com".to_string(),
            window_label: "arena-leader".to_string(),
            window_kind: "leader".to_string(),
            assigned_window_label: "arena-leader".to_string(),
            assigned_window_kind: "leader".to_string(),
            is_selected_leader: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_navigation_url: Some("https://chatgpt.com/".to_string()),
            last_ready_at: None,
            last_send_detected_at: None,
            last_response_at: None,
            last_error: None,
            current_phase: "priming".to_string(),
            last_blocker: "none".to_string(),
            last_blocker_url_redacted: None,
            last_challenge_detected_at: None,
            resume_attempt_count: 0,
            last_resume_at: None,
            input_found: true,
            send_button_found: true,
            last_send_probe_at: None,
            last_user_submit_event_at: None,
            last_message_count_seen: None,
            sent_signal_emitted: false,
            expected_agent_id: Some("chatgpt".to_string()),
            last_signal_agent_id: None,
            last_signal_type: None,
            last_signal_at: None,
            stale_signal_count: 0,
            response_observed_before_send: false,
            response_observed_after_injection: false,
            setup_completion_reason: None,
            prompt_injected_at: Some(chrono::Utc::now().to_rfc3339()),
            prompt_injection_error: None,
            prompt_injection_method: Some("textarea-native-setter".to_string()),
            prompt_visible_prefix_ok: Some(true),
            prompt_visible_suffix_ok: Some(true),
            prompt_visible_length: Some(262),
            send_button_enabled_after_injection: Some(false),
            injection_target_tag: Some("TEXTAREA".to_string()),
            injection_target_role: Some("textbox".to_string()),
            injection_target_contenteditable: Some("".to_string()),
            readiness_timeout_ms: Some(45000),
            readiness_probe_count: Some(5),
            input_candidate_count: Some(1),
            composer_candidate_count: Some(15),
            send_button_candidate_count: Some(1),
            page_state_hint: Some("composer_detected".to_string()),
            page_health_hint: Some("interactive".to_string()),
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
            console_diagnostics: vec![],
            browser_console_error_count: 0,
            browser_console_warning_count: 0,
            browser_console_last_error_at: None,
            navigation_diagnostics: vec![],
            setup_navigation_recovery_count: 0,
            last_navigation: None,
        };
        let md = generate_reliability_report_markdown(&tl, &[diag]);
        assert!(md.contains("ChatGPT"));
        assert!(md.contains("priming_injection_started"));
        assert!(md.contains("BROWSER_RELIABILITY_REPORT"));
    }

    #[test]
    fn navigation_intent_id_unique_and_bounded() {
        let id1 = new_navigation_intent_id();
        let id2 = new_navigation_intent_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("nav-"));
    }

    #[test]
    fn failure_classification_maps_correctly() {
        assert_eq!(FailureClassification::NavigationFailed.as_str(), "navigation_failed");
        assert_eq!(FailureClassification::LoginButtonMissing.as_str(), "login_button_missing");
        let ev = build_browser_event("s", "chatgpt", "ChatGPT", "arena-nav", "nav", 1, "priming", "op", "navigation_failed", "https://chatgpt.com/", serde_json::json!({}), None);
        let cls = classify_failure_from_timeline(&[ev]);
        assert_eq!(cls, FailureClassification::NavigationFailed);
    }

    #[test]
    fn safe_dom_forensics_redacts_and_bounds() {
        let mut forensics = empty_safe_dom_forensics("op-1", "https://example.com/?token=SECRET&next=/chat");
        forensics.title = "Test Title ".repeat(50);
        forensics.button_labels = vec!["a".repeat(100)];
        let json = serde_json::to_string(&forensics).expect("serialize");
        assert!(json.contains("example.com"));
        assert!(!json.contains("SECRET"));
        // label truncated
        assert!(sanitize_button_label(&"a".repeat(100)).len() <= MAX_LABEL_LENGTH + 10);
    }

    #[test]
    fn bounded_retention_respects_limits() {
        // Simulate bounded retention for lifecycle (100)
        let mut deque: std::collections::VecDeque<PageLifecycleEvent> = std::collections::VecDeque::new();
        for i in 0..(MAX_LIFECYCLE_RECORDS_PER_AGENT + 10) {
            if deque.len() >= MAX_LIFECYCLE_RECORDS_PER_AGENT {
                deque.pop_front();
            }
            deque.push_back(PageLifecycleEvent {
                event_type: "load".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                url: format!("https://example.com/{}", i),
                title: "t".to_string(),
                agent_id: "chatgpt".to_string(),
                window_label: "arena-nav".to_string(),
                window_kind: "nav".to_string(),
                setup_generation: 1,
                operation_id: "op".to_string(),
            });
        }
        assert_eq!(deque.len(), MAX_LIFECYCLE_RECORDS_PER_AGENT);
    }

    #[test]
    fn timeline_ordering_chronological() {
        let tl = BrowserTimeline::new();
        let ev1 = build_browser_event("s", "chatgpt", "ChatGPT", "arena-nav", "nav", 1, "priming", "op1", "navigation_started", "https://a.com/", serde_json::json!({}), None);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let ev2 = build_browser_event("s", "chatgpt", "ChatGPT", "arena-nav", "nav", 1, "priming", "op1", "dom_snapshot", "https://a.com/", serde_json::json!({}), None);
        tl.record(ev2.clone());
        tl.record(ev1.clone());
        let sorted = tl.all_events_sorted();
        assert!(sorted[0].timestamp <= sorted[1].timestamp);
    }

    #[test]
    fn json_serialization_safe_dom_and_action() {
        let forensics = empty_safe_dom_forensics("op", "https://example.com/");
        let json = serde_json::to_string(&forensics).expect("serialize");
        let de: SafeDomForensics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(de.operation_id, "op");
        let target = ActionTarget {
            tag: "BUTTON".to_string(),
            role: "button".to_string(),
            aria_label: "Send".to_string(),
            placeholder: "".to_string(),
            method: "click".to_string(),
            text_length: 4,
            text_hash: "abc".to_string(),
            classification: "Send".to_string(),
            enabled: true,
            visible: true,
            coordinates: Some((10.0, 20.0)),
            bounding_rect: Some(BoundingRect { x: 0.0, y: 0.0, width: 100.0, height: 30.0 }),
            selection_logic: "text-match".to_string(),
        };
        let json2 = serde_json::to_string(&target).expect("serialize target");
        let de2: ActionTarget = serde_json::from_str(&json2).expect("deserialize target");
        assert_eq!(de2.classification, "Send");
    }
}
