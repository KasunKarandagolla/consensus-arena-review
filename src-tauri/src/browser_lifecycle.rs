//! Revocable browser readiness leases (Session 03).
//!
//! Readiness is native controller state, not a lossy `Ready(agent)` event.
//! Every Ready lease belongs to an exact `(window, owner agent, document
//! generation, policy revision, runtime version)` tuple:
//!
//! ```text
//! PASSIVE   browser/user owns the document; no prompt injection authority
//! VERIFYING native policy admits the document; bounded read-only composer
//!           verification may run; prompt injection still forbidden
//! READY     current generation proved a usable composer; active automation
//!           may run while this exact lease remains current
//! ```
//!
//! Locking: short `std::sync::Mutex` transitions only; `Notify` is a wakeup,
//! never the authority. No lock is held across `.await`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

/// Runtime version stamped into every lease. The generic browser runtime is
/// one static script; a lease minted for another runtime version is stale.
pub const LEASE_RUNTIME_VERSION: u32 = 1;

/// Bound for free-form lease reasons carried over the `arena://` bridge.
pub const LEASE_REASON_MAX_CHARS: usize = 128;

/// Exact identity of one managed-window document for one owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentToken {
    pub window_label: String,
    pub owner_agent_id: String,
    pub document_generation: u64,
}

/// Revocable authorization to run active automation. Valid only while it is
/// still the controller's current lease for its document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyLease {
    pub document: DocumentToken,
    pub policy_revision: u64,
    pub runtime_version: u32,
}

/// Authority phase of one managed window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeasePhase {
    Passive,
    Verifying,
    Ready,
}

/// Semantic lease signal kind carried from the generic runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaseSignalKind {
    Ready,
    Blocked,
    Revoked,
    Error,
}

impl LeaseSignalKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ready" => Some(Self::Ready),
            "blocked" => Some(Self::Blocked),
            "revoked" => Some(Self::Revoked),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
            Self::Revoked => "revoked",
            Self::Error => "error",
        }
    }
}

/// Declarative native provider policy. The default authorization rule is an
/// exact normalized application origin (`scheme + host + effective port`);
/// arbitrary subdomains or lookalike hosts are never implicitly eligible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPolicy {
    pub agent_id: String,
    pub application_origin: String,
    pub is_custom: bool,
    pub revision: u64,
}

/// Conservative browser-owned path prefixes. An eligible origin plus one of
/// these paths remains `PASSIVE` (login/challenge/OAuth/security surface).
fn browser_owned_path_prefixes() -> &'static [&'static str] {
    &[
        "/login",
        "/signin",
        "/sign-in",
        "/auth",
        "/oauth",
        "/challenge",
        "/captcha",
        "/verify",
        "/security",
        // Cloudflare challenge platform path served from the provider origin.
        "/cdn-cgi",
    ]
}

/// True when the URL path is a browser-owned auth/security surface.
/// Comparison is case-insensitive; prefix matching is deliberately
/// conservative (fail closed: `/authwall` stays `PASSIVE`).
pub fn is_browser_owned_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    browser_owned_path_prefixes()
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// True for challenge/login-shaped lease reasons (drives the setup
/// verification UX, never readiness itself).
pub fn is_auth_blocker_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("challenge")
        || lower.contains("login")
        || lower.contains("auth")
        || lower.contains("captcha")
        || lower.contains("verify")
        || lower.contains("security")
}

/// Normalized `scheme://host[:port]` identity. Default ports are omitted so
/// `https://claude.ai` and `https://claude.ai:443` compare equal.
pub fn normalized_origin(url: &tauri::Url) -> Option<String> {
    let scheme = url.scheme();
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let mut origin = format!("{scheme}://{host}");
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    Some(origin)
}

/// Developer-supplied reason bound for the `arena://` bridge (no newlines,
/// no control characters, bounded length).
pub fn bounded_reason(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(LEASE_REASON_MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

impl ProviderPolicy {
    /// Derive the exact application origin from a validated base URL. Used
    /// for built-ins and for persisted custom participants alike: the origin
    /// comes from the merged-registry base URL, never from a baked-in host
    /// list, so custom participants reach generic verification.
    pub fn from_base_url(
        agent_id: &str,
        base_url: &str,
        is_custom: bool,
        revision: u64,
    ) -> Option<Self> {
        let parsed = base_url.parse::<tauri::Url>().ok()?;
        let application_origin = normalized_origin(&parsed)?;
        Some(Self {
            agent_id: agent_id.to_string(),
            application_origin,
            is_custom,
            revision,
        })
    }

    /// Native eligibility: exact application origin plus a non-browser-owned
    /// path. Anything else (foreign origin, OAuth host, challenge host,
    /// auth/security path, malformed URL) is `PASSIVE`.
    pub fn application_eligible(&self, url: &str) -> bool {
        let Ok(parsed) = url.parse::<tauri::Url>() else {
            return false;
        };
        let Some(origin) = normalized_origin(&parsed) else {
            return false;
        };
        if origin != self.application_origin {
            return false;
        }
        !is_browser_owned_path(parsed.path())
    }
}

/// Per-window lifecycle state. Generation is per managed model window and
/// identifies the current full-document JS realm.
struct WindowLifecycleState {
    owner_agent_id: Option<String>,
    document_generation: u64,
    phase: LeasePhase,
    /// Sanitized current URL (origin + path, no query/fragment secrets).
    current_url: Option<String>,
    /// URL observed at the last full-document Started event.
    started_url: Option<String>,
    /// Bounded history of recent Started URLs. A Finished that matches an
    /// older Started entry but not the latest one is a superseded
    /// navigation and fails closed — while a redirect target that was never
    /// a Started URL can still enter VERIFYING.
    started_history: std::collections::VecDeque<String>,
    policy: Option<ProviderPolicy>,
    ready_lease: Option<ReadyLease>,
    blocked_reason: Option<String>,
}

impl WindowLifecycleState {
    fn new() -> Self {
        Self {
            owner_agent_id: None,
            document_generation: 0,
            phase: LeasePhase::Passive,
            current_url: None,
            started_url: None,
            started_history: std::collections::VecDeque::new(),
            policy: None,
            ready_lease: None,
            blocked_reason: None,
        }
    }

    fn revoke(&mut self) {
        self.phase = LeasePhase::Passive;
        self.ready_lease = None;
    }

    fn note_started(&mut self, sanitized_url: &str) {
        if self.started_history.len() >= 8 {
            self.started_history.pop_front();
        }
        self.started_history.push_back(sanitized_url.to_string());
    }
}

struct LifecycleInner {
    windows: HashMap<String, WindowLifecycleState>,
    policy_revision_counter: u64,
}

/// Outcome of a native full-document Finished evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishDecision {
    /// Candidate application document: runtime may verify (never Ready yet).
    Verify {
        token: DocumentToken,
        policy: ProviderPolicy,
    },
    /// Browser-owned/ineligible/ownerless document: no automation authority.
    Passive { reason: &'static str },
    /// Ambiguous or superseded completion: ignored, fail closed.
    Stale,
}

/// Outcome of applying one generation-bearing lease signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalOutcome {
    AcceptedReady(ReadyLease),
    AcceptedRevocation,
    RejectedStale,
}

/// Why a synchronous readiness check failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequireError {
    pub window_label: String,
    pub agent_id: String,
    pub phase: LeasePhase,
    pub detail: String,
}

/// Why an async readiness wait failed. `blocked_reason` carries the last
/// native blocker (challenge/login) so setup can show verification UX while
/// still requiring genuine composer evidence before proceeding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitTimeout {
    pub window_label: String,
    pub agent_id: String,
    pub last_phase: LeasePhase,
    pub blocked_reason: Option<String>,
}

/// Process-lifetime browser readiness authority shared by persistent-window
/// callbacks. Cloned handles share one state; never replaced per session.
#[derive(Clone)]
pub struct BrowserLifecycleController {
    inner: Arc<Mutex<LifecycleInner>>,
    changed: Arc<Notify>,
}

impl BrowserLifecycleController {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LifecycleInner {
                windows: HashMap::new(),
                policy_revision_counter: 0,
            })),
            changed: Arc::new(Notify::new()),
        }
    }

    /// Wakeup only (e.g. after user-completed verification). State inside the
    /// mutex stays authoritative; a wakeup never manufactures readiness.
    pub fn request_recheck(&self) {
        self.changed.notify_waiters();
    }

    /// Session boundary: revoke every window owner/lease without destroying
    /// the process-lifetime controller or its generation counters. Counters
    /// stay monotonic so stale in-flight JS signals cannot equal a fresh
    /// generation after reset.
    pub fn reset_for_session(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for state in inner.windows.values_mut() {
            state.owner_agent_id = None;
            state.phase = LeasePhase::Passive;
            state.ready_lease = None;
            state.blocked_reason = None;
            state.current_url = None;
            state.started_url = None;
            state.started_history.clear();
            state.policy = None;
        }
        drop(inner);
        self.changed.notify_waiters();
    }

    /// Register (or replace) the declarative policy for a managed window.
    /// Returns the fresh policy revision.
    pub fn register_policy(&self, window_label: &str, mut policy: ProviderPolicy) -> u64 {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        inner.policy_revision_counter = inner.policy_revision_counter.saturating_add(1);
        let revision = inner.policy_revision_counter;
        policy.revision = revision;
        inner
            .windows
            .entry(window_label.to_string())
            .or_insert_with(WindowLifecycleState::new)
            .policy = Some(policy);
        drop(inner);
        self.changed.notify_waiters();
        revision
    }

    /// Intentionally assign a model to a managed window, before navigation.
    /// Immediately revokes any old lease so the previous owner's evidence can
    /// never authorize the new owner — even before navigation finishes.
    pub fn assign_owner(
        &self,
        window_label: &str,
        agent_id: &str,
        mut policy: ProviderPolicy,
    ) -> u64 {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        inner.policy_revision_counter = inner.policy_revision_counter.saturating_add(1);
        let revision = inner.policy_revision_counter;
        policy.revision = revision;
        let state = inner
            .windows
            .entry(window_label.to_string())
            .or_insert_with(WindowLifecycleState::new);
        state.owner_agent_id = Some(agent_id.to_string());
        state.policy = Some(policy);
        state.revoke();
        state.blocked_reason = None;
        state.current_url = None;
        state.started_url = None;
        state.started_history.clear();
        drop(inner);
        self.changed.notify_waiters();
        revision
    }

    /// Full-document invalidation boundary. Increments the per-window
    /// document generation, revokes any prior lease, records the Started URL.
    /// A Start never grants `VERIFYING` or `READY`. Returns `None` when the
    /// window has no current owner (unmanaged navigation: fail closed).
    pub fn page_started(&self, window_label: &str, sanitized_url: &str) -> Option<DocumentToken> {
        let token = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let state = inner
                .windows
                .entry(window_label.to_string())
                .or_insert_with(WindowLifecycleState::new);
            let Some(owner) = state.owner_agent_id.clone() else {
                return None;
            };
            state.document_generation = state.document_generation.saturating_add(1);
            state.revoke();
            state.blocked_reason = None;
            state.current_url = Some(sanitized_url.to_string());
            state.started_url = Some(sanitized_url.to_string());
            state.note_started(sanitized_url);
            DocumentToken {
                window_label: window_label.to_string(),
                owner_agent_id: owner,
                document_generation: state.document_generation,
            }
        };
        self.changed.notify_waiters();
        Some(token)
    }

    /// Evaluate a native full-document Finished event against the current
    /// owner/generation. Never grants Ready. Ambiguous or superseded
    /// completions fail closed without mutating authority.
    pub fn page_finished(&self, window_label: &str, sanitized_url: &str) -> FinishDecision {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(state) = inner.windows.get_mut(window_label) else {
            return FinishDecision::Stale;
        };
        let Some(owner) = state.owner_agent_id.clone() else {
            return FinishDecision::Passive { reason: "no_owner" };
        };
        let Some(policy) = state.policy.clone() else {
            return FinishDecision::Passive {
                reason: "no_policy",
            };
        };
        let exact_started_match = state.started_url.as_deref() == Some(sanitized_url);
        // A Finished that matches an older Started entry but not the latest
        // one is a superseded navigation (delayed/out-of-order completion):
        // ignore without touching authority, even when same-origin.
        let superseded =
            !exact_started_match && state.started_history.iter().any(|u| u == sanitized_url);
        if superseded {
            return FinishDecision::Stale;
        }
        // Cross-origin completion that clearly belongs to a superseded
        // navigation (e.g. a delayed OAuth callback Finished after the app
        // document Started): ignore without touching authority.
        if !exact_started_match {
            let started_origin = state
                .started_url
                .as_deref()
                .and_then(|u| u.parse::<tauri::Url>().ok())
                .and_then(|u| normalized_origin(&u));
            let finished_origin = sanitized_url
                .parse::<tauri::Url>()
                .ok()
                .and_then(|u| normalized_origin(&u));
            if started_origin.is_some()
                && finished_origin.is_some()
                && started_origin != finished_origin
            {
                return FinishDecision::Stale;
            }
        }
        if !policy.application_eligible(sanitized_url) {
            // Only an exact-URL Finished may downgrade to Passive; an
            // ambiguous delayed completion must never clobber a newer
            // document's VERIFYING/READY authority.
            if exact_started_match {
                state.phase = LeasePhase::Passive;
                state.ready_lease = None;
                state.current_url = Some(sanitized_url.to_string());
                drop(inner);
                self.changed.notify_waiters();
                return FinishDecision::Passive {
                    reason: "policy_ineligible",
                };
            }
            return FinishDecision::Stale;
        }
        // Eligible candidate: enter VERIFYING (upgrade only — never downgrade
        // an existing READY lease; a real reload always Starts first).
        if state.phase == LeasePhase::Ready {
            return FinishDecision::Stale;
        }
        state.phase = LeasePhase::Verifying;
        state.blocked_reason = None;
        state.current_url = Some(sanitized_url.to_string());
        let token = DocumentToken {
            window_label: window_label.to_string(),
            owner_agent_id: owner,
            document_generation: state.document_generation,
        };
        drop(inner);
        self.changed.notify_waiters();
        FinishDecision::Verify { token, policy }
    }

    /// Apply one generation-bearing semantic lease signal. The browser can
    /// never change owner, generation, or policy — those are native
    /// authority. Stale owner/generation signals are rejected; a stale
    /// Blocked/Revoked can never revoke a newer generation's lease.
    pub fn apply_signal(
        &self,
        window_label: &str,
        agent_id: &str,
        document_generation: u64,
        kind: LeaseSignalKind,
        reason: &str,
    ) -> SignalOutcome {
        let outcome = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let Some(state) = inner.windows.get_mut(window_label) else {
                return SignalOutcome::RejectedStale;
            };
            if state.owner_agent_id.as_deref() != Some(agent_id) {
                return SignalOutcome::RejectedStale;
            }
            if state.document_generation != document_generation {
                return SignalOutcome::RejectedStale;
            }
            let reason = bounded_reason(reason);
            match kind {
                LeaseSignalKind::Ready => {
                    if state.phase != LeasePhase::Verifying {
                        return SignalOutcome::RejectedStale;
                    }
                    let eligible = match (&state.policy, &state.current_url) {
                        (Some(policy), Some(url)) => policy.application_eligible(url),
                        _ => false,
                    };
                    if !eligible {
                        return SignalOutcome::RejectedStale;
                    }
                    let Some(policy) = state.policy.clone() else {
                        return SignalOutcome::RejectedStale;
                    };
                    let lease = ReadyLease {
                        document: DocumentToken {
                            window_label: window_label.to_string(),
                            owner_agent_id: agent_id.to_string(),
                            document_generation,
                        },
                        policy_revision: policy.revision,
                        runtime_version: LEASE_RUNTIME_VERSION,
                    };
                    state.phase = LeasePhase::Ready;
                    state.ready_lease = Some(lease.clone());
                    state.blocked_reason = None;
                    SignalOutcome::AcceptedReady(lease)
                }
                LeaseSignalKind::Blocked | LeaseSignalKind::Error => {
                    state.phase = LeasePhase::Passive;
                    state.ready_lease = None;
                    state.blocked_reason = Some(reason);
                    SignalOutcome::AcceptedRevocation
                }
                LeaseSignalKind::Revoked => {
                    // Same-document composer loss with a still-eligible route:
                    // back to VERIFYING so fresh evidence can reacquire Ready
                    // within this generation. Never retains the old lease.
                    state.phase = LeasePhase::Verifying;
                    state.ready_lease = None;
                    state.blocked_reason = Some(reason);
                    SignalOutcome::AcceptedRevocation
                }
            }
        };
        self.changed.notify_waiters();
        outcome
    }

    /// Same-document route evaluation (SPA navigation without a full
    /// document load). Generation is unchanged: the JS realm is identical.
    /// Moving to an ineligible/browser-owned route revokes readiness
    /// immediately. Returns true when a lease/phase was revoked.
    pub fn spa_route_changed(&self, window_label: &str, sanitized_url: &str) -> bool {
        let revoked = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let Some(state) = inner.windows.get_mut(window_label) else {
                return false;
            };
            let eligible = match &state.policy {
                Some(policy) => policy.application_eligible(sanitized_url),
                None => false,
            };
            if eligible {
                return false;
            }
            let had_authority = state.phase != LeasePhase::Passive || state.ready_lease.is_some();
            state.phase = LeasePhase::Passive;
            state.ready_lease = None;
            state.blocked_reason = Some(format!("auth_route:{sanitized_url}"));
            state.current_url = Some(sanitized_url.to_string());
            had_authority
        };
        if revoked {
            self.changed.notify_waiters();
        }
        revoked
    }

    /// Same-document composer loss with a still-eligible route: clear the
    /// lease and return to VERIFYING for fresh evidence. Returns true when a
    /// Ready lease was cleared.
    pub fn composer_lost(&self, window_label: &str, reason: &str) -> bool {
        let cleared = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let Some(state) = inner.windows.get_mut(window_label) else {
                return false;
            };
            if state.ready_lease.is_none() {
                return false;
            }
            state.phase = LeasePhase::Verifying;
            state.ready_lease = None;
            state.blocked_reason = Some(bounded_reason(reason));
            true
        };
        if cleared {
            self.changed.notify_waiters();
        }
        cleared
    }

    /// Synchronous authorization: the exact current lease or a precise
    /// failure. Even `wait_ready=false` injection must pass through here.
    pub fn require_ready(
        &self,
        window_label: &str,
        agent_id: &str,
    ) -> Result<ReadyLease, RequireError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(state) = inner.windows.get_mut(window_label) else {
            return Err(RequireError {
                window_label: window_label.to_string(),
                agent_id: agent_id.to_string(),
                phase: LeasePhase::Passive,
                detail: "unknown window".to_string(),
            });
        };
        if state.owner_agent_id.as_deref() != Some(agent_id) {
            return Err(RequireError {
                window_label: window_label.to_string(),
                agent_id: agent_id.to_string(),
                phase: state.phase.clone(),
                detail: "window owned by another agent".to_string(),
            });
        }
        match (&state.phase, &state.ready_lease) {
            (LeasePhase::Ready, Some(lease)) => {
                let lease = lease.clone();
                // Self-healing fail-closed: if the current document route no
                // longer satisfies the exact policy (same-document SPA move
                // the hook has not observed yet), demote immediately instead
                // of authorizing against a stale URL snapshot.
                let still_eligible = match (&state.policy, &state.current_url) {
                    (Some(policy), Some(url)) => policy.application_eligible(url),
                    _ => false,
                };
                if !still_eligible {
                    let url = state.current_url.clone().unwrap_or_default();
                    state.phase = LeasePhase::Passive;
                    state.ready_lease = None;
                    state.blocked_reason = Some(format!("auth_route:{url}"));
                    drop(inner);
                    self.changed.notify_waiters();
                    return Err(RequireError {
                        window_label: window_label.to_string(),
                        agent_id: agent_id.to_string(),
                        phase: LeasePhase::Passive,
                        detail: "current route no longer eligible".to_string(),
                    });
                }
                let current_revision = state.policy.as_ref().map(|p| p.revision);
                if Some(lease.policy_revision) != current_revision
                    || lease.runtime_version != LEASE_RUNTIME_VERSION
                    || lease.document.document_generation != state.document_generation
                {
                    return Err(RequireError {
                        window_label: window_label.to_string(),
                        agent_id: agent_id.to_string(),
                        phase: state.phase.clone(),
                        detail: "lease revision/generation mismatch".to_string(),
                    });
                }
                Ok(lease)
            }
            _ => Err(RequireError {
                window_label: window_label.to_string(),
                agent_id: agent_id.to_string(),
                phase: state.phase.clone(),
                detail: "no current readiness lease".to_string(),
            }),
        }
    }

    /// Exact application origin currently registered for a window, for
    /// stamping into per-turn JS guards.
    pub fn application_origin(&self, window_label: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .windows
            .get(window_label)
            .and_then(|s| s.policy.as_ref())
            .map(|p| p.application_origin.clone())
    }

    /// Revalidate a previously awaited lease immediately before eval. Closes
    /// the async boundary where the page changed after the waiter returned
    /// but before the prompt mutation executes.
    pub fn validate_lease(&self, lease: &ReadyLease) -> Result<(), RequireError> {
        let current =
            self.require_ready(&lease.document.window_label, &lease.document.owner_agent_id);
        match current {
            Ok(current_lease) if current_lease == *lease => Ok(()),
            Ok(_) => Err(RequireError {
                window_label: lease.document.window_label.clone(),
                agent_id: lease.document.owner_agent_id.clone(),
                phase: LeasePhase::Ready,
                detail: "lease superseded before eval".to_string(),
            }),
            Err(e) => Err(e),
        }
    }

    /// Race-safe async wait for the exact current lease. `Notify` is only a
    /// wakeup: every iteration re-reads authoritative state, the notification
    /// is armed before the final state re-check, and no lock is held across
    /// `.await`. A waiter succeeds only with the exact current lease.
    pub async fn wait_until_ready(
        &self,
        window_label: &str,
        agent_id: &str,
        timeout: Duration,
    ) -> Result<ReadyLease, WaitTimeout> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.changed.notified();
            // Final state re-check after arming the wakeup: no lost wakeups.
            match self.require_ready(window_label, agent_id) {
                Ok(lease) => return Ok(lease),
                Err(_) => {}
            }
            let blocked_reason = self.blocked_reason_snapshot(window_label);
            let phase = self.phase_snapshot(window_label);
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Err(WaitTimeout {
                    window_label: window_label.to_string(),
                    agent_id: agent_id.to_string(),
                    last_phase: self.phase_snapshot(window_label),
                    blocked_reason: self.blocked_reason_snapshot(window_label),
                });
            }
            // Spurious wakeup guard: loop re-checks; keep snapshots fresh is
            // unnecessary — next iteration re-reads. Silence unused warnings.
            let _ = (blocked_reason, phase);
        }
    }

    fn phase_snapshot(&self, window_label: &str) -> LeasePhase {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .windows
            .get(window_label)
            .map(|s| s.phase.clone())
            .unwrap_or(LeasePhase::Passive)
    }

    fn blocked_reason_snapshot(&self, window_label: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .windows
            .get(window_label)
            .and_then(|s| s.blocked_reason.clone())
    }

    /// Read-only authority summary for diagnostics mirroring (never the
    /// authorization path itself).
    pub fn describe(&self, window_label: &str) -> (LeasePhase, Option<String>, u64, bool) {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match inner.windows.get(window_label) {
            Some(state) => (
                state.phase.clone(),
                state.owner_agent_id.clone(),
                state.document_generation,
                state.ready_lease.is_some(),
            ),
            None => (LeasePhase::Passive, None, 0, false),
        }
    }
}

impl Default for BrowserLifecycleController {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-turn JS fail-closed guard stamped with the exact expected document
/// authority (owner, generation, application origin). Aborts before composer
/// mutation on owner, generation, origin, route, or readiness mismatch.
/// Submit mechanics below the guard are unchanged.
pub fn injection_lease_guard_js(
    agent_id: &str,
    document_generation: u64,
    application_origin: &str,
) -> String {
    let agent_json = serde_json::to_string(agent_id).unwrap_or_else(|_| "\"\"".to_string());
    let origin_json =
        serde_json::to_string(application_origin).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"var __ca_expectedAgent = {agent_json};
  var __ca_expectedGeneration = {document_generation};
  var __ca_expectedOrigin = {origin_json};
  var __ca_leaseError = '';
  if ((window.__ca_agentId || '') !== __ca_expectedAgent) {{ __ca_leaseError = 'lease_agent_mismatch'; }}
  else if (window.__ca_documentGeneration !== __ca_expectedGeneration) {{ __ca_leaseError = 'lease_generation_mismatch'; }}
  else if (window.__ca_ready !== true) {{ __ca_leaseError = 'lease_not_ready'; }}
  else {{
    var __ca_originOk = false;
    try {{ __ca_originOk = (window.location.origin === __ca_expectedOrigin); }} catch (e) {{}}
    if (!__ca_originOk) {{ __ca_leaseError = 'lease_origin_mismatch'; }}
    else {{
      var __ca_leasePath = '';
      try {{ __ca_leasePath = (window.location.pathname || '').toLowerCase(); }} catch (e) {{}}
      var __ca_leaseVeto = ['/login','/signin','/sign-in','/auth','/oauth','/challenge','/captcha','/verify','/security','/cdn-cgi'];
      for (var __ca_vi = 0; __ca_vi < __ca_leaseVeto.length; __ca_vi++) {{
        if (__ca_leasePath.indexOf(__ca_leaseVeto[__ca_vi]) === 0) {{ __ca_leaseError = 'lease_route_ineligible'; break; }}
      }}
    }}
  }}
  if (__ca_leaseError) {{
    try {{ window.location.href = 'arena://prompt-injection/' + encodeURIComponent(__ca_expectedAgent) + '/none/0/0/0/0/none///' + encodeURIComponent(__ca_leaseError); }} catch (e) {{}}
    try {{ window.location.href = 'arena://active-submit/' + (typeof OPERATION_ID !== 'undefined' && OPERATION_ID ? OPERATION_ID : 'missing') + '/' + encodeURIComponent(__ca_expectedAgent) + '/' + (typeof TURN !== 'undefined' ? TURN : 0) + '/0/' + encodeURIComponent('none') + '/0/' + encodeURIComponent(__ca_leaseError); }} catch (e) {{}}
    return;
  }}"#
    )
}

/// Per-document identity/config script eval'd after a VERIFYING Finished and
/// before the static generic runtime. Carries current document authority
/// (owner, generation, policy snapshot); the static script stays generic.
pub fn document_identity_script(
    token: &DocumentToken,
    policy: &ProviderPolicy,
) -> Result<String, String> {
    let agent_json = serde_json::to_string(&token.owner_agent_id)
        .map_err(|e| format!("agent serialization failed: {e}"))?;
    let origin_json = serde_json::to_string(&policy.application_origin)
        .map_err(|e| format!("origin serialization failed: {e}"))?;
    Ok(format!(
        "window.name = '__consensus_arena_agent__:' + {agent_json}; window.__ca_agentId = {agent_json}; window.__ca_documentGeneration = {}; window.__ca_policyRevision = {}; window.__ca_applicationOrigin = {origin_json}; window.__ca_runtimeVersion = {}; window.__ca_ready = false;",
        token.document_generation, policy.revision, LEASE_RUNTIME_VERSION
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy(agent_id: &str, base_url: &str) -> ProviderPolicy {
        ProviderPolicy::from_base_url(agent_id, base_url, false, 1).expect("test policy must parse")
    }

    fn controller_with_owner(
        window: &str,
        agent: &str,
        base_url: &str,
    ) -> BrowserLifecycleController {
        let lifecycle = BrowserLifecycleController::new();
        let policy = test_policy(agent, base_url);
        lifecycle.assign_owner(window, agent, policy);
        lifecycle
    }

    fn verifying_controller(
        window: &str,
        agent: &str,
        base_url: &str,
        doc_url: &str,
    ) -> BrowserLifecycleController {
        let lifecycle = controller_with_owner(window, agent, base_url);
        lifecycle.page_started(window, doc_url);
        match lifecycle.page_finished(window, doc_url) {
            FinishDecision::Verify { .. } => lifecycle,
            other => panic!("expected VERIFYING, got {other:?}"),
        }
    }

    fn accept_ready(
        lifecycle: &BrowserLifecycleController,
        window: &str,
        agent: &str,
        generation: u64,
    ) -> ReadyLease {
        match lifecycle.apply_signal(
            window,
            agent,
            generation,
            LeaseSignalKind::Ready,
            "composer",
        ) {
            SignalOutcome::AcceptedReady(lease) => lease,
            other => panic!("expected AcceptedReady, got {other:?}"),
        }
    }

    // ── P: policy tests ───────────────────────────────────────────────

    #[test]
    fn p1_exact_builtin_base_origin_eligible() {
        let policy = test_policy("claude", "https://claude.ai");
        assert!(policy.application_eligible("https://claude.ai/new"));
        assert!(policy.application_eligible("https://claude.ai/chat/abc123"));
        let gpt = test_policy("chatgpt", "https://chatgpt.com");
        assert!(gpt.application_eligible("https://chatgpt.com/"));
        let gemini = test_policy("gemini", "https://gemini.google.com");
        assert!(gemini.application_eligible("https://gemini.google.com/app"));
    }

    #[test]
    fn p2_lookalike_host_rejected() {
        let policy = test_policy("claude", "https://claude.ai");
        assert!(!policy.application_eligible("https://claude.ai.evil.example/"));
        assert!(!policy.application_eligible("https://evil-claude.ai/"));
        assert!(!policy.application_eligible("https://notclaude.ai/new"));
    }

    #[test]
    fn p3_arbitrary_subdomain_rejected_without_explicit_alias() {
        let policy = test_policy("claude", "https://claude.ai");
        assert!(!policy.application_eligible("https://sub.claude.ai/new"));
        assert!(!policy.application_eligible("https://www.claude.ai/new"));
        assert!(!policy.application_eligible("https://foo.chatgpt.com/"));
    }

    #[test]
    fn p4_oauth_origin_rejected_as_application_origin() {
        let policy = test_policy("claude", "https://claude.ai");
        assert!(!policy.application_eligible("https://accounts.google.com/o/oauth2/auth"));
        assert!(!policy.application_eligible("https://accounts.google.com/signin"));
    }

    #[test]
    fn p5_challenge_origin_rejected() {
        let policy = test_policy("claude", "https://claude.ai");
        assert!(!policy.application_eligible("https://challenges.cloudflare.com/turnstile"));
        assert!(!policy.application_eligible("https://claude.ai/cdn-cgi/challenge-platform"));
    }

    #[test]
    fn p6_browser_owned_paths_remain_passive() {
        let policy = test_policy("claude", "https://claude.ai");
        for path in [
            "/login",
            "/Login",
            "/signin",
            "/sign-in",
            "/auth/callback",
            "/oauth/authorize",
            "/challenge",
            "/captcha",
            "/verify/email",
            "/security/check",
        ] {
            assert!(
                !policy.application_eligible(&format!("https://claude.ai{path}")),
                "path {path} must stay PASSIVE"
            );
        }
        // Non-prefix lookalikes of the veto list stay eligible candidates.
        assert!(policy.application_eligible("https://claude.ai/new"));
    }

    #[test]
    fn p7_custom_base_exact_origin_eligible() {
        let policy =
            ProviderPolicy::from_base_url("acme", "https://app.acme.example/chat", true, 1)
                .expect("custom policy must parse");
        assert!(policy.is_custom);
        assert!(policy.application_eligible("https://app.acme.example/chat/1"));
        assert!(policy.application_eligible("https://app.acme.example/new"));
    }

    #[test]
    fn p8_custom_foreign_origin_rejected() {
        let policy =
            ProviderPolicy::from_base_url("acme", "https://app.acme.example/chat", true, 1)
                .expect("custom policy must parse");
        assert!(!policy.application_eligible("https://other.example/"));
        assert!(!policy.application_eligible("https://app.acme.example.evil.example/"));
        assert!(!policy.application_eligible("https://sub.app.acme.example/chat"));
        assert!(!policy.application_eligible("https://app.acme.example/login"));
        assert!(!policy.application_eligible("not-a-url"));
        assert!(!policy.application_eligible("about:blank"));
    }

    #[test]
    fn p_malformed_policy_inputs_fail_closed() {
        assert!(ProviderPolicy::from_base_url("x", "not-a-url", false, 1).is_none());
        assert!(ProviderPolicy::from_base_url("x", "about:blank", false, 1).is_none());
        assert!(ProviderPolicy::from_base_url("x", "ftp://files.example/", false, 1).is_none());
        let policy = test_policy("claude", "https://claude.ai");
        assert!(!policy.application_eligible(""));
        assert!(!policy.application_eligible("http://claude.ai/new"));
    }

    // ── G: generation/owner tests ─────────────────────────────────────

    #[test]
    fn g1_assign_owner_revokes_prior_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_ok());
        // Owner switch immediately revokes, before any navigation finishes.
        lifecycle.assign_owner(
            "arena-nav",
            "claude",
            test_policy("claude", "https://claude.ai"),
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
        assert!(lifecycle.validate_lease(&lease).is_err());
        let (phase, owner, _, has_lease) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Passive);
        assert_eq!(owner.as_deref(), Some("claude"));
        assert!(!has_lease);
    }

    #[test]
    fn g2_page_started_increments_generation_and_revokes_ready() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        let token = lifecycle
            .page_started("arena-nav", "https://chat.qwen.ai/c/2")
            .expect("owner exists");
        assert_eq!(token.document_generation, 2);
        assert_eq!(token.owner_agent_id, "qwen");
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        assert!(lifecycle.validate_lease(&lease).is_err());
        // A Start never grants VERIFYING or READY.
        let (phase, _, generation, _) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Passive);
        assert_eq!(generation, 2);
    }

    #[test]
    fn g3_stale_ready_generation_cannot_ready_newer_generation() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/c/2");
        // Stale Ready(1) after navigation to generation 2: rejected.
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        // Fresh evidence path still works after re-verify.
        match lifecycle.page_finished("arena-nav", "https://chat.qwen.ai/c/2") {
            FinishDecision::Verify { .. } => {}
            other => panic!("expected Verify, got {other:?}"),
        }
        accept_ready(&lifecycle, "arena-nav", "qwen", 2);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_ok());
    }

    #[test]
    fn g4_wrong_agent_cannot_ready_current_window() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn g5_wrong_window_cannot_ready_another_window() {
        let leader = controller_with_owner("arena-leader", "claude", "https://claude.ai");
        leader.page_started("arena-leader", "https://claude.ai/new");
        match leader.page_finished("arena-leader", "https://claude.ai/new") {
            FinishDecision::Verify { .. } => {}
            other => panic!("expected Verify, got {other:?}"),
        }
        // Signal addressed to the nav window must not Ready the leader.
        assert_eq!(
            leader.apply_signal("arena-nav", "claude", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        assert!(leader.require_ready("arena-leader", "claude").is_err());
    }

    #[test]
    fn g6_stale_blocked_revoked_cannot_revoke_newer_generation() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/c/2");
        match lifecycle.page_finished("arena-nav", "https://chat.qwen.ai/c/2") {
            FinishDecision::Verify { .. } => {}
            other => panic!("expected Verify, got {other:?}"),
        }
        accept_ready(&lifecycle, "arena-nav", "qwen", 2);
        // Stale revocation for generation 1 cannot clear generation 2's lease.
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "qwen",
                1,
                LeaseSignalKind::Revoked,
                "composer_lost"
            ),
            SignalOutcome::RejectedStale
        );
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "qwen",
                1,
                LeaseSignalKind::Blocked,
                "challenge:x"
            ),
            SignalOutcome::RejectedStale
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_ok());
    }

    #[test]
    fn g7_current_blocked_revoked_removes_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "qwen",
                1,
                LeaseSignalKind::Blocked,
                "challenge:turnstile"
            ),
            SignalOutcome::AcceptedRevocation
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        let (phase, _, _, has_lease) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Passive);
        assert!(!has_lease);
    }

    #[test]
    fn g8_current_ready_accepted_only_from_verifying_with_eligible_policy() {
        // Ready without VERIFYING (fresh owner, no Finished yet): rejected.
        let lifecycle = controller_with_owner("arena-nav", "qwen", "https://chat.qwen.ai");
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 0, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        // Finished on an ineligible auth path never enters VERIFYING.
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/login");
        match lifecycle.page_finished("arena-nav", "https://chat.qwen.ai/login") {
            FinishDecision::Passive { .. } => {}
            other => panic!("expected Passive, got {other:?}"),
        }
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        // Eligible Finished enters VERIFYING but is not itself Ready.
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/c/1");
        match lifecycle.page_finished("arena-nav", "https://chat.qwen.ai/c/1") {
            FinishDecision::Verify { token, .. } => {
                assert_eq!(token.document_generation, 2);
            }
            other => panic!("expected Verify, got {other:?}"),
        }
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        accept_ready(&lifecycle, "arena-nav", "qwen", 2);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_ok());
    }

    #[test]
    fn g_finished_without_owner_is_passive_and_stale_without_state() {
        let lifecycle = BrowserLifecycleController::new();
        match lifecycle.page_finished("arena-nav", "https://claude.ai/new") {
            FinishDecision::Stale => {}
            other => panic!("expected Stale, got {other:?}"),
        }
        assert!(
            lifecycle
                .page_started("arena-nav", "https://claude.ai/new")
                .is_none()
        );
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "claude", 0, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
    }

    #[test]
    fn g_cross_origin_superseded_finished_fails_closed() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        // New navigation toward OAuth started; delayed app Finished is stale.
        lifecycle.page_started("arena-nav", "https://accounts.google.com/o/oauth2/auth");
        match lifecycle.page_finished("arena-nav", "https://claude.ai/new") {
            FinishDecision::Stale => {}
            other => panic!("expected Stale, got {other:?}"),
        }
    }

    #[test]
    fn g_ambiguous_delayed_finished_never_downgrades_verifying() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        // Ambiguous delayed completion for an ineligible path of the same
        // origin must not clobber the current document's VERIFYING.
        match lifecycle.page_finished("arena-nav", "https://claude.ai/login") {
            FinishDecision::Stale => {}
            other => panic!("expected Stale, got {other:?}"),
        }
        let (phase, _, _, _) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Verifying);
    }

    // ── S: SPA / revocation ───────────────────────────────────────────

    #[test]
    fn s1_eligible_app_route_remains_candidate() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        assert!(!lifecycle.spa_route_changed("arena-nav", "https://chat.qwen.ai/c/2"));
        let (phase, _, _, _) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Verifying);
    }

    #[test]
    fn s2_same_document_move_to_login_revokes() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert!(lifecycle.spa_route_changed("arena-nav", "https://chat.qwen.ai/login"));
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        assert!(lifecycle.validate_lease(&lease).is_err());
        let (phase, _, generation, _) = lifecycle.describe("arena-nav");
        // Revocation inside the same document generation (no Started bump).
        assert_eq!(phase, LeasePhase::Passive);
        assert_eq!(generation, 1);
    }

    #[test]
    fn s3_composer_loss_clears_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert!(lifecycle.composer_lost("arena-nav", "composer_lost"));
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        assert!(lifecycle.validate_lease(&lease).is_err());
        let (phase, _, generation, _) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Verifying);
        assert_eq!(generation, 1);
    }

    #[test]
    fn s4_current_generation_reverification_can_reacquire_ready() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "qwen",
                1,
                LeaseSignalKind::Revoked,
                "composer_lost"
            ),
            SignalOutcome::AcceptedRevocation
        );
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
        // Fresh current-generation evidence reacquires without a new load.
        accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_ok());
    }

    #[test]
    fn s5_oauth_popup_event_alone_never_readies_owner_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        // Popup windows never receive an owner, so their signals are stale.
        assert_eq!(
            lifecycle.apply_signal(
                "oauth-popup",
                "claude",
                0,
                LeaseSignalKind::Ready,
                "composer"
            ),
            SignalOutcome::RejectedStale
        );
        // OAuth Finished on the managed window is policy-ineligible.
        lifecycle.page_started("arena-nav", "https://accounts.google.com/o/oauth2/auth");
        match lifecycle.page_finished("arena-nav", "https://accounts.google.com/o/oauth2/auth") {
            FinishDecision::Passive { .. } => {}
            other => panic!("expected Passive, got {other:?}"),
        }
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    // ── W: wait / injection ───────────────────────────────────────────

    #[tokio::test]
    async fn w1_waiter_ignores_stale_generation_ready() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/c/2");
        // Stale Ready(1) must not satisfy a waiter for generation 2.
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        let wait = lifecycle.wait_until_ready("arena-nav", "qwen", Duration::from_millis(50));
        let err = tokio::time::timeout(Duration::from_secs(2), wait)
            .await
            .expect("wait must resolve")
            .expect_err("stale evidence must not satisfy waiter");
        assert_eq!(err.agent_id, "qwen");
    }

    #[tokio::test]
    async fn w2_waiter_ignores_wrong_agent_ready() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        let err = lifecycle
            .wait_until_ready("arena-nav", "claude", Duration::from_millis(50))
            .await
            .expect_err("wrong-agent evidence must not satisfy waiter");
        assert_eq!(err.last_phase, LeasePhase::Verifying);
    }

    #[tokio::test]
    async fn w3_waiter_returns_current_ready_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let handle = tokio::spawn({
            let lifecycle = lifecycle.clone();
            async move {
                lifecycle
                    .wait_until_ready("arena-nav", "claude", Duration::from_secs(5))
                    .await
            }
        });
        tokio::task::yield_now().await;
        let expected = accept_ready(&lifecycle, "arena-nav", "claude", 1);
        let lease = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("waiter must resolve")
            .expect("join ok")
            .expect("waiter must succeed");
        assert_eq!(lease, expected);
    }

    #[test]
    fn w4_require_ready_fails_without_lease() {
        // wait_ready=false injection guard: no lease, no eval.
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let err = lifecycle
            .require_ready("arena-nav", "claude")
            .expect_err("VERIFYING without evidence must fail");
        assert_eq!(err.phase, LeasePhase::Verifying);
    }

    #[test]
    fn w5_stale_lease_between_wait_and_eval_rejected() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "claude", 1);
        assert!(lifecycle.validate_lease(&lease).is_ok());
        // Navigation invalidates the lease before eval: zero prompt mutation.
        lifecycle.page_started("arena-nav", "https://claude.ai/new?fresh=1");
        assert!(lifecycle.validate_lease(&lease).is_err());
    }

    #[test]
    fn w6_current_lease_permits_eval_guard() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "claude", 1);
        // Final native revalidation directly before eval succeeds; the JS
        // guard below carries the same authority without changing submit
        // mechanics.
        assert!(lifecycle.validate_lease(&lease).is_ok());
        let origin = lifecycle
            .application_origin("arena-nav")
            .expect("policy origin registered");
        let guard = injection_lease_guard_js("claude", lease.document.document_generation, &origin);
        assert!(guard.contains("__ca_expectedGeneration = 1"));
        assert!(guard.contains("__ca_expectedOrigin = \"https://claude.ai\""));
        assert!(guard.contains("window.__ca_ready !== true"));
        assert!(guard.contains("lease_generation_mismatch"));
        assert!(guard.contains("lease_agent_mismatch"));
        assert!(guard.contains("lease_not_ready"));
        assert!(guard.contains("lease_origin_mismatch"));
        assert!(guard.contains("lease_route_ineligible"));
    }

    #[test]
    fn w_injection_guard_reports_without_mutating() {
        let guard = injection_lease_guard_js("qwen", 10, "https://chat.qwen.ai");
        // Guard reports through existing diagnostic channels and returns
        // before any composer lookup; it never clicks Send itself.
        assert!(!guard.contains("click"));
        assert!(guard.contains("return;"));
    }

    // ── U-adjacent: controller halves of setup authority ──────────────

    #[test]
    fn u4_matching_retry_reevaluates_without_fake_ready() {
        // Explicit retry navigates: owner reassigned, generation bumped on
        // Started, VERIFYING after eligible Finished — but no lease exists
        // until genuine composer evidence arrives.
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        lifecycle.assign_owner(
            "arena-nav",
            "claude",
            test_policy("claude", "https://claude.ai"),
        );
        lifecycle.page_started("arena-nav", "https://claude.ai/new");
        match lifecycle.page_finished("arena-nav", "https://claude.ai/new") {
            FinishDecision::Verify { .. } => {}
            other => panic!("expected Verify, got {other:?}"),
        }
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn u5_recheck_wakeup_never_manufactures_ready() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        lifecycle.request_recheck();
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn u6_manual_confirm_cannot_replace_lease() {
        // Manual setup confirmation is a user override recorded elsewhere; it
        // never touches the controller, so later active injection still
        // requires a real ReadyLease.
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    // ── R: required race proofs ───────────────────────────────────────

    #[test]
    fn r1_stale_ready_after_owner_switch_rejected() {
        // arena-nav owner qwen gen 1 VERIFYING; switch owner to claude;
        // qwen Ready(gen 1) arrives late: rejected, Claude not Ready.
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        lifecycle.assign_owner(
            "arena-nav",
            "claude",
            test_policy("claude", "https://claude.ai"),
        );
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "qwen", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
        let (phase, owner, _, has_lease) = lifecycle.describe("arena-nav");
        assert_eq!(owner.as_deref(), Some("claude"));
        assert_eq!(phase, LeasePhase::Passive);
        assert!(!has_lease);
    }

    #[test]
    fn r2_navigation_invalidates_lease_before_eval() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "claude", 1);
        lifecycle.page_started("arena-nav", "https://claude.ai/new");
        assert!(lifecycle.validate_lease(&lease).is_err());
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn r3_critical_signal_exact_generation() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        // Current signal mutates; stale one cannot.
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "claude",
                999,
                LeaseSignalKind::Blocked,
                "challenge:x"
            ),
            SignalOutcome::RejectedStale
        );
        let (phase, _, _, has_lease) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Verifying);
        assert!(!has_lease);
        accept_ready(&lifecycle, "arena-nav", "claude", 1);
        assert_eq!(
            lifecycle.apply_signal(
                "arena-nav",
                "claude",
                1,
                LeaseSignalKind::Blocked,
                "challenge:x"
            ),
            SignalOutcome::AcceptedRevocation
        );
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn r5_wrong_explicit_retry_identity_rejected() {
        // Setup expects Claude; a Qwen-scoped lease/wait must not satisfy it.
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        accept_ready(&lifecycle, "arena-nav", "claude", 1);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
    }

    #[test]
    fn r6_custom_provider_exact_origin_verifies_lookalike_rejected() {
        let lifecycle = BrowserLifecycleController::new();
        let custom =
            ProviderPolicy::from_base_url("acme", "https://app.acme.example/chat", true, 1)
                .expect("custom policy must parse");
        lifecycle.assign_owner("arena-nav", "acme", custom);
        lifecycle.page_started("arena-nav", "https://app.acme.example/chat/1");
        match lifecycle.page_finished("arena-nav", "https://app.acme.example/chat/1") {
            FinishDecision::Verify { token, policy } => {
                assert!(policy.is_custom);
                assert_eq!(token.owner_agent_id, "acme");
            }
            other => panic!("expected Verify for custom exact origin, got {other:?}"),
        }
        accept_ready(&lifecycle, "arena-nav", "acme", 1);
        assert!(lifecycle.require_ready("arena-nav", "acme").is_ok());
        // Same-host lookalike can never enter VERIFYING for this owner.
        lifecycle.page_started("arena-nav", "https://app.acme.example.evil.example/");
        match lifecycle.page_finished("arena-nav", "https://app.acme.example.evil.example/") {
            FinishDecision::Passive { .. } | FinishDecision::Stale => {}
            other => panic!("expected Passive/Stale for lookalike, got {other:?}"),
        }
        assert!(lifecycle.require_ready("arena-nav", "acme").is_err());
    }

    #[test]
    fn r4_aux_ready_powerless_without_controller() {
        // The controller has no aux-event input at all: only apply_signal
        // with exact owner+generation mutates lease state. A legacy
        // agent-only Ready has no representation here by construction.
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        // No signal applied: still no lease (compile-time + state proof).
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
    }

    #[test]
    fn stale_same_origin_finished_in_history_fails_closed() {
        // Started(/c/1) → Finished(/c/1) → Started(/login) → delayed
        // Finished(/c/1): the delayed completion matches an older Started
        // entry but not the latest one, so it must not promote or mutate.
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        lifecycle.page_started("arena-nav", "https://chat.qwen.ai/login");
        match lifecycle.page_finished("arena-nav", "https://chat.qwen.ai/c/1") {
            FinishDecision::Stale => {}
            other => panic!("expected Stale for superseded Finished, got {other:?}"),
        }
        let (phase, _, _, has_lease) = lifecycle.describe("arena-nav");
        assert_eq!(phase, LeasePhase::Passive);
        assert!(!has_lease);
        assert!(lifecycle.require_ready("arena-nav", "qwen").is_err());
    }

    #[test]
    fn require_ready_demotes_ineligible_current_route() {
        // Same-document SPA moved to /login without the native hook firing
        // yet: the synchronous check demotes instead of authorizing.
        let lifecycle = verifying_controller(
            "arena-nav",
            "qwen",
            "https://chat.qwen.ai",
            "https://chat.qwen.ai/c/1",
        );
        accept_ready(&lifecycle, "arena-nav", "qwen", 1);
        assert!(lifecycle.spa_route_changed("arena-nav", "https://chat.qwen.ai/login"));
        let err = lifecycle
            .require_ready("arena-nav", "qwen")
            .expect_err("ineligible route must not authorize");
        assert_eq!(err.phase, LeasePhase::Passive);
    }

    #[test]
    fn reset_for_session_revokes_without_resetting_generations() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        accept_ready(&lifecycle, "arena-nav", "claude", 1);
        lifecycle.reset_for_session();
        assert!(lifecycle.require_ready("arena-nav", "claude").is_err());
        // Generations stay monotonic: a stale in-flight signal (gen 1) can
        // never equal a fresh post-reset generation.
        lifecycle.assign_owner(
            "arena-nav",
            "claude",
            test_policy("claude", "https://claude.ai"),
        );
        let token = lifecycle
            .page_started("arena-nav", "https://claude.ai/new")
            .expect("owner exists");
        assert!(token.document_generation > 1);
        assert_eq!(
            lifecycle.apply_signal("arena-nav", "claude", 1, LeaseSignalKind::Ready, "composer"),
            SignalOutcome::RejectedStale
        );
    }

    #[test]
    fn policy_reregistration_revises_lease() {
        let lifecycle = verifying_controller(
            "arena-nav",
            "claude",
            "https://claude.ai",
            "https://claude.ai/new",
        );
        let lease = accept_ready(&lifecycle, "arena-nav", "claude", 1);
        // Owner reassignment (new policy revision) invalidates the old lease
        // even though window/owner/generation textually match.
        lifecycle.assign_owner(
            "arena-nav",
            "claude",
            test_policy("claude", "https://claude.ai"),
        );
        assert!(lifecycle.validate_lease(&lease).is_err());
    }

    #[test]
    fn document_identity_script_carries_authority() {
        let token = DocumentToken {
            window_label: "arena-nav".to_string(),
            owner_agent_id: "claude".to_string(),
            document_generation: 42,
        };
        let policy = ProviderPolicy {
            agent_id: "claude".to_string(),
            application_origin: "https://claude.ai".to_string(),
            is_custom: false,
            revision: 7,
        };
        let script = document_identity_script(&token, &policy).expect("script must build");
        assert!(script.contains("__ca_documentGeneration = 42"));
        assert!(script.contains("__ca_policyRevision = 7"));
        assert!(script.contains("__ca_applicationOrigin = \"https://claude.ai\""));
        assert!(script.contains("__ca_runtimeVersion = 1"));
        assert!(script.contains("__ca_ready = false"));
    }

    #[test]
    fn lease_reasons_are_bounded() {
        let long = "x".repeat(500);
        assert!(bounded_reason(&long).len() <= LEASE_REASON_MAX_CHARS);
        assert_eq!(bounded_reason("ok\ndrop"), "okdrop");
    }
}
