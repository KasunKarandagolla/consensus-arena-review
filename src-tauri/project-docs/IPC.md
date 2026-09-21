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

The current Delivery prerequisite command is `get_dsh_prerequisite`. It
returns a JSON-serialized string with `available`, `compatible`,
`executable`, `version`, and owner-facing `message` fields. The frontend must
`JSON.parse()` this command result. It does not return credentials or raw DSH
output.

### Product OS research authority

The current source exposes these JSON-string commands for the bounded durable
research seam:

- `create_product_research_work_order`
- `run_product_research_work_order`
- `create_product_fact_verifier_work_order`
- `run_product_fact_verifier_work_order`
- `create_product_web_research_work_order`
- `run_product_web_research_work_order`
- `create_product_web_fact_verifier_work_order`
- `run_product_web_fact_verifier_work_order`
- `cancel_product_work_order`
- `get_product_os_snapshot`
- `get_latest_product_os_snapshot`
- `admit_product_ambiguity`
- `provide_product_owner_decision`

All multiword arguments use the snake-case command rename convention. These
operations validate Arena-owned work-order/project identity and persist
sanitized records through the existing backend store. WebDiscovery uses the
built-in OpenCode `plan` role with bounded `websearch`; proposals are admitted
as unverified evidence and a distinct FactVerifier work order performs final
disposition. A researcher can submit only an unverified proposal; the renderer
cannot set verification, resolver, authority, or gate fields. The verifier and
owner-decision commands are semantic Arena operations, not direct
ProductAuthority record mutation.

### Production Product OS coordinator — Milestone 07

The founder-facing production sequencing boundary exposes these
JSON-serialized commands:

- `start_product_project(founder_idea, repo_path)`
- `get_product_coordinator_status(run_id?)`
- `answer_product_question(run_id, selected_option)`
- `cancel_product_project(run_id, reason)`
- `resume_product_project(run_id)`

These commands admit or reconcile one Arena-owned run. They do not expose raw
stage transitions, `GateInput`, `ProductAuthorityRecords`, DeliveryState, or a
worker self-verification operation. Multiword arguments use
`rename_all = "snake_case"`; serialized results must be parsed by the
renderer. M07 runtime-proves the production Linux backend path; in-flight
restart/reconciliation and native GUI IPC behavior remain separate evidence.

The current known-source proof uses direct official GitHub HTTPS retrieval, not
GitHub MCP. The current WebDiscovery proof uses OpenCode 1.18.31/Muse Spark's
hosted Exa websearch path without manual relay. Search output is not a verified
fact until the independent verifier path succeeds.

M09A extends the status payload with the Arena-owned route/stage context,
omitted-stage reasons, execution epoch, bounded remediation state, and
`pending_owner_decision`. Renderer options map to canonical backend decisions:
`authorize_validation_experiment`, `authorize_narrow_build`, `stop`, and
`pivot` (with the existing build/apply/release options retained where their
question applies). Validation experiments return to Decide and never directly
enter Delivery. These are serialized status/decision values, not renderer
authority to mutate `ProductAuthorityRecords`, gates, verification, or Apply.

### Shared frontier consultation

The backend `consultation` domain contract is provider-neutral at the Arena
boundary and transport-specific only inside the selected adapter. A request
contains `request_id`, `work_order_id`, `origin`, `question`, curated evidence,
`disclosure_scope`, `allowed_provider`, `allowed_transport`, `deadline_ms`, and
`budget_tokens`. A result contains the correlation IDs, provider, transport,
answer, source references, conversation reference, timestamp, status, and a
sanitized error.

The current source exposes this as a Rust domain operation for an Arena-owned
internal caller; it is not a renderer- or worker-owned authority command. The
declared `origin` and disclosure scope are metadata and are not authorization.
The caller must bind the operation to current Arena admission and lifecycle state. If a
Tauri command is added later, it must use the same serde contract and must
persist or reconcile the result before emitting any notification. Unknown, cancelled,
superseded, or mismatched results must fail closed. Do not reuse uncorrelated
legacy Consult events as the authoritative result channel.

### Credential settings

The `get_agent_brain_config`, `get_fallback_brain_config`, and
`get_secondary_brain_config` commands return JSON-serialized strings. Their
`api_key` field is always empty; `api_key_configured` reports whether a key
is saved. The renderer must never receive an Agent Brain credential.
`get_credential_storage_status` also returns a JSON-serialized string with
`available`, `migration_pending`, and owner-facing `message` fields.

`clear_brain_credential` accepts `{ kind }`, where `kind` is `primary`,
`fallback`, or `secondary`; it returns no credential data. `get_hackathon_config`
returns the frontend-safe projection without model API keys. Memory export and
restore operate on `memory.db`, separate from settings and OS credentials.

Diagnostic capture/export is available only after the owner enables
Maintenance mode. `export_browser_diagnostics` returns a JSON-serialized
string describing the locally saved export directory. Exports are retained in
the app-data directory. Starting a new export prunes matching timestamped
export directories so at most five matching directories are targeted for
retention; Arena does not record a validity marker for each bundle. Dated
application logs are pruned after 14 days. No diagnostic upload occurs.

Saving a brain with a blank key preserves its saved key. The explicit
`clear_brain_credential` command takes `{ kind }`, where `kind` is `primary`,
`fallback`, or `secondary`, and removes that saved key from the operating
system credential store. Hackathon keys are keyed by stable model ID and are
removed when their model is deleted from `save_hackathon_config`.

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
