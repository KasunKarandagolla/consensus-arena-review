# Consensus Arena — IPC and Boundary Contract

## Rule of reality

This file documents **contract rules and stable categories**. Exact command/event names for a change must be checked against current Rust/TypeScript source because the Build lane was added after the older exhaustive IPC list.

Do not invent a new event/field from memory.

## Boundary categories

### Consult session lifecycle

Legacy commands/events cover:

- start/abort/pause/resume/recover consultation;
- setup/progress;
- routing/model state;
- blueprint sections;
- session completion/error;
- connected-account/browser diagnostics;
- session history/export/details.

Existing exact names in source/older contract remain valid until source proves otherwise.

### Owner interaction

Stable concept:

```text
backend emits owner question
→ frontend modal
→ frontend invokes provide_user_answer
→ backend resolves pending answer
```

Every close path must answer. Build may persist a delivery question before emitting the same owner-facing interaction.

### Build / Delivery lifecycle

The 2026-09-14 audits establish new IPC semantics for:

- starting Build without leader/participants;
- delivery phase/status updates;
- `WaitingForUser`;
- cancellation/abort;
- verified completion;
- explicit Apply.

The supplied audits do **not** provide a complete authoritative symbol/event-name list. Before touching this IPC, read the actual local source that implemented Delivery V1 and update this document if necessary.

## Serialization convention

A recurring risk in this project is Rust commands returning a JSON-serialized `String` while TypeScript assumes it received an already-parsed object.

For every command:

1. inspect Rust return type;
2. inspect whether it calls `serde_json::to_string`;
3. if yes, frontend receives a string and must `JSON.parse()`;
4. if command returns plain text by design, do not parse it.

Do not rely on generic `invoke<T>()` typing to transform a string at runtime.

## Event matching

For every backend `app.emit()` / window emit:

- exact event name must match frontend listener;
- payload field names/types must match;
- listener cleanup must occur on unmount;
- stale session/run generations must not update a newer UI state.

## Browser `arena://` boundary

Consult browser JS signals Rust using intercepted pseudo-navigation. Preserve:

- generic static init script;
- runtime agent identity;
- turn/session matching;
- synchronous callback channel requirements.

This boundary is consultation-specific. Do not use it as the Build worker/process IPC.

## Future Dagu boundary

If Dagu is validated/integrated, Arena should use a narrow CLI/REST contract for:

- start/read durable run;
- observe waiting/completion/failure;
- fetch root human task;
- complete human task with typed value;
- resume/reconcile run.

Do not mirror Dagu's entire internal state into frontend IPC. Frontend should receive Arena-level product status/decision events.
