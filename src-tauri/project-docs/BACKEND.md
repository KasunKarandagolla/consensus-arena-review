# Consensus Arena — Backend

## Status: COMPLETE THROUGH PHASE 1 MEMORY

- cargo check: 0 errors
- All 9 original Gemini audit fixes applied and verified, plus the full
  D-035-D-042 batch, plus triage-fix-plan.md Tasks 1-12
- All IPC event payloads match IPC.md exactly
- Settings, blueprint, and (as of Task 7/CRIT-5) transcript databases all
  persist to disk -- transcript was previously in-memory
- Lock-safe injection pattern implemented throughout
- Atomic save_agent_brain_config implemented
- NavEvent all variants handled in wait_for_response
- TranscriptStore/BlueprintStore/SessionVault now use std::sync::Mutex
  (not tokio::sync::Mutex) so their synchronous rusqlite calls run inside
  spawn_blocking via a new db_helpers.rs module -- see Task 9 section below
- Phase 1 Memory is implemented and checkpointed at `f0847c0`. Its post-audit
  passed with no FAIL items; the remaining check is an interactive Tauri
  runtime exercise.

This document previously described an 8-file "Pending Backend Batch" as
not-yet-implemented for an extended period after it had actually shipped --
confirmed stale via direct cargo check + direct file reads. Do not rewrite
any file without reading the current version completely first. Follow
PROCESS.md absolutely -- no exceptions.

---

## Module Map

```
src-tauri/src/
├── main.rs                — App entry, Tauri .setup() closure, AppState registration,
│                            memory_store module registration, tracing init,
│                            app_data_dir resolution, all 38 commands registered
│                            (BUGFIX: pause_session/resume_session existed as
│                            functions but were missing from generate_handler! —
│                            found by an independent Cline audit; fixed)
├── commands.rs            — 38 handlers and registrations as of the Phase 1
│                            Memory post-audit: the prior 26 plus 12 memory commands
├── db_helpers.rs           — NEW MODULE (Task 9). run_blocking(): wraps a
│                            synchronous closure in tokio::task::spawn_blocking
│                            with a 3-attempt retry/backoff (50ms x attempt) for
│                            transient DB failures. Shared by every call site
│                            touching TranscriptStore/BlueprintStore/SessionVault/
│                            MemoryStore
│                            from async code.
├── browser_backend.rs     — WebView management, JS injection, arena:// IPC,
│                            inject_to_window (lock-safe), inject_to_agent (setup phase),
│                            NavEvent enum, BrowserState, GENERIC_INIT_SCRIPT [BUILT]
│                            Kimi contenteditable injection support implemented (D-042)
│                            NOTE: D-040 Tier 2 (console.error override / arena://log
│                            branch) was not directly re-confirmed in the most recent
│                            pass -- verify directly if this becomes relevant to a task,
│                            don't assume either way
├── context_manager.rs     — Full history prompt building, SessionType [BUILT]
├── orchestrator.rs        — AppState (16 fields — see below), OrchestratorStatus,
│                            SessionConfig, ModelHealth [BUILT]
├── session_runner.rs      — run_setup + run_debate (delegates to response_router).
│                            DEF-001 resolved: clones AgentBrain out of the lock
│                            before starting the session loop. session_vault calls
│                            now go through db_helpers::run_blocking (Task 9). [BUILT]
├── agent_brain.rs         — AI brain OpenAI-compatible HTTP API client [BUILT]
│                            RouteCompare + AskUser variants live. Fallback retry
│                            (D-038) implemented, including without_fallback()
│                            builder to explicitly clear a configured fallback.
│                            HTTP timeout (60s, HIGH-4) on both primary and
│                            fallback clients.
├── response_router.rs     — Dynamic agent-driven session loop, lock-safe injection,
│                            RouteCompare arm, AskUser arm, 7 Tier-1 log points,
│                            IMP-2 retry-with-backoff for participant injection,
│                            IMP-5 model_health tracking, IMP-10 brain_fail_count
│                            + automatic switch to agent_brain_2 after 3 consecutive
│                            failures. Blueprint and Phase 1 memory DB calls go
│                            through db_helpers::run_blocking; memory failures
│                            are non-fatal. [BUILT]
├── settings_store.rs      — User settings + prompt templates (SQLite on disk) [BUILT]
│                            Fallback brain fields + FallbackBrainConfig struct +
│                            get/save_fallback_brain_config (Task 5/HIGH-3 — the
│                            storage keys existed before this task but had no
│                            command reaching them). Secondary brain fields +
│                            SecondaryBrainConfig (D-039). NOT converted to
│                            std::sync::Mutex — deliberately out of Task 9's scope,
│                            its reads are tiny single-key lookups on the hot path
│                            of nearly every command; a separate, larger pass.
├── turn_manager.rs        — Agent order tracking [BUILT]
├── blueprint_store.rs     — Section persistence, status tracking, export,
│                            delete_session_sections (Task 3/CRIT-4 cascade) [BUILT]
├── session_vault.rs       — Cookie encryption and storage (SQLite + ring AES-256-GCM),
│                            delete_session_urls (Task 3/CRIT-4 cascade — cookies
│                            table deliberately untouched, see D-046 in DECISIONS.md).
│                            Remains in-memory (SessionVault::new()) — this was
│                            never flagged by any audit and is out of scope. [BUILT]
├── transcript_store.rs    — Session transcript persistence (SQLite), NOW FILE-BACKED
│                            (Task 7/CRIT-5 — was previously in-memory via new()).
│                            get_session, rename_session, delete_session (Task 3
│                            new methods) [BUILT]
├── token_budget.rs        — Per-agent real-time token tracking. reset_all()
│                            (Task 10/HIGH-7 new method, called at session start)
│                            [BUILT — NOTE: record_tokens() is still never called
│                            anywhere in the session loop, so reset_all() is
│                            correct but currently has no visible effect until
│                            token recording itself is wired in — a separate,
│                            unscoped gap, not something Task 10 was asked to fix]
├── resource_monitor.rs    — RAM monitoring, hibernation thresholds [STUB]
├── capability_registry.rs — Per-model capability map [STUB]
├── persona_manager.rs     — Browser fingerprint generation [STUB]
├── agentic_manager.rs     — File operations, user approval workflow [STUB]
├── signals.rs             — Completion signal definitions [STUB]
├── errors.rs              — AgentError enum [BUILT]
├── memory_store.rs        — Implemented Phase 1 SQLite memory store: schema,
│                            bounded context, provenance, reliability, health/
│                            repair, export/restore, and project config [BUILT]
└── proxy_manager.rs       — Optional SSE capture [STUB]
```

The remaining `[STUB]` modules above have never been part of any implementation
batch to date and remain exactly as originally scaffolded -- this is
expected, not a regression.

---

## AppState — 16 Fields (all implemented)

```rust
pub struct AppState {
    pub orchestrator:     Arc<Mutex<Orchestrator>>,
    // Task 9: switched from tokio::sync::Mutex to std::sync::Mutex —
    // see db_helpers.rs. All call sites route through run_blocking().
    pub transcript_store: Arc<std::sync::Mutex<TranscriptStore>>,
    pub token_budget:     Arc<Mutex<TokenBudget>>,
    // Task 9: same rationale as transcript_store. Storage backend itself
    // (in-memory) is UNCHANGED — only the lock type changed.
    pub session_vault:    Arc<std::sync::Mutex<SessionVault>>,
    pub browser_state:    Arc<Mutex<BrowserState>>,
    pub context_manager:  Arc<Mutex<ContextManager>>,
    // Task 9: same rationale as transcript_store.
    pub blueprint_store:  Arc<std::sync::Mutex<BlueprintStore>>,
    // Deliberately NOT converted — see settings_store.rs note above.
    pub settings_store:   Arc<Mutex<SettingsStore>>,
    pub agent_brain:      Arc<Mutex<Option<AgentBrain>>>,
    // D-041
    pub ask_user_tx:      Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    // D-039
    pub agent_brain_2:    Arc<Mutex<Option<AgentBrain>>>,
    // IMP-3: concurrency guard, one session loop at a time
    pub session_active:   Arc<AtomicBool>,
    // IMP-5: per-agent health map for the frontend sidebar dots
    pub model_health:     Arc<Mutex<HashMap<String, ModelHealth>>>,
    // IMP-10: consecutive decide() failure counter, triggers agent_brain_2 switch at 3
    pub brain_fail_count: Arc<AtomicU32>,
    pub memory_store: Arc<std::sync::Mutex<MemoryStore>>,
    pub last_memory_health: MemoryHealth,
}
```

`AppState::new(data_dir: &str)` — takes the app's resolved data directory
directly (from `app.path().app_data_dir()` in main.rs's `.setup()`
closure) and derives `settings.db`, `blueprint.db`, `transcript.db`, and
`memory.db` as independent paths. (Previously `blueprint.db` was derived by
string-replacing `"settings.db"` inside the settings path — silently broke
if that filename ever changed; fixed as part of Task 7/CRIT-5, LOW-B2.)

`agent_brain` is `Option<AgentBrain>` — `None` until user configures via
`save_agent_brain_config`.

---

## Commands — 38 Total, All Implemented AND Registered

The Phase 1 Memory post-audit counted 38 command definitions and 38
`generate_handler!` registrations: the previous 26 plus 12 memory commands.

(Corrected from an earlier version of this doc that said 24 — an
independent Cline audit found that `pause_session` and `resume_session`
were fully implemented as functions in commands.rs but had never been
added to main.rs's `generate_handler!` list, meaning the frontend could
never actually invoke them despite both existing and working. This has
been fixed — see main.rs. If you are auditing this doc again in the
future, always diff commands.rs's function list against
generate_handler!'s registration list directly; don't assume the two are
in sync just because most entries match.)

### Session Management
- start_session(project_brief, session_type, agent_ids, leader_agent_id)
- pause_session() — now registered (previously a registration gap, fixed)
- resume_session() — now registered (previously a registration gap, fixed)
- abort_session()

### User Interaction
- user_input(text)
- captcha_resolved(agent_id)
- rate_limit_decision(agent_id, decision)
- setup_agent_sent(agent_id)
- provide_user_answer(answer) — uses .take() on ask_user_tx to atomically
  clear the Option, preventing double-send (RISK-ASKCHANNEL resolved)

### Settings & Configuration
- save_agent_brain_config(api_key, base_url, model, system_prompt)
  → ATOMIC: construct AgentBrain first → read any existing fallback
  config and attach it → save to DB → update AppState
- get_agent_brain_config() → returns JSON string of AgentBrainConfig
- save_secondary_brain_config(api_key, base_url, model, system_prompt)
- get_secondary_brain_config() → JSON string of SecondaryBrainConfig
- save_fallback_brain_config(api_key, base_url, model) — new (Task 5/HIGH-3).
  Same ATOMIC shape; also keeps a live primary AgentBrain in sync if one is
  already configured, via with_fallback()/without_fallback()
- get_fallback_brain_config() → JSON string of FallbackBrainConfig — new
- save_prompt_template(template_name, content)
- get_prompt_template(template_name) → returns template string (plain
  string, NOT JSON-wrapped)

### Data Retrieval
- get_transcript() → JSON array of TurnRecord
- get_session_list() → JSON array of SessionSummary
- export_blueprint(format, session_id: Option<String>) → file path string.
  session_id now honoured (HIGH-8) — if provided and non-empty, exports
  that specific session; falls back to the active session if omitted, same
  as before this fix. format is validated against 'markdown'|'txt' before
  anything else (CRIT-6) — an unrecognised value now returns Err instead of
  silently falling through to the plaintext branch.
- get_agent_health() → JSON health map (real data from model_health,
  populated by response_router.rs on every Route/RouteCompare cycle —
  returns {} before any session has run)

### Session CRUD (new — Task 3/CRIT-3, CRIT-4)
- delete_session(session_id) → cascades across TranscriptStore (turns +
  session row), BlueprintStore (sections), SessionVault (saved
  conversation URLs — NOT cookies). Refuses to delete the currently-active
  session.
- rename_session(session_id, title) → updates project_brief (the field
  already displayed, truncated, as the session's title). Rejects empty
  title and unknown session_id.
- get_session_details(session_id) → JSON string with turn_count,
  section_count, and the distinct agent_ids that actually participated —
  strictly more than get_session_list's raw SessionSummary row.

### Session Recovery
- get_recovery_state() → JSON: { available: bool, session_id: string }
- recover_session(session_id) → re-emits blueprint-section-added for every
  section of the given incomplete session. Does NOT restart the
  autonomous session loop.

---

## AgentDecision Enum — All 6 Variants Implemented

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentDecision {
    Route { target_model: String, prompt: String },
    Blueprint { section_title: String, section_content: String },
    Continue,
    Complete,
    RouteCompare { models: Vec<String>, prompt: String },   // D-035
    AskUser { question: String, options: Vec<String>, allow_custom: bool }, // D-041
}
```

Note: `rename_all = "snake_case"`, not `"lowercase"` as an earlier version
of this document said — this specifically matters for `RouteCompare` (→
`"route_compare"`) and `AskUser` (→ `"ask_user"`), which need snake_case
word-splitting, not just simple lowercasing of an already-single word.

---

## agent_brain.rs — Key Implementation Details

```rust
#[derive(Clone)]
pub struct AgentBrain {
    api_key: String,
    base_url: String,
    model: String,
    system_prompt: String,
    client: reqwest::Client,
    fallback_api_key: Option<String>,
    fallback_base_url: Option<String>,
    fallback_model: Option<String>,
}
```

`Clone` is required so `session_runner::run_debate` can clone the brain out
of the `agent_brain` lock before starting the session loop (DEF-001 fix) —
`reqwest::Client` is cheaply `Clone` (Arc-backed connection pool), the rest
is `String`/`Option<String>`.

`AgentBrain::new()` returns `Result<Self, AgentError>`.
`reqwest::Client::builder().timeout(Duration::from_secs(60)).build()` error
→ `AgentError::NetworkError`. The 60-second timeout (HIGH-4) applies to
both the primary client and any fallback client constructed on retry.

`with_fallback(api_key, base_url, model)` — builder method, attaches a
fallback config (no `system_prompt` param — the fallback always reuses the
primary's).
`without_fallback()` — explicitly clears a previously-attached fallback
(used when the user saves an empty fallback config, since passing empty
strings to `with_fallback` would otherwise leave the brain thinking a
fallback is configured).

`decide()` flow:
1. Try the primary client/config first.
2. On failure, if a fallback is attached, construct a fresh fallback
   client (also with the 60s timeout) and retry once.
3. On fallback failure too, surface the *original* primary error.
4. POST to `{base_url}/chat/completions` (trailing slash stripped).
5. `Authorization: Bearer {api_key}` header.
6. Check `is_success()` before parsing — error on non-2xx.
7. Strip markdown code fences (` ```json...``` `) from response.
8. Find first `'{'` via `json_start` index — handles prose-prefixed responses.
9. If no `'{'` found → `AgentError::NetworkError` (explicit, not silent).
10. Parse JSON into `AgentDecision` via serde.
11. All errors → `AgentError` variants — no unwrap/expect anywhere.
12. `tracing::debug!` logs both the raw response (truncated 400 chars) and
    the parsed decision (D-040 Tier 1).

---

## settings_store.rs — Key Implementation Details

```rust
pub struct SettingsStore {
    conn: Connection,  // rusqlite::Connection — NOT converted to
                       // std::sync::Mutex (see Module Map note above)
}

pub struct AgentBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
    pub leader_priming_prompt: String,
    pub participant_priming_prompt: String,
}

pub struct SecondaryBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
}

pub struct FallbackBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    // No system_prompt — the fallback always reuses the primary's.
}
```

`SettingsStore::new(db_path: &str)` — accepts real file path, never ":memory:".

SQLite schema:
```sql
CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);
```

All key strings (implemented, none pending):
- brain_api_key, brain_base_url, brain_model, brain_system_prompt
- prompt_leader_priming, prompt_participant_priming
- brain_fallback_api_key, brain_fallback_base_url, brain_fallback_model
- brain2_api_key, brain2_base_url, brain2_model, brain2_system_prompt
- last_session_id, session_complete (used by get_recovery_state/recover_session)

Public methods:
- new(db_path: &str) -> Result<Self, AgentError>
- get(&self, key: &str) -> Result<Option<String>, AgentError>
- set(&mut self, key: &str, value: &str) -> Result<(), AgentError>
- get_agent_brain_config(&self) -> Result<AgentBrainConfig, AgentError>
- save_agent_brain_config(&mut self, config: &AgentBrainConfig) -> Result<(), AgentError>
- get_secondary_brain_config / save_secondary_brain_config
- get_fallback_brain_config / save_fallback_brain_config (new)
- get_fallback_api_key / get_fallback_base_url / get_fallback_model
  (used internally by save_agent_brain_config's ATOMIC read-then-attach step)

Missing keys return `Ok(None)` from `get()`, correctly distinguished from a
real DB error (`Err(AgentError::DatabaseError)`) — every call site does
`.map_err(...)?` before any `.unwrap_or_default()`, so a genuine DB failure
already short-circuits via `?` and never silently reaches a default. (This
was investigated as a possible bug — DEF-003 in DECISIONS.md — and
confirmed to be correct behaviour, not a defect.)
`set()` uses INSERT OR REPLACE with unix timestamp for `updated_at`.

---

## db_helpers.rs — New Module (Task 9/HIGH-5, HIGH-6)

```rust
pub async fn run_blocking<T, F>(op: F) -> Result<T, AgentError>
where
    F: Fn() -> Result<T, AgentError> + Send + Sync + 'static,
    T: Send + 'static,
```

Runs a synchronous closure (typically: lock a `std::sync::Mutex`-wrapped
store, call a rusqlite method, return the result) via
`tokio::task::spawn_blocking`, entirely off the async runtime thread. Up
to 3 total attempts with a 50ms × attempt backoff between them for
transient failures (e.g. SQLITE_BUSY under write contention). A panic
inside the blocking task is NOT retried — surfaces immediately as
`AgentError::DatabaseError`.

Every call site touching `TranscriptStore`, `BlueprintStore`, `SessionVault`,
or `MemoryStore` from async code (`commands.rs`, `response_router.rs`,
`session_runner.rs`) goes through this helper. `settings_store.rs` is
deliberately excluded — see the Module Map note above.

---

## response_router.rs — Key Implementation Details

```rust
pub async fn run_agent_loop(
    config: &SessionConfig,
    brain: &AgentBrain,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError>
```

Loop behaviour:
- Increments iteration counter each cycle
- Emits agent-state-change before wait, after response
- Waits for leader response via wait_for_response()
- IMP-10: checks brain_fail_count; once >= 3, permanently switches to
  agent_brain_2 for the rest of the session (never switches back)
- Builds context string, calls brain.decide() (or agent_brain_2's decide()
  if switched)
- On decide() failure: increments brain_fail_count, emits a boss-message,
  skips this iteration (does not abort the session)
- On decide() success: resets brain_fail_count to 0
- Acts on AgentDecision: Route / Blueprint / Continue / Complete /
  RouteCompare / AskUser (all 6 arms implemented)

RouteCompare arm: routes to each listed model in sequence via
`inject_and_wait_with_retry` (IMP-2: exponential backoff, 3 retries,
rate-limit cooldown tracking), collects all responses, returns combined
`"[X said: ...][Y said: ...]"` to leader.

AskUser arm: creates a oneshot channel, stores the sender in
`ask_user_tx`, emits `agent-ask-user`, awaits the receiver (loop suspended,
no spin — `RISK-ASKCHANNEL` resolved via `.take()` in the
`provide_user_answer` command), injects the answer as context to the
leader on receipt.

Blueprint arm: `blueprint_store.upsert_section()` now runs through
`db_helpers::run_blocking` (Task 9) instead of a direct
`.lock().await` + synchronous call.

`wait_for_response()`:
- `tokio::time::timeout(300s)` — matched as Ok/Err, NEVER uses `?`
- `NavEvent::Response`, `Done`, `Error` matched; `Ready`/`SendDetected` skipped
- Checks BOTH agent_id AND turn number — `RISK-STALERESPONSE`: CLEAR

Lock pattern — all locks in scoped blocks, released before any `.await`.
`RISK-BLOCKING`: CLEAR throughout, including the new `db_helpers` calls,
which lock + call + unlock entirely synchronously inside
`spawn_blocking`, never touching `.await` while a `std::sync::MutexGuard`
is alive.

---

## Memory System — Phase 1 Implemented

The local database is `app_data_dir/memory.db`. It uses WAL and
`user_version=1`, with six normal tables — `session_memory`,
`project_memory`, `global_memory`, `open_questions`, `model_reliability`, and
`pattern_memory` — plus the external-content `project_memory_fts` FTS5 table.
All async DB access locks its `std::sync::Mutex<MemoryStore>` inside
`db_helpers::run_blocking()`.

Memory commands:

- `get_project_memory`, `get_global_memory`, `clear_project_memory`
- `get_open_questions`, `get_model_strengths`
- `save_project_config`, `get_project_config`
- `get_memory_health`, `repair_memory_index`, `get_patterns`
- `export_memory`, `restore_memory`

At session start, `response_router` archives prior session memory and applies
decay, then injects bounded, prioritized memory context into the agent brain.
Route and RouteCompare store routing facts and create model-reliability
adoption checks. Blueprint finalization stores project memory; AskUser confirmed
answers are stored as `user`/`confirmed`; completion stores a session summary.
Memory failures are logged/defaulted locally and do not terminate the session.

Export and restore use SQLite backup support; restore is refused during an
active session and creates a pre-restore backup. Health inspection and FTS
repair are exposed to the frontend. A real interactive Tauri test covering
Route, RouteCompare, Blueprint, AskUser, export, and restore remains.

---

## browser_backend.rs — Key Facts

NavEvent enum — tuple variants:
```rust
pub enum NavEvent {
    Ready(String),
    Error(String),
    Response(String, u32, String),  // agent_id, turn, text
    Done(String, u32),              // agent_id, turn
    SendDetected(String),           // agent_id
}
```

Two injection functions:
1. inject_to_agent(&BrowserState, agent_id, is_leader, prompt, turn, nav_rx)
2. inject_to_window(WebviewWindow, agent_id, prompt, turn, nav_rx, wait_ready)

GENERIC_INIT_SCRIPT (static &str):
- Generic across all agents — never agent-specific
- Runs on every navigation via initialization_script
- Polls for input field (textarea OR contenteditable), signals arena://ready
- Implements send detection via polling
- Runtime input type detection for Kimi's Lexical contenteditable (D-042):
  `el.focus(); document.execCommand('selectAll'); document.execCommand('insertText')`
- D-040 Tier 2 (console.error override / window.onerror → arena://log) was
  not directly re-confirmed present in the most recent read-through of this
  file — verify directly against the real file before assuming either way

on_navigation handles arena://ready, arena://response, arena://done,
arena://sent patterns (and, if D-040 Tier 2 is actually present,
arena://log/{level}/{msg} — see note above).

make_nav_closure rules (NEVER VIOLATE):
- Captures ONLY tx (SyncSender<NavEvent>)
- Uses std::sync::mpsc (NOT tokio)
- Agent identity always from URL path segments

---

## Cargo.toml — Dependencies (all implemented, none pending)

```toml
serde = "1.0.228"
serde_json = "1.0.149"
tauri = { version = "2", features = [] }
tokio = { version = "1.52.3", features = ["full"] }
urlencoding = "2.1.3"
rusqlite = { version = "0.31", features = ["bundled", "backup"] }
tauri-plugin-dialog = "2"
ring = "0.17"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.11", features = ["json"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

`tracing-appender` is also in use (for the daily rolling file-backed log —
D-040 Tier 1) — confirm its exact version is present in the real
Cargo.toml if this becomes relevant to a task; it was referenced in main.rs
but this document's dependency list was not directly re-verified against
the file for that specific crate.

---

## AgentError Variants (errors.rs)

```rust
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
```

Also has an `ErrorKind` classification (`RateLimit` / `Permanent` / other)
used by `response_router.rs`'s `inject_and_wait_with_retry` to decide
whether to trigger a cooldown, retry, or fail immediately — confirm the
exact variant/method names directly in errors.rs before writing code
against this, since it was referenced but not fully re-transcribed here.

---

## Agent Configuration

| agent_id | display_name | base_url                      | Input selector                              | Input type      | Send selector           |
|----------|--------------|-------------------------------|---------------------------------------------|-----------------|-------------------------|
| chatgpt  | ChatGPT      | https://chatgpt.com           | (existing)                                  | textarea        | (existing)              |
| claude   | Claude       | https://claude.ai             | (existing)                                  | contenteditable | (existing)              |
| gemini   | Gemini       | https://gemini.google.com     | (existing)                                  | textarea        | (existing)              |
| deepseek | DeepSeek     | https://chat.deepseek.com     | (existing)                                  | textarea        | (existing)              |
| qwen     | Qwen         | https://chat.qwen.ai          | (existing)                                  | textarea        | (existing)              |
| glm      | GLM          | https://chat.z.ai/            | #chat-input                                 | textarea        | #send-message-button    |
| kimi     | Kimi         | https://www.kimi.com/         | div.chat-input-editor[contenteditable=true] | Lexical/CE      | div.send-button-container |

GLM notes: Svelte framework — ignore .svelte-* hash classes. ID selectors are stable.
Kimi notes: Vue.js — ignore data-v-* scoped attributes. Class selectors only.
           Conversation URL: https://www.kimi.com/chat/{uuid}
           Input is Lexical editor — requires execCommand injection path.

---

## Deferred Items — Resolved or Confirmed Non-Issues

DEF-001: run_debate holds agent_brain lock for entire session — RESOLVED.
         session_runner.rs now clones the brain out of the lock before
         starting the session loop.

DEF-002: AgentBrainConfig built with empty priming prompts — NOT A REAL BUG.
         save_agent_brain_config never constructs an AgentBrainConfig with
         priming-prompt fields at all; it writes 4 specific keys directly
         and never touches the priming-prompt keys, which are independently
         managed by save_prompt_template. Confirmed via direct code read.

DEF-003: unwrap_or_default on settings DB reads — NOT A REAL BUG.
         Every call site does .map_err(...)? before .unwrap_or_default(),
         so a real DB error already short-circuits. Confirmed via direct
         code read.

---

## Task 3/5/9/10 — Triage Fix Batch Details

See DECISIONS.md's "Decisions Made — Batch A/B/C" section (D-046 through
D-054) for the full narrative of what these tasks were, why some triage
findings turned out to be non-issues (DEF-002, DEF-003 above), and what
was scoped differently than the original triage document guessed (e.g.
Task 9 never touched browser_backend.rs, because it has no direct
database calls — the triage's file list for that task was incorrect).

---

## What Comes Next

First run the remaining interactive Phase 1 memory test. Then begin Phase 2
Skills planning and implementation. For any new backend feature:
1. Read every file the change touches completely first (this doc's Module
   Map is a starting point, not a substitute for reading the real file).
2. Confirm cargo check is clean before making changes, so any new errors
   are attributable to the new change, not pre-existing drift.
3. Deliver complete replacement files, never partial diffs (see PROCESS.md).
4. Any new command returning a struct/collection must use
   `serde_json::to_string()`, and the corresponding frontend caller must
   `JSON.parse()` it — see FRONTEND.md's IPC Wiring section and
   DECISIONS.md's Non-Negotiable Constraints for why this specific pattern
   has bitten this project four separate times already.
