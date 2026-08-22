use crate::blueprint_store::BlueprintStore;
use crate::browser_backend::BrowserState;
use crate::context_manager::ContextManager;
use crate::agent_brain::AgentBrain;
use crate::session_vault::SessionVault;
use crate::token_budget::TokenBudget;
use crate::transcript_store::TranscriptStore;
use crate::settings_store::SettingsStore;
use crate::memory_store::{MemoryHealth, MemoryStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::path::PathBuf;
use tokio::sync::Mutex;

pub use crate::context_manager::SessionType;

// ── Orchestrator status ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum OrchestratorStatus {
    Idle,
    Preparing,
    Setup,
    Requirements,
    Running,
    Paused,
    Complete,
    Ended,
}

// ── Session config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id: String,
    pub project_brief: String,
    pub session_type: SessionType,
    pub agent_ids: Vec<String>,
    pub leader_agent_id: String,
}

impl SessionConfig {
    pub fn setup_order(&self) -> Vec<String> {
        let mut order = Vec::with_capacity(self.agent_ids.len());
        if self.agent_ids.iter().any(|id| id == &self.leader_agent_id) {
            order.push(self.leader_agent_id.clone());
        }
        for agent_id in &self.agent_ids {
            if agent_id != &self.leader_agent_id {
                order.push(agent_id.clone());
            }
        }
        order
    }
}

// ── Model health (IMP-5) ──────────────────────────────────────────────────────

/// Per-agent health record updated on every Route / RouteCompare cycle.
/// Serialised and returned by get_agent_health for the frontend sidebar dots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHealth {
    pub agent_id: String,
    pub is_available: bool,
    pub error_count: u32,
    pub last_error: Option<String>,
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

pub struct Orchestrator {
    pub status: OrchestratorStatus,
    pub current_session: Option<SessionConfig>,
    pub current_iteration: u32,
    pub rate_limit_decisions: HashMap<String, String>,
}

impl Orchestrator {
    pub fn new() -> Self {
        Orchestrator {
            status: OrchestratorStatus::Idle,
            current_session: None,
            current_iteration: 0,
            rate_limit_decisions: HashMap::new(),
        }
    }
}

// ── AppState ──────────────────────────────────────────────────────────────────

pub struct AppState {
    pub orchestrator:     Arc<Mutex<Orchestrator>>,
    /// Task 9 (HIGH-5/HIGH-6): switched from `tokio::sync::Mutex` to
    /// `std::sync::Mutex`. All call sites now go through
    /// `db_helpers::run_blocking`, which locks + calls + unlocks entirely
    /// inside `tokio::task::spawn_blocking`, with retry/backoff on failure.
    /// A `tokio::sync::Mutex` invites locking it directly with `.lock().await`
    /// and then calling the synchronous `rusqlite` method right there on the
    /// async runtime thread — exactly the bug being fixed. `std::sync::Mutex`
    /// makes that mistake a compile error instead (no `.await` on its lock).
    pub transcript_store: Arc<std::sync::Mutex<TranscriptStore>>,
    pub token_budget:     Arc<Mutex<TokenBudget>>,
    /// Task 9: same rationale as transcript_store.
    pub session_vault:    Arc<std::sync::Mutex<SessionVault>>,
    pub browser_state:    Arc<Mutex<BrowserState>>,
    pub context_manager:  Arc<Mutex<ContextManager>>,
    /// Task 9: same rationale as transcript_store.
    pub blueprint_store:  Arc<std::sync::Mutex<BlueprintStore>>,
    /// Deliberately NOT part of the Task 9 conversion — out of that task's
    /// scope (see triage-fix-plan.md TASK 9 file list: blueprint_store.rs,
    /// transcript_store.rs, session_vault.rs only). settings_store.rs reads
    /// are tiny single-key lookups on the hot path of nearly every command;
    /// converting it is a separate, larger pass left for later.
    pub settings_store:   Arc<Mutex<SettingsStore>>,
    pub agent_brain:      Arc<Mutex<Option<AgentBrain>>>,
    /// D-041: oneshot sender through which provide_user_answer delivers the
    /// user's answer to the suspended run_agent_loop.  Set when AskUser fires,
    /// cleared via take() after use.  RISK-ASKCHANNEL: resolved.
    pub ask_user_tx:      Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    /// D-039: optional secondary (alternative) orchestration brain.
    pub agent_brain_2:    Arc<Mutex<Option<AgentBrain>>>,
    /// IMP-3: concurrency guard — only one session loop may run at a time.
    /// compare_exchange(false → true) in start_session; store(false) in every
    /// exit path of the spawned task and in abort_session.
    pub session_active:   Arc<AtomicBool>,
    /// IMP-5: per-agent health map updated on every Route/RouteCompare cycle.
    pub model_health:     Arc<Mutex<HashMap<String, ModelHealth>>>,
    /// IMP-10: consecutive decide() failure counter.  Increments when the full
    /// decide() call (primary + D-038 fallback) fails.  Resets on success.
    /// Once >= 3, run_agent_loop switches permanently to agent_brain_2.
    pub brain_fail_count: Arc<AtomicU32>,
    pub memory_store: Arc<std::sync::Mutex<MemoryStore>>,
    pub last_memory_health: MemoryHealth,
    pub setup_generation: Arc<AtomicU32>,
}

impl AppState {
    /// CRIT-5 / LOW-B2: `data_dir` is the app's resolved data directory
    /// (e.g. from `app.path().app_data_dir()`). All three SQLite databases —
    /// settings.db, blueprint.db, transcript.db — are derived directly from
    /// this directory, each independently, rather than by string-replacing
    /// "settings.db" inside another path (the old LOW-B2 bug).
    ///
    /// CRIT-5: TranscriptStore now uses the file-backed `open()` constructor
    /// instead of the in-memory `new()` — transcripts persist across restarts.
    ///
    /// Task 9: transcript_store, blueprint_store, and session_vault are
    /// wrapped in `std::sync::Mutex` (see field docs above) instead of
    /// `tokio::sync::Mutex`.
    pub fn new(data_dir: &str) -> Self {
        let settings_db_path   = format!("{}/settings.db", data_dir);
        let blueprint_db_path  = format!("{}/blueprint.db", data_dir);
        let transcript_db_path = format!("{}/transcript.db", data_dir);
        let memory_db_path = PathBuf::from(data_dir).join("memory.db");

        let memory_store = MemoryStore::new(memory_db_path.to_string_lossy().as_ref())
            .unwrap_or_else(|e| {
                eprintln!("[MEMORY] Init failed: {e}. Using in-memory fallback.");
                MemoryStore::new_empty()
            });
        let last_memory_health = memory_store.check_health();

        let (nav_tx, _nav_rx) =
            std::sync::mpsc::sync_channel::<crate::browser_backend::NavEvent>(256);

        AppState {
            orchestrator: Arc::new(Mutex::new(Orchestrator::new())),
            transcript_store: Arc::new(std::sync::Mutex::new(
                TranscriptStore::open(&transcript_db_path)
                    .expect("transcript store init failed"),
            )),
            token_budget:     Arc::new(Mutex::new(TokenBudget::new())),
            // NOTE: SessionVault::new() is in-memory, exactly as before this
            // batch. Task 9 only changes the *lock type* (tokio::Mutex →
            // std::sync::Mutex) so its rusqlite calls can run inside
            // spawn_blocking — it does not change SessionVault's storage
            // backend. SessionVault's own in-memory-vs-file-backed status
            // was never flagged in the triage audit and is out of scope for
            // this batch; flagging it separately rather than silently
            // "fixing" an unrequested behaviour change.
            session_vault: Arc::new(std::sync::Mutex::new(SessionVault::new())),
            browser_state:    Arc::new(Mutex::new(BrowserState::new(nav_tx))),
            context_manager:  Arc::new(Mutex::new(ContextManager::new(
                String::new(),
                SessionType::Custom,
            ))),
            blueprint_store: Arc::new(std::sync::Mutex::new(
                BlueprintStore::new(&blueprint_db_path)
                    .expect("blueprint store init failed"),
            )),
            settings_store: Arc::new(Mutex::new(
                SettingsStore::new(&settings_db_path)
                    .expect("settings store init failed"),
            )),
            agent_brain:      Arc::new(Mutex::new(None)),
            ask_user_tx:      Arc::new(Mutex::new(None)),
            agent_brain_2:    Arc::new(Mutex::new(None)),
            session_active:   Arc::new(AtomicBool::new(false)),
            model_health:     Arc::new(Mutex::new(HashMap::new())),
            brain_fail_count: Arc::new(AtomicU32::new(0)),
            memory_store: Arc::new(std::sync::Mutex::new(memory_store)),
            last_memory_health,
            setup_generation: Arc::new(AtomicU32::new(0)),
        }
    }
}
