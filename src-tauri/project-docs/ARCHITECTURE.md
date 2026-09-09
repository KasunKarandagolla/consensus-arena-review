# Consensus Arena — Architecture

## Purpose

This document describes the **current intended architecture and reliability boundaries**.
It deliberately avoids fragile counts and historical batch narratives.

If this document conflicts with current source/runtime, source/runtime wins.

---

## Product Identity

Consensus Arena is a native Tauri 2.x desktop application that orchestrates multiple AI web models as an autonomous expert panel to produce project blueprints.

The leader model runs the meeting. A separate OpenAI-compatible agent brain interprets the leader's decisions and controls routing, Blueprint, AskUser, Hackathon, Continue, and Complete actions.

Participant models use the user's personal web accounts; no paid participant-model API keys are required.

This is not a round-robin debate UI and not a multi-column chat monitor.

---

## Reliability Goal

The core architecture must remain correct under:

- slow SPA hydration;
- provider DOM changes;
- provider redirects and `/new → /chat/...` route changes;
- Cloudflare/login/OAuth flows;
- rate limits and server-busy states;
- dropped/late/duplicate browser signals;
- long model generations and temporary generation pauses;
- response extraction failure after a successful Send;
- user Stop/pause/recovery races;
- participant failure or unavailability;
- app restart.

The system must prefer **safe failure/manual recovery** over duplicate model side effects.

---

## Major Components

### React/Tauri main window

Owns the user-facing workflow:

- setup;
- live status;
- Blueprint rendering;
- session history;
- Connected Accounts;
- AskUser;
- CAPTCHA/rate-limit recovery;
- diagnostics;
- settings/memory.

Raw model conversations remain in model WebViews and are not the main product UI.

### Rust backend

Owns:

- session configuration/lifecycle;
- browser ownership/navigation;
- JS→Rust event handling;
- active-turn orchestration;
- agent-brain decisions;
- participant routing;
- Blueprint persistence;
- AskUser/Hackathon state;
- checkpoints/recovery;
- transcript/session/memory stores;
- diagnostics.

### Agent brain

A user-configured OpenAI-compatible model, separate from the meeting participants.

Current decision family:

- Route
- RouteCompare
- Blueprint
- Continue
- Complete
- AskUser
- Hackathon

Prompt rules help the brain behave well, but important correctness guarantees must be enforced by Rust state, not only by prompt obedience.

### Model WebViews

Two long-lived Arena-managed browsing contexts:

- `arena-leader` — persistent leader context;
- `arena-nav` — shared participant context.

The shared nav window navigates/reuses conversations for non-leaders.

---

## Browser Architecture

### Hard rule: browser first, automation second

The provider document must bootstrap without the full Arena automation runtime executing at document start.

Current intended lifecycle:

```text
create/reuse named WebView
→ navigate provider
→ provider/login/challenge/OAuth bootstrap naturally
→ PageLoadEvent::Finished
→ validate current provider/origin/state
→ set current runtime identity
→ post-load eval of static GENERIC_INIT_SCRIPT
→ composer/readiness detection
```

The static runtime remains generic across providers and is installed idempotently per document.

Do not reintroduce `.initialization_script(GENERIC_INIT_SCRIPT)` on model builders without explicit new evidence and approval.

### Linux browser identity

Production uses WebKitGTK's native UA; no forced Safari/Chrome compatibility UA should be assumed.

Native `Version/60.5` was exonerated as the root cause of the Claude blank-shell failure by runtime testing.

### Linux WebKit context

Current code can enable an `epiphany-like` WebKit context mode for diagnostics/compatibility, including ITP behavior. The provider bootstrap repair, not UA spoofing, was the decisive Claude fix.

Astra should inspect the exact current default/context switch behavior rather than assuming the environment variable is production-required forever.

### OAuth/new-window tension

Current browser code contains narrow new-window/OAuth handling.

The product constraint remains two Arena-managed model browsing contexts. A transient provider-created popup may or may not create a third physical WebView/resource context depending on Tauri/Wry behavior. This is an **audit question**, not a resolved exemption.

### arena:// protocol

The JS runtime communicates with Rust by pseudo-navigation intercepted in `on_navigation`.

The architectural decision is to retain this bridge.

However, individual event formats are implementation details and have evolved. At checkpoint `0cc76c9`, core active events still used agent+turn without a uniformly generation-bearing event key. A later local batch may add chunked response transport.

Audit current source before documenting exact wire variants.

### on_navigation rules

- Synchronous callback.
- Standard-library mpsc ingress, not Tokio mpsc inside the callback.
- No async lock / `blocking_lock()` inside callback.
- Do not capture agent identity by value.
- Identity is derived from runtime state / URL signal contents.
- Critical session events must not be silently lost because telemetry filled a bounded queue.

---

## Session Flow

### Phase 1 — User configuration

User selects:

- project brief;
- session type;
- leader;
- at least one participant / supported roster according to UI validation;
- brain configuration.

Built-ins currently include ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, and Kimi.
Current source also supports persisted custom participants through a merged registry.

### Phase 2 — Browser setup/readiness

Setup is **not a priming conversation**.

For each selected model:

```text
resolve provider
→ create/reuse correct named WebView
→ navigate/reuse provider page
→ allow login/challenge/OAuth if needed
→ post-load automation activation
→ detect stable usable composer
→ setup-agent-complete
```

No role-priming message should be sent in setup.

Historical commands/events such as `setup_agent_sent` may remain for legacy/manual recovery compatibility; they must not redefine the current setup semantics.

### Phase 3 — First real leader turn

The leader's first useful submitted message contains:

```text
[rendered leader priming]

--- CURRENT ARENA TASK ---

[real first task/project brief]
```

The first envelope is not considered committed merely because text appears in the composer.
It should become committed when physical submission is proven.

### Phase 4 — Active leader response

Intended physical turn state:

```text
Ready
→ PromptPrepared
→ PromptInjected
→ SubmitConfirmed
→ WaitingForResponse
→ ResponseCaptured
→ ConversationUrlPersisted
→ TurnCommitted
```

After `SubmitConfirmed`, the same active turn must not automatically be resubmitted or restarted.

The exact active identity should be at least:

`agent_id + turn + setup/document generation`

If current source carries less identity, that is a reliability defect to audit.

### Phase 5 — Agent-brain decision

Only after a leader response is captured does the backend call the orchestration brain.

Therefore a participant not receiving a routed prompt can be a downstream effect of a missing leader response, not necessarily a brain-routing failure.

The brain returns one of the seven actions.

### Phase 6 — Participant route

For a participant's first real consultation:

```text
[rendered participant priming]

--- CURRENT ARENA TASK ---

[real routed question]
```

Later consultations omit priming.

A healthy already-loaded same-agent nav page should be reusable without an unnecessary navigation.
After a successful response, the real provider conversation URL should be persisted before the shared window is reused.

### Phase 7 — Leader synthesis

Participant findings are returned to the leader.
The leader decides what to adopt or reject.

### Phase 8 — Blueprint / review

Prompt design expects review and synthesis before final Blueprint sections.

Backend correctness must distinguish:

- consultation attempted;
- participant response successfully captured;
- reviewer successfully counted;
- section eligible to commit.

Do not treat prompt rules as proof that Rust already enforces review coverage.

### Phase 9 — Complete

Complete should end the autonomous loop only when deterministic backend-known work is finished.

A robust implementation should guard against early Complete when known required review/AskUser/Hackathon/process state remains.

---

## AskUser Architecture

AskUser is backend-owned:

```text
brain decides AskUser
→ backend stores oneshot sender
→ emits agent-ask-user
→ orchestration loop awaits receiver
→ frontend answers or dismisses
→ provide_user_answer takes sender
→ answer returned to leader context
```

Every UI close path must resolve the backend wait.

Audit Stop/abort/restart races and stale/duplicate answers.

---

## Hackathon Architecture

Hackathon is a first-class optional advisory workflow.

Core properties:

- invitation/run state exists separately from normal Route/RouteCompare;
- group outputs are advisory material, not automatic Blueprint sections;
- partial failure, cancellation, pause/resume, nested requests, and restart must fail deterministically;
- the normal two-WebView/resource architecture still applies to participant access.

Do not assume all of these properties are currently enforced; they are pipeline-audit targets.

---

## Review / Blueprint Reliability Boundary

Important product rule:

**The leader remains the decision-maker, but the backend must enforce process invariants that cannot safely depend on LLM memory/obedience.**

Recommended minimal backend-owned review state includes:

- required reviewers for current review cycle/section;
- successful reviewers;
- failed/unavailable reviewers;
- current cycle;
- pending section/proposal;
- committed Blueprint progress.

The brain should receive this authoritative state rather than inventing it.

---

## Response Capture / Transport

### Baseline requirement

Response detection should compare post-submit DOM/message changes against a pre-submit baseline so an old visible answer is not returned as the new answer.

Selectors are hints, not the sole definition of a response.

### Completion requirement

Stable text alone is not always proof generation finished.
Prefer generation-state evidence plus stable response; otherwise use a conservative bounded fallback.

### Long responses

The old architecture encoded a response into one URL and truncated to ~8k characters. That is not acceptable for long technical outputs.

A later local transport batch was reported to implement bounded `response-start` / `response-chunk` / `response-end` reassembly with length/checksum validation. Treat this as **candidate current source**, not an assumed verified fact, until the audit confirms current HEAD and tests.

---

## State and Persistence

### Persistent stores

Current architecture includes local SQLite-backed persistence for:

- settings;
- transcripts;
- Blueprint sections;
- memory;
- SessionVault conversation URL/cookie metadata.

Checkpoint source at `0cc76c9` opens `session_vault.db` under the app data directory, with fallback behavior if file opening fails.

### Conversation continuity

Persistent storage only helps if active turns write the real current conversation URL after provider-created chat navigation.

The audit must verify:

- URL validation;
- post-response save;
- same-agent reuse;
- restoration after switching participants;
- login/challenge/OAuth URLs never becoming saved chat URLs.

### Recovery concepts are distinct

- `recover_session` historically replays existing Blueprint sections for an incomplete session.
- `pause_session` / `resume_session` use a checkpoint-based flow for paused sessions.

Do not conflate Blueprint replay with deterministic active-loop resume.

Checkpoint data must contain enough safety-critical state to avoid duplicate model side effects after restart.

---

## Memory

Phase 1 Memory is implemented as a local SQLite system with bounded context selection and provenance/reliability records.

Memory must remain **non-fatal to the live session**: memory DB degradation should not corrupt or abort the core orchestration loop.

For pipeline auditing, focus only on:

- when memory is read/written;
- retry duplication of durable memory effects;
- context size/truncation;
- recovery consistency.

Do not spend the Astra audit budget re-auditing unrelated memory CRUD internals.

---

## Frontend Architecture

Current UI ground truth: `src-tauri/project-docs/mockup/preview.html`.

Relevant pipeline surfaces:

- Setup view;
- active status / Stop;
- AskUser modal;
- CAPTCHA/rate-limit overlays;
- Connected Accounts;
- recovery/pause controls;
- Diagnostic Brief / maintenance diagnostics;
- Blueprint rendering.

Visual styling is not part of the pipeline reliability audit unless a UI path can strand or duplicate backend state.

---

## Supported Participants

Built-in source baseline:

| ID | Name | Base URL |
|---|---|---|
| chatgpt | ChatGPT | `https://chatgpt.com` |
| claude | Claude | `https://claude.ai` |
| gemini | Gemini | `https://gemini.google.com` |
| deepseek | DeepSeek | `https://chat.deepseek.com` |
| qwen | Qwen | `https://chat.qwen.ai` |
| glm | GLM | `https://chat.z.ai/` |
| kimi | Kimi | `https://kimi.ai/` |

Current source also has a merged custom-participant registry. Built-in IDs remain reserved.

Provider-specific DOM selectors are not architectural truth; they can change at any time.

---

## Resource Constraints

Target machine is low-end (~4GB RAM), with an app budget around 2GB.

Design implications:

- no extra persistent model WebViews;
- bounded queues/diagnostics;
- bounded response assembly;
- bounded history/context;
- no expensive whole-DOM polling when event-driven/debounced observation is sufficient;
- no unbounded maps/vectors tied to turns/sessions;
- avoid repeated provider reloads.

---

## Current High-Risk Architecture Questions

A read-only pipeline audit must answer these from current source:

1. Are active events generation-safe end to end?
2. Can critical Submit/Response/Done events be dropped behind telemetry?
3. Is physical Send truth independent of a single bridge ACK?
4. Is response extraction provider-neutral enough for Qwen and future DOM changes?
5. Are long responses exact and bounded?
6. Can post-submit recovery ever duplicate a Send?
7. Does a hard timeout stay absolute under event traffic?
8. Are healthy participant pages reused without unnecessary reload?
9. Are real conversation URLs saved after active responses?
10. Is OAuth provider-aware and domain-safe?
11. Are review coverage, Blueprint, and Complete mechanically guarded?
12. Does checkpoint/resume preserve enough state for idempotent recovery?
13. Can AskUser/Hackathon/Stop races strand the loop?
14. Can passive polling/diagnostics overwhelm the low-resource system?
15. Do custom participants receive correct runtime origin/identity handling?

---

## Roadmap Priority

1. Pipeline reliability audit and consolidated repair.
2. Deterministic stress tests.
3. Seven-provider manual acceptance matrix.
4. Beta reliability checkpoint.
5. Only then resume Skills/tools/self-improvement roadmap work.
