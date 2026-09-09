# Consensus Arena — Active Decisions & Current Handoff

## Purpose

This file is the **current active decision set**, not a historical diary.
Historical implementation detail belongs in Git history and `project-docs/audits/`.

**Read this file first. Then read the current source before making claims about implementation.**

### Reality hierarchy

1. Current source/worktree and current runtime evidence.
2. Current canonical docs in `src-tauri/project-docs/`.
3. Historical audit reports and old comments.
4. Old status summaries.

If source/runtime conflicts with this file, **source/runtime wins and this file must be corrected**.
Do not preserve a stale statement merely because it once had a decision number.

---

## Product

Consensus Arena is a native Tauri 2.x desktop application that orchestrates an AI expert panel to produce verified project blueprints.

- Participant models use the user's personal web accounts; no paid participant-model API keys.
- A separate OpenAI-compatible agent brain performs orchestration decisions.
- The leader model is the meeting lead, not merely one turn in a round-robin debate.
- The main application UI presents the blueprint and live status, not a wall of model chats.
- The browser layer is constrained to one persistent leader WebView and one shared navigating participant WebView.

The intended product property is **reliable autonomous orchestration under unreliable free web-model conditions**: slow pages, DOM changes, rate limits, challenges, long generations, stale events, and model outages must fail safely rather than duplicate work or corrupt state.

---

## Current Reliability Status — 2026-09-09

### Do not describe the backend as “fully complete” or “beta-reliable” yet

Feature coverage is broad, but the core session pipeline is still under reliability hardening.
The browser bootstrap problem was substantially repaired, while active-turn transport, response capture, continuity, OAuth, review guards, and deterministic recovery still require audit/hardening.

### Known safe checkpoint

The last explicitly confirmed pushed checkpoint in this project conversation is:

`0cc76c9 — checkpoint: pipeline reliability and priming fixes`

A later local transport batch may exist in the worktree when this document is read. That batch was reported to add:

- absolute response deadline behavior;
- stronger physical submit confirmation;
- broader baseline-aware response candidates;
- chunked `response-start` / `response-chunk` / `response-end` transport replacing the old 8,000-character response truncation.

That later batch was only reported as `cargo fmt --check` + `git diff --check` verified in its Codex runner; full `cargo check`, `cargo test`, and frontend build did not complete there because of a stale build lock / runner timeout.

**At the start of any new audit, run `git rev-parse HEAD` and `git status --short`; inspect current source and do not assume either checkpoint state.**

---

## Runtime Evidence That Must Be Preserved

### Browser-first/post-load repair

The following runtime evidence is established:

- Native WebKitGTK `Version/60.5` is **not itself the root cause** of Claude failures.
- Normal Epiphany using the same WebKitGTK engine successfully completed Claude + Cloudflare + Google login.
- Arena with the same environment succeeded only when the invasive model runtime was not installed at document start.
- Production architecture was changed so provider pages bootstrap first and the static Arena runtime activates after `PageLoadEvent::Finished` on eligible provider documents.
- Claude subsequently loaded successfully, authenticated, reached `/new`, installed automation, detected its composer, received its first leader task/priming envelope, and returned a captured response without observed re-priming in the tested run.

Do not reintroduce document-start `GENERIC_INIT_SCRIPT` installation or a forced Linux UA without new source/runtime evidence and explicit approval.

### Qwen evidence

Two live matrices showed:

- Qwen could load and become composer-ready.
- The old false login/verification classification improved; a merely loading Qwen page was no longer treated as login/verification in the successful run.
- Qwen visibly accepted a first prompt and visibly generated a response, but Arena remained `active_waiting_for_response` and the pipeline stalled.
- In one Qwen-leader run, Claude participant loaded but was never injected because the orchestration never progressed beyond the missing Qwen leader response. This is **not evidence of an agent-brain routing failure**; the brain had not received the leader response yet.
- Qwen composer candidate counts grew very large (tens to 100+), showing generic DOM discovery/polling remains too broad/noisy.

### OAuth evidence

- Claude + Google OAuth works in the repaired browser-first path.
- Google OAuth remains unreliable/incomplete for other providers such as GLM/DeepSeek in manual testing.
- Current implementation must be audited as a provider-aware auth flow, not treated as “Google is supported everywhere” merely because `accounts.google.com` is allowlisted.

---

## Active Architecture Decisions

### D-001 — Two Arena-managed model browsing contexts

The core architecture remains:

1. `arena-leader` — persistent leader WebView.
2. `arena-nav` — shared navigating participant WebView.

Do not introduce a third persistent Arena-managed model WebView.

**Open architecture question:** current code may permit transient site-requested OAuth/new-window behavior. Do not assume this is exempt from the physical two-WebView/resource constraint. An audit must establish what Tauri/Wry actually creates and whether the flow is compatible with the product limit.

### D-002 — `arena://` pseudo-protocol remains the JS→Rust mechanism

Retain the `arena://` navigation-based bridge. Do not replace it casually.

However, “retain the protocol” does **not** mean every current event shape is reliable. Active-turn identity, critical-event delivery, long-response transport, and stale-event handling are still audit targets.

### D-003 — Browser first, automation second

Model WebViews must bootstrap provider pages naturally.

- No full invasive Arena runtime at document start.
- Provider login/OAuth/challenge pages remain browser-owned.
- On eligible provider documents, Arena installs its static generic runtime after page load.
- Runtime installation is idempotent per document.
- Agent identity is assigned at runtime, not captured in a long-lived navigation closure.

### D-004 — Setup means readiness/authentication only

Setup is **not a priming conversation**.

Correct setup responsibility:

`navigate/bootstrap → authentication/challenge if needed → post-load automation → stable composer → setup complete`

Setup must not claim a model is primed because text appeared in its composer.

### D-005 — Priming travels with the first useful submitted task

The first leader active task contains:

`leader priming + CURRENT ARENA TASK + real first task`

The first routed task to a participant contains:

`participant priming + CURRENT ARENA TASK + real routed question`

Later tasks to that model omit priming.

**Important:** visible/injected text is not proof of priming. The first envelope should become committed only when physical submit is proven.

### D-006 — Submit confirmation is the safety boundary

Intended invariant:

`Ready → PromptInjected → SubmitConfirmed → WaitingForResponse → ResponseCaptured → ConversationPersisted → TurnCommitted`

Once an exact turn is physically SubmitConfirmed, the system must not automatically re-inject, click Send again, or navigate/restart that same turn merely because of Ready, SPA navigation, response delay, or recovery telemetry.

Current source may not yet enforce this invariant across every path. Audit it rather than assuming it is DONE.

### D-007 — Active identity must include generation

The intended stale-response identity is:

`agent_id + turn + setup/document generation`

At checkpoint `0cc76c9`, diagnostics contain generation metadata, but core `NavEvent::Response`, `Done`, `ActiveSubmitReport`, and `BrowserState.active_turn` are not uniformly generation-bearing at the event boundary.

Do not state “generation-safe active events are complete” until current source proves it.

### D-008 — Agent brain has seven actions

Current decision family:

- Route
- RouteCompare
- Blueprint
- Continue
- Complete
- AskUser
- Hackathon

The leader remains the meeting lead; the brain interprets/executes decisions.

**Prompt rules are not runtime guarantees.** If prompts say every required reviewer must review a module before Blueprint/Complete, Rust must mechanically enforce any guarantee the product depends on. Current source has known concerns around reviewer accounting and early Blueprint/Complete; these remain audit targets.

### D-009 — AskUser is backend-owned and blocking

AskUser suspends the orchestration loop using a oneshot channel.

Required invariants:

- only one pending AskUser sender;
- `provide_user_answer` consumes the sender with `.take()`;
- option click, custom submit, Escape, backdrop close, and other dismissal paths all resolve the backend wait;
- Stop/abort cannot leave a permanent wait.

### D-010 — Hackathon is first-class but advisory

Hackathon is a real action and has backend/frontend state/events.
Its output is advisory material for the leader; it is not automatically a Blueprint section.

Do not assume Hackathon pause/resume/cancellation/partial-failure behavior is reliable merely because the action exists. It is part of the pipeline audit scope.

### D-011 — Session and data persistence

Persistent stores include settings, transcript, blueprint, memory, and SessionVault data.

`SessionVault` is **file-backed in current checkpoint source** using `app_data_dir/session_vault.db` with an in-memory fallback only if opening the file-backed store fails.

The old statement “SessionVault remains in-memory / out of scope” is rejected.

Conversation continuity is still incomplete if active conversation URLs are not saved after real responses. File-backed storage does not help if the runtime never writes the new `/chat/...` URL.

### D-012 — Memory Phase 1 exists; pipeline reliability has priority over Skills

Phase 1 Memory is implemented.

The current next milestone is **core pipeline reliability**, not Phase 2 Skills.
Do not start Skills work until the session pipeline is beta-reliable and the seven-provider acceptance matrix passes.

### D-013 — Custom participants exist

Checkpoint source includes persisted custom participants and a merged runtime participant registry.
Built-in IDs are reserved and custom participant base URLs are validated.

Custom participant browser-origin/automation behavior must remain generic; do not bake all supported origins directly into the static runtime.

### D-014 — Frontend visual ground truth

`src-tauri/project-docs/mockup/preview.html` remains the approved design reference.

- Blue / Light / Dark themes.
- Local Inter + JetBrains Mono fonts.
- No runtime font CDN.
- Main view shows Blueprint content, not raw participant responses.

### D-015 — Source over status counters

Do not hardcode documentation claims such as:

- “N commands”;
- “N modules”;
- “N AppState fields”;
- “all IPC PASS”;
- “backend fully complete”.

Those counts drift and have repeatedly caused false confidence. Count/inspect source when relevant.

---

## Known Open Reliability Areas — Audit, Do Not Assume

These are not all necessarily unfixed in the current worktree; verify source at the audit HEAD.

### Physical active-turn protocol

- generation-bearing active event identity;
- critical-event delivery vs lossy telemetry;
- physical SubmitConfirmed evidence;
- response baseline/new-message detection;
- long-response integrity and chunk assembly;
- overall absolute response deadline;
- monitor safety-limit/error behavior;
- safe post-submit recovery without duplicate side effects.

### Browser reuse and continuity

- attempt-zero reuse of an already healthy participant page;
- provider `/new → /chat/<id>` persistence after successful active turns;
- restoration after switching participants;
- stale window/registry reconciliation;
- broad composer-candidate scanning and passive polling pressure;
- delayed SPA hydration vs false empty-shell classification.

### OAuth

- global IdP host vs provider auth/callback host policy;
- exact/dot-boundary host validation;
- denied-host diagnostics without query/token leakage;
- interaction with the two-WebView constraint.

### Logical panel state

- consultation attempted vs successful review;
- RouteCompare roster validation/deduplication;
- backend-owned review coverage;
- Blueprint guard;
- Complete guard;
- brain-failure fallback must not bypass review guarantees;
- brain context must include the process state its prompt expects.

### Pause/resume/recovery

- checkpoint must contain enough state to avoid duplicate submit after restart;
- review/first-envelope/pending-action state must not silently reset;
- Stop/abort/AskUser/Hackathon races;
- `recover_session` blueprint replay is distinct from checkpoint-based `resume_session`.

---

## Rejected / Superseded Facts

The following statements must not be reintroduced:

- “Backend is fully complete / all pipeline behavior verified.”
- “Next step is Phase 2 Skills.”
- “Setup injects priming and waits for the user to press Send.”
- “A model is primed when priming text is present in the composer.”
- “GENERIC_INIT_SCRIPT runs via Tauri `initialization_script(...)` on every navigation.”
- “Linux production forces a Safari/Chrome compatibility UA.”
- “Native WebKitGTK Version/60.5 caused the Claude blank-shell regression.”
- “SessionVault remains in-memory.”
- “All active events are already generation-safe.”
- “All IPC payloads have been fully stress-audited and are all PASS.”
- “Kimi canonical URL is `https://www.kimi.com/`.” Current checkpoint source uses `https://kimi.ai/`.
- “Prompt-level reviewer/module rules guarantee Blueprint/Complete correctness.”
- “A temporary OAuth popup is automatically harmless with respect to the two-WebView constraint.”

---

## Non-Negotiable Technical Constraints

- Zero paid API keys for participant models.
- Native Tauri 2.x desktop application.
- Target system: 4GB RAM / low-end CPU; app memory target under ~2GB.
- One persistent leader WebView + one shared nav WebView; do not casually introduce a third model context.
- No `blocking_lock()` in async/live browser paths.
- No Tokio mpsc inside synchronous `on_navigation`; use the standard-channel ingress design.
- No `.unwrap()` / `.expect()` in live-session production paths except explicit unrecoverable startup/test contexts.
- `GENERIC_INIT_SCRIPT` remains one static generic runtime, not per-agent source generation.
- `on_navigation` must not capture an agent ID by value; identity is runtime-derived.
- AskUser close paths must resolve the backend wait.
- IPC source and frontend parsing must agree; JSON-string-returning commands must be parsed before use.
- Critical side effects must be idempotent: retry only when source proves the previous side effect did not happen.

---

## Immediate Project Priority

1. Protect the latest checkpoint/worktree.
2. Run one independent, read-only, source-first Astra pipeline audit.
3. Reconcile that report against source/runtime evidence.
4. Implement a consolidated reliability batch rather than one bug per session.
5. Add deterministic stress tests for side-effect, event-order, response-integrity, review, and recovery invariants.
6. Run one seven-provider acceptance matrix.
7. Only then call the core beta-reliable and resume Skills/features.
