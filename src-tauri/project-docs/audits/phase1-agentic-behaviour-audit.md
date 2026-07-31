# Phase 1 Agentic Behaviour Audit

## Scope and evidence

Audited branch: `phase1-e2e-reliability-fix` at `8aa0198` (`fix: make
composer injection and send safe`), including `795e8b5`, `18cded9`,
`e8f45b0`, and `7eb6529`.

Files read: `/home/kasun/Music/arena/consensus-arena/AGENTS.md`, the requested
`DECISIONS.md`, `ARCHITECTURE.md`, `BACKEND.md`, `FRONTEND.md`, `IPC.md`,
`PROCESS.md`, and prior reliability audit; `src/response_router.rs`,
`src/agent_brain.rs`, `src/browser_backend.rs`, `src/orchestrator.rs`,
`src/commands.rs`, `src/session_runner.rs`, `src/memory_store.rs`,
`src/blueprint_store.rs`, `src/context_manager.rs`, `src/turn_manager.rs`,
`src/hooks/useIpcListeners.ts`, and `src/stores/useAppStore.ts`.

`src/setup_flow.rs` does not exist. Setup is implemented in
`src/session_runner.rs` and commands delegate into that flow.

## Conclusion

There is no hardcoded three-round/three-turn debate loop. `run_agent_loop`
increments an unbounded `iteration` and follows `AgentDecision`; completion is
the brain's `Complete` arm or user abort. The only `3` related limits found are
brain-failure failover, participant retry count, and a memory-summary threshold
(`sections_finalized >= 3`), none of which terminates a debate at round three.
The memory threshold should nevertheless be separated from completion wording
in A.6 because it can make a short valid session look incomplete in memory.

## Actual lifecycle

1. `start_session` sets session state and invokes `session_runner::run_setup`.
   It creates at most the leader and shared participant WebViews, then setup
   injects a non-auto-submitted priming prompt and waits for browser proof or
   explicit user confirmation.
2. `run_agent_loop` starts leader turn 1 with `inject_active_prompt`.
   Browser state records the exact `(agent, turn)` before injection.
3. `build_inject_js` locates an input, injects text, emits a secret-free prompt
   report, and for active turns invokes the static submit helper. The helper
   reports success/failure through acknowledged `active-submit` signals.
4. `wait_for_response` accepts only matching agent/turn response chunks or a
   matching manual response. It then feeds leader text to `AgentBrain::decide`.
5. `Route`/`RouteCompare` navigates/reuses the one participant WebView, injects
   a self-contained critique prompt, captures its response, and injects the
   combined result into the leader's next turn.
6. `Blueprint` persists/emits a blueprint section then prompts the leader to
   continue. `Complete` persists session summary and emits session completion;
   user abort sends `SessionAborted`.

## Findings

### Critical: ChatGPT active composer integrity is measured against the wrong node

`findInput` returns the first visible candidate across broad selectors. After
prior messages, ChatGPT can expose multiple visible `contenteditable`, role
textbox, or ProseMirror descendants. `detectComposerContext` derives a root,
but `reportInjection` reads only `input.value || input.textContent`; it does
not verify the framework-owned composer state or the root's current editable
descendant. Thus a successful event sequence can still read stale/empty DOM
text and emit `prompt_integrity_failed`. This precisely matches the observed
ChatGPT active turn 2 failure.

The contenteditable path calls `selectAll` globally, dispatches paste/beforeinput,
then calls `execCommand('insertText')`. It can duplicate or target document
selection rather than the selected editor unless focus/selection is constrained
to the composer. The fallback `textContent` mutation is visible-only and is
correctly not trusted, but it also explains a visible prompt that the provider
does not consider sendable.

### High: active-submit reporting is not guaranteed before recovery state

The Rust 10-second missing-report watchdog records
`active_submit_report_missing`, but an injection script can repeatedly retry
`findInput()` forever when no input is found. It neither reports an injection
failure nor bounds the retry. A delayed report failure intentionally keeps the
turn active for manual Send, but there is no explicit state distinguishing
"manual recovery available" from "still auto-capturing". The UI’s manual-paste
target remains exact, but the path is ambiguous for a user who clicked Send.

### High: terminal WebView loss leaves session ownership active

The participant guard checks the nav WebView immediately before injection and
emits `active_window_closed`; leader injection checks likewise. Neither wait
loop observes window destruction while waiting for a response. `BrowserState`
retains handles and diagnostic records; `session_active` is an independent
atomic flag. Consequently `session_active=true` with both snapshot existence
booleans false is plausible after user/app window closure, not merely stale
diagnostics. No terminal-window-loss path aborts or pauses the session and
reconciles frontend state. The latest snapshot is therefore a real invariant
violation until proven otherwise by close-event logging.

### High: stale active ownership can survive failure paths

`mark_active_turn_failed` changes diagnostics but does not clear ownership.
Some caller paths clear it; the missing-report path intentionally does not.
That is necessary for manual recovery, but no bounded expiry/retry identity
exists. A later injected turn can overwrite `active_turn`, making a late manual
response correctly rejected but leaving old diagnostic agent/turn fields until
overwritten. A.6 needs one explicit active-turn state machine and transitions.

### Medium: setup and response proof remain too permissive

Setup may accept post-injection response observation as proof while composer
diagnostics are incomplete. It proves page activity, not that the intended
priming composer accepted the prompt. Setup should require correlated send
proof or explicit manual confirmation, and label response-only observation as
recoverable rather than complete.

### Medium: completion truth is brain-controlled but unconstrained

There is no fixed-round finish (good), but `Complete` does not require a
non-empty blueprint section. Existing context consensus helpers and the memory
threshold are not authoritative gates. A.6 needs quality/consensus evidence
and a no-progress safety limit, not a round count.

## Provider compatibility readiness

| Provider | Current base/composer/send | Existing handling | Main risk / required gate |
|---|---|---|---|
| ChatGPT | `chatgpt.com`; likely ProseMirror/contenteditable; semantic send | generic ProseMirror/contenteditable path | Select the live composer root deterministically and verify provider state after each injection. |
| DeepSeek | `chat.deepseek.com`; likely textarea or contenteditable; semantic send | generic textarea/CE path and shared-nav reuse | `active_auto_submit_attempted=false` means no report reached Rust; capture input selection/report path and distinguish no JS from no submit. |
| Qwen | `chat.qwen.ai`; unknown current DOM | generic selectors only | Gate behind runtime composer/send capability proof; no provider-specific adapter yet. |
| Kimi | `kimi.com`; Lexical CE; `div.send-button-container` | explicit selector and execCommand path | Lexical state may ignore DOM/paste; require a focused adapter and send-enable verification. |
| GLM | `chat.z.ai`; `#chat-input` textarea; `#send-message-button` | explicit IDs | likely strongest current match; still needs runtime capability proof and response ownership selector. |

Response selectors are generic assistant-container heuristics for every
provider; none has a provider-specific author/stream-completion contract.

## Unsafe-click audit

PASS for the requested constraint: no
`document_rightmost_pointer_click` remains. The only rightmost fallback is
composer-root scoped, and no-root candidates produce
`unsafe_document_candidate_rejected`. Semantic filtering excludes attachment,
voice/mic, reasoning/search, and Stop controls. Residual risk: a wrongly
identified composer root can still make a nearby non-send control eligible;
A.7 must make root discovery provider-verified and apply geometry only within
that root or its immediate send-control sibling.

## State-invariant matrix

| Invariant | Status | Evidence / required repair |
|---|---|---|
| active session has live leader/nav windows | FAIL | Window destruction is not reconciled with `session_active`; add close observers and terminal pause/abort state. |
| injected prompt implies submit attempt/report | PARTIAL | JS can loop awaiting input; watchdog records failure late. Bound retries and report every exit. |
| submit failure stays manually recoverable | PARTIAL | Exact turn is retained, but UI does not show a distinct manual-recovery state or window focus action. |
| captured response updates phase | PASS on matching capture | capture paths mark `active_response_captured`; stale/late events are ignored by identity. |
| setup completion proves intended prompt | PARTIAL | response-after-injection can be treated as proof without composer acceptance. |
| old turn cannot satisfy later turn | PASS transport / PARTIAL diagnostics | agent+turn checks reject stale responses, but diagnostic ownership can linger. |

## Prioritized repair plan

### A.6 — lifecycle/state invariants and dynamic completion

Implement a single active-turn state machine with explicit `auto_submit`,
`manual_recovery`, `captured`, `failed`, and `window_lost` transitions. Bound
input discovery and report every active injection outcome. Reconcile WebView
close events with session state; pause/end only after both model windows are
lost or route recovery is impossible. Require blueprint/brain quality evidence
for `Complete`, with a no-progress safety guard that is not a fixed round cap.

### A.7 — ChatGPT and DeepSeek active adapters

Add safe, secret-free runtime capability telemetry (editor kind, selected root
relationship, provider state accepted, send enabled) and fixture/live DOM
evidence. Use a ChatGPT root-scoped ProseMirror adapter and a DeepSeek-specific
editor adapter only after captured DOM evidence. Ensure every branch emits one
`ActiveSubmitReport`; never use document selection or visible-only mutation as
an auto-submit success basis.

### A.8 — provider compatibility gates

Add runtime readiness gates and provider fixtures for Qwen, Kimi, and GLM:
known composer, known send control inside/adjacent to root, and owned response
selector. Keep models recoverable/manual when a gate is not met.

### B.1 — custom model URL support

Defer until generic and provider adapters have stable lifecycle and capability
contracts. It must not bypass A.8 gates or create additional WebViews.

## A.6 implementation note

- Active diagnostics now carry explicit lifecycle state, recovery allowance, and
  secret-free failure reason. Failed auto-submit and missing submit reports
  transition to `manual_recovery`, preserving the exact active turn.
- Active input discovery is bounded at 20 seconds and reports
  `input_not_found`/`composer_not_found` rather than retrying forever.
- Response waits detect required WebView loss, transition to `window_lost`, and
  emit `active_window_closed` instead of consuming the 300-second timeout.
- `Complete` is deferred without a blueprint section; the leader is asked to
  finalize one. This is a quality gate, not a fixed-round cap.
- Setup proof hardening and provider-specific acceptance remain A.7 work; no
  provider adapter or additional WebView was added in A.6.

## A.7 implementation note

- Fixed the A.6 manual-recovery regression: recoverable active-submit failures,
  including a missing submit report, now retain the exact active turn and enter
  `manual_recovery` with recovery enabled.
- Removed the remaining document-wide send-button scan. Candidate controls are
  limited to the composer root or geometry-proven immediate siblings, with
  unsafe controls excluded.
- Added the ChatGPT current-composer adapter and editor-scoped Range selection;
  it prefers the bottom `#prompt-textarea`/test-id composer and rejects prior
  message/output nodes.
- Added the DeepSeek bottom-composer path and exactly-once active-submit report
  guard. DeepThink/Search controls remain excluded.
- Diagnostics now distinguish BrowserState handle presence from Tauri label
  lookup presence. Qwen/Kimi/GLM compatibility remains A.8; custom URL support
  remains B.1.

## Final Phase 1 automation stabilization

- Browser pages now expose the single `window.__caAutomation` contract for
  capability detection, scoped injection, submit dispatch, and response-monitor
  ownership. Setup and active callers require that contract rather than silently
  starting a second detector.
- Setup/debate isolation is strict: only same-agent send detection or explicit
  manual confirmation completes priming. Setup responses and `Done` events are
  weak diagnostics and are drained before the active loop.
- A proposal gate reprompts leader role/process-only output in the leader window;
  it is never routed to a participant. No fixed debate round cap was added.
- Active automation remains composer-scoped; document-wide send clicking is
  disabled. ChatGPT and DeepSeek use current-composer capability reasons.
- Qwen/Kimi/GLM remain future provider gates; custom URL support remains
  deferred.

## Initial participant review stabilization

- Removed the old setup fallback composer, send-control finder, and setup
  response poller. Setup and active prompts now use the same page-resident
  `window.__caAutomation` engine, while priming completion remains strong-only:
  same-agent `SendDetected` or explicit `SetupManualConfirmed`.
- Added a mandatory initial participant review invariant whenever a selected
  non-leader exists. The first concrete leader proposal is sent to the first
  selected non-leader for concise practical critique, that critique is returned
  to the leader, and the matching leader revision must be captured before the
  review is complete.
- `Blueprint` and `Complete` decisions cannot end the session before that
  critique/revision loop. Process-only leader output is reprompted in the
  leader window and never routed to the participant.
- ChatGPT readiness is based on the page automation API, matching window
  identity, and a stable current-composer capability. It does not wait for
  account, profile, or header controls; a hydrated composer without a safe Send
  control is diagnosed explicitly for manual recovery.
- Active prompt ownership is checked against the selected leader/nav handle,
  expected window label, diagnostics owner, and page identity before injection.
  Send discovery remains composer-scoped; the global document button scan and
  `document_rightmost_pointer_click` remain absent.
- This invariant is a two-or-more-model bootstrap, not a debate round limit. No
  hardcoded round or turn cap was added. Qwen/Kimi/GLM provider gates and custom
  URL support remain deferred.
