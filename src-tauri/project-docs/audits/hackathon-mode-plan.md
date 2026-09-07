# Hackathon Mode — Internal Technical Implementation Plan

**Date:** 2026-09-05
**Basis:** HACKATHON_MODE_DESIGN.md + pre-audit (hackathon-mode-pre.md)
**Principle:** Additive, minimal, reversible. Reuse existing patterns; no new dependency.

---

## 1. Architecture Overview

```
Persisted (settings.db)                 Transient (AppState.hackathon)
─────────────────────                   ──────────────────────────────
HackathonConfig                         HackathonRunState
 ├─ groups: GroupConfig[]                ├─ run_id: String (Uuid)
 ├─ models: ModelConfig[]                ├─ task_brief: String
 └─ max_questions: Option<u32>           ├─ groups: GroupRunState[]
                                         │    ├─ group_id, name
                                         │    ├─ participants: ParticipantRun[]
                                         │    │    ├─ model_id, display, base_url (no key)
                                         │    │    ├─ status: Pending|Confirmed|Failed
                                         │    │    └─ consultation_count: u32
                                         │    ├─ leader_id: Option<String>
                                         │    ├─ history: Vec<ChatMessage>
                                         │    ├─ status: Pending|Running|Completed|Failed|Locked
                                         │    └─ final_output: Option<String>
                                         └─ cancellation: Arc<AtomicBool>

Wiring:
  SetupView toggle → opens HackathonMiniWindow (Tauri UI window, not AI WebView)
  Mini-window CRUDs config via IPC → settings_store
  Send Invitations → fan-out health checks (concurrent)
  Go → closes mini-window, stores selected groups for session start
  Session start → orchestrator reads hackathon config + emits report-up after group execution
  Leader loop → receives combined group report via inject
```

---

## 2. Data Model (Loop A)

### 2.1 Persisted Types (settings_store.rs)

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonModelConfig {
    pub id: String,                 // Uuid
    pub model_name: String,         // e.g. "llama-3.3-70b"
    pub base_url: String,           // e.g. https://integrate.api.nvidia.com/v1
    pub api_key: String,            // persisted — NEVER emitted to frontend in plaintext
    pub group_id: String,           // belongs to exactly one group
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonGroupConfig {
    pub id: String,                 // Uuid
    pub name: String,               // e.g. "Falcon"
    pub model_ids: Vec<String>,     // ordered — position defines leadership/fallback
    pub selected: bool,             // checkbox: included in next run?
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonConfig {
    pub groups: Vec<HackathonGroupConfig>,
    pub models: Vec<HackathonModelConfig>,
    pub max_questions_per_teammate: Option<u32>, // None = Unlimited, Some(n) = cap
}

impl Default for HackathonConfig { ... } // empty groups/models, cap = Some(3)
```

Persistence via settings.db key `hackathon_config` as JSON string — same pattern as custom_participants.

**Safe view** sent to frontend: HackathonModelSafe { id, model_name, base_url_redacted? or full base_url, group_id } but API key never included. Frontend receives base_url (needed for display host), not key. The api_key field is omitted from the safe serialization via custom struct.

Provide two serializers:
- StoredHackathonConfig (full, backend-only)
- FrontendHackathonConfig (keys omitted, base_url kept)

### 2.2 Transient Run Types (hackathon.rs)

```rust
pub enum ParticipantRunStatus { Pending, Confirmed, Failed(String) }
pub enum GroupRunStatus { Pending, Running, Completed, Failed, Locked }

pub struct ParticipantRunState {
    pub model_id: String,
    pub model_name: String,
    pub group_id: String,
    pub base_url: String, // needed for API but not key
    pub status: ParticipantRunStatus,
    pub consultation_count: u32,
}

pub struct GroupRunState {
    pub group_id: String,
    pub group_name: String,
    pub model_ids_ordered: Vec<String>, // saved ordering
    pub participants: Vec<ParticipantRunState>,
    pub leader_id: Option<String>, // current acting leader (first live)
    pub history: Vec<HackathonMessage>, // {role, content}
    pub status: GroupRunStatus,
    pub final_output: Option<String>,
}

pub struct HackathonRunState {
    pub run_id: String,
    pub task_brief: String,
    pub max_questions: Option<u32>,
    pub groups: Vec<GroupRunState>,
    pub cancelled: Arc<AtomicBool>,
}
```

Placed in new `src-tauri/src/hackathon.rs`.

AppState addition:
```rust
pub hackathon_run: Arc<Mutex<Option<HackathonRunState>>>,
pub hackathon_config: Arc<Mutex<HackathonConfig>> // cached from settings? or read-through
```
Actually config persistence via SettingsStore is enough; cache via Arc<Mutex> for quick access without DB read per command. Load at startup.

Simpler: store only `hackathon_run: Arc<Mutex<Option<HackathonRunState>>>` and `hackathon_cancel: Arc<AtomicBool>`? But config still via DB reads. Use AppState field `hackathon_state: Arc<Mutex<HackathonState>>` containing Option<RunState> + active_run_id.

Proposed AppState addition:
```rust
pub hackathon_run: Arc<Mutex<Option<HackathonRunState>>>,
pub hackathon_run_id: Arc<Mutex<Option<String>>>,
```

Keep config via SettingsStore; no duplicate cache needed eagerly, but add helper to load/save.

---

## 3. Persistence / Security (Loop B)

- Key: `"hackathon_config"` in settings table.
- Methods in `settings_store.rs`:
  - `get_hackathon_config() -> Result<HackathonConfig, AgentError>` (parse JSON or default)
  - `save_hackathon_config(&HackathonConfig) -> Result<(), AgentError>`
- API keys stored plaintext (matching brain precedent). Documented as `NEEDS DESIGN` deferred encryption — but still ensure:
  - Never include api_key in any `app.emit` payload.
  - Never log api_key (use redact helpers).
  - Frontend-safe DTO strips api_key: `HackathonSafeModel { id, model_name, base_url, group_id }`
  - Diagnostics snapshot must not include hackathon keys (like brain keys excluded).
- Redaction: reuse `redact_diagnostic_text` / new helper.

---

## 4. API / Invitation Engine (Loop C)

### 4.1 HTTP Abstraction

Reuse reqwest::Client with timeout 15s for invitations (shorter than brain 60s — health check is lightweight).

```rust
async fn call_hackathon_model(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model_name: &str,
    messages: &[ChatMessage],
) -> Result<String, AgentError>
```

Messages: `[{role: system, content: "You are ..."}, {role: user, content: prompt }]`

Invitation prompt: lightweight — e.g. `"Consensus Arena invitation health check. Reply with exactly: OK"`. Timeout 15s.

Classify errors same as AgentBrain: http_status_category, network_error_category.

### 4.2 Health-Check Fan-Out

```rust
pub async fn run_invitations(
    run_id: String,
    groups: Vec<GroupRunState>, // with participants populated
    app: AppHandle,
    state: &AppState,
)
```

- For each selected group, for each model: spawn tokio task via JoinSet.
- Each task: call_hackathon_model with invitation prompt; on success mark Confirmed, on failure Failed.
- After each task completes, emit `hackathon-invitation-update` event with `{run_id, group_id, model_id, status}` and also update in-memory AppState.
- After all: compute per-group responsive count; if zero responders → mark group Locked (checkbox inactive) and emit `hackathon-group-locked`.
- Responders float top while preserving order: helper `sort_by_responder_status`.

Concurrency: all model tasks run simultaneously via JoinSet (not sequential). Memory: each task holds only small JSON payloads — well under 2GB even with 50 models × ~1KB.

No mutex held across await: collect participant data clone before spawning.

### 4.3 Validation

- Validate model: base_url valid URL, model_name non-empty, api_key non-empty (at save time).
- Validate group: name non-empty, at least 1 model (allow single-member groups; zero-member groups treated as Locked).
- Validate max_questions: Some(1|2|3|5) or None (Unlimited). Mockup shows 1,3,5,Unlimited — implement 1,2,3,5,Unlimited to cover design candidate list, default 3.

---

## 5. Group Orchestration (Loop D)

### 5.1 Decision Contract

Isolated enum (not AgentDecision):

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HackathonDecision {
    Route { target_model: String, prompt: String },
    Submit { final_output: String },
}
```

System prompt for group leader:
```
You are the leader of hackathon group '{group_name}'. Your team members are: {member list}.
You can either route a question to a teammate or submit the group's final output.
Respond with exactly one JSON object:
{"action":"route","target_model":"<teammate_id>","prompt":"<question for teammate>"}
or
{"action":"submit","final_output":"<synthesized group output>"}
Rules: only route to non-leader teammates, respect team composition, produce final_output when ready.
```

Prompt includes history: concat HackathonMessage {role: user/assistant} where user=leader decisions/prompts, assistant=teammate responses.

Robust parsing: reuse `extract_json_object` tolerant extractor + serde; on invalid → treat as error, retry once? Spec says invalid decision must be error, not panic, trigger controlled retry/failure path. Implement: attempt 2 tries with error injection (“Your previous response was invalid JSON; retry”), then if still invalid → treat as leader failure → fallback.

### 5.2 Group Loop (per group)

```
fn select_leader(group: &GroupRunState, live: &HashSet<String>) -> Option<String>
  // first model_id in ordered list where participant status != Failed

async fn run_group(
    run_id: String,
    group: GroupRunState,
    max_questions: Option<u32>,
    app: AppHandle,
    state: &AppState,
) -> GroupRunState
```

Steps:
1. Init history = [user: task_brief]
2. Loop up to SAFETY_CAP (20) iterations:
   a. Select leader = first live in order; if None → group Failed, break.
   b. Call leader model with history → parse HackathonDecision.
      - On network/timeout failure → mark leader Failed, emit hackathon-group-leader-failed, continue (select next leader, preserve history).
   c. If Route:
      - Validate target is in group, not leader, live, and consultation_count < cap.
      - If invalid → log, emit hackathon-invalid-route, treat as leader error? Options: retry leader next iteration with correction prompt, or mark leader Failed after 2 invalid routes. Choose: inject correction message to leader history and loop (bounded retries).
      - Else increment consultation_count, append route prompt to history, call teammate model with full history, append teammate response to history, continue.
   d. If Submit → set final_output, status Completed, break.
   e. Else invalid decision → retry logic per 5.1.
3. If SAFETY_CAP reached → force Submit with last history synthesis? Or group Failed with partial? Choose: emit warning, finalize with current history concatenated as output and mark Completed (with truncation notice) — better than losing work. Document cap.

### 5.3 Concurrency

Groups run concurrently via `tokio::task::JoinSet` — one task per group. Each task owns its GroupRunState clone (no shared history). Use Arc<AtomicBool> cancellation token checked at each loop iteration.

No mutex held across await: clone group data, drop lock, run, then reacquire to write back final GroupRunState.

Failure isolation: one group's panic/failure does not cancel others (unless global run_id cancellation).

### 5.4 Pure Helpers (testable)

- `select_leader(ordered_ids, live_set) -> Option<String>`
- `fallback_leader(ordered_ids, failed_leader_id, live_set) -> Option<String>`
- `is_route_allowed(target, leader_id, consultation_counts, cap) -> bool`
- `sort_responders(models, statuses)` — float responders preserving order
- `format_report(groups: &[GroupRunState]) -> String` — report-up formatting
- `validate_hackathon_config(config) -> Result<(), String>`

All pure, unit-testable without HTTP.

### 5.5 Safety Caps

- Per-teammate cap: config value (None = unlimited via Option).
- Emergency group-round cap: const HACKATHON_SAFETY_MAX_ROUNDS: u32 = 20 (internal, not UI). Documented in audit and code comment.
- When reached: group final_output = history assembled + "[truncated: safety cap reached]" prefix.

---

## 6. Main Leader Integration (Loop E)

### 6.1 Report-Up Formatting

Analogous to RouteCompare: combined string with delimited sections.

```
[HACKATHON Group: Falcon — leader llama-3.3-70b]
<final_output or failure note>

[HACKATHON Group: Orbit — leader deepseek-v3]
...

[HACKATHON Group: Vega — no output (all members unavailable)]
```

Prepend header: `=== Hackathon Results (run_id) ===` + brief reference.

### 6.2 Injection Point

After groups complete (JoinSet done), if at least one group completed, inject combined report to main leader via same `inject_active_prompt` path used by Route returns — OR, if no active leader window (pre-session), store for next session start and emit `hackathon-results-ready` event for frontend to display.

Minimal integration chosen: Add command `run_hackathon(task_brief)` that orchestrates invitation-then-execution using current HackathonConfig's selected groups, returns combined report as JSON string. Separately, `get_hackathon_report` etc. For pre-enabled flow, `start_session` will optionally trigger hackathon and prepend report to context_manager.history before leader loop starts? Simpler: `start_session` checks if hackathon had run and injects report as first turn history entry.

For Loop E: implement `run_hackathon` command that does full pipeline and returns formatted report (JSON string), plus event `hackathon-complete` with `{run_id, report, group_outputs}`. Document that main leader authority unchanged — report is raw material, leader decides.

Mid-session trigger (deferred): stub `trigger_hackathon_mid_session(task_brief)` that requires session_active true, otherwise error. Document as minimal.

---

## 7. IPC (Loop F)

Commands (all JSON-string where returning structs):

```
get_hackathon_config                -> Promise<string> JSON HackathonSafeConfig
save_hackathon_config {config}      -> Promise<void>  (takes full config JSON, strips keys server-side validation)
get_hackathon_status                -> Promise<string> JSON HackathonRunStatus (active run)
create_hackathon_group {name}       -> Promise<string> JSON GroupSafe
delete_hackathon_group {group_id}   -> Promise<void>
rename_hackathon_group {group_id, name} -> Promise<void>
reorder_hackathon_model {group_id, model_id, direction: "up"|"down"} -> Promise<string> JSON updated group
set_hackathon_selected {group_id, selected} -> Promise<void>
set_hackathon_max_questions {max_questions: number|null} -> Promise<void>  // null = unlimited
// Model CRUD
add_hackathon_model {model_name, base_url, api_key, group_id} -> Promise<string> JSON model safe
update_hackathon_model {id, model_name, base_url, api_key, group_id} -> Promise<void>
delete_hackathon_model {id}         -> Promise<void>
// Invitation / run
send_hackathon_invitations          -> Promise<string> JSON {run_id}  // starts fan-out, emits live updates
run_hackathon {task_brief}          -> Promise<string> JSON {run_id, report} // full execution
cancel_hackathon_run                -> Promise<void>
get_hackathon_report {run_id?}      -> Promise<string> JSON report
```

Simplified proposal: Collapse to fewer commands for Loop F to reduce surface:
- `get_hackathon_config`
- `save_hackathon_config`
- `send_hackathon_invitations`
- `run_hackathon`
- `cancel_hackathon`
- `get_hackathon_run_state`

But to match mockup's fine-grained UI (up/down arrows, per-group checkbox, Add model/team), need at least save/load plus invitation. Choose minimal 5 commands initially:

- `get_hackathon_config` -> JSON safe config
- `save_hackathon_config` {groups, models, max_questions_per_teammate}
- `send_hackathon_invitations` -> returns run_id string, emits events
- `run_hackathon` {task_brief} -> runs groups concurrently, returns report
- `cancel_hackathon_run` -> cancels

All new commands use `#[tauri::command(rename_all = "snake_case")]` where multiword args, and return `Result<String,String>` via serde_json::to_string for JSON payloads.

Events:

```
hackathon-invitation-update  {run_id, group_id, model_id, status, error?}
hackathon-group-status       {run_id, group_id, status}
hackathon-run-started        {run_id, task_brief, group_ids[]}
hackathon-group-output       {run_id, group_id, output}
hackathon-complete           {run_id, report}
hackathon-error              {run_id, message}
```

All events use exact names matching IPC.md entries.

Frontend listeners: extend useIpcListeners.ts or new useHackathonListeners.ts.

---

## 8. Mini-Window Frontend (Loop G)

Source: `src-tauri/project-docs/mockup/hackathon-mini-window.html` (656 lines)

Structure mapping:
- Outer modal `.hk-modal` with header (icon, title subtitle, Close X)
- Toolbar (Add model, New team, count chip)
- Body columns (one column per group)
  - Col header: checkbox (hk-check), name, status chip (live/pending/dead), count
  - Col body: rows (rank badge, name+meta, state icon: check/spinner/x-circle, hover actions: up/down arrows, more menu)
  - Col note (info / warn)
- Rounds control: .hk-rounds with select (1,3,5,Unlimited) — mockup value set = 3 selected
- Footer: left status text, actions Cancel / Send invitations / Go
- Popups: Add model (model_name, base_url, api_key, Team select), New team (Team name)

Reuse:
- Theme variables already in index.css — no new theme needed.
- Inter / JetBrains Mono already loaded locally.
- Icon system: lucide-react (already in deps? check package.json). Verify before using.
- Visual language: same shadows, borders, radii.

Implementation plan:
- Single React component `src/components/hackathon/HackathonMiniWindow.tsx` (modal overlay, not a Tauri second window — simpler lifecycle, no extra WebView, no Tauri window config changes). However spec mentions "Tauri UI window" — modal overlay is safest additive (no new Tauri window creation). If true Tauri window needed later, can evolve.
- But spec says mini-window is UI window, distinguish from model WebViews. Modal satisfies without adding WebView. Document decision: modal overlay over SetupView, not separate Tauri window, to minimize WebView risk and complexity.
- Alternative: if spec expects real Tauri window, we can add tauri.conf second window — but that adds WebView count ambiguity. Choose modal overlay and document as conservative.

State:
- Extend useAppStore with `hackathon: HackathonSlice`:
  - config: HackathonSafeConfig|null
  - runState: HackathonRunState|null
  - invitationPending: bool
  - etc.

Or dedicated `src/stores/useHackathonStore.ts` to avoid bloating main store — but spec says reuse Zustand architecture, either is fine. Prefer extending useAppStore for single source.

Styling: copy relevant CSS from mockup into index.css under /* hackathon */ section, namespaced .hk-* already, reuse vars.

Interactions:
- Fetch config on mount via get_hackathon_config (JSON.parse).
- Add model: form validation (url, name, key required, group exists).
- New team: name required, unique.
- Delete/edit via more menu.
- Up/down arrows: invoke reorder command or mutate local then save.
- Checkbox: toggle selected.
- Send invitations: invoke send_hackathon_invitations, listen for invitation-update events to update UI live, sort responders float top.
- Go: close modal, return to SetupView, mark hackathon enabled flag (or just leave selected groups).

Edge: zero-response group → checkbox disabled (class .disabled), opacity .6, note warn "Locked — no responders".

---

## 9. Setup Integration (Loop H)

- Add state to SetupView: `hackathonEnabled: boolean`, `hackathonOpen: boolean`.
- UI: toggle switch row above Start button: "Hackathon Mode" with toggle (reusing .tgl styles). When enabled, show button "Configure Hackathon" that opens mini-window.
- Persist enabled flag via hackathon_config? Add field `enabled: bool` to HackathonConfig (or separate key `hackathon_enabled`).
- Validation: when enabled, ensure at least one group selected and invitation confirmed before allowing Start? For now, canStart remains independent; hackathon failure does not block Start — just report warning.
- Flow:
  1. User toggles Hackathon ON → mini-window opens.
  2. User configures, invitations, sees live updates.
  3. User clicks Go in mini-window → modal closes, SetupView resumes, enabled flag true + selected groups saved.
  4. User clicks Start session → start_session + (if hackathon enabled) run_hackathon with project_brief as task_brief? Or do hackathon before start_session and inject report? Two orders debated; choose pre-session hackathon run whose report is injected as initial context to leader (stored in transcript?).
  Simplest: run_hackathon is manual step from mini-window (before Start). Report stored in HackathonRunState finalReport; start_session reads it and prepends to context_manager before leader loop.

- Existing Setup behavior when OFF: unchanged (no hackathon commands called).

---

## 10. Testing (Loop I companion)

Pure-logic tests (in hackathon.rs #[cfg(test)]):
1. group ordering — reorder up/down preserves order
2. leader selection — first live wins
3. leader fallback — failure moves to next live
4. zero-live-member group — Locked status
5. responder sorting — confirmed float top preserving order
6. per-teammate cap — counting increments, blocks when >= cap
7. unlimited mode — None never blocks
8. invalid route target — rejected, leader retries
9. group isolation — histories separate
10. stale run ID rejection — event with old run_id ignored
11. cancellation — cancelled flag stops loop
12. report-up formatting — delimited sections

Mock HTTP tests not needed; real API calls require keys.

---

## 11. Risk Mitigation Checklist per Loop

- Loop A: no unwrap, serde correct, key redaction structs done.
- Loop B: persistence via settings key, tests for round-trip, no plaintext emission.
- Loop C: timeout set, per-model isolation, JoinSet, no mutex across await.
- Loop D: safety cap constant, leader fallback helpers, validation, cancellation token.
- Loop E: report delimiter follows RouteCompare precedent, no blueprint bypass.
- Loop F: IPC.md updated after contract stable, generate_handler verified, JSON parse discipline.
- Loop G: mockup fidelity, theme reuse, no CDN, accessible listeners cleanup.
- Loop H: toggle additive, OFF path unchanged.

---

## 12. File Plan

New files:
- src-tauri/src/hackathon.rs (core backend)
- src/components/hackathon/HackathonMiniWindow.tsx
- src/stores/useHackathonStore.ts (optional) — or extend useAppStore

Modified:
- src-tauri/src/orchestrator.rs (add hackathon RunState field)
- src-tauri/src/settings_store.rs (hackathon config methods)
- src-tauri/src/commands.rs (or new hackathon_commands.rs included) + main.rs registration
- src/stores/useAppStore.ts (hackathon slice)
- src/components/views/SetupView.tsx (toggle)
- src/hooks/useIpcListeners.ts (hackathon events)
- src/index.css (hackathon styles)
- src-tauri/project-docs/IPC.md (hackathon contract)

Not touched:
- browser_backend.rs, browser_harness.rs, agent_brain.rs (except reuse helpers), memory_store.rs, etc.

---

## 13. Open Decisions Documented

- API key storage: plaintext in settings (matches brain), safe DTO, future encryption TBD.
- Task brief source: verbatim SessionConfig.project_brief.
- Safety round cap: 20 (internal only).
- Max questions values: 1,2,3,5,Unlimited — default 3.
- Window architecture: modal overlay (not separate Tauri window) to avoid WebView count inflation — document deviation and allow future promotion to real window without logic change.
- Model icons: use initial-letter placeholder circle (same style as group icon) — document missing designer assets.
- Mid-session trigger: stub command that checks session_active, delegates to same run_hackathon pipeline, documented as minimal.
- Memory integration: none (reuse would be speculative).

---

## 14. Verification Steps

After each loop:
- `cd src-tauri && cargo check`
- `cd src && npm run build`
- `git diff --check`

Final: cargo check + npm build + git diff --check + post-audit creation + security greps.

