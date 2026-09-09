# Consensus Arena — Backend

## Status

The backend is **feature-rich but not yet proven beta-reliable**.

Phase 1 Memory, session CRUD, brain configuration/fallbacks, AskUser, Hackathon, pause/resume checkpoints, browser diagnostics, custom participants, and the seven built-in providers exist in current source.

The remaining blocker is not feature absence; it is **end-to-end pipeline reliability** under real browser/model failure conditions.

Do not use phrases such as “backend fully complete”, “all IPC PASS”, or “all active events generation-safe” unless the exact current source and tests prove them.

---

## Source-First Rule

This file intentionally avoids fragile implementation counts such as:

- number of modules;
- number of commands;
- number of AppState fields.

Those counts changed repeatedly and created false confidence.

For exact current signatures, read:

- `main.rs` — registered modules/commands;
- `commands.rs` — Tauri command signatures/return types;
- `orchestrator.rs` — AppState;
- relevant module source.

---

## Core Backend Modules

### `orchestrator.rs`

Owns the central `AppState`, active session configuration/status, brain handles, browser state, stores, AskUser sender, model health, pause/resume/checkpoint state, and Hackathon state.

Current source has substantially more state than the old “16 fields” documentation claim. Never copy a field count from docs; inspect the struct.

### `browser_backend.rs`

Owns:

- built-in and merged participant registry;
- named WebView lifecycle;
- navigation and page-load callbacks;
- post-load automation activation;
- `GENERIC_INIT_SCRIPT`;
- JS→Rust `arena://` parsing;
- browser event ingress/forwarding;
- active prompt injection/submission helpers;
- response observation/transport;
- browser diagnostics/timeline;
- Connected Accounts and new-window policy helpers.

### `browser_harness.rs`

Owns compact/bounded browser forensic records and error classification.
Arena-generated automation errors must not be mislabeled as provider website errors.

### `session_runner.rs`

Owns session setup/readiness and entry into the autonomous loop.

**Current semantics:** setup is browser readiness/authentication only. It should not inject or send role priming.

Any old setup priming/re-priming code remaining after an unconditional readiness completion path is retired/dead code and should be audited/removed rather than treated as current architecture.

### `response_router.rs`

Owns the active autonomous loop:

- leader first task;
- response wait;
- brain decision;
- Route/RouteCompare;
- participant injection/recovery;
- Blueprint;
- AskUser;
- Hackathon handoff;
- Continue/Complete;
- checkpoints and some memory/health effects.

This is the highest-risk pipeline module and must be audited together with `browser_backend.rs`, not independently.

### `agent_brain.rs`

OpenAI-compatible orchestration-brain client with primary/fallback behavior.
The seven `AgentDecision` actions are part of the current contract.

Audit malformed responses, fallback semantics, unavailable brains, and whether runtime context actually contains the process state the prompt expects.

### `checkpoint.rs`

Owns persisted pause/resume checkpoint shape and validation.
Audit it against every safety-critical in-memory state needed to resume without duplicate side effects.

### `session_vault.rs`

Current checkpoint source uses file-backed `session_vault.db` through `SessionVault::open(...)`, with fallback behavior if open fails.

The old documentation claim that SessionVault is intentionally in-memory is rejected.

Conversation continuity still depends on active routing actually saving the current provider conversation URL after successful real turns.

### `hackathon.rs`

Owns Hackathon run/group state and report formatting/execution helpers.
Hackathon is advisory and must not automatically become Blueprint content.

### Persistent stores

- `settings_store.rs`
- `transcript_store.rs`
- `blueprint_store.rs`
- `memory_store.rs`
- `session_vault.rs`

Synchronous SQLite access protected by `std::sync::Mutex` must run through the project's blocking helper where required; do not hold synchronous DB work on async runtime threads.

---

## Browser Backend — Current Facts

### Named windows

- `arena-leader`: persistent leader context.
- `arena-nav`: shared participant context.

Window registry state is authoritative. Cached handles must not be trusted when the Tauri registry says the named window no longer exists.

### Provider bootstrap

The current checkpoint has explicit tests asserting the leader/nav builders do **not** contain model-runtime `.initialization_script(...)` calls and do **not** force `.user_agent(...)`.

Post-load activation is driven from `PageLoadEvent::Finished`.

Do not re-document the old document-start/forced-Safari behavior as current.

### Participant registry

Current source contains:

- seven immutable built-ins;
- persisted custom participants;
- a merged participant registry;
- validation preventing custom IDs from shadowing built-ins.

Built-in baseline URLs at checkpoint `0cc76c9`:

- ChatGPT: `https://chatgpt.com`
- Claude: `https://claude.ai`
- Gemini: `https://gemini.google.com`
- DeepSeek: `https://chat.deepseek.com`
- Qwen: `https://chat.qwen.ai`
- GLM: `https://chat.z.ai/`
- Kimi: `https://kimi.ai/`

The old `https://www.kimi.com/` claim is stale for this checkpoint.

### Current NavEvent caveat

At checkpoint `0cc76c9`, `NavEvent` includes many operational/diagnostic variants, not the old five-variant enum copied into historical docs.

Core active events at that checkpoint include agent+turn but are **not uniformly generation-bearing at the event boundary**. Diagnostics have generation fields, but that is not equivalent to generation-safe wire identity.

A later local transport patch may add response-start/chunk/end variants. Inspect current source at the actual audit HEAD.

### Event forwarding caveat

At checkpoint `0cc76c9`, the async navigation consumer is bounded and `try_send` can drop an event when full. This is a high-risk design if critical active events share capacity with noisy telemetry.

Do not document `RISK-STALERESPONSE: CLEAR` or “critical events cannot drop” until source/tests actually prove it.

### Composer discovery

Recent source fixed a self-recursive normalizer that caused `Maximum call stack size exceeded` on Claude.

However, live Qwen diagnostics still showed composer candidate counts growing to 100+, indicating generic container discovery/polling remains too broad.

### Readiness classification

Recent changes improved false login/verification classification by allowing a valid composer to outrank weak login keywords.

A remaining source/runtime concern is delayed sparse-SPA hydration being promoted to empty-shell too early before the composer appears.

---

## Setup Semantics

The correct setup lifecycle is:

```text
resolve model
→ create/reuse proper window
→ navigate/bootstrap
→ allow login/challenge/OAuth
→ activate automation post-load
→ detect stable composer
→ setup-agent-complete
```

Setup must not:

- send role priming;
- treat unsent textbox contents as primed;
- re-prime after a Ready/navigation event.

The first real active task is responsible for priming.

Legacy setup-recovery commands/events may remain for compatibility. Their presence is not evidence that manual setup priming is still the main flow.

---

## First-Task Priming

Current checkpoint source constructs a first-task envelope for leader/participant roles.

Leader first task:

`leader priming + --- CURRENT ARENA TASK --- + real task`

Participant first routed task:

`participant priming + --- CURRENT ARENA TASK --- + routed question`

Important audit points:

- role-aware placeholder rendering;
- custom participant display names;
- no unresolved `{{...}}` placeholders;
- first envelope committed at physical SubmitConfirmed, not only after response capture;
- no duplicate priming after a response-extraction failure.

---

## Active-Turn Reliability Contract

The intended state progression is monotonic:

```text
Ready
→ PromptPrepared
→ PromptInjected
→ SubmitConfirmed
→ WaitingForResponse
→ ResponseCaptured
→ ConversationPersisted
→ Complete
```

### Required active key

Use an immutable correlation identity equivalent to:

`agent_id + turn + generation`

At checkpoint `0cc76c9`, `BrowserState.active_turn` is still agent+turn and core events are not uniformly generation-bearing. This remains an audit/repair target unless later source changes prove otherwise.

### SubmitConfirmed

`button.click()` is not sufficient physical proof.

Strong generic evidence can include:

- input clears;
- submitted user message appears;
- generation/Stop state begins;
- new response begins.

Once SubmitConfirmed, the same turn must not automatically be sent again.

### Response wait deadline

Checkpoint `0cc76c9` used a 300-second timeout pattern that could be restarted by unrelated events in the receive loop.

A later local Codex batch was reported to replace this with an absolute deadline. Inspect the current source and tests before declaring this fixed.

### Response capture

Live Qwen sessions proved that a visibly generated answer could remain uncaptured and leave the backend in `active_waiting_for_response`.

The detector must be baseline-aware and provider-neutral enough to identify a new post-submit response without relying exclusively on a small fixed CSS-selector set.

### Response completion

A temporary stable-text pause must not be confused with generation completion if the provider still shows active generation evidence.

### Long responses

The old single-URL response path truncated long response text around 8k characters.

A later local patch was reported to introduce bounded response chunk transport with UTF-8 length/checksum validation and no partial delivery. This is **not a fully verified fact until current source + tests are inspected**.

### Event queue safety

Critical active events should not be discardable because diagnostics filled the same bounded queue.

Telemetry may be lossy/coalesced; side-effect/result events must be reliable.

---

## Participant Routing and Continuity

### Reuse

A healthy already-loaded `arena-nav` page for the same participant should be reusable on attempt zero when:

- same assigned agent;
- valid provider origin;
- automation installed;
- composer healthy;
- no high-confidence blocker.

Do not reload simply because a participant turn begins.

### Real conversation URL persistence

After a successful active participant response:

1. read current URL;
2. validate expected provider origin;
3. reject login/OAuth/challenge/external URLs;
4. update in-memory conversation URL;
5. persist through SessionVault;
6. only then reuse nav window for another participant.

At checkpoint `0cc76c9`, post-response persistence remained a known gap.

---

## Agent Brain / Panel Logic

### Decision family

Current actions:

- Route
- RouteCompare
- Blueprint
- Continue
- Complete
- AskUser
- Hackathon

### Runtime guarantees vs prompt instructions

Prompt templates may state strong review/module rules, but Rust must enforce any process guarantee the product depends on.

Audit current source for:

- attempted consultation vs successful reviewer;
- canonical selected-roster validation in RouteCompare;
- review coverage state;
- Blueprint guard;
- Complete guard;
- safe behavior when agent-brain decision parsing/API fails;
- runtime context supplied to the brain.

A keyword fallback that turns leader prose into Blueprint during brain failure is unsafe if it bypasses review/process guards.

---

## AskUser

Backend owns the wait using a oneshot sender.

Required safety properties:

- one pending sender;
- answer path consumes with `.take()`;
- all modal dismissal paths resolve it;
- Stop/abort cannot leave it hanging;
- stale/duplicate answers cannot satisfy a later question.

---

## Hackathon

Hackathon has real commands/events/state and must be audited as an extension of the core session state machine.

Stress:

- participant failure;
- partial group completion;
- cancel/Stop;
- pause/resume/restart;
- repeated/nested Hackathon action;
- rate limits;
- report handoff back to leader.

Do not treat existence of handlers as proof of deterministic recovery.

---

## Pause / Resume / Recovery

There are two different recovery concepts:

### Blueprint replay

`get_recovery_state` / `recover_session` can expose/replay already-persisted Blueprint sections for an incomplete session.

### Paused-session checkpoint resume

`pause_session` / `resume_session` use `SessionCheckpoint` state and can resume a paused autonomous loop.

These must not be conflated.

The pipeline audit must compare checkpoint fields with all safety-critical live state. If an already SubmitConfirmed turn cannot be resumed safely, manual recovery is preferable to duplicate-send.

---

## Memory Interaction

Phase 1 Memory exists and is file-backed.

For pipeline reliability, audit only:

- when a memory write becomes durable relative to turn commit;
- whether retry can duplicate a memory side effect;
- context budget/truncation;
- memory error behavior;
- restart consistency.

Memory errors in the router should remain non-fatal unless the missing state is itself safety-critical.

---

## Database / Lock Rules

Never run synchronous rusqlite work while holding an async-runtime thread unnecessarily.

For stores wrapped in `std::sync::Mutex`, use the established blocking helper pattern from async call sites.

Never hold a Tokio mutex guard across unrelated awaited browser/network work.

No `blocking_lock()` in live async/browser callbacks.

---

## Error Handling Rules

For every error path ask:

- was a side effect already performed?
- is the turn still marked active?
- can a retry duplicate work?
- is ownership released?
- is a pending oneshot/channel cleared?
- is partially assembled response state bounded/cleaned?
- does status accurately reflect failure?

Do not use `.unwrap()` / `.expect()` in live-session production paths except explicit unrecoverable startup contexts.

---

## Diagnostics

Maintenance-mode diagnostics include compact `get_diagnostic_brief` and deeper forensic exports.

The compact brief is the preferred routine artifact because giant raw snapshots/timeline clones previously consumed excessive memory/tokens.

Diagnostics must remain redacted:

- no cookie values;
- no OAuth codes/tokens;
- no API keys;
- no prompt/response bodies in browser forensic telemetry unless explicitly part of a user-requested transcript export.

Diagnostic traffic itself must not be capable of breaking critical event delivery.

---

## Current Known Reliability Targets

Audit current HEAD for these before implementation:

1. generation-bearing active event identity;
2. priority/reliable critical event delivery;
3. absolute response deadline;
4. physical SubmitConfirmed evidence;
5. baseline-aware response discovery;
6. robust generation-completion signal;
7. exact long-response transport/reassembly;
8. no destructive queue draining/search;
9. attempt-zero page reuse;
10. active conversation URL persistence;
11. provider-aware OAuth callbacks;
12. Qwen composer/polling cost;
13. delayed-hydration classification;
14. reviewer accounting;
15. Blueprint/Complete guards;
16. checkpoint state completeness;
17. AskUser/Hackathon races;
18. dead setup priming code/comments;
19. custom-participant origin/identity support.

---

## Current Verification Baseline

At pushed checkpoint `0cc76c9`, the previous reliability batch reported:

- `cargo fmt --check`: PASS
- `cargo check`: PASS
- `cargo test --quiet`: PASS, 146 tests
- `npm run build`: PASS
- `git diff --check`: PASS

A later local two-file transport batch reported only:

- `cargo fmt --check`: PASS
- `git diff --check`: PASS

while Cargo/frontend full verification did not complete in that runner due to a stale build lock / timeout.

Astra must record exact HEAD/worktree and not transfer verification claims from one state to another.

---

## Next Step

Do **not** start Skills or another broad implementation batch first.

Next step is an independent read-only end-to-end pipeline reliability audit, followed by one consolidated repair plan and deterministic stress tests.
