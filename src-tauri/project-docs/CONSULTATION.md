# Consensus Arena — Consult Lane

## Purpose

The Consult lane is the original Consensus Arena mechanism. It remains useful as a **selective frontier-reasoning service** inside the broader product.

It should not be used for routine build/test/process mechanics.

## Current model set

Built-in consumer-chat participants:

- ChatGPT
- Claude
- Gemini
- DeepSeek
- Qwen
- GLM
- Kimi

Current source may also contain custom-participant support; verify real registry/source before changing participant assumptions.

## Roles

### Leader

Runs the expert discussion and decides what reasoning/input is useful.

### AgentBrain

A separate OpenAI-compatible programmatic model interprets the leader and routes the application. It is not a meeting participant.

Stable historical decisions include:

- no hardcoded round-robin debate;
- leader natural language is interpreted by AI;
- route one model when useful;
- compare several when useful;
- ask the user sparingly;
- progressively emit blueprint/advisory content;
- stop on completion/user abort.

Current source has grown beyond the original six-action documentation (for example Hackathon-related capabilities exist in current project context). When exact action variants matter, read the real enum/source rather than relying on historical counts.

## Browser transport

The current Consult lane deliberately uses at most two WebViews:

1. persistent leader;
2. shared navigating participant window.

Browser JS → Rust signals use the established `arena://` pseudo-protocol intercepted by Tauri navigation callbacks.

This design exists because authenticated consumer chats are a practical route to frontier reasoning without separate paid APIs.

## Current strategic role

Use Consult when the extra reasoning quality is worth latency/fragility, such as:

- architecture;
- product ambiguity;
- security challenge;
- difficult debugging hypothesis;
- high-risk migration;
- adversarial review;
- independent strategic comparison.

Do not invoke a panel for deterministic build/test/log/file operations.

## Shared frontier consultation seam

Consultation is also an optional Arena capability that a future Arena-owned
research, engineering, or reviewer work order may request. The current
implementation is the narrow `consultation` domain module, which reuses the
existing Hackathon OpenAI-compatible chat transport. It does not create a new
browser, MCP, search, or workflow runtime.

The Arena-owned request contract carries a request ID, work-order ID, origin,
bounded question, curated evidence, disclosure scope, selected provider
configuration ID, allowed transport, deadline, and response budget. The result
returns the same correlation IDs plus provider/transport, answer, curated
source references, timestamp, terminal status, and a sanitized failure reason.

Disclosure is fail-closed: evidence requiring a broader scope than the request
is rejected before provider invocation. Known stored credentials are redacted
from the provider prompt and returned answer, while credentials remain in
secure settings and are never part of the request/result contract. Provider
output is advisory evidence; it cannot change acceptance, candidate identity,
verification, release, or Safe Apply authority.

The current operation is a callable domain seam, not yet a durable work-order
runtime. Its Arena-owned caller must bind admission, cancellation/supersession,
candidate/version identity, and result persistence/reconciliation before
invoking it. The declared `origin` and disclosure scope are not authorization;
workers and renderers cannot use them to grant themselves access.

The existing consumer-web Consult journey remains the current browser
transport. The shared seam does not make consumer websites critical path and
does not assume that legacy `agent-message` or Hackathon events are sufficient
for non-Consult result correlation.

## AskUser

The existing AskUser path remains a reusable product primitive:

- backend owns the pending question/channel;
- frontend modal blocks/asks owner;
- every close path must answer, including dismissal as `"Cancelled"`;
- backend must consume the sender safely (`take()` pattern in current implementation).

Build/Delivery now reuses the owner-facing concept while persisting its delivery question before emitting the UI event.

## Named browser risks

When touching this lane, always check:

- no `blocking_lock()` in async / `on_navigation`;
- synchronous navigation callback must not use Tokio mpsc;
- callback should not capture agent identity by value;
- generic init script remains generic/static;
- agent + turn identity must both match responses;
- backend/frontend event names and fields match;
- setup/navigation generations are not confused;
- duplicate-send protection remains intact;
- browser diagnostics remain gated appropriately by Maintenance mode where current source requires it.

See `RELIABILITY.md` and real source before editing.
