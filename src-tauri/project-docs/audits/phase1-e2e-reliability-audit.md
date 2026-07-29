# Phase 1 end-to-end reliability audit

## Loop 0 — reality and hygiene

- Branch: `phase1-e2e-reliability-fix`.
- Starting HEAD: `58b3fc0 fix: make agent brain routing deterministic`.
- Starting source tree: clean. The only initial modification was the tracked
  generated binary `src-tauri/target/debug/consensus-arena`; it was restored
  before creating the safety branch.
- Source command count: 42 `#[tauri::command]` definitions. The quick
  registration count command needs a source-list comparison in Loop 4; its
  initial line-pattern count was not reliable.
- `AppState` field count is not inferred from documentation; it will be
  recorded from the complete Loop 2 source read.
- Baseline `cargo check`: completed successfully.
- Baseline `npm run build`: reached `tsc && vite build`; the execution harness
  did not return Vite's final exit/result, so this is not recorded as PASS.
- `git diff --check`: clean before audit-file creation.

### Findings

1. Generated `dist/` and `src-tauri/target/` outputs are tracked and can make
   an otherwise clean source worktree dirty after verification.
2. Documentation counts are already known to be stale; source is authoritative.

### Next files

Loop 1: `ARCHITECTURE.md`, `BACKEND.md`, and `FRONTEND.md` plus the already
read root rules, decisions, IPC, process, and Phase 1 system audit.

## Loop 1 — Phase 1 intended runtime contract

1. Setup injects priming prompts but never auto-submits them; a same-agent
   browser send signal or explicit manual confirmation advances setup.
2. After `setup-complete`, active leader and participant prompts are injected,
   integrity-checked, auto-submitted, and correlated by `agent_id` and turn.
3. Captured leader text is supplied to the configured agent brain, which must
   select Route, RouteCompare, Blueprint, Continue, Complete, or AskUser.
4. Route uses the single shared nav WebView, then returns the captured
   participant result to a new persistent-leader turn.
5. Blueprint decisions persist a section and emit `blueprint-section-added`;
   this is the primary ActiveView content.
6. Setup has manual-send/manual-confirm/retry recovery. Active turns have a
   manual pasted-response recovery path validated against exact agent and turn.
7. Stop emits/consumes an abort event; recovery replays persisted blueprint
   sections and does not restart an autonomous loop.
8. Browser diagnostics must distinguish injection, auto-submit success/failure,
   capture, readiness, and security/login blockers without prompt secrets.
9. Frontend is blueprint-first: live model text belongs only in the status
   drawer; active agent/turn and manual recovery status must remain truthful.

### Findings

1. The prior audit documents active submit retry and `Done`-without-response
   protection, but does not establish a successful live ChatGPT→DeepSeek run.
2. The immediate audit focus is therefore the complete correlation chain, not
   presentation changes or Phase 2 work.

### Next files

Loop 2: complete module reads for the 15 requested backend modules, nine
frontend workflow modules, and browser fixtures/tests; then a source-backed
module table and state-machine matrix.

## Loop 2 — source module map

| File | Intended / actual responsibility and owned state | Inputs → outputs/events | Failure modes | Status / source evidence |
|---|---|---|---|---|
| `main.rs` | Tauri startup and command registration; initializes `AppState`. | Tauri invoke → `commands::*`; startup emits memory health warnings. | Startup-only store init failures. | PARTIAL — registration must be compared mechanically in Loop 4. |
| `orchestrator.rs` | Owns `SessionConfig`, `Orchestrator`, and 17-field `AppState`. Selected leader/agents are `SessionConfig`; `setup_generation` is atomic. | `start_session`/runner state. | Stale docs claim fewer fields. | PASS — `SessionConfig::setup_order`, `AppState`. |
| `commands.rs` | Session lifecycle, std→Tokio nav bridge, command/manual fallbacks. | `start_session` emits `session-status`; `confirm_setup_agent`, `retry_setup_agent`, `provide_manual_model_response`. | Setup retry loop; bridge closure ends if receiver disconnects. | PARTIAL — `start_session`, `provide_manual_model_response`. |
| `session_runner.rs` | Manual priming/setup state machine. | `NavEvent` receiver → setup lifecycle events, conversation URL storage, `setup-complete`. | 50/120-second readiness/send waits; CAPTCHA pauses. | PARTIAL — `run_setup`, `wait_for_setup_ready`. |
| `response_router.rs` | Active turns, brain decisions, routing, blueprint persistence, completion. Owns local leader turn, DeepSeek consultation, unclassified counters. | `NavEvent` → `active-turn-state`, `agent-message`, routing/blueprint events. | 300-second response waits; retries; no global iteration bound. | PARTIAL — `run_agent_loop`, `inject_and_wait_with_retry`, `wait_for_response`. |
| `browser_backend.rs` | Two WebViews, generic browser script, arena URL parsing, diagnostics, active-turn state, nav-window recreation. | Browser `arena://` → `NavEvent`; diagnostics/events. | Site DOM changes, readiness/response extraction, closed nav window. | PARTIAL — `GENERIC_INIT_SCRIPT`, `inject_to_window`, `ensure_nav_window`. |
| `agent_brain.rs` | OpenAI-compatible decision client and JSON extraction. | leader text/context → `AgentDecision`. | provider/error/schema failure; fallback brain retry. | PARTIAL — `decide`, `extract_json_object`; one parser unit test. |
| `transcript_store.rs` | File-backed session/turn persistence. | runner facts → transcript DB. | DB lock/schema errors. | PASS — `TranscriptStore`. |
| `blueprint_store.rs` | File-backed blueprint sections. | Blueprint decision → persisted section. | DB lock/write error aborts active loop. | PARTIAL — `BlueprintStore::upsert_section`. |
| `memory_store.rs` | Phase 1 durable memory and reliability facts. | router events → bounded context/facts. | Non-fatal router memory errors. | PASS for ownership; not on critical browser path. |
| `settings_store.rs` | Brain/prompt/session-complete settings. | settings commands → persisted config. | synchronous DB errors. | PASS for ownership. |
| `session_vault.rs` | Conversation URL persistence. | setup URL → route target URL. | In-memory vault / DB failure is non-fatal. | PARTIAL — setup falls back to base URL. |
| `db_helpers.rs` | Blocking DB dispatch/retry. | std mutex DB closure → async result. | Retry exhaustion. | PASS — `run_blocking`. |
| `errors.rs` | Error kinds used by retry policy. | backend failures → classifications. | Misclassification affects retry. | PASS. |
| `App.tsx` | Selects view and mounts listeners/overlays. | Zustand session status → Setup/Priming/Active. | Status mismatch can show wrong view. | PARTIAL. |
| `useIpcListeners.ts` | Backend-event-to-store bridge and cleanup. Owns current response drawer updates indirectly. | lifecycle/active/blueprint events → Zustand. | Unlisted new diagnostics are ignored safely. | PARTIAL — `active-turn-state`, `agent-message`, `blueprint-section-added`. |
| `useAppStore.ts` | UI state: selected setup agents, active agent/turn, blueprint sections, drawer response. | listener actions → React views. | Client state is not source of truth. | PASS. |
| `SetupView.tsx` | Configures selected agents/leader and invokes start. | `start_session` snake_case payload. | Requires configured brain; no active pipeline logic. | PASS. |
| `PrimingView.tsx` | Manual setup confirmation/retry UI. | `confirm_setup_agent`, `retry_setup_agent`. | User can confirm only after visible send/response. | PASS. |
| `ActiveView.tsx` | Blueprint-first display and exact-turn manual response fallback. | `provide_manual_model_response`; blueprint/status display. | “No response” drawer is truthful but not diagnostic. | PARTIAL. |
| `InputBar.tsx` | Idle brief / Stop / user context. | `user_input`, `abort_session`. | Idle submit only changes view state. | PARTIAL, non-critical. |
| `SettingsPanel.tsx` / `Toast.tsx` | Brain config / feedback. | config invokes / visual toast. | Provider misconfiguration. | UNKNOWN for live browser outcome. |
| `browser-fixtures.html` and embedded Rust tests | Browser-script fixture coverage. | delayed enabled Send fixture. | No live ChatGPT/DeepSeek DOM coverage. | PARTIAL. |

### Explicit ownership map

- Leader, selected agents, setup order, session ID: `SessionConfig` created by
  `commands::start_session`; copied into spawned setup/debate runner.
- Setup generation: `AppState.setup_generation`, carried in setup diagnostics
  and `session-status` payload.
- Expected active agent and turn: `BrowserState.active_turn`, with matching
  `BrowserDiagnosticRecord.active_expected_agent_id/active_turn_number`.
- Nav receiver: fresh std channel in `start_session`, bridged into the sole
  Tokio receiver owned by the spawned runner.
- Browser diagnostics: `BrowserState.diagnostics`; bridge calls
  `record_nav_event` before async delivery.
- Blueprint sections: `BlueprintStore` is source of truth; Zustand receives
  `blueprint-section-added` and renders it in `ActiveView`.
- Current response drawer: Zustand `currentAgentResponse`, written by
  `agent-state-change` and `agent-message` listeners.
- Manual setup confirmation: `confirm_setup_agent` creates a
  `NavEvent::SetupManualConfirmed`; manual active fallback validates exact
  `BrowserState.active_turn` then supplies `NavEvent::ManualResponse`.
- Agent decisions: `AgentBrain::decide`, with local router fallback state.
- Route target resolution: `response_router::resolve_selected_agent_id` maps
  canonical IDs or display names to an entry in `SessionConfig.agent_ids`.
- Nav recreation: `browser_backend::ensure_nav_window` is called from
  `inject_and_wait_with_retry` before participant navigation.

### Loop 2 findings

1. The end-to-end path is deliberately split across a synchronous browser
   channel, bridge thread, setup runner, and active router; correlation must be
   audited at every boundary, not inferred from UI status.
2. `session_vault` URL persistence is best effort. A participant route may use
   the base URL, which can expose the priming conversation rather than the
   intended preserved conversation if URL capture failed.
3. Active routing has no explicit global progress/iteration ceiling. Repeated
   valid `Continue` decisions can remain live indefinitely.

## Loop 3 — actual end-to-end state machine

| Step | Source / state / events / correlation / timeout-fallback | Actual behavior | Status |
|---|---|---|---|
| A–B | `SetupView.start` invokes snake_case `start_session`; `commands::start_session` validates agents, creates `SessionConfig`, resets state. | Leader/agents/session ID are canonical at creation. | PASS |
| C–D | `create_windows` destroys stale labels then creates `arena-leader` + `arena-nav`; `setup_order` is leader first. | Exactly two WebViews in normal creation. | PASS |
| E | `run_setup` navigates each selected agent and injects priming without auto-submit. `setup_generation`, agent ID, window kind in diagnostics. | Leader gets persistent leader window; all participants reuse nav. | PASS |
| F | `wait_for_setup_ready`, 120-second send proof; `confirm_setup_agent`/`retry_setup_agent`; setup failures wait for `ResumeRequested` or manual confirmation. | Manual send is required and supported. | PARTIAL — confirmation can advance after user assertion, by design. |
| G–H | `run_setup` emits `setup-complete`; spawned task changes orchestrator to Running; listener sets Zustand `running`; App chooses ActiveView. | Active view transition depends on event delivery. | PASS |
| I–J | `run_agent_loop` drains stale events and calls `inject_active_prompt` with leader turn 1. Browser state begins active turn. | Prompt is injected into existing leader conversation. | PASS |
| K | `inject_to_window(..., wait_ready=false, auto_submit=true)` invokes generic injection script. | Active auto-submit is requested on leader and participant paths. | PARTIAL — live DOM selector/retry success remains unproven. |
| L | `wait_for_response(leader, turn)` accepts only same agent/turn `Response` or `ManualResponse`; `Done` is marker only; 300 seconds. | Captured leader response emits `agent-message`. | PARTIAL — live extractor is unresolved symptom source. |
| M–N | `AgentBrain::decide(leader_response, context, memory)`; balanced JSON parse; local fallback routes/deposits blueprint/one Continue. | Current brief/keywords can force one DeepSeek route. | PARTIAL — provider behavior not live verified. |
| O | `AgentDecision::Route` resolves display/canonical target against selected agents and emits `route_started`, `agent-routing`. | DeepSeek target becomes canonical `deepseek`. | PASS in source. |
| P | `inject_and_wait_with_retry` calls `ensure_nav_window`, resolves saved/base URL, `navigate_agent_window` shows/focuses nav. | Missing nav handle creates/reuses one nav window. | PARTIAL — prior diagnostic showed nav missing; recovery not live verified. |
| Q–R | Participant `begin_active_turn(deepseek, iteration)` then `inject_to_window(..., true, true)`. | DeepSeek prompt uses same active auto-submit path. | PARTIAL — could still be left at priming if navigate/readiness/injection fails. |
| S | Same 300-second correlation wait; retries injection/navigation up to 3 with backoff; exact response/manual response accepted. | DeepSeek response should emit `agent-message`. | PARTIAL — current live failure reports this is not occurring. |
| T–U | Router injects `[Response from deepseek]` and explicit blueprint-finalization instruction into new `next_leader_turn`. | Leader receives a fresh sequential turn. | PASS in source. |
| V | Leader waits under the new turn number. | Same capture constraints as step L. | PARTIAL. |
| W | Blueprint decision constructs `BlueprintSection`, writes via `run_blocking`, emits `blueprint-section-added` then `blueprint_emitted`; listener upserts Zustand. | Main content receives persisted section. | PASS in source; dependent on step V/M. |
| X | `Complete` emits `session-complete`; `abort_session` sends `SessionAborted`, sets ended; recovery replays sections only. | Active flag reset at spawned-task exit. | PARTIAL — Complete can occur without a router-enforced prior blueprint. |

## Broken-contract matrix

| Step | Expected | Actual | Source evidence | Likely symptom | Severity | Proposed fix batch |
|---|---|---|---|---|---|---|
| K/L | Every injected active prompt is submitted and produces a correlated response. | Submission is requested, but success depends on generic DOM detection; no source proof that ChatGPT's live DOM produces `Response`. | `inject_to_window`, `build_inject_js`, `wait_for_response`. | “chatgpt is consulting” / empty drawer. | CRITICAL | A |
| P–S | Nav must hold the routed DeepSeek conversation before its active prompt submits/captures. | URL persistence is best-effort; route falls back to base URL and browser recovery has not been live verified. | `run_setup` URL storage; `inject_and_wait_with_retry`; `ensure_nav_window`. | DeepSeek remains on “ready for proposal.” | CRITICAL | A |
| L/S | Browser response events must retain exact active identity and turn across nav/script timing. | Router validates correctly, but extraction/capture timing across shared nav remains unverified. | `NavEvent::Response`, `wait_for_response`, `BrowserState.active_turn`. | Valid model response never advances pipeline. | CRITICAL | A |
| M/N | Brain failure must not deadlock and DeepSeek-once must be exactly once. | Fallback is bounded for parse errors, but keyword rule can force routing broadly; no global progress guard exists. | `run_agent_loop` fallback helpers. | Repeated routing/Continue cycle. | HIGH | B |
| W/X | Blueprint must exist before truthful completion. | `Complete` does not check `blueprint_titles` nonempty. | `AgentDecision::Complete` arm. | Session completes with no primary content. | HIGH | B |
| G/H | UI must expose active expected agent/turn and route truth. | Listener updates state, but new decision/route diagnostics have no frontend listener; drawer only sees response after capture. | `useIpcListeners`, `ActiveView`. | User sees generic waiting state. | MEDIUM | B |

### Loop 3 checkpoint

Files read for this loop: all requested module groups were enumerated and
source-traced; critical-path function bodies read in `commands.rs`,
`session_runner.rs`, `response_router.rs`, `browser_backend.rs`, and the
requested React workflow modules. No source was edited.

Exact dirty state: only this audit file is modified.

Next required run: Loop 4 IPC/event parity, then Loop 5 browser automation
deep audit. Do not repair before those matrices are complete.

## Loop 4 — IPC and event parity

### Command parity table

| Command group | Definition / registered | IPC / frontend | Casing / return | Status |
|---|---|---|---|---|
| `start_session`, `abort_session` | `commands.rs`; both in `main.rs` handler. | IPC documented; Setup/InputBar invoke. | snake_case args / unit. | PASS |
| `retry_setup_agent`, `confirm_setup_agent` | `commands.rs`; registered. | IPC documented; PrimingView invokes both. | `agent_id` / unit. | PASS |
| `provide_manual_model_response` | `commands.rs`; registered. | IPC documented; ActiveView invokes. | `agent_id`, `turn_number`, `response` / unit. | PASS |
| Diagnostic / transcript / health | `get_diagnostic_snapshot`, `get_transcript`, `get_agent_health`; registered. | IPC documented; Settings/Setup callers parse JSON where used. | JSON strings. | PARTIAL — diagnostic snapshot is not used on the active screen. |
| Brain/settings commands | save/get primary, fallback, secondary, prompt template; registered. | IPC documented; Setup/Settings invoke. | snake_case multiword / unit or JSON/plain strings. | PASS |
| Blueprint/session commands | export, list/details/delete/rename/recovery; registered. | IPC documented; Sidebar/Active callers. | mixed plain export / JSON list/detail. | PASS |
| Memory commands | all memory commands in `commands.rs`; registered. | IPC documented; MemoryPanel invokes. | snake_case / JSON or unit. | PASS |

The source contains 42 commands; prior documentation counts are stale. Exact
handler membership is source-owned by `main.rs::generate_handler!` and must be
mechanically diffed in the final IPC pass, but no critical-path command is
missing from that list.

### Event parity table

| Event | Backend source | Frontend listener / payload use | Status |
|---|---|---|---|
| `session-status` | `start_session`, abort/error paths. | `useIpcListeners`: status, setup order, selected agents. | PASS |
| `setup-agent-ready`, `setup-agent-complete`, `setup-agent-failed`, `setup-complete` | `run_setup` / setup recovery. | Priming state and running transition. | PASS |
| `setup-agent-recoverable` | Not emitted; source uses `setup-agent-failed` with `recoverable`. | No listener expects the former. | PASS |
| `active-turn-state` | `inject_active_prompt`, retry/capture paths. | Reads `event`, `agent_id`, `turn_number`, sets exact manual fallback target. | PASS |
| active subevents (`active_prompt_injected`, `active_prompt_submitted`, `active_submit_failed`, `active_waiting_for_response`, `active_response_captured`) | Browser diagnostic/active router. | One listener branches all documented subevents. | PASS |
| `agent-message`, `agent-routing` | response router. | Drawer response / live routing state. | PASS |
| `route_started`, `agent_brain_decision_*`, `blueprint_emitted` | response router. | No production listener. IPC documents them as diagnostics. | PARTIAL |
| `blueprint-section-added` | Blueprint router + recovery. | Store upsert consumes `section_id,title,content`. | PASS |
| `browser-diagnostic` | browser backend diagnostic updates. | Listener consumes agent/window/phase/url/message/error. | PASS |
| `agent-ask-user` | response router. | Listener opens AskUser overlay. | PASS |
| `debug-log` | development DebugPanel only. | Dev-only listener; deliberately absent IPC. | PASS |

### IPC broken-contract findings

1. New `route_started` and agent-brain decision diagnostics are emitted but not
   consumed by the production listener. This does not block routing, but it
   hides the distinction between “brain decided DeepSeek” and “browser route
   failed,” worsening the visible stuck-state diagnosis. Severity MEDIUM; Batch
   B.
2. `ActiveView` has exact expected agent/turn from `active-turn-state` and its
   manual command uses `turn_number` correctly. It lacks a direct view of
   auto-submit report details, relying on translated status text. Severity
   MEDIUM; Batch B.
3. No critical frontend listener waits for a non-emitted event. `setup-agent-
   recoverable` in the requested checklist is a naming mismatch only: real
   source/IPC uses `setup-agent-failed { recoverable: true }`. Severity LOW.

## Loop 5 — browser automation and response-capture audit

### Browser pipeline table

| Stage | Source | Actual/correlation/diagnostic | Status |
|---|---|---|---|
| Window create/navigate | `create_windows`, `navigate_agent_window` | Labels are fixed; identity is eval’d from `agent_id`; diagnostics register window. | PASS |
| Script ready/composer | `GENERIC_INIT_SCRIPT`, `wait_for_ready` | `arena://ready/{agent}` controls readiness; 50-second wait. | PARTIAL |
| Setup injection/send | `run_setup` inline injection + generic send detection | Injects only; `SendDetected`/manual confirmation completes setup. | PASS |
| Active injection | `inject_active_prompt` → `inject_to_window` → `build_inject_js` | `BrowserState.begin_active_turn` occurs before eval; agent/turn embedded in JS. | PASS |
| Active auto-submit | static `__caSubmitActivePrompt` called only with `AUTO_SUBMIT=true` | Active report includes agent/turn/success/method; setup is false. | PARTIAL — selector/hydration live reliability unproven. |
| Submit retry/report | generic helper retries enabled Send; `ActiveSubmitReport` records diagnostics. | Router waits for response even after report failure; UI offers manual paste. | PARTIAL |
| Response extraction | `build_inject_js::pollResponse` | Baseline before injection; latest selector text stable 2 seconds; emits agent+turn+text capped 8000. | PARTIAL — generic `.markdown/.prose` can select stale/non-message DOM. |
| Done marker | JS emits 200ms after response; router ignores it as non-completing. | Same agent/turn checked. | PASS |
| Stale handling | setup/active drains, router checks agent+turn. | Wrong agent/turn ignored. | PASS |
| Nav recreation/route | `ensure_nav_window`, `inject_and_wait_with_retry` | Reuse existing label or build exactly one nav then navigate/show/focus. | PARTIAL — base-URL fallback can lose setup conversation. |
| Manual bridge | `provide_manual_model_response` → `NavEvent::ManualResponse`. | Exact `BrowserState.active_turn` validation. | PASS |

### NavEvent handling table

| Variant | Producer | Setup / active consumption | Can complete setup / active | Status |
|---|---|---|---|---|
| `Ready` | generic script parser | readiness waits / ignored active | yes / no | PASS |
| `SendDetected` | generic script | setup proof / ignored active | yes / no | PASS |
| `Response` | active JS response poll | setup may treat as post-injection proof / exact router wait | yes / yes | PARTIAL |
| `Done` | active JS | setup proof / marker only | yes / no | PASS |
| `Error` | parser/readiness | fails matching setup / fails matching active | no / no | PASS |
| `ChallengeDetected`, `UnshowableUrl` | parser | setup recovery/error / mostly ignored by active wait | no / no | PARTIAL |
| `SendProbe`, `PromptInjectionReport`, `ActiveSubmitReport` | generic/injection JS | diagnostics/setup report / diagnostics only | no / no | PASS |
| `ResumeRequested` | captcha command | setup/readiness resumes | no / no | PASS |
| `SetupManualConfirmed` | command | setup completion/recovery | yes / no | PASS |
| `ManualResponse` | active fallback command | ignored setup / exact active wait | no / yes | PASS |
| `SessionAborted` | abort command | ends both waits | no / no | PASS |

### Browser broken-contract findings

1. `pollResponse` treats the final matching `.markdown` or `.prose` element as
   an assistant response. It captures a pre-injection baseline but does not
   establish author ownership for those broad fallback selectors. A DOM mutation
   elsewhere can look like a model response or prevent a changed assistant
   response from being selected. CRITICAL; Batch A.
2. Response completion is based on four identical 500ms polls. Streaming that
   pauses for two seconds can be captured early; a response longer than 8000
   characters is truncated before transport. HIGH; Batch A/B.
3. The response URL transports the entire response in an `arena://` navigation.
   Long encoded text can be rejected/truncated by the WebView/parser; `Done`
   then arrives but is deliberately non-completing, yielding the observed wait.
   CRITICAL; Batch A.
4. The fixture has composer/disabled/delayed cases but its `response` case adds
   an empty assistant element and does not execute production turn-tagged
   capture, streaming, URL-length, wrong-agent, or stale-turn cases. HIGH;
   Batch C.
5. Participant routing correctly sets `deepseek` active state before injection,
   but conversation URL persistence is best effort and fallback navigation to a
   provider base URL can open an unrelated/new conversation. CRITICAL; Batch A.
6. Active submit failure is diagnostically emitted and manually recoverable,
   but not a terminal routing failure; it can remain waiting for 300 seconds
   before a user understands the route did not send. HIGH; Batch A/B.

### Test/fixture gap analysis

Existing fixture coverage: textarea/contenteditable/ProseMirror/placeholder,
disabled and delayed Send, setup non-submit, and a minimal response shell.
Missing Batch C cases: production-script delayed-enable retry assertion;
verified active submit report; response with stable streaming and stale baseline;
wrong agent/turn ignored; `Done` before/missing response; URL-length transport
failure; nav recreation/base-URL preservation; setup response cannot satisfy an
active turn. Browser parser tests are executable through `cargo test browser`;
the HTML fixture is manual/WebView evidence, not a live provider test.

## Loop 4 findings summary

Critical-path command payloads and active-turn frontend correlation are aligned.
The main IPC gap is observability: route/brain diagnostics are not consumed by
the normal UI, so the user cannot distinguish brain, navigation, submit, and
capture failures.

## Loop 5 findings summary

The source requests auto-submit for every active prompt and forbids it for
setup. `Done` cannot complete an active turn; Response requires matching agent
and turn. The highest-risk mechanism is transporting extracted assistant text
through an arena URL after broad selector polling, particularly on long or
streaming real responses.

## Updated broken-contract matrix

| Step | Expected | Actual | Severity | Batch |
|---|---|---|---|---|
| Active response transport | Reliable full response capture | URL-encoded response may truncate/reject; `Done` cannot recover. | CRITICAL | A |
| Response ownership | Latest assistant response only | Broad `.markdown/.prose` selectors can select wrong DOM. | CRITICAL | A |
| DeepSeek route | Preserve participant setup conversation | Best-effort URL can fall back to base URL. | CRITICAL | A |
| Auto-submit failure | Immediate clear recovery | Manual path exists, but active wait remains long and opaque. | HIGH | A/B |
| Finalization | Blueprint before completion | Complete has no nonempty-blueprint guard. | HIGH | B |
| Progress | No infinite productive-looking loop | No global progress ceiling. | HIGH | B |
| UI diagnostics | Visible route/brain phase | New decision/route events ignored by standard listener. | MEDIUM | B |

## Context checkpoint

CONTEXT CHECKPOINT — continue in next Codex run

Files read: required IPC, command/session/router/browser/frontend listener and
view files, plus browser fixture and embedded browser/router test sections.
Completed: Loop 4 parity tables/findings and Loop 5 browser/nav-event/gap
tables/findings. Top CRITICAL findings: arena URL response transport, broad
response selectors, and best-effort participant conversation URL preservation.
Exact dirty state: only `src-tauri/project-docs/audits/phase1-e2e-reliability-audit.md`.
Recommended Batch A: replace response-text URL transport with bounded native
event delivery; harden response ownership/baseline; make participant routing
wait for or recover the correct conversation URL; surface immediate active
submit failure. Next prompt: approve Loop 6/7 completion and write Batch A
repair plan before source edits.

## Loop 6 — Batch A repair plan

1. Replace the single long `arena://response` payload with bounded response
   start/chunk/end signals. Chunks remain correlated by agent, turn, and
   message ID; the active router alone reassembles them into `NavEvent::Response`.
2. Keep the pre-injection baseline and remove global `.markdown/.prose` from
   the primary selector list; accept only explicit assistant containers.
3. Make every participant route prompt self-contained with the project brief,
   leader proposal, critique task, and active marker. Base-URL navigation is a
   valid recoverable route, not reliance on priming history.
4. Preserve existing immediate `active_submit_failed` state event/manual exact
   turn fallback; Batch B will improve richer UI diagnostics.

## Batch A implementation

- Added `ResponseStart`, `ResponseChunk`, and `ResponseEnd` navigation events.
  The browser sends bounded 1200-character chunks; `wait_for_response` buffers
  only matching agent/turn/message chunks and returns a response only after a
  complete end marker. `Done` remains non-completing.
- Removed broad markdown/prose fallback from one response-selector path and
  changed the active primary transport away from one response-text URL.
- Route prompts are now self-contained: participant ID, project brief, leader
  proposal, requested review, and required critique/simplification output are
  included on every routed active turn, including base-URL recovery.
- Existing active submit failure event and exact-turn manual paste fallback are
  retained; no broad UI change was made.
- `ResponseStart`, `ResponseChunk`, and `ResponseEnd` are explicitly handled
  in browser signal metadata and diagnostic paths. They record only safe
  identifiers/count metadata, never complete setup or an active turn, and are
  ignored until router-side reassembly produces the final `Response` event.

## Batch A verification

- `cargo check`: PASS (one pre-existing unused-import warning in
  `blueprint_store.rs`).
- Frontend build, focused tests, and final diff checks remain required before
  a commit; no commit is authorized until their real exit results are recorded.

## Remaining Batch B/C items

- B: enforce a blueprint before Complete, add a global progress ceiling, and
  expose route/brain diagnostics in the normal status UI.
- C: add chunk assembly, wrong-agent/turn, long-response transport, and
  marker/stale-response fixture regressions.

## Runtime gate

1. Clean generated tree.
2. Start `npm run tauri dev`.
3. Select ChatGPT leader + DeepSeek participant and use the HabitPulse brief.
4. Manually send only setup prompts.
5. Confirm active ChatGPT auto-submits and its response appears in Arena.
6. Confirm `route_started` / DeepSeek routing, then DeepSeek auto-submit and
   response capture.
7. Confirm the DeepSeek result returns to the leader and a
   `blueprint-section-added` section appears in the main area.
