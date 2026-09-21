# 01 — Source-Verified Module Map (replaces BACKEND.md/ARCHITECTURE.md maps)

Derived: 2026-09-06 from real source on commit a3ab85f (forensics/browser-auth-diagnostics). Not from docs.

**Correction (session 2):** Session 1's ~17,563 total was arithmetically inconsistent with its own per-file listing (which summed to 19,913); re-verified against real `wc -l` output in session 2: **19,913** (19,916 with build.rs). All depth-budget decisions below use the corrected 19,913 baseline.

## AppState Field Count (verified from orchestrator.rs:121-173)

Real fields: **21**

| # | Field | Type | Lock kind | Notes |
|---|-------|------|-----------|-------|
| 1 | orchestrator | `Arc<Mutex<Orchestrator>>` | tokio::sync::Mutex | status/current_session/iteration |
| 2 | transcript_store | `Arc<std::sync::Mutex<TranscriptStore>>` | std::sync::Mutex | Task 9 converted; via run_blocking |
| 3 | token_budget | `Arc<Mutex<TokenBudget>>` | tokio::sync::Mutex | in-memory, reset in start_session |
| 4 | session_vault | `Arc<std::sync::Mutex<SessionVault>>` | std::sync::Mutex | in-memory SessionVault::new(), Task 9 lock type only |
| 5 | browser_state | `Arc<Mutex<BrowserState>>` | tokio::sync::Mutex | windows/diagnostics/timeline |
| 6 | context_manager | `Arc<Mutex<ContextManager>>` | tokio::sync::Mutex | project brief + session type + pending_input |
| 7 | blueprint_store | `Arc<std::sync::Mutex<BlueprintStore>>` | std::sync::Mutex | file-backed blueprint.db, Task 9 |
| 8 | settings_store | `Arc<Mutex<SettingsStore>>` | tokio::sync::Mutex | deliberately NOT converted (Task 9 scope comment line 139-144) |
| 9 | agent_brain | `Arc<Mutex<Option<AgentBrain>>>` | tokio::sync::Mutex | primary |
| 10 | ask_user_tx | `Arc<Mutex<Option<oneshot::Sender<String>>>>` | tokio::sync::Mutex | D-041 take() pattern |
| 11 | agent_brain_2 | `Arc<Mutex<Option<AgentBrain>>>` | tokio::sync::Mutex | secondary D-039 |
| 12 | session_active | `Arc<AtomicBool>` | AtomicBool | IMP-3 compare_exchange guard |
| 13 | model_health | `Arc<Mutex<HashMap<String,ModelHealth>>>` | tokio::sync::Mutex | IMP-5 |
| 14 | brain_fail_count | `Arc<AtomicU32>` | AtomicU32 | IMP-10 switch to brain_2 at >=3 |
| 15 | memory_store | `Arc<std::sync::Mutex<MemoryStore>>` | std::sync::Mutex | via run_blocking, 6-table SQLite |
| 16 | last_memory_health | `MemoryHealth` | (value, not Arc) | is_healthy/fts_needs_repair + issues/warnings |
| 17 | setup_generation | `Arc<AtomicU32>` | AtomicU32 | anti-stale window generation |
| 18 | active_brain | `Arc<Mutex<ActiveBrainStatus>>` | tokio::sync::Mutex | primary/fallback/secondary/unknown |
| 19 | hackathon_run | `Arc<Mutex<Option<HackathonRunState>>>` | tokio::sync::Mutex | transient |
| 20 | hackathon_run_id | `Arc<Mutex<Option<String>>>` | tokio::sync::Mutex | staleness check |
| 21 | hackathon_cancel | `Arc<AtomicBool>` | AtomicBool | cancellation flag |

AppState::new() derives all three DB paths independently from `data_dir` (settings.db, blueprint.db, transcript.db via format!("{}/xxx.db", data_dir) each, memory.db via PathBuf::join). No string-replace of one path from another — D-048 LOW-B2 fix holds.

Nav channel in AppState::new is created but immediately discarded (`_nav_rx`) — the real channel is recreated per start_session (line 247 sync_channel, then bridge thread).

## Command Count (verified)

- Functions decorated `#[tauri::command]` in commands.rs: **59**
- Entries in `generate_handler!` in main.rs: **59**
- Diff: **ZERO mismatch** — every defined command appears in handler, and every handler entry corresponds to a real function.

Breakdown: 59 includes 5 session-mgmt, 8 user-interaction, ~14 settings/config, 4 diagnostic (`get_diagnostic_snapshot`, `get_browser_timeline`, `get_browser_reliability_report`, `export_browser_diagnostics`, plus `run_single_model_diagnostic`), 4 data retrieval, 3 session CRUD, 2 recovery, 2 connected-accounts/brain-status, 11 Phase 1 memory, 6 Hackathon.

`rename_all = "snake_case"` present on **32** of 59 commands (multi-arg snake_case ones). Remaining **27** have either zero args or single arg where rename is irrelevant; verified that commands with multi-word snake_case args (e.g. `provide_user_answer(answer)`, `save_agent_brain_config(api_key, base_url, ...)`) all carry the attribute. No RISK-IPCPARSE case-sensitivity bug found at this level (full check deferred to 03-findings where each command's frontend caller is cross-referenced).

Notable newly-registered since D-056 gap: `pause_session` and `resume_session` now correctly registered (lines 102-103 main.rs with comment citing the audit).

## Event Inventory — Three-way cross-check

### Backend .emit() canonical names (from grep emit("...") across src-tauri/src/*.rs)

Distinct canonical event names actually emitted (excluding harness-internal emit_harness_event/emit_timeline):

| Event | Count (emit sites) | IPC.md documents? | Frontend listens? |
|-------|-------------------|-----------------|-------------------|
| `active-turn-state` | 14 | YES (Active Turn) | YES |
| `boss-message` | 8 | YES | YES |
| `session-status` | 5 | YES | YES |
| `captcha-detected` | 4 | YES | YES |
| `brain-status` | 3 | NOT listed as separate section but implied via get_brain_status? — IPC.md DOES document `brain-status`? Check: IPC.md events list does NOT have brain-status header but mentions it indirectly. NEEDS mismatch note. | YES |
| `agent_brain_decision_fallback` | 3 | YES (diagnostic section) | NO — frontend never listens for this; this is diagnostic-only but still an emit without a consumer. Worth noting. |
| `agent-message` | 2 | YES | YES |
| `setup-complete` | 1 | YES | YES |
| `setup-agent-ready` | 1 | YES | YES |
| `setup-agent-failed` | 1 | YES | YES |
| `setup-agent-complete` | 1 | YES | YES |
| `route_started` | 1 | YES (IPC says `route_started`) | NO — frontend never listens for `route_started`; diagnostic-only. |
| `blueprint_emitted` | 1 | YES | NO — frontend not listening |
| `agent_brain_decision_started` | 1 | YES | NO — frontend not listening |
| `agent_brain_decision_failed` | 1 | YES | NO |
| `agent-routing` | 2 (response_router) | YES | YES |
| `agent-state-change` | 2 | YES | YES |
| `agent-ask-user` | 1 | YES | YES |
| `rate-limit-reached` | 2 | YES | YES |
| `session-complete` | 1 | YES | YES |
| `blueprint-section-added` | 2 | YES | YES |
| `memory-updated` | 5 | YES | YES |
| `memory-health-warning` | 2 (main.rs + response_router?) | YES | YES |
| `hackathon-run-started` | 2 | YES | YES |
| `hackathon-invitation-update` | 1 | YES | YES |
| `hackathon-group-status` | 1 | YES | YES |
| `hackathon-invitations-complete` | 1 | YES | YES |
| `hackathon-group-output` | 1 | YES | YES |
| `hackathon-complete` | 1 | YES | YES |
| `browser-diagnostic` | NOT via plain emit — emitted via harness? Frontend DOES listen for `browser-diagnostic` | YES (Agent State) | YES — but backend uses `browser_backend` diagnostics harness plus direct emits? Check: browser_backend may emit via different path; however raw grep for `"browser-diagnostic"` emit returns 0 — this suggests a mismatch: frontend listens but no backend emit uses that exact string; instead backend emits `browser-diagnostic` might be unified under different event? NEED THOROUGH TRACE — recorded as open question. |
| `blueprint-update` | 0 emits found | YES | YES — frontend listens, but no backend emit matches; backend instead uses `blueprint-section-added` and `blueprint_emitted`. This is a potential CRITICAL contract break — needs full trace in Phase 1.5 + 2.3. |
| `session-checkpoint` | 0 emits found | YES | YES — similarly no emit found; maybe `session-checkpoint` is dead? |
| `debug-log` | ? | Explicitly NOT in IPC.md, dev-only | Not found in listen list — but frontend DebugPanel may gate listener registration behind isTauri? Check. |

Additional emits that IPC.md does NOT document but backend does emit: `route_started`, `blueprint_emitted`, `agent_brain_decision_started/failed/fallback` — these are documented as diagnostic events in IPC.md's Blueprint section but frontend never consumes them; that's not a mismatch, it's intentional diagnostic noise, but worth noting as undocumented-but-harmless if frontend doesn't need them.

Frontend listen list (from useIpcListeners.ts): **27** distinct names (including hackathon 6 + session 4 + agent 4 + blueprint 2 + diagnostic + checkpoint etc.) — exhaustive list:

`session-status`, `setup-agent-ready`, `setup-agent-complete`, `setup-agent-failed`, `setup-complete`, `agent-state-change`, `active-turn-state`, `agent-routing`, `boss-message`, `browser-diagnostic`, `blueprint-section-added`, `blueprint-update`, `agent-message`, `agent-ask-user`, `captcha-detected`, `rate-limit-reached`, `session-checkpoint`, `memory-updated`, `memory-health-warning`, `brain-status`, `hackathon-invitation-update`, `hackathon-group-status`, `hackathon-invitations-complete`, `hackathon-run-started`, `hackathon-group-output`, `hackathon-complete`, `session-complete`

Three-way mismatch candidates flagged for deeper verification (carry to findings, do not pre-judge severity):
1. Backend emits `blueprint_emitted` / `route_started` / `agent_brain_decision_*` that frontend never listens for — benign if intentional diagnostic, but IPC.md should clarify.
2. Frontend listens for `blueprint-update` and `session-checkpoint` with ZERO matching backend emits found via grep — potential dead listeners or grep false negative due to dynamic event construction (must re-check by reading response_router.rs fully before final verdict).
3. Frontend listens for `browser-diagnostic` but no plain `emit("browser-diagnostic")` found — may be emitted via BrowserDiagnostics harness using different emitter path; must verify by reading browser_backend.rs emit_timeline vs emit.
4. Frontend listens for `brain-status` but IPC.md has no dedicated event section for it — doc gap.

Action: these four are marked as provisional MISMATCH-HOLD for Phase 1.5/2.3 to resolve with full file reads, not as confirmed findings yet.

## Five Uncertain-Status Items — Classification (Step 0.4)

### 1. Phase 1 Memory System — **FULLY_IMPLEMENTED**

- File: `src-tauri/src/memory_store.rs` — **1474 lines**, not a stub.
- Schema: 6+ tables (session_memory, project_memory, global_memory, open_questions, model_strengths, patterns + FTS index + project_config), verified via grep of `CREATE TABLE` in file (not yet counted precisely but file is substantial).
- AppState fields: `memory_store: Arc<std::sync::Mutex<MemoryStore>>` + `last_memory_health: MemoryHealth`.
- Commands: 11 memory commands registered and implemented (`get_project_memory`, `get_global_memory`, `clear_project_memory`, `get_open_questions`, `get_model_strengths`, `save_project_config`, `get_project_config`, `get_memory_health`, `repair_memory_index`, `get_patterns`, `export_memory`, `restore_memory` — actually 12? Count shows 11-12 depending on include; generate_handler lists 11 distinct memory entries lines 149-160).
- DB path: `memory.db` via `PathBuf::from(data_dir).join("memory.db")` — file-backed, not in-memory.
- Frontend: `MemoryPanel.tsx` (211 lines) + SettingsPanel integration + `memory-updated`/`memory-health-warning` events.
- `db_helpers::run_blocking` used for all async access — verified via AppState doc comments.
- Verdict: FULLY_IMPLEMENTED — full audit required in later phases. Not a stub.

### 2. Phase 2 Skill & Orchestration Engine — **STUB_ONLY / NOT_PRESENT**

- `src-tauri/src/capability_registry.rs` (40 lines) — struct with HashMap, two methods, zero integration points elsewhere? Grep shows no callers.
- `src-tauri/src/persona_manager.rs` (61 lines) — fingerprint generation only, no integration.
- `src-tauri/src/agentic_manager.rs` (66 lines) — workspace + pending approvals, zero callers.
- `src-tauri/src/resource_monitor.rs` (60 lines) — memory thresholds + heartbeat, zero callers.
- `src-tauri/src/signals.rs` (65 lines) — TurnSignals weighted sum, zero callers.
- `src-tauri/src/proxy_manager.rs` (19 lines) — placeholder fetch_through_proxy returning NetworkError, zero callers.
- No SKILL.md registry, no pipeline engine, no subagent forking, no scanner/policy layer found in inventory.
- All flagged by cargo check as `dead_code` (unused).
- Verdict: STUB_ONLY — budget saved, no deep audit, but worth flagging dead code in findings if doc claims otherwise.

### 3. Hackathon Mode — **FULLY_IMPLEMENTED**

- File: `src-tauri/src/hackathon.rs` — **1304 lines**, complete with validation, leader fallback, group execution loops, HTTP client, safety cap, tests (15+ tests).
- Frontend: `src/components/hackathon/HackathonMiniWindow.tsx` (502 lines) — full window.
- AppState fields: `hackathon_run`, `hackathon_run_id`, `hackathon_cancel` (3 fields).
- Commands: 6 commands (`get_hackathon_config`, `save_hackathon_config`, `get_hackathon_run_state`, `cancel_hackathon_run`, `send_hackathon_invitations`, `run_hackathon`).
- Events: 6 hackathon events, all wired in useIpcListeners.ts.
- Settings: persisted via `settings_store` key `hackathon_config`.
- Verdict: FULLY_IMPLEMENTED — full audit required. Note: design doc `HACKATHON_MODE_DESIGN.md` exists, now implemented; need to reconcile.

### 4. Dev-Team / OpenCode Bridge — **NOT_PRESENT**

- No file matching `*opencode*`, `*bridge*`, `*dev_team*` found in inventory.
- Grep for "bridge" across src returns only harness bridge phrase, not a dev-team bridge.
- IPC.md mentions no bridge events.
- Grep for "OpenCode" yields only dialog plugin.
- Verdict: NOT_PRESENT — skip entirely.

### 5. Diagnostic black-box / flight-recorder — **FULLY_IMPLEMENTED** (as browser reliability harness, not screenshot capture)

- File: `src-tauri/src/browser_harness.rs` — **1556 lines**, comprehensive timeline/ring buffer/FailureClassification/DomSnapshot/etc.
- File: `src-tauri/src/browser_backend.rs` — embedded BrowserDiagnostics with `emit_harness_event`/`emit_timeline`/`NavigationForensics`/console collection, plus screenshot? Check: no image capture found, but structured logs + timeline + diagnostics snapshot present.
- SettingsPanel Diagnostic Snapshot (get_diagnostic_snapshot) + get_browser_timeline + export_browser_diagnostics + get_browser_reliability_report + run_single_model_diagnostic — all implemented.
- No literal screenshot file generation found (no `capture_screenshot` grep), so "screenshot capture" portion of spec is NOT implemented; structured log portion IS.
- Verdict: FULLY_IMPLEMENTED for the harness/diagnostics/logging portion; screenshot portion NOT_PRESENT. Classified overall as PARTIALLY_IMPLEMENTED vs the original two-part spec, but for audit purposes treat the harness as real and audited, screenshot as open question.

## Module Status Summary Table (for 08-progress-tracker seeding)

| Module | Lines | Status | Findings scope |
|--------|------|--------|----------------|
| main.rs | 182 | FULL | Phase 1.1 |
| orchestrator.rs | 248 | FULL | Phase 1.2 |
| commands.rs | 3011 | FULL | Phase 1.3 (highest RISK-IPCPARSE) |
| agent_brain.rs | 532 | FULL | Phase 1.4 |
| response_router.rs | 2453 | FULL | Phase 1.5 (highest risk) |
| session_runner.rs | 1099 | FULL | Phase 1.6 |
| browser_backend.rs | 6389 | FULL | Phase 1.7 (GENERIC_INIT_SCRIPT, etc.) |
| browser_harness.rs | 1556 | FULL | Phase 1.7/1.9 |
| settings_store.rs | 348 | FULL | Phase 1.8 |
| blueprint_store.rs | 198 | FULL | Phase 1.9 |
| transcript_store.rs | 235 | FULL | Phase 1.9 |
| session_vault.rs | 180 | FULL | Phase 1.9 |
| turn_manager.rs | 52 | STUB | Phase 1.10 — dead |
| token_budget.rs | 59 | STUB/PARTIAL | Phase 1.9 — reset_all exists but record_tokens never called |
| context_manager.rs | 149 | FULL | Phase 1.9 |
| errors.rs | 72 | FULL | Phase 1.9 |
| db_helpers.rs | 61 | FULL | Phase 1.9 |
| memory_store.rs | 1474 | FULL | Phase 1.10 (if landed → now FULL) |
| hackathon.rs | 1304 | FULL | Phase 1.10 (if landed → now FULL) |
| resource_monitor.rs | 60 | STUB_ONLY | skip |
| capability_registry.rs | 40 | STUB_ONLY | skip |
| persona_manager.rs | 61 | STUB_ONLY | skip |
| agentic_manager.rs | 66 | STUB_ONLY | skip |
| signals.rs | 65 | STUB_ONLY | skip |
| proxy_manager.rs | 19 | STUB_ONLY | skip |
| useAppStore.ts | 305 | FULL | Phase 2.1 |
| lib/tauri.ts | 82 | FULL | Phase 2.2 |
| hooks/useIpcListeners.ts | 377 | FULL | Phase 2.3 |
| App.tsx | 80 | FULL | Phase 2.4 |
| views/* (4) | 79+101 | FULL | Phase 2.5 |
| overlays/* (3) | 30 | FULL | Phase 2.6 |
| shared/* (3) + layout/* (2) | ~400 | FULL | Phase 2.7 |
| lib/* (3 small) | ~120 | FULL | Phase 2.8 |
| index.html + index.css | 12+234 | FULL | Phase 2.9 |
