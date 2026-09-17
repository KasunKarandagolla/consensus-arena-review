use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    /// Temporary failure — worth retrying with backoff.
    Transient,
    /// Rate limit hit — back off and set cooldown on the agent.
    RateLimit,
    /// Permanent failure — do not retry; surface to operator.
    Permanent,
}

#[derive(Debug)]
pub enum AgentError {
    InjectionFailed(String),
    ExtractionFailed(String),
    Timeout(String),
    CaptchaRequired(String),
    ContextLimitReached(String),
    SessionExpired(String),
    NavigationFailed(String),
    DatabaseError(String),
    NetworkError(String),
    UnknownError(String),
}

impl AgentError {
    /// A bounded category safe for persistent logs. The Display message can
    /// contain provider, browser, or user supplied text.
    pub fn category(&self) -> &'static str {
        match self {
            AgentError::InjectionFailed(_) => "injection_failed",
            AgentError::ExtractionFailed(_) => "extraction_failed",
            AgentError::Timeout(_) => "timeout",
            AgentError::CaptchaRequired(_) => "captcha_required",
            AgentError::ContextLimitReached(_) => "context_limit",
            AgentError::SessionExpired(_) => "session_expired",
            AgentError::NavigationFailed(_) => "navigation_failed",
            AgentError::DatabaseError(_) => "database_error",
            AgentError::NetworkError(_) => "network_error",
            AgentError::UnknownError(_) => "unknown_error",
        }
    }

    /// Classify this error so callers can decide whether to retry.
    ///
    /// Default is Permanent — unknown errors should not be silently retried.
    pub fn kind(&self) -> ErrorKind {
        match self {
            // Transient: network / timing issues that may resolve on retry
            AgentError::Timeout(_) => ErrorKind::Transient,
            AgentError::InjectionFailed(_) => ErrorKind::Transient,
            AgentError::ExtractionFailed(_) => ErrorKind::Transient,
            AgentError::NavigationFailed(_) => ErrorKind::Transient,
            AgentError::NetworkError(_) => ErrorKind::Transient,
            // Permanent: require human action or indicate fundamental limits
            AgentError::CaptchaRequired(_) => ErrorKind::Permanent,
            AgentError::ContextLimitReached(_) => ErrorKind::Permanent,
            AgentError::SessionExpired(_) => ErrorKind::Permanent,
            AgentError::DatabaseError(_) => ErrorKind::Permanent,
            AgentError::UnknownError(_) => ErrorKind::Permanent,
        }
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::InjectionFailed(msg) => write!(f, "Injection failed: {}", msg),
            AgentError::ExtractionFailed(msg) => write!(f, "Extraction failed: {}", msg),
            AgentError::Timeout(msg) => write!(f, "Timeout: {}", msg),
            AgentError::CaptchaRequired(msg) => write!(f, "CAPTCHA required: {}", msg),
            AgentError::ContextLimitReached(msg) => write!(f, "Context limit reached: {}", msg),
            AgentError::SessionExpired(msg) => write!(f, "Session expired: {}", msg),
            AgentError::NavigationFailed(msg) => write!(f, "Navigation failed: {}", msg),
            AgentError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            AgentError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            AgentError::UnknownError(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<rusqlite::Error> for AgentError {
    fn from(e: rusqlite::Error) -> Self {
        AgentError::DatabaseError(e.to_string())
    }
}
