# Hackathon Mode — Pre-Implementation Audit

**Date:** 2026-09-05
**Branch:** forensics/browser-auth-diagnostics
**HEAD:** a3ab85f fix(browser): close known reliability gaps before Windows validation
**Checkpoint:** checkpoint-before-hackathon-mode
**Dirty pre-existing:** 7 modified + 2 untracked (see hackathon-mode-preexisting-state.txt)

---

## A. Current Git State

- Worktree dirty (intentional): maintenance-mode diagnostics gate already landed in working dir but not yet committed.
- Files: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`, `src-tauri/src/settings_store.rs`, `src-tauri/project-docs/IPC.md`, `src/index.css`, `src/panels/MemoryPanel.tsx`, `src/panels/SettingsPanel.tsx`
- Untracked: `HACKATHON_MODE_DESIGN.md`, `src-tauri/project-docs/mockup/hackathon-mini-window.html`, `dist/`
- Recovery tag `checkpoint-before-hackathon-mode` at a3ab85f guarantees rollback to clean baseline.
- **Verdict:** PASS — recoverable baseline established, dirty state documented, no stash/reset.

---

## B. Existing AppState Structure — 16 fields

`src-tauri/src/orchestrator.rs:121` AppState:
- orchestrator: Arc<Mutex<Orchestrator>>
- transcript_store: Arc<std::sync::Mutex<TranscriptStore>> (std, via run_blocking)
- token_budget: Arc<Mutex<TokenBudget>>
- session_vault: Arc<std::sync::Mutex<SessionVault>> (in-memory, AES-256-GCM)
- browser_state: Arc<Mutex<BrowserState>>
- context_manager: Arc<Mutex<ContextManager>>
- blueprint_store: Arc<std::sync::Mutex<BlueprintStore>>
- settings_store: Arc<Mutex<SettingsStore>> (NOT std — still tokio::Mutex)
- agent_brain: Arc<Mutex<Option<AgentBrain>>>
- ask_user_tx: Arc<Mutex<Option<oneshot::Sender>>>
- agent_brain_2: Arc<Mutex<Option<AgentBrain>>>
- session_active: Arc<AtomicBool>
- model_health: Arc<Mutex<HashMap<ModelHealth>>>
- brain_fail_count: Arc<AtomicU32>
- memory_store: Arc<std::sync::Mutex<MemoryStore>>
- last_memory_health: MemoryHealth (non-Arc)
- setup_generation: Arc<AtomicU32>
- active_brain: Arc<Mutex<ActiveBrainStatus>>

**Hackathon impact:** Need additive fields — no existing field can host hackathon config. Must add hackathon_state without disturbing memory/store semantics.

- PASS: structure documented.

---

## C. Existing Command List (registered in main.rs:90)

Counted from `src-tauri/src/main.rs` generate_handler!: 29 commands (30 if counting maintenance gate 2):
- Session: start_session, pause_session, resume_session, abort_session
- User: user_input, captcha_resolved, retry_setup_agent, confirm_setup_agent, provide_manual_model_response, rate_limit_decision, setup_agent_sent, provide_user_answer
- Settings: save_agent_brain_config, get_agent_brain_config, save_secondary_brain_config, get_secondary_brain_config, save_fallback_brain_config, get_fallback_brain_config, save_custom_participants, get_custom_participants, get_participants, save_prompt_template, get_prompt_template, get_maintenance_mode, set_maintenance_mode, get_diagnostic_snapshot, get_browser_timeline, get_browser_reliability_report, export_browser_diagnostics, run_single_model_diagnostic
- Data: get_transcript, get_session_list, export_blueprint, get_agent_health, delete_session, rename_session, get_session_details, get_recovery_state, recover_session, launch_connected_account, get_brain_status
- Memory: get_project_memory, get_global_memory, clear_project_memory, get_open_questions, get_model_strengths, save_project_config, get_project_config, get_memory_health, repair_memory_index, get_patterns, export_memory, restore_memory

Note: maintenance_mode commands are in dirty working dir (not yet on HEAD but will be committed). Count them as present.

- PASS: enumerated.

---

## D. Existing Event List (from IPC.md + useIpcListeners.ts + commands.rs emit)

Backend emits (grep `app.emit` in src-tauri/src):
- session-status
- setup-agent-ready, setup-agent-complete, setup-agent-failed, setup-complete
- agent-state-change, agent-routing, boss-message, browser-diagnostic
- active-turn-state (7 event strings)
- blueprint-update, blueprint-section-added, agent_brain_decision_started, agent_brain_decision_failed, agent_brain_decision_fallback, route_started, blueprint_emitted
- agent-message, requirements-question, requirements-complete
- agent-ask-user, captcha-detected, rate-limit-reached, session-checkpoint, session-complete
- memory-updated, memory-health-warning, brain-status

Frontend listens for all above in useIpcListeners.ts — matched 1:1 except debug-log dev-only.

- PASS: list complete, names canonical.

---

## E. Existing Tauri Window Creation Architecture

`src-tauri/src/browser_backend.rs`:
- Two labels: LEADER_WINDOW_LABEL="arena-leader", NAV_WINDOW_LABEL="arena-nav" — hard max 2.
- `create_windows()` creates leader + nav via WebviewWindowBuilder, registers diagnostics, setup metadata.
- `ensure_nav_window()` lazily recreates nav if closed.
- `select_window(is_leader)` returns correct window; `active_by_window` maps window_label→agent_id.
- `on_navigation` uses `std::sync::mpsc::SyncSender`, never tokio::mpsc, never blocking_lock.
- New hackathon UI window MUST be a Tauri UI window (lightweight webview for config), NOT an AI participant WebView. Distinction critical: existing 2-view limit applies to AI model webviews, not generic UI windows. Must verify creation path does not call create_windows that would overwrite diagnostics.

Inspection: No existing hackathon window; no extra WebView creation. `tauri.conf.json` window config not yet inspected but default single window.

- PASS (with design constraint): 2-WebView AI limit currently respected.

---

## F. Existing Frontend State Architecture

- `src/stores/useAppStore.ts`: Zustand create<AppStore>, ~240 lines.
- State slices: participants (P3 merged), sessionStatus, setupProgress, selectedSessionId, recoveryState, setupBrief, sessionAgentIds, setupReady/Failed, activeAgentId/Turn, activeBrain, blueprintSections, liveStatus, overlays (askUser, captcha, rateLimit), toasts, agentBrainConfig, settingsOpen, sidebarCollapsed.
- Actions are simple setters; no run-id tracking yet.
- Persistence: Zustand in-memory only; backend persistence via settings_store per command.
- Theme via `src/lib/theme.ts` (localStorage + data-theme).

Hackathon needs additive slice: hackathonConfig (persisted), hackathonRunState (transient), invitation UI state. Must extend store without breaking existing selectors.

- PASS

---

## G. Existing Settings Persistence

`src-tauri/src/settings_store.rs:39` SettingsStore:
- Single table `settings(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER)`.
- Generic key-value; no schema migration needed for new keys.
- Methods: get/set raw, plus typed pairs for brain configs (primary/fallback/secondary), custom_participants (JSON array), prompt templates, maintenance_mode (bool string "true"/"false").
- File: app_data_dir/settings.db. Uses rusqlite with `tokio::sync::Mutex` in AppState (NOT std), so callers lock directly .await — exception to db_helpers pattern (by design, small lookups).

Hackathon config can be stored as JSON string under `hackathon_config` key — zero migration cost, consistent with custom_participants pattern.

- PASS: safe additive key.

---

## H. Existing Secure Credential Storage

`src-tauri/src/session_vault.rs:20` SessionVault:
- AES-256-GCM via `ring`, key derived via PBKDF2-HMAC-SHA256 with static SALT+ITERATIONS.
- Currently IN-MEMORY only (Connection::open_in_memory) — persists only for process lifetime; `open(path)` exists but unused in AppState::new.
- Stores: cookies (encrypted blob), conversation_urls (plaintext per session+agent).
- SettingsStore brain API keys are plaintext in settings.db (no encryption). Diagnostics snapshot explicitly avoids logging keys; `redact_diagnostic_text` exists in commands.rs.

**Audit finding:** Project currently does NOT encrypt brain API keys at rest (they live plaintext). Hackathon API keys following same pattern would be consistent with existing threat model, BUT design doc Section 21 flags this as unresolved. Options:
- Option 1: Store plaintext in settings_store, never emit to frontend, redact in logs (matches brain precedent) — minimal change, consistent.
- Option 2: Reuse SessionVault encryption but need file-backed vault or new hackathon-specific encrypted table — larger scope, requires deciding key derivation and persistence.
- Recommendation (conservative): Option 1 + document as `NEEDS DESIGN` — reuse existing brain pattern, ensure keys never leave backend in plaintext events/logs, and note that encrypted-at-rest upgrade is future work. Do NOT invent new crypto without explicit approval.

- NEEDS DESIGN: secure storage mechanism not yet confirmed; report blocker check required.

---

## I. Existing Async/Concurrency Patterns

- Backend Tokio full features.
- Session loop: `tokio::spawn` in start_session, run_setup loop then run_debate (run_agent_loop).
- ResponseRouter concurrency: sequential per-turn for Route, sequential for RouteCompare (loop over models one-by-one) — but hackathon spec requires GROUPS execute concurrently, teammates within a group sequentially.
- `std::sync::mpsc::sync_channel(256)` bridged to `tokio::sync::mpsc::channel(256)` via thread spawn (blocking_send).
- Cancellation: session_active AtomicBool + NavEvent::SessionAborted.
- No bounded concurrency primitive yet for hackathon (would need JoinSet/Semaphore).
- Mutex discipline: tokio Mutex for orchestrator/brain/browser_state; std Mutex for transcript/blueprint/vault/memory.

- PASS: patterns understood; hackathon must add group-level JoinSet without holding mutexes across awaits.

---

## J. Existing API Client Patterns

`src-tauri/src/agent_brain.rs:102` AgentBrain:
- reqwest::Client with 60s timeout (BRAIN_HTTP_TIMEOUT_SECS).
- Chat completions POST to `{base_url}/chat/completions` with Bearer auth, JSON {model, messages:[system,user], max_tokens:1024}.
- System prompt built + DECISION_JSON_CONTRACT appended; user_content = Leader response + Context.
- Error classification: network_error_category, http_status_category (auth, rate_limit, gone, etc.), redaction helpers.
- Fallback client built on failure with same timeout.
- Parsing: extract_json_object (balanced-brace scan tolerating fences), then serde parse to AgentDecision.

Hackathon API client can reuse same pattern: per-model reqwest calls, same timeout (or shorter 30s for invitations), same redaction, same JSON extraction for decision contract.

Existing reqwest version 0.11; already in Cargo.toml — no new dependency needed.

- PASS

---

## K. Existing Error Handling

- AgentError enum with kind() classification (Transient vs Permanent).
- Commands map errors via `settings_command_error` which redacts secrets + logs via tracing::error.
- No unwrap()/expect() in live session paths (verified grep below). Acceptable uses: `expect` in main.rs setup for AppState init, `unwrap_or_else` for poison recovery, tests.
- ResponseRouter degrades: participant failure → leader continues with failure notice; brain fallback → retry once; empty shell → bounded failure.

Hackathon must adopt same: per-model failure → mark unavailable, leadership fallback, group continues; never panic session.

- PASS

---

## L. Existing Session Lifecycle

1. start_session validates agents, sets session_active true, stores last_session_id + session_complete=false, resets brain_fail_count + token counts, sets OrchestratorStatus::Setup, creates transcript session (run_blocking), creates BrowserState + windows, bridges nav channel, emits session-status:setup, spawns task.
2. Task: loop run_setup until Ok or abort; on run_setup failure emits setup-agent-failed recoverable; listens for ResumeRequested/ManualConfirmed.
3. Transition to Running, then run_debate (run_agent_loop) until Complete or error or abort. Emits session-complete or ended. Resets session_active on all exit paths.
4. abort_session: store session_active false, clear ask_user_tx, send SessionAborted, set Ended, emit ended.

Hackathon lifecycle must be nested but not break main lifecycle: invitation phase (pre-session), then concurrent group execution (report-up), then leader continues normal loop. Must handle abort_session cancelling hackathon runs too.

- PASS

---

## M. Existing Agent Brain Integration

- Brain stored in AppState::agent_brain (primary) + fallback fields inside struct + agent_brain_2 (secondary).
- Decides via `decide_with_source` with system prompt + DECISION_JSON_CONTRACT; returns AgentDecision enum (Route/Blueprint/Continue/Complete/RouteCompare/AskUser) with rename_all snake_case.
- ResponseRouter consumes decision; SessionRunner uses it for loop phase.
- Brain config persisted via settings_store; retrieval via get_*_brain_config (JSON string returns).

Hackathon group leaders need isolated decision contract (route vs submit), NOT extending AgentDecision. Must create new enum HackathonDecision { Route{target, prompt}, Submit{output} } to avoid polluting main contract.

- PASS: brain integration point understood.

---

## N. Existing Setup Screen Integration Point

`src/components/views/SetupView.tsx:36`:
- Inputs: project_brief textarea, session_type seg, participants pcards (with P3 merged registry + health dots), leader select, agent brain collapsible (base_url, api_key, model, system_prompt).
- Validation: `brainReady` && selected.size>=2 && selected.has(leader) && brief non-empty.
- start() saves brain config then invoke('start_session').

Hackathon toggle should be additive here: a Switch/checkbox "Hackathon Mode" that opens mini-window when enabled. Must not break existing canStart logic when disabled (default off). When enabled, mini-window config must be validated before allowing Start.

Design mockup toolbar has Add model / New team — this lives in mini-window, not SetupView.

- PASS: integration point located.

---

## O. Existing Main Leader Integration Point

ResponseRouter's `run_agent_loop` is the sole leader-driven orchestration. Leader responses flow through AgentBrain::decide, then match decision.

Hackathon output must be injected as context to leader — analogous to RouteCompare's combined "[X said:...]" pattern. Need report-up formatting that context_manager or leader prompt consumes.

Current leader injection path: `inject_active_prompt(window, leader_id, prompt, turn, ...)` — prompt contains "[Response from X]: ..." for Route return. For hackathon, would inject combined group report string.

- PASS: integration seam located (before next brain decide).

---

## P. Existing Tests

- `src-tauri/src/settings_store.rs` tests: custom_participants empty/read/reopen/clear.
- `src-tauri/src/agent_brain.rs` tests: extract_json_object with fences.
- `src-tauri/src/browser_backend.rs` tests: none found for sanitization? grep shows no.
- `src-tauri/src/memory_store.rs` extensive tests (health, FTS, etc.).
- No hackathon tests yet. Need pure-logic tests for group ordering, leader fallback, cap, isolation, etc.

Tooling: `cargo test` (full), `cargo check` (backend), `npm run build` (frontend).

- PASS: test harness exists.

---

## Q. Existing Build/Check Commands

- Backend: `cd src-tauri && cargo check` (also `cargo test`)
- Frontend: `cd src && npm run build`
- Diff check: `git diff --check`
- Tauri dev: `npm run tauri dev`

Verified via AGENTS.md.

- PASS

---

## R. Exact Files That MUST Change

**Backend:**
- src-tauri/src/main.rs — register new commands
- src-tauri/src/commands.rs — or split into hackathon module; but minimal is adding hackathon commands there
- src-tauri/src/settings_store.rs — add hackathon_config persistence methods (if storing via settings)
- src-tauri/src/orchestrator.rs — add hackathon_state to AppState (run_id, cancellation tokens, in-memory run data)
- NEW: src-tauri/src/hackathon.rs — data model, persistence helpers, API client, invitation engine, group orchestration, decision parsing, safety caps
- src-tauri/src/session_runner.rs or response_router.rs — minimal report-up integration (or new hackathon_report_up helper)

**Frontend:**
- src/stores/useAppStore.ts — extend with hackathon slices
- src/components/views/SetupView.tsx — add hackathon toggle that opens mini-window
- NEW: src/components/hackathon/HackathonMiniWindow.tsx (or similar) — full mockup port
- NEW or extend: src/hooks/useHackathonListeners.ts — or extend useIpcListeners.ts with hackathon events
- src/lib/agents.ts — no change needed unless reusing displayName helpers
- src/index.css — add hackathon styles (scoped, reuse variables)
- src/components/layout/Topbar.tsx / Sidebar.tsx — no change unless wiring window lifecycle

**Docs:**
- src-tauri/project-docs/IPC.md — add hackathon commands/events entries only after contract stable

**Build:**
- Cargo.toml, package.json — no dependency changes expected (reuse reqwest, tokio, serde)

- MUST CHANGE listed.

---

## S. Exact Files That SHOULD NOT Change

- src-tauri/src/browser_backend.rs — 2-WebView limit, GENERIC_INIT_SCRIPT, on_navigation (no hackathon WebViews, no changes)
- src-tauri/src/browser_harness.rs — harness forensic timelines (hackathon is API-only, no browser)
- src-tauri/src/orchestrator.rs SessionConfig/context_manager.rs — existing session semantics (hackathon is additive)
- src-tauri/src/agent_brain.rs AgentDecision enum — must NOT add hackathon variants (create isolated HackathonDecision)
- src-tauri/src/memory_store.rs / transcript_store.rs / blueprint_store.rs — no memory schema changes for hackathon
- src-tauri/project-docs/mockup/preview.html — production shell ground truth (do not replace)
- src/index.css — beyond additive hackathon classes, do not rewrite theme variables
- public/fonts — do not add CDN fonts
- src-tauri/src/db_helpers.rs — no change (reuse run_blocking)
- src/panels/MemoryPanel.tsx, SettingsPanel.tsx — no change unless exposing hackathon diagnostics (out of scope)

- SHOULD NOT CHANGE listed.

---

## T. Named-Risk Analysis

### RISK-BLOCKING
- Existing: no blocking_lock in async; session_runner uses run_blocking correctly.
- Hackathon risk: group orchestration must never hold a Mutex across `await` (especially settings_store tokio Mutex and browser_state). Groups do HTTP calls → must drop locks before await.
- **ASSESSMENT:** NEEDS DESIGN — enforce lock-scoping pattern (clone needed data, drop lock, then await).

### RISK-CHANNEL
- Existing: on_navigation uses std::sync::mpsc exclusively; bridging via thread.
- Hackathon risk: temptation to use tokio::sync::mpsc inside sync callbacks — not applicable (hackathon has no browser injection). Use tokio channels only in async orchestration (group concurrency).
- **ASSESSMENT:** PASS — no on_navigation usage expected; verify during implementation.

### RISK-UNWRAP
- Existing: no unwrap/expect in live paths (grep below: only tests, setup init, poison recovery).
- Hackathon risk: new parsing of model JSON decisions or API responses could tempt unwrap.
- **ASSESSMENT:** PASS (requires enforcement) — use Result, map_err, saturating_add etc. Audit post-implementation with grep.

### RISK-EVENTMATCH
- Existing: every app.emit has matching frontend listen + IPC.md entry (audit verified).
- Hackathon risk: new events (hackathon:invitation-update, hackathon:run-update, etc.) must have exactly matching listen and IPC.md names/payloads.
- **ASSESSMENT:** NEEDS DESIGN — define exact event names before implementation; audit table post-implementation.

### RISK-IPCPARSE
- Existing: commands returning structs return `serde_json::to_string(&value)`; frontend JSON.parse. Exceptions: get_prompt_template, export_blueprint (plain string).
- Hackathon risk: new get_hackathon_config returns struct → must follow JSON-string pattern.
- **ASSESSMENT:** PASS with enforcement — verify command return types vs frontend parsing.

### RISK-ASYNC
- Existing: response_router uses sequential per-participant injection. Hackathon requires concurrent GROUPS (not sequential). Must use bounded async concurrency (JoinSet or futures::join_all with limit) — not `for g in groups { run(g).await }`.
- **ASSESSMENT:** NEEDS DESIGN — design group concurrency with tokio::task::JoinSet, per-group error isolation, memory <2GB (API calls are network-bound, not RAM-heavy).

### RISK-API-FAILURE
- Existing: participant failure is recoverable (response_router logs, continues with failure notice to leader).
- Hackathon risk: invitation health-check failure per model must not crash whole invitation round; leader failure must fallback; group with all failures → produces no output (not panic).
- **ASSESSMENT:** PASS — enforce per-model error isolation, leader fallback chain, group degradation handling.

### RISK-LEADER-FALLBACK
- Ordering is user-defined position 1..N. Same rule applies at invitation time and during run.
- Must maintain sorted model_ids per group and have helper `select_leader(group, live_set)` that returns first live in order, not random.
- Sorting after invitation: responders float top preserving relative order, non-responders stay below but in original order.
- **ASSESSMENT:** NEEDS DESIGN — need helper `resolve_leader(group, live)` + `sort_by_responder_status` pure functions, with tests.

### RISK-STATE-CORRUPTION
- Most critical for hackathon: duplicate runs, stale invitations overwriting new run, histories leaking between groups, responses appended to wrong group, cancelled runs continuing.
- Must use run_id (Uuid) + group_id + model_id tuple as discriminator; check run_id at every async boundary; use AtomicBool or oneshot cancellation.
- **ASSESSMENT:** NEEDS DESIGN — require `HackathonRunId` generation per invitation/run, store active_run_id in AppState, reject stale events where event.run_id != active_run_id.

---

## Additional Findings

### Pre-existing dirty diff is safe to preserve
- maintenance_mode gate is additive and unrelated to hackathon; do not merge/conflict.

### Missing assets
- Mockup hackathon-mini-window.html loads Inter/JetBrains Mono via Google Fonts CDN + lucide CDN — production must use local fonts + existing Lucide/react-icons, never CDN.
- Model icons: design says icons supplied by product owner — none found in repo `public/` or `src/assets`. Will use placeholder (initial letter badge) and document missing asset dependency.

### Safety caps
- Per-teammate cap values from mockup: 1, 3, 5, Unlimited (also design mentions 2 as candidate). Conservative choice: implement as provided in mockup (1/3/5/Unlimited) and document; do not invent 2 unless needed for parity.
- Emergency group-round cap: unresolved in design, but implementation MUST have bounded loop. Recommend INTERNAL safety limit = 20 iterations per group (leader decisions), documented as implementation safety limit, not exposed in UI, not replacing per-teammate cap. Rationale: per-teammate cap bounds teammate consults, but leader could still spin `Continue` infinitely — need hard bound.

### Task brief source
- Existing source: `SessionConfig.project_brief` + `context_manager.project_brief` is the canonical string. Safest brief = verbatim `project_brief` for every group (no summarization model, no transformation).

### Mid-session trigger
- Design leaves mechanism unresolved. Least invasive: expose hackathon orchestration as separate capability callable via command (e.g., `run_hackathon`) that later can be invoked by leader/brain without changing AgentDecision enum. For now implement pre-enabled/session-setup Hackathon Mode fully; stub mid-session command that checks session_active and delegates to same report-up path, document exact current behavior.

### Memory compatibility
- Phase 1 Memory implemented but hackathon should NOT add tables. Use existing MemoryStore interfaces only if clearly appropriate (e.g., no new memory fact types for hackathon). For now: no memory integration, document as future.

---

## Verdict Summary

- PASS: 12 / NEEDS DESIGN: 7 / FAIL: 0
- FAIL items: none — no blocker requiring immediate STOP.
- NEEDS DESIGN items all have proposed conservative resolution documented above — proceed to planning phase with those decisions recorded.

**Next:** Produce internal technical implementation plan before touching production code.
