# Phase 1 system-functionality audit

## Loop 0 — baseline

- HEAD: `97e8f88 fix: auto-submit active pipeline prompts`.
- Initial dirt was generated output only (`src-tauri/target/debug/consensus-arena` and build `dist/` assets); restored/removed before source edits.
- Baseline: `cargo check` passed with warnings; `npm run build` passed; `git diff --check` passed.
- Source reality: 43 `#[tauri::command]` definitions. Docs claiming 38 commands/16 AppState fields are stale; source has 17 AppState fields (including `setup_generation`).

## Intended Phase 1 runtime contract

1. Setup injects but never submits priming; same-agent send/manual confirmation advances setup.
2. Active leader and participant prompts inject, verify integrity, auto-submit, then await a same-agent/same-turn response.
3. The agent brain turns a captured leader response into Route, RouteCompare, Blueprint, AskUser, Continue, or Complete.
4. Route/RouteCompare return captured participant text in a new leader turn; Blueprint persists and emits a main-area section; AskUser suspends on a oneshot; Stop aborts the nav wait.
5. Browser diagnostics report submission separately from capture; memory is non-fatal and bounded to the brain.

## Module ownership / trace findings

| Module | Status | Evidence |
|---|---|---|
| `commands.rs` | PARTIAL | Builds std→Tokio nav bridge, starts setup/debate, validates manual response against `BrowserState.active_turn`. |
| `session_runner.rs` | PASS | Setup is manual-send/confirm; setup response cannot directly enter active loop. |
| `response_router.rs` | FAIL | `wait_for_response` accepted `NavEvent::Done` as `Ok(String::new())`, allowing a missing response URL to discard the actual leader response. |
| `browser_backend.rs` | FAIL | Active submit used one immediate enabled-button lookup; delayed hydration/reactivity could leave an inserted prompt unsubmitted. |
| `useIpcListeners.ts` / store / ActiveView | PARTIAL | Truthful auto-submit/manual fallback status exists, but can only be useful once backend signals are reliable. |

## Loop 5 — frontend contract

- `useIpcListeners.ts` subscribes to the documented lifecycle, setup, active-turn,
  blueprint, AskUser, diagnostic, and completion events and cleans every listener
  on unmount.
- Setup confirmation and active manual response use documented snake_case payloads.
  The manual-response command independently validates session state, running state,
  agent, and exact turn before it resumes the normal backend loop.
- `active-turn-state` distinguishes insertion, automatic submission, submission
  failure, capture, and timeout. `agent-message` places non-empty automatic and
  manual response text into the status drawer. Blueprint events populate the
  blueprint-first main area.
- The one frontend caveat is presentational: the bottom InputBar's idle submit only
  changes local view state; the actual Start session control is SetupView's button.
  This is unrelated to the active-pipeline fault and was not changed.

## Repair plan status

- Batch A — active pipeline minimum viable autonomy: implemented for submit retry
  and response-text preservation. Route, RouteCompare, Blueprint, AskUser,
  Complete, and manual response were traced through the existing router and have
  matching IPC/UI handlers.
- Batch B — state/diagnostic truthfulness: existing active-submit events plus the
  current UI handler are sufficient after Batch A; no broad UI rewrite required.
- Batch C — regression safety: the embedded-script Rust test now requires retry
  and native click paths; the fixture's delayed-enable Send control is clickable.

## Broken contract matrix and Batch A repair

| Contract | Expected | Actual before repair | Severity | Fix |
|---|---|---|---|---|
| Active submit | Submit after UI enables Send | Single lookup immediately after input event | CRITICAL | Retry enabled-button lookup for 3 seconds; click the resolved button. |
| Response capture | Only response text satisfies a turn | `done` could complete an active turn with empty text | CRITICAL | Treat `done` solely as a marker and continue waiting for a same-agent/same-turn response/manual response. |
| Diagnostics | Explicit submit result | Existing event already distinguishes submit/capture | PASS after above | Preserve existing `active-submit` report. |

## Verification

- `git diff --check` passes after the repair.
- `cargo check` completed successfully after the checkpoint repair (42 existing
  dead-code/unused-import warnings). A local unreachable `ActiveSubmitReport`
  match warning in `browser_backend.rs` was removed without changing behavior.
- `npm run build` reached `tsc && vite build`, but this execution harness returned
  before Vite printed its completion/exit result; it is not recorded as PASS.
- `cargo test browser` started compiling but was similarly interrupted by the
  harness. Its orphaned build lock then blocked the requested final `cargo check`;
  no visible Cargo/Rust process was present, and no lock file was deleted.
- `cargo fmt --check` reports existing repository-wide formatting drift and was not
  applied because it would rewrite unrelated files.
- Forbidden-lock searches are clean: no `blocking_lock()`, no Tokio mpsc in
  `browser_backend.rs`, and no `memory_store.lock().await`.
- `unwrap`/`expect` matches are limited to test-only in-memory stores,
  `NonZeroU32::new` for a nonzero compile-time iteration constant, and Tauri
  startup initialization in `main.rs`; none were introduced by this repair.
- Targeted test/build commands must be rerun to an actual successful exit before
  committing.
- Live account/WebView verification is still required; no CAPTCHA/login controls are bypassed.

## Decision-routing follow-up

- The captured leader response reached `AgentBrain::decide`, but prose after a
  valid JSON object made the old whole-string parser fail. The router then
  repeatedly injected a generic unclassified Continue prompt.
- The brain now receives an appended strict JSON decision contract, balanced
  JSON extraction accepts fenced/prose-wrapped output, and failures emit
  redacted lifecycle diagnostics.
- A deterministic Phase 1 fallback routes canonical `deepseek` when the brief
  requires one DeepSeek consultation or the leader requests critique; useful
  blueprint-like text emits a draft section; only one unclear Continue is
  allowed before a recoverable visible error.
- Route targets are resolved against selected participant IDs (with display-name
  compatibility), and a missing shared nav WebView is recreated safely without
  creating a third WebView. The DeepSeek response returns in a new leader turn
  requesting incorporation into the next/final blueprint.

## Final changed files and runtime gate

- Changed: `src-tauri/src/browser_backend.rs`,
  `src-tauri/src/response_router.rs`, `src-tauri/tests/browser-fixtures.html`,
  and this audit.
- Manual runtime gate: configure a brain; select ChatGPT leader plus DeepSeek;
  complete both manual priming sends; start the active turn; confirm ChatGPT
  auto-submits and its visible response appears in the status drawer; use a brain
  Route to DeepSeek; confirm DeepSeek auto-submits/captures and the next ChatGPT
  turn receives the result; verify Blueprint then Complete. If automation reports
  submit failure, click Send in the model WebView and use the existing paste-response
  fallback for the exact displayed agent/turn.
