# Hackathon / Session Reliability Batch — Implementation Plan

**Date:** 2026-09-06
**Branch:** forensics/browser-auth-diagnostics
**HEAD:** a3ab85f6544505eb6affd52fbc184dad150c1bf2
**Checkpoint:** checkpoint-before-hackathon-mode → a3ab85f (intact)
**Auditor/Author:** OpenCode (Muse Spark 1.2) — full source read, no assumption
**Baseline:** `cargo check` PASS (69 warnings, 0 errors, dev profile 24s), `npm run build` PASS (1710 modules, 362kB gz110kB, 36s), `git diff --check` PASS

---

## 1. Current Architecture

Two-WebView Tauri 2.0 desktop app (Rust + React/Zustand). Single persistent leader WebView (`arena-leader`) + one shared navigating participant WebView (`arena-nav`). No third WebView. Browser automation via `GENERIC_INIT_SCRIPT` static + `arena://` fake navigation intercepted in `on_navigation` using `std::sync::mpsc::SyncSender`. Async session loop is leader-driven via `AgentBrain` (OpenAI-compatible HTTP, 60s timeout, fallback retry, secondary after 3 fails). State lives in `AppState` (orchestrator, transcript/blueprint/session_vault/memory stores, browser_state, context_manager, token_budget, brain stores, session_active AtomicBool, model_health, brain_fail_count, setup_generation, active_brain, plus hackathon fields: `hackathon_run`, `hackathon_run_id`, `hackathon_cancel`). Frontend state in `useAppStore` (Zustand, ~305 lines, no persistence except via IPC). Persistence: SQLite `settings.db` (generic key-value), `transcript.db`, `blueprint.db`, `memory.db` (file-backed, WAL, user_version=1). `transcript_store/blueprint_store/session_vault/memory_store` now `std::sync::Mutex` + `db_helpers::run_blocking` (spawn_blocking 3× retry). Hackathon is additive API-only layer: `hackathon.rs` (1304 lines) provides domain model, pure helpers, HTTP client (`call_hackathon_model`), and `run_single_group` loop; `commands.rs` adds 6 commands + abort cancellation; `HackathonMiniWindow.tsx` (502 lines, 620px modal, hk-* classes) mounts from `App.tsx`; listeners in `useIpcListeners.ts` (6 events). Memory 1.68GB budget, no heavy deps.

Source truth verified via direct reads of: `commands.rs` (~3011 lines), `hackathon.rs`, `orchestrator.rs` (248 lines), `response_router.rs` (~2400 lines), `session_runner.rs` (~890 lines), `settings_store.rs`, `browser_backend.rs` (~5000 lines), `useAppStore.ts`, `Sidebar.tsx` (34 lines), `InputBar.tsx` (47 lines), `ActiveView.tsx`, `useIpcListeners.ts` (377 lines), `IPC.md` (745 lines), `HACKATHON_MODE_DESIGN.md` (482 lines), audits `hackathon-mode-pre/post/review`.

---

## 2. Existing Session Lifecycle

```
start_session (commands.rs:131)
  validate_session_agents against merged registry (built-ins + custom)
  compare_exchange session_active false→true (IMP-3)
  SessionConfig {session_id=Uuid, project_brief, session_type, agent_ids, leader} setup_order() leader-first
  setup_generation fetch_add(1)
  last_session_id + session_complete=false in settings_store
  brain_fail_count=0, token_budget.reset_all(), orchestrator.status=Setup, context_manager=new, transcript Store.create_session via run_blocking
  BrowserState::new(std_nav_tx) + create_windows (destroy stale, build leader+nav with about:blank, GENERIC_INIT_SCRIPT, on_navigation)
  std→tokio bridge thread, emit session-status{setup, session_id, setup_generation, selected_leader_id, selected_agent_ids, setup_order}
  spawn task: loop run_setup until Ok else emit setup-agent-failed recoverable, wait ResumeRequested/SetupManualConfirmed/SessionAborted
  → Running → run_debate → run_agent_loop until Complete or error or abort → Ended, store(false) on all exits

pause_session / resume_session: existed as functions but previously missing from generate_handler! (D-056 fixed); now registered; they simply flip OrchestratorStatus and emit session-status. No checkpoint, no graceful boundary, no persistence. Frontend `InputBar` stop button calls `abort_session` (hard cancel), not pause.

abort_session: store session_active false, set hackathon_cancel true + per-run cancelled, clear ask_user_tx, try_send SessionAborted to browser nav_tx, orch=Ended, emit ended. No graceful checkpoint, no next_step.

Recovery: `get_recovery_state` checks last_session_id + session_complete flag; `recover_session` re-emits blueprint-section-added for saved sections only, does NOT restart loop.
```

Weakness: pause is not graceful, no checkpoint, next_step not persisted, stale handling minimal, InputBar’s STOP is hard abort.

---

## 3. Existing Persistence Model

| Store | File | Lock | Access | Durable? |
|---|---|---|---|---|
| SettingsStore | settings.db | tokio::Mutex | direct .lock().await (tiny lookups) | yes, key-value |
| TranscriptStore | transcript.db | std::sync::Mutex | run_blocking | yes |
| BlueprintStore | blueprint.db | std::sync::Mutex | run_blocking | yes |
| SessionVault | in-memory (new()) | std::sync::Mutex | run_blocking | no (cookies in mem, convo URLs in mem) |
| MemoryStore | memory.db | std::sync::Mutex | run_blocking | yes |
| HackathonConfig | settings.db key `hackathon_config` | via settings_store | direct | yes, JSON |
| HackathonRunState | transient `AppState.hackathon_run` | tokio::Mutex | in-memory only, lost on restart | no |

HackathonRunState fields: run_id (Uuid), task_brief (ctx.project_brief verbatim or fallback), max_questions Option<u32>, groups Vec<GroupRunState>, cancelled Arc<AtomicBool>, created_at. GroupRunState: group_id/name, model_ids_ordered, participants Vec<ParticipantRunState>, leader_id Option, history Vec<HackathonMessage>, status Pending/Running/Completed/Failed/Locked, final_output Option, consultation_counts HashMap. No version field, no checkpoint_version, no next_step persistence, no pause_reason.

No general session checkpoint exists. Leader/participant turn state, pending user_input (context_manager.pending_user_input), history/context needed to continue, and next decision are all in-memory only.

---

## 4. Existing Recent-Session Loading Path

```
Sidebar.tsx:17 loadSessions() → invoke('get_session_list') → JSON.parse → Session[] {id, project_brief, session_type, created_at, status}
  render: sessions.map(si onClick => setSelectedSessionId(session.id))  // line 29
  selected highlight via selectedSessionId === session.id
  ctx menu: Rename → rename_session, Export → export_blueprint{format, session_id}, Details → get_session_details, Delete → delete_session

NO transcript fetch, NO blueprint fetch, NO session-details auto-load, NO transcript/history reconstruction, NO checkpoint load, NO status restore, NO stale guard.
App.tsx renders views based on sessionStatus (idle/setup/priming/running…), not selectedSessionId. So clicking a recent session only changes highlight, does not load that session’s transcript, blueprint sections, or leader/blueprint state. Details popup uses get_session_details but is toast-only.

Stale risk: rapid clicks setSelectedSessionId A → B → C synchronously; if we add async loads without request_id guard, late A/B responses can overwrite C (no session_id/turn_id filtering, no abort).

File: Sidebar.tsx:29, useAppStore.ts:242 setSelectedSessionId, commands.rs:1450 get_session_list, 1678 get_session_details, 1421 get_transcript, 1762 recover_session (only replays blueprint, not transcript).
```

Bug confirmed: reported “clicking a recent session does not load that session correctly” is true — current handler does not load at all.

---

## 5. Existing Stop/Abort Behavior

- Frontend: `InputBar.tsx:25 stop()` → `invoke('abort_session')`; button shows Square (stop) when `sessionStatus === running||paused`, ArrowUp otherwise. No pauseRequested state, no resume transformation. `Sidebar newSession` does clearSessionState + setSelectedSessionId(null) + setSessionStatus('setup'); ActiveView manual response uses provide_manual_model_response.

- Backend `abort_session` (commands.rs:449): immediate hard cancel — stores session_active false, hackathon_cancel true, per-run cancelled true, clears ask_user_tx, tries nav_tx SessionAborted, orch=Ended, emit ended. Participants in-flight are abandoned (wait_for_response will get SessionAborted and error out). No graceful boundary, no response preservation after Stop, no checkpoint.

- `pause_session`/`resume_session` (426-446): flip OrchestratorStatus Paused/Running and emit status only. Not wired to InputBar, not persisted, not checkpointed, backend does not actually suspend the loop — loop continues unless abort is sent.

Gap vs. required “graceful pause”: need PauseRequested → finish atomic movement → persist checkpoint + next_step → Paused → Reload/Resume.

---

## 6. Existing AgentBrain Decision Path

```
response_router::run_agent_loop
  iteration++,
  emits agent-state-change consulting,
  wait_for_response(leader_id, turn) captures leader_response (with drain_stale, ChallengeDetected 600s resume handling, ManualResponse, SessionAborted)
  context = iteration, selected ids, leader, deepseek flags
  mem_ctx = memory_context (built via memory_store.build_memory_context, injected only into brain, not participant WebViews)
  decide via AgentBrain::decide_with_source(leader_response, context, mem_ctx)
    AgentBrain::decide: primary client 60s, fallback once, 3rd fail → switch to agent_brain_2 permanently
    POST {base_url}/chat/completions {model, messages:[system+DECISION_JSON_CONTRACT, user: leader_response+context+memory_context], max_tokens:1024}
    strip fences, find '{', parse AgentDecision enum rename_all snake_case
    Variants: Route{target_model,prompt}, Blueprint{section_title,section_content}, Continue, Complete, RouteCompare{models,prompt}, AskUser{question,options,allow_custom}
  on decide fail: inc brain_fail_count, emit decision_failed, fallback: route deepseek or blueprint or bounded continue (max 1); unclassified_count tracks
  on success: reset brain_fail_count, emit brain-status, update active_brain
  match decision:
    Route → emit route_started+agent-routing, memory routing fact, inject_and_wait_with_retry to participant (IMP-2 retry 3× exponential 2,4,8s, W1-C empty-shell no-retry, cooldown 60s, model_health), fallback to failure notice, inject return_prompt to leader
    RouteCompare → sequential per-model route via inject_and_wait_with_retry, combined "[X said:]" to leader
    Blueprint → upsert_section via run_blocking, memory record, emit blueprint-section-added, ack to leader
    Continue → boss-message + "Please continue." to leader
    AskUser → oneshot tx in ask_user_tx, emit agent-ask-user, await rx, inject answer
    Complete → mark session_complete=true, emit session-complete, return Ok (loop exits)
  loop
```

Brain is leader-only; hackathon not represented. No HackathonDecision variant in AgentDecision; hackathon has isolated HackathonDecision enum (Route/Submit) used only inside hackathon.rs groups.

---

## 7. Existing Leader Routing Path

- Invocation: same as AgentBrain decision above; `inject_and_wait_with_retry` handles participant side (navigate_agent_window to saved conversation URL or base_url, begin_active_turn, inject_to_window with wait_ready=true, confirm_active_submit with retry_active_submit bounded, wait_for_response with stale draining, challenge handling, model_health).

- Leader injection: `inject_active_prompt(window, agent_id, prompt, turn, state, app, nav_rx)` — begin_active_turn, inject_to_window with wait_ready=false (leader already loaded), confirm_active_submit (bounded retries), mark_active_waiting, emit active-turn-state.

- Stale protection: `wait_for_response` checks both agent_id && turn; `drain_stale_active_events` before turn1; but rapid session switch can still cause late A response to mutate B if session_id not checked (only agent+turn checked). `response_router::should_retry_after_failure` uses diagnostics.page_state_hint to avoid empty-shell hammering.

- Leader failure: no automatic fallback (leader is single). Participant failure → failure notice injected, leader continues. Rate-limit classification via `ErrorKind::RateLimit` sets cooldown.

Gap: user message injection via `context_manager::set_pending_user_input` (commands.rs:485 user_input) is stored but response_router never reads pending_user_input during active routing; context is built only from leader_response + static session context + memory_context. User message queued but not surfaced before next leader decision.

---

## 8. Existing Hackathon Integration

Persisted `HackathonConfig` {groups: GroupConfig{id,name,model_ids,selected}, models: ModelConfig{id,model_name,base_url,api_key,group_id}, max_questions_per_teammate Option<u32> (1|2|3|5|Unlimited), enabled bool} default max 3 enabled false. Validates duplicate ids/names, empty names, URL http(s) parse, api_key non-empty, group membership, cap values. Stored via `settings_store.get/save_hackathon_config` JSON key `hackathon_config`, with api_key preservation on empty round-trip.

Transient `HackathonRunState` created on `send_hackathon_invitations` (fan-out JoinSet per model, 15s timeout, live-updates via hackathon-invitation-update per model, hackathon-group-status + invitations-complete after all join, responder sort preserving order, zero-live → Locked). `run_hackathon(task_brief)` creates/executes groups concurrently via JoinSet per group, each `run_single_group` hierarchical loop: private history, leader selects via select_leader/fallback_leader, is_route_allowed checks self-route, membership, liveness, cap, Unlimited never blocks, safety cap 20 rounds emergency, invalid decision → correction injection retry twice then leader fallback, teammate failure → live_set remove but continue, leadership fallback preserves history. Emits hackathon-run-started, hackathon-group-output per group, hackathon-complete with combined `format_report` `[Hackathon Group: Name]` + `=== End Hackathon Results ===`.

Frontend: SetupView toggle defaults OFF (additive), persists enabled via save_hackathon_config, opens HackathonMiniWindow. Window: toolbar Add model/New team, columns (surface2, border, radius12, max-height 290, flex-wrap), column head check (16px), status chips live/pending/dead, rows rank/mono + name + host meta + state ok/spin/dead, hover reveals up/down + more (delete), degraded opacity .6 for locked, note warn, rounds select 88px (1,2,3,5,Unlimited default 3), footer Cancel/Send invitations/Go. Popups 340px overlay blur for add model (model_name, base_url, api_key, team select) single-model only, new team. `useAppStore` hackathonConfig (safe) + hackathonRun + hackathonOpen. Listeners 6 events with run_id filtering.

Gaps vs batch:
- No bulk model registration (single model_name field, no + control)
- Rounds is dropdown, not numeric input
- Team card member list not independently scrollable in practice (flex-wrap columns, max-height 290 on col but host page may overflow; header/footer not sticky; 50 models would make window tall not per-card scroll)
- No participant checkboxes — responders not selectable/deselectable; Go uses all confirmed silently
- No server-trusted selection validation (backend trusts config, not selected roster)
- No checkpoint/persistence of run state beyond transient
- No mid-session AgentBrain trigger verification
- Kimi URL regressed to kimi.ai (should be www.kimi.com) and readiness timeout doubled to 90/100 (undocumented) — both flagged P2

---

## 9. Required State-Machine Changes

Extend `OrchestratorStatus` (orchestrator.rs:22) and/or introduce `SessionCheckpointStatus` alongside, preserving existing Idle/Setup/Requirements/Running/Paused/Complete/Ended but adding explicit transitions:

```
Idle → Setup → Running → PauseRequested → Paused → Resuming → Running → Complete
                         ↓                ↓          ↓
                       (abort)          (abort)   (abort)
                                      → Ended → Idle
```

- `PauseRequested`: frontend requested via `request_pause` (replaces hard abort semantics for Stop when graceful needed). Backend sets orchestrator.status = PauseRequested (or new field `pause_requested: Arc<AtomicBool>` to avoid conflicting with simple enum), persists intent, but continues current atomic pipeline movement.

- Atomic boundaries defined by response_router loop iterations: checkpoint eligible AFTER `wait_for_response` completes + history/blueprint write finished, BEFORE next `brain.decide` or BEFORE next `inject_active_prompt`. I.e., after leader response captured, after participant response appended, after blueprint upsert, after AskUser answer injected. Never mid-injection or mid-wait.

- On PauseRequested detection at loop top, backend finishes current action, constructs `SessionCheckpoint` (see §12), validates, serializes, persists atomically via settings_store `checkpoint:<session_id>` (or new SQLite table), only then sets status Paused, emits session-status paused, clears resuming guard.

- `Resuming`: set when `resume_session` invoked while Paused; guard against duplicate resume via `resuming: Arc<AtomicBool>` compare_exchange or run_id check — second Resume returns Err("already resuming"). Resume validates checkpoint (belongs to session, structurally valid, next_step known, models still exist, credentials still present, run not active), restores context (history, pending_user_messages, next_step), continues from `next_step` without repeating completed route.

- Hackathon run cancellation remains separate (`hackathon_cancel` + per-run cancelled). Pause vs hard cancel distinguished: graceful pause preserves checkpoint; `abort_session` is hard cancellation (Ended, checkpoint not required, run discarded). `abort_session` must not destroy a valid Paused checkpoint unless explicitly requested.

- Add `pause_reason` enum if needed: UserRequested | RateLimit | ProviderFailure | SystemRecovery — do not overload single bool.

- Frontend Zustand `sessionStatus` mirrors backend (Idle/Setup/Priming/Requirements/Running/Paused/Complete/Ended) + transient `resuming` UI flag; never authoritative (backend owns via session-status events).

Implementation bounded: first extend orchestrator.rs AppState with checkpoint fields (checkpoint: Arc<Mutex<Option<SessionCheckpoint>>>, pause_requested, resuming flag, last_checkpoint_version), keep existing session_active guard.

---

## 10. Required IPC Changes

| Command | Current | Required | Rename | Return |
|---|---|---|---|---|
| send_hackathon_invitations | exists | keep, but add server-trusted selection validation on run_hackathon | — | String JSON {run_id} |
| run_hackathon | exists (task_brief verbatim) | extend to accept `selected_participant_ids?: string[]` or derive from UI selection; validate against live responders, group membership, current run, and configured models; reject nonresponders/failed/deleted/other-group/stale-run ids | rename_all snake_case, arg `task_brief` + `selected_ids` | String JSON {run_id,report} |
| save_hackathon_config | exists | validate bulk models individually per §9 — shared base_url/api_key/group expanded to N records before validation; reject empty model_name, duplicates globally or group-local per existing rule | rename_all | void |
| get_hackathon_config | exists | keep (safe DTO) | — | String JSON |
| get_hackathon_run_state | exists | keep | — | String JSON |
| cancel_hackathon_run | exists | keep (hard cancel) | — | void |
| user_input | exists (sets pending_user_input) | add validation + session_id/run_id stamping; ensure session_active true, return Err if no active session; store with session_id to prevent stale injection | rename_all | void |
| request_pause (new) | missing | graceful pause: set pause_requested=true, return once Paused persisted (or async with event). Must not be confused with abort_session | rename_all pause_requested | void or String checkpoint_id |
| resume_session | exists (flip) | strengthen: load checkpoint, validate, set Resuming guard, continue exactly next_step, idempotent | — | void |
| get_session | existing get_transcript/get_session_details | keep, but fix Frontend to actually invoke them on recent selection; add `get_session_transcript` with explicit session_id to avoid active-session-only bug | — | String JSON |
| get_checkpoint | missing | `get_session_checkpoint` → String JSON checkpoint-or-null, versioned, secret-free | — | String |
| select_recent_sessions | missing | extend Sidebar context menu with Select All; requires selection state in store | — | — |

Events:
- Existing hackathon-run-started / invitation-update / group-status / invitations-complete / group-output / complete remain, must keep payload field names matching IPC.md.
- `session-status` already carries `{status, session_id, setup_generation, selected_leader_id, selected_agent_ids, setup_order}` — extend for Paused: include checkpoint_id/phase/next_step metadata via `session-checkpoint` event `{checkpoint_id, phase, session_id, run_id, next_step}` already exists for toast, but need richer.
- `rate-limit-reached` already exists for participant 429 — need new `rate-limit-choice` flow reusing AskUser oneshot pattern (two options: continue/temporary stop). Do not block backend indefinitely; use oneshot with take().

All JSON-string commands return via `serde_json::to_string` and frontend `JSON.parse`; plain-string commands (`get_prompt_template`, `export_blueprint`) remain not parsed. Verify registration in `main.rs` generate_handler! for every new command.

---

## 11. Required Frontend Changes

- **HackathonMiniWindow — bulk registration:** Replace single `model_name` input with list `pendingNames: string[]`. UI: model-name field + small `+` button next to it; pressing `+` pushes trimmed non-empty name into `pendingNames` list displayed below field, each with × remove (does not affect saved models). Base URL, API key, team fields shared above. On Save, expand `pendingNames + last field if non-empty` into N `HackathonModelConfig` entries each with same base_url/api_key/group_id but distinct id (`hk-m-${Date.now()}-${i}`) and individual model_name. Validate each non-empty, reject duplicates within batch and against existing models (global or group-local per current validate rule), prevent empty entries. Persist atomically: `save_hackathon_config` with expanded models; backend still validates per-model. Keep api_key never echoed beyond transient `apiKeyMap`; preserve empty→keep-old behavior.

- **Maximum questions:** Replace `<select>` with numeric `<input type="number" min="1">` plus Unlimited checkbox/toggle. Value typed by user. On change validate: reject 0, negative, NaN, non-numeric garbage; if Unlimited checked → null stored; else parse int, must be >=1 (design allows 1,2,3,5 but spec now says numeric free; keep validation relaxed to >=1 integer, but still reject unsupported huge values that could cause infinite loop — cap validated, safety cap 20 remains internal). Keep internal emergency limit independent.

- **Scrollable team cards:** `hk-col` already max-height 290, flex column, but `hk-col-body` must be independent scroll container: ensure `flex:1; min-height:0; overflow-y:auto; scrollbar-gutter:stable;` header `.hk-col-head` stays sticky (`position:sticky; top:0; z-index:1;`), footer controls remain outside scroll. Add text-overflow handling: `.hk-row-name` and `.hk-row-meta` `min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap` with long model names; scrollbar thin styled. Do not make entire `hk-modal` scroll — only per-team list. Test with 50 models per team.

- **Invitation participant selection:** After `send_hackathon_invitations` → responders shown per `hackathonRun.participants` status confirmed/failed. Add checkbox per responder row (only enabled when status confirmed). Default selected = all confirmed (or previously selected preserved). State: `selectedParticipants: Set<string>` per run_id, persisted in store alongside run. UI: checkbox next to each responded model; nonresponders/failed cannot be checked; failed shows dead icon, cannot select. Footer Go validates every selected group has ≥1 selected; zero-selected group becomes inactive/locked rather than runtime panic; 1-participant group edge documented (runs with single leader fallback semantics, no teammate consult). On Go, send explicit `selected_ids` to backend `run_hackathon`; backend validates server-side against current run/group/live/failed, rejects staleness/cross-group.

- **Sidebar recent sessions:** Fix `onClick` to actually load: `invoke('get_session_details', {session_id})` + `get_transcript` (with explicit session_id) + `get_checkpoint` (if exists) + set blueprintSections via `blueprint-section-added` replay or direct set; handle rapid clicks with requestId guard (`loadSeq` ref increment, check before setState) to prevent stale overwrite. Do not auto-start new session, do not clear selectedSession unintentionally. Also add Select All in context menu.

- **InputBar pipeline:** Keep `user_input` IPC but ensure backend routes to leader context correctly. Change placeholder/button behavior: when `sessionStatus===running`, InputBar’s main textarea becomes active pipeline input; submit → invoke `user_input`; backend queues to `context_manager.pending_user_input` or dedicated `pending_user_messages` deque stamped with session_id/run_id. Leader injection point: `response_router` before next `brain.decide`, check pending messages and inject as `[User: ...]` into leader’s next decision context (preserving history, after current completed turn, before next leader decision). Do not bypass AgentBrain or inject directly into WebView.

- **Stop/Resume transformation:** `InputBar` button currently `abort_session`. Extend to graceful pause: when `running`, click → `request_pause` (not abort), show intermediate spinner “Pausing…”, wait for backend `session-status Paused` confirmation before switching label to Reload/Resume. When `paused`, button reads RELOAD/RESUME and invokes `resume_session` (idempotent, disables while resuming). Do not switch label merely on click; mirror backend state.

- **Rate limit popup:** Reuse existing `RateLimitOverlay` / `AskUser` pattern: backend emits `rate-limit-reached` with agent_id, frontend shows modal with two choices Continue with existing members / Temporary stop. Choices map to `rate_limit_decision` or pause request; backend handles continue (mark unavailable, fallback, preserve history) vs temporary stop (pause_requested + checkpoint).

- **Recent chat Select All:** Inspect current right-click menu (Sidebar ctx: Rename/Export/Details/Delete). Add Select All capability: checkbox selection mode with indeterminate, Support Select All / Deselect All, individual toggle, Delete selected bulk, ensure selection state survives rerenders via Zustand but does not leak into newly created session.

All event listeners clean up on unmount; no memory leak.

---

## 12. Required Persistence Changes

Current checkpoint does not exist. Implement minimal versioned checkpoint stored in `settings.db`:

```
key = "checkpoint:<session_id>"
value = JSON SessionCheckpoint {
  checkpoint_version: u32 = 1,
  session_id: string,
  run_id: string,
  turn_number: u32,
  phase: string,          // before_leader_decision | after_route | after_response | etc.
  leader_id: string,
  target_participant: string|null,
  completed_action: string,   // last completed response id/hash
  last_completed_response: string|null, // or omitted, length-limited
  next_step: enum { LeaderDecision, Route{target,prompt}, Submit, AskUser, Continue, Complete },
  history_snapshot_ref: string|null, // or inline small history for recovery (no secrets)
  pending_user_messages: Vec<string>, // session/run stamped
  pause_requested: bool,
  pause_reason: string,   // UserRequested | RateLimit | ProviderFailure | SystemRecovery
  created_at: string,
}
```

Rules:
- Construct complete checkpoint, validate (schema, next_step known, belongs to session), serialize, persist atomically via `settings_store.set()` inside single lock scope, only then mark Paused.
- Never persist api_key, Authorization, browser cookies, provider secrets — only ids/refs; credentials retrieved from `settings_store.hackathon_config` or `agent_brain` on resume.
- Atomic write: single INSERT OR REPLACE; half-written state avoided because SQLite is atomic for single row; checkpoint_version included for forward compatibility; unknown future version rejected.
- Reading: `get_session_checkpoint(session_id)` parses JSON, validates version, returns null if absent; resume path validates before use.

Alternative if settings.db row too large for history: store transcript-derived history via transcript_store, keep checkpoint minimal (next_step + run_id + turn + pending messages). History restoration via `get_transcript(session_id)` + blueprint replay, not duplicate storage.

Crash recovery: if process crashes before checkpoint flush, only last persisted checkpoint recoverable; document that guarantee (checkpoint at safe boundaries, not after every webview event).

Hackathon state: not required to be resumable initially — persist its status (Locked/Failed/Completed) as part of checkpoint’s group summary, not full histories; stale hackathon run not auto-resumed unless explicitly requested.

---

## 13. Test Strategy

Add deterministic, network-free tests (no live API credentials):

- `checkpoint_serialization` round-trip, version field preserved, secret fields absent
- `checkpoint_validation` rejects unknown version, missing session_id, unknown next_step, mismatched session
- `pause_state_transition` Idle→Running→PauseRequested→Paused→Resuming→Running
- `resume_state_transition` and idempotency: second Resume while Resuming does not spawn second task
- `next_step_persistence` — completed route’s response preserved, next_step = LeaderDecision, not duplicate route
- `stale_run_rejection` — event with old run_id does not mutate new run (already helper stale_run_rejection_concept)
- `stale_session_rejection` — user message / late response with old session_id does not affect new session (check session_id + turn)
- `rate_limit_classification` — 429 → RateLimit kind, not auth/network; wrong api_key (401) not treated as rate limit
- `rate_limit_participant_removal` — teammate marked unavailable, not routed again, live_set updated
- `leader_fallback_after_rate_limit` — leader 429 triggers fallback_leader, history preserved
- `participant_selection_validation` — rejects selecting model from another group, nonresponding, failed, deleted, stale-run
- `bulk_model_validation` — empty name rejected, duplicate within batch rejected, duplicate against persisted rejected, per-model url/key/group validated
- `duplicate_model_validation` — already covered by config validate
- `numeric_cap_validation` — rejects 0, -1, NaN, "foo", accepts 1,5, accepts null Unlimited, emergency cap 20 independent

Reuse existing `hackathon.rs` 14 tests (ordering, fallback, zero-live, sort, cap, unlimited, invalid route, report, parse etc.) as baseline; add new tests in `hackathon.rs` and `commands.rs` (checkpoint) with `#[cfg(test)]` and pure logic where possible; async integration tests using `tokio::task` + oneshot/mpsc mocks without real reqwest.

Manual/Tauri: simulate Scenario 1–13 from spec (bulk 10 models, 8 respond 2 fail 3 deselect →5 participants, 50 models scroll, rapid session A/B/C switch →C wins, user message during DeepSeek routing preserved, pause during network preserves response and next_step = leader_decision, rate limit continue vs pause, resume exact next step, resume twice idempotent, session switch while paused/resuming no cross-mutation, AgentBrain capability check).

Verification per loop: `cargo check`, `cargo test hackathon`, `npm run build`, `git diff --check`, plus grep audits for blocking_lock / unwrap / api_key.

---

## 14. Risk Register

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| blocking_lock in async | P0 | grep before merge, no new std::sync::Mutex lock across await; use run_blocking for DB, clone-before-await for AppState | backend |
| tokio::sync::mpsc in on_navigation | P0 | on_navigation captures only std::sync::mpsc SyncSender, never tokio | browser_backend |
| unwrap/expect in live path | P0 | grep, use map_err, saturating_add, ok()?; only tests/setup init may unwrap | all |
| api_key leakage via DTO/event/logs/checkpoint/React | P0 | to_safe omits key, events never include key, redact_api_key_logs, checkpoint stores ids only, frontend apiKeyMap transient only, diagnostics snapshot excludes hackathon keys, test hackathon_safe_omits_keys | security |
| stale async response mutates wrong session/run | P0 | check run_id + session_id + turn_id at every async boundary (invitation tasks, group execution, wait_for_response, user injection, sidebar loadSeq, frontend listeners filter run_id) | concurrency |
| checkpoint half-write corruption | P1 | single SQLite row atomic, validate before mark Paused, frontend mirrors backend only after emit | persistence |
| WebView over-expansion (2 limit) | P0 | HackathonMiniWindow stays React modal, never WebviewWindowBuilder, no extra webview, navigation reuses existing windows | arch |
| memory <2GB breach via parallel groups | P1 | concurrency is network-bound, per-group history small (KB), reqwest per-call client, JoinSet bounded, no WebView per model | arch |
| rate limit misclassification | P1 | classify via http status 429 only, not 401/403; auth failure shows distinct message, not “Continue with members” | routing |
| pause faked as frontend bool | P0 | backend owns PauseRequested→Paused, checkpoint persisted before status change, UI updates only after emit | state |
| resume duplicate run | P0 | AtomicBool resuming guard + run_id check + AtomicBool cancelled, second resume returns Err | concurrency |
| duplicate model ids / empty names in bulk | P1 | frontend prevents empty, backend validate rejects empty + duplicate global/group-local, atomic batch | hackathon |
| infinite loop via max_questions 0 or Unlimited | P1 | input rejects 0/neg/NaN/garbage, Unlimited = null, safety cap 20 independent | hackathon |
| recent session stale overwrite | P0 | requestId monotonic guard in Sidebar load, backend session_id check, frontend JSON.parse correct per RISK-IPCPARSE | frontend |
| AskUser / rate-limit oneshot hang | P1 | use take() to clear ask_user_tx, backdrop/Escape send "Cancelled", rate-limit choice uses same oneshot with timeout | IPC |
| document drift | P2 | source is truth, verify claims via direct read + cargo check + npm build, not doc status | process |
| pre-existing dirty work overwrite | P1 | never reset/clean/stash, preserve 7 pre-existing modified files + 2 designs, tag checkpoint untouched | git |
| Kimi URL + readiness double timeout regression | P2 | revert to HEAD values 45s/50s and https://www.kimi.com/ unless separate approved maintenance commit | regression |

---

## 15. Unresolved Questions

1. **Mid-session Hackathon trigger:** Design §10 left mechanism unresolved. Least invasive is existing `run_hackathon` callable any time (including mid-session when session_active true) without new AgentDecision variant. Automatic leader-initiated trigger would need new decision contract (`AgentDecision::Hackathon{task_brief}`) — requires prompt change and brain re-training, major redesign. Decision: verify AgentBrain capability (does prompt describe Hackathon?), expose command for manual trigger, document automatic trigger as deferred (not fake). If brain prompt does not mention Hackathon, leader cannot trigger; we should add advisory line to system prompt without adding new variant yet, and note exact blocker.

2. **Group size floor:** Design raised but not answered whether single-member groups allowed. Implementation currently allows (permits 1, locking only if zero live). No UI floor enforcement — accept permissive, document as chosen. Single live member still viable (leader only, no teammate consult, just submits).

3. **Checkpoint storage choice:** settings_store key vs new SQLite table vs file. Key is minimal (generic table, zero migration) but stores large JSON in single row; new table would be cleaner but needs migration. Choose key for now with version field, note migration path if size grows.

4. **Numeric cap allowed range:** Design candidate 1/2/3/5/Unlimited, but new requirement says numeric input free. Should we accept any positive integer (e.g., 10, 100) or keep allowlist? Accepting large values (e.g., 100) plus safety cap 20 still bounds total rounds but could allow longer teammate consult loops. Decision: accept any integer >=1 (reject >100 to prevent accidental huge input causing long runs but unlikely due to safety cap). Keep Unlimited as null.

5. **Rate-limit exact UX:** Existing modal design patterns use overlays with backdrop click + Escape → Cancelled. Rate-limit popup should follow same: use AskUser oneshot pattern, not browser alert, explicit two buttons + backdrop dismiss handling (dismiss = ? treat as Continue or cancel? Must not block backend).

6. **Select All persistence:** Should selection survive rerenders but not leak into new session — store selection in Zustand with session-scoped key, clear on newSession().

7. **History size for checkpoint:** Full history could be large (many turns). Persist only next_step + pending messages + turn + leader references, reconstruct history via transcript_store rather than full inline snapshot to keep checkpoint small and secret-free.

8. **Security reviewer approval for plaintext hackathon keys:** Current stores plaintext in settings.db matching brain precedent; encryption deferred per pre-audit H. Need explicit approval before introducing SessionVault encryption for hackathon.

---

**Next Loop:** LOOP 1 — Fix recent-session loading with stale guard (highest reliability), verify via cargo check + npm build + diff, then proceed loop-by-loop per spec §61.
