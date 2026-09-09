# Consensus Arena — IPC Contract

## Purpose and Authority

This file defines the **stable frontend↔Rust contract and the rules for verifying it**.

It is not proof that every current source call matches the contract.

When auditing:

1. `commands.rs` defines real Tauri command signatures/return types.
2. `main.rs` defines which commands are actually registered/reachable.
3. backend `app.emit(...)` call sites define emitted event names/payloads.
4. frontend `invoke`/`listen` call sites define what the UI actually sends/expects.
5. This file records intended stable semantics and must be corrected if source intentionally changed.

The old statement “full stress audit — all PASS” is removed. It created false confidence.

---

## Two Different IPC Layers

Do not mix them.

### 1. Tauri application IPC

Frontend ↔ Rust commands/events.

Examples:

- `invoke('start_session', ...)`
- `listen('agent-ask-user', ...)`

### 2. Browser-internal `arena://` protocol

Injected model-page JavaScript → Rust `on_navigation`.

Examples include Ready, submit, response, challenge, diagnostics, and potentially chunked response events depending on current source.

Browser-internal event shapes are **not frontend IPC** and can evolve independently.

---

## Naming / Serialization Rules

### Command arguments

For multiword public Rust arguments, use the explicit snake-case Tauri contract where required:

`#[tauri::command(rename_all = "snake_case")]`

Frontend keys must match the effective command wrapper exactly.

### JSON-string return convention

Many commands return:

`Result<String, String>`

where the success String is actually:

`serde_json::to_string(&value)`.

Frontend must parse that string before treating it as a struct/list.

Do not infer from `invoke<T>()` generic types.

Before adding/changing any caller:

- inspect the real Rust return type;
- inspect whether it calls `serde_json::to_string`;
- parse exactly once.

### Plain-string exceptions

Known patterns such as prompt-template contents or exported path/Markdown strings may be plain strings.
Never JSON.parse a command solely because other commands do.

---

## Registered Command Surface — Source Baseline `0cc76c9`

This list is provided to prevent omission, not to replace reading `main.rs`.
Current HEAD may contain additions.

### Session management

- `start_session`
- `pause_session`
- `resume_session`
- `abort_session`
- `request_pause`
- `get_session_checkpoint`
- `get_recovery_state`
- `recover_session`

### Active/manual recovery

- `user_input`
- `captcha_resolved`
- `retry_setup_agent`
- `confirm_setup_agent`
- `provide_manual_model_response`
- `rate_limit_decision`
- `setup_agent_sent` — legacy/manual acknowledgement; normal setup is readiness-only
- `provide_user_answer`

### Brain configuration

- `save_agent_brain_config`
- `get_agent_brain_config`
- `save_secondary_brain_config`
- `get_secondary_brain_config`
- `save_fallback_brain_config`
- `get_fallback_brain_config`
- `get_brain_status`

### Participant configuration

- `save_custom_participants`
- `get_custom_participants`
- `get_participants` — merged built-in + custom registry

The old IPC document omitted these custom-participant commands even though current source registers them.

### Prompt/settings

- `save_prompt_template`
- `get_prompt_template`
- `get_maintenance_mode`
- `set_maintenance_mode`

### Browser/account diagnostics

- `launch_connected_account`
- `get_diagnostic_brief`
- `get_diagnostic_snapshot`
- `get_browser_timeline`
- `get_browser_reliability_report`
- `export_browser_diagnostics`
- `run_single_model_diagnostic`

### Session/data retrieval

- `get_transcript`
- `get_session_list`
- `export_blueprint`
- `get_agent_health`
- `delete_session`
- `rename_session`
- `get_session_details`
- `get_session_transcript`
- `get_blueprint_sections`

### Memory

- `get_project_memory`
- `get_global_memory`
- `clear_project_memory`
- `get_open_questions`
- `get_model_strengths`
- `save_project_config`
- `get_project_config`
- `get_memory_health`
- `repair_memory_index`
- `get_patterns`
- `export_memory`
- `restore_memory`

### Hackathon

- `get_hackathon_config`
- `save_hackathon_config`
- `get_hackathon_run_state`
- `cancel_hackathon_run`
- `send_hackathon_invitations`
- `run_hackathon`

**Audit rule:** compare every current `#[tauri::command]` against `generate_handler!` instead of trusting this list.

---

## Pipeline-Critical Command Semantics

### `start_session`

Starts one session if concurrency guard permits.

Inputs include project/session/participant/leader configuration according to current source.
Custom participants must resolve through the merged participant registry.

### `abort_session`

Stop path. Must terminate/mark the active loop and clear safety-critical ownership/waits without leaving a duplicate-resumable side effect.

### `pause_session` / `request_pause`

Pause is intended to occur at a safe checkpoint boundary, not by freezing arbitrary browser side effects mid-flight.

Audit current frontend usage because both names exist in source baseline.

### `resume_session`

Checkpoint-based paused-session resume.
This is **not the same mechanism** as `recover_session`.

### `get_recovery_state` / `recover_session`

Historical incomplete-session recovery path that replays persisted Blueprint sections.
At checkpoint `0cc76c9`, `recover_session` does not itself restart the autonomous loop.

### `retry_setup_agent`

Re-probes/retries setup readiness for the selected setup agent.
Normal setup semantics are readiness/authentication only, not setup-time role priming.

### `confirm_setup_agent`

Explicit manual recovery path for an unfinished setup agent.
Do not let it fabricate active-turn response/submission state.

### `setup_agent_sent`

Legacy/manual acknowledgement command.
It must not be treated as the normal current priming mechanism.

### `provide_manual_model_response`

Manual active-turn recovery.
Must validate against the exact currently awaited active operation.

Current generation-safety must be audited; agent+turn alone is not sufficient for stale-page protection.

### `provide_user_answer`

Resolves the current AskUser oneshot.

Required behavior:

- fail if no pending AskUser exists;
- sender consumed with `.take()`;
- every UI dismiss path calls it, including Escape/backdrop cancellation.

### `captcha_resolved`

Signals that the user completed a verification step and requests re-evaluation.
It must not fabricate Ready itself.

### `rate_limit_decision`

Carries user-selected recovery policy for a rate-limited participant.
A retry decision must never duplicate a side effect already physically accepted by a model.

### `launch_connected_account`

Uses the shared browser architecture to open/focus a provider account flow.
Registry state must be authoritative over stale cached handles.
OAuth/new-window behavior must remain provider/domain-safe.

---

## Important Return-Type Families

These families reflect the established project convention, but the audit must still inspect source.

### JSON-serialized values — frontend parses

Typical examples:

- brain config getters;
- participant list getters;
- session list/details/transcript/sections;
- recovery/checkpoint state;
- agent health;
- memory collection/health getters;
- structured Hackathon state;
- diagnostic snapshot/timeline/export metadata where source serializes JSON.

### Plain strings — frontend does not JSON.parse

Typical examples:

- prompt template content;
- Markdown diagnostic brief/report;
- exported file paths/content where command source returns a literal String.

Do not maintain a giant hand-copied type table here; it becomes stale. Verify each changed call against `commands.rs`.

---

## Backend → Frontend Event Families

The following names exist in the uploaded/current documentation baseline and are pipeline-relevant. Current source must be compared against them.

### Session/setup lifecycle

- `session-status`
- `setup-agent-ready`
- `setup-agent-complete`
- `setup-agent-failed`
- `setup-complete`
- `session-checkpoint`
- `session-complete`

**Semantic correction:** `setup-agent-complete` means setup/readiness complete; it no longer guarantees a priming message was sent.

### Agent/browser state

- `agent-state-change`
- `active-turn-state`
- `browser-diagnostic`
- `agent-routing`
- `route_started`
- `boss-message`

### Brain lifecycle

- `agent_brain_decision_started`
- `agent_brain_decision_failed`
- `agent_brain_decision_fallback`

A fallback event is diagnostic; fallback behavior must still respect Blueprint/Complete/review guards.

### Blueprint

- `blueprint-update`
- `blueprint-section-added`
- `blueprint_emitted`

The UI receiving a Blueprint event is not proof that reviewer/process invariants were satisfied. That must be enforced before emit/persist.

### User intervention

- `agent-ask-user`
- `captcha-detected`
- `rate-limit-reached`

### Conversation/status

- `agent-message`
- `requirements-question`
- `requirements-complete`

### Memory

- `memory-updated`
- `memory-health-warning`

### Hackathon

- `hackathon-run-started`
- `hackathon-invitation-update`
- `hackathon-group-status`
- `hackathon-invitations-complete`
- `hackathon-group-output`
- `hackathon-complete`

---

## AskUser Event Contract

`agent-ask-user` carries the current question/options/custom-input semantics.

Frontend requirement:

- display immediately;
- block incompatible interaction;
- option/custom submit calls `provide_user_answer`;
- Escape/backdrop/close resolves with a cancellation answer rather than abandoning the backend sender.

Audit listener mount/unmount and duplicate-event behavior.

---

## Browser Diagnostic Contract

Routine debugging should prefer:

`get_diagnostic_brief`

because it is compact/redacted and avoids cloning/rendering giant raw diagnostics.

Full snapshot/timeline/export commands are advanced forensic tools.

Do not maintain field-by-field copies of the entire internal diagnostic structs in IPC.md; those structs evolve rapidly and are not normal frontend business-state contracts.

Security requirements:

- no cookie values;
- no OAuth codes/tokens/query secrets;
- no API keys;
- browser forensic details redacted/bounded.

---

## Browser-Internal `arena://` Contract

### Architectural invariants

- intercepted in `on_navigation`;
- callback remains synchronous/non-blocking;
- no Tokio mpsc inside callback;
- agent identity must not come from a captured closure value;
- messages that cause/confirm model side effects require exact active identity;
- diagnostic traffic may be lossy, critical active traffic must not silently disappear.

### Event evolution warning

Do **not** copy the old five-variant protocol from historical docs.
At checkpoint `0cc76c9`, `NavEvent` already includes Ready, Error, Response, Done, SendDetected, manual/setup events, submit reports, probes, challenge/navigation events, console/lifecycle/DOM/action diagnostics, and UA telemetry.

A later local transport patch was reported to add response start/chunk/end semantics.

The Astra audit must read current `NavEvent`, parser, and JS emitter code directly.

### Required active correlation

Desired identity:

`agent_id + turn + setup/document generation`

At checkpoint `0cc76c9`, core automatic active event variants are not uniformly generation-bearing. Do not state otherwise.

---

## Frontend State Semantics

Avoid preserving stale semantic names as architectural truth.

For example, frontend state/view names may still contain `priming` because of historical UI wiring even though backend setup no longer sends priming messages.

When auditing state transitions, distinguish:

- UI label/name;
- backend state;
- physical browser state.

---

## Wiring Rules

1. Every backend event name/payload must match frontend listeners.
2. Every frontend invoke name/argument casing must match command wrappers.
3. Structured JSON strings must be parsed exactly once.
4. Plain strings must not be JSON-parsed.
5. Listener cleanup is mandatory on unmount/remount.
6. AskUser dismissal must resolve the backend channel.
7. Main UI shows Blueprint, not raw model chats.
8. `debug-log` remains development-only and is not promoted to production IPC merely for convenience.
9. Browser-internal `arena://` events are not frontend events.
10. Any command/event changed by a reliability repair must be audited end to end: Rust producer/handler ↔ registration ↔ IPC doc ↔ frontend caller/listener.

---

## Astra Audit Priorities for IPC

Do not waste time retyping every payload.
Prioritize these failure classes:

- command defined but not registered;
- frontend invokes wrong argument case;
- JSON-string/object parsing mismatch;
- event name/payload mismatch;
- duplicate listeners / missing cleanup;
- Stop/AskUser/CAPTCHA/rate-limit command races;
- stale manual response acceptance;
- browser critical event identity/drop behavior;
- checkpoint/resume command semantics;
- provider auth popup/new-window handoff.
