# Consensus Arena — Backend Guide

## 2026-09-17 release closure status

The DSH source integration is blocked by its current external-worker probe:
two matching clean npm installs timed out on Arena's exact headless profile
check, and neither Muse task ran. The worker assumption is materially
challenged; Astra Trigger A is pending. The M07 production coordinator instead
runtime-proves the qualified OpenCode Delivery path on Linux; the DSH path and
Tauri GUI path remain unqualified.

Agent Brain primary/fallback/secondary credentials and Hackathon model keys
now pass through an injectable `CredentialStore` boundary backed in production
by the OS credential store. SQLite legacy values migrate only after secure
write/read-back; failures preserve access and set a visible pending status.
Configuration commands serialize an empty API key plus a configured flag.
See `audits/secure-credential-storage.md` for migration tests and separate
native platform results.

## Current backend shape

Rust/Tauri backend now supports two distinct product lanes:

- **Consult**: browser-backed leader/participant orchestration.
- **Build / Delivery**: isolated Git/DSH/verification loop.

Do not describe the backend as “feature-complete” globally. Consultation is mature; Delivery is a new V1 with important runtime/packaging/next-workflow work still open.

Do not preserve historical numeric claims such as exact module/command/AppState counts without recounting current source.

## Major responsibility groups

### Application/orchestration state

Current source contains shared application state for:

- consultation orchestrator;
- `SessionRuntime` concurrency/ownership;
- settings;
- transcripts;
- blueprint storage;
- session vault/browser state;
- AgentBrain(s);
- AskUser state;
- memory;
- model health/reliability;
- diagnostics/recovery;
- Build/Delivery state introduced by the 2026-09-14 slice.

Read `orchestrator.rs` / real `AppState` before relying on field names/counts.

### SessionRuntime

`SessionRuntime` is a real concurrency/ownership authority and is reused by Delivery.

Important properties in current project context:

- exclusive start/resume admission;
- run-generation/owner semantics;
- owner-safe abort/await;
- pause/run transitions;
- stale-owner protection.

Do not add a second competing task-ownership primitive without evidence.

### AgentBrain

Programmatic OpenAI-compatible orchestration intelligence.

Current project supports primary/fallback/secondary brain concepts. Exact decision enum/actions have evolved; read real source before editing decision contracts.

For future Build architecture, do not force every deterministic state transition through an LLM. Use AI at semantic boundaries; use code for known transitions.

### Browser backend

Consult-only transport. Key historical constraints remain:

- at most two WebViews in this lane;
- static/generic init script;
- `arena://` navigation protocol;
- synchronous navigation channel discipline;
- runtime agent identity from page/window state, not captured closure values;
- provider-specific behavior outside generic init script where possible.

### Persistent stores

The project has disk-backed stores for settings, transcripts, blueprint/session data, and memory. DB work that can block must remain off the async runtime using the project's established blocking helper pattern.

Milestone 05B reuses `TranscriptStore` for the durable Product OS boundary.
Its `product_authority` and `product_work_orders` tables persist the current
authority snapshot and the small research/verifier work-order record. This is
not a second database or event log. The live task lease, cancellation, and
run-generation checks remain in `SessionRuntime`; reopen marks an unexplained
persisted `Running` order as `ReconciliationRequired` rather than completing
it.

The runtime module admits a bounded official GitHub metadata query and a
bounded OpenCode WebDiscovery query, persists sanitized `Unverified` proposals,
and requires a distinct current Fact Verifier work order before finalization.
Product OS commands expose admission, execution, cancellation, snapshot,
ambiguity, and owner-decision operations; they do not expose direct mutation
of ProductAuthority records. M06 runtime-proves the fresh Linux
research-to-BuildPackage/Delivery path, while Windows, packaging installation,
and native GUI remain separate release evidence.

### Memory

Phase 1 memory is implemented, not merely a future specification. The
production SQLite store and its `run_blocking` access boundary are runtime
qualified by the isolated Phase 1 exercise; native UI and live provider
integration remain unproven. See `MEMORY.md` and
`audits/memory-phase1-runtime-qualification.md`.

### Delivery backend

Latest audit establishes source behavior and selected production-component
tests, including:

- clean repository admission;
- app-data worktree creation;
- bounded DSH subprocess;
- acceptance freeze/protected hashes;
- conservative verifier execution;
- bounded repair;
- persist-before-notify `WaitingForUser` handling and a source recovery path;
- explicit Apply guards, with dirty/changed/non-fast-forward refusals tested.

The production-service restart/answer/continue scenario and successful Apply
have not been runtime-qualified in the current closure pass; see the scenario
matrix in `audits/delivery-v1-backend-runtime-qualification.md`.

See `DELIVERY.md`. Exact file/module/command names must come from current source; the supplied audit documents semantics, not a complete symbol table.

Before Delivery admission, the backend checks the external DSH prerequisite
without exposing credentials or mutating the user's system. The current
source prerequisite probe expects DSH `0.1.5-rc.1` with a working `headless`
profile; the frontend receives the serialized result of
`get_dsh_prerequisite`. Two fresh installs from the new preserved manifest and
lock matched, but both failed the exact headless probe by timeout. This is a
current blocker and triggers Astra consultation; no replacement is selected.
The probe does not establish worker-result compatibility.

The backend dogfood qualification boundary invokes the production
`SessionRuntime`, Delivery supervisor/services, persistence, verifier, and
Apply logic without a Tauri `AppHandle`. It suppresses only UI event emission;
it does not mock Git, DSH, verification, state, or Apply. The model-backed
end-to-end backend test is opt-in and has no successful current run, so this is
not a GUI E2E claim. Receipts correlate session, attempt, unique verification
run, acceptance SHA, candidate SHA, and frozen profile hash.

### Production Product OS coordinator — Milestone 07

`product_os_coordinator.rs` is the production sequencing boundary for the
founder-to-Delivery path. Its five Tauri commands start/status/answer/cancel/
resume one durable run; it does not introduce a second live task authority.
`TranscriptStore` persists only the run phase and work-order/package/session
references. Semantic role calls use the existing contained OpenCode adapter;
typed Product OS operations admit their proposals, while `DeliveryState`
continues to own candidate execution and independent verification.

The real Linux dogfood proved the production path with three research work
orders, independent verification, Product Director review, two architecture
proposals, challenge/reuse evidence, feasibility, current gates, Build Package
admission, and a Verified candidate. The final run took 963.57 seconds after
the test binary was ready. It is not a claim of GUI E2E, Windows, packaging,
Safe Apply, full restart/reconciliation, or concurrent role execution.

## DB/async rules

- Never hold synchronous DB/file locks across `.await`.
- Use existing `db_helpers::run_blocking` pattern for blocking DB work.
- No `blocking_lock()` in async or browser navigation callbacks.
- Handle poisoned sync mutexes deliberately; current code sometimes recovers via `into_inner()` where appropriate.

## Product OS package handoff

M05C keeps Product OS handoff backend-owned. Typed Product Director work
orders admit reviewed scope and evidence by current project revision; reuse and
architecture operations accept only current typed evidence references. The
only NarrowBuild operation creates an adopted Owner decision record. The
renderer has no raw `ProductAuthorityRecords` or `GateInput` mutation
command. Existing `assemble_build_package()` and
`evaluate_current_preimplementation_gates()` derive the package and gate
facts from reopened `TranscriptStore` authority.

## Command return convention

Historical project pattern: many Tauri commands return JSON-serialized strings rather than typed Tauri objects. Frontend callers must inspect the **real command signature** before deciding whether to `JSON.parse()`.

Known historical exceptions returned plain strings (for example prompt template/export content). Do not generalize from old lists; source is authoritative.

New commands should follow the current local convention unless a deliberate contract migration is approved.

## Security boundaries

- Never log/store API keys.
- Browser credentials/cookies are not generic session data and should not be deleted casually.
- Build workers should be bounded to candidate workspace and project commands.
- Worktrees are repository isolation, not OS sandboxing.
- Protected/irreversible external actions require explicit owner permission before execution.

## Cross-platform

All new Build backend work must remain native Linux + native Windows:

- `PathBuf` / platform path APIs;
- explicit program + arg vectors;
- no hardcoded Unix paths/signals as sole mechanism;
- process cleanup must be qualified on both OS families.

## Verification commands

For backend changes, minimum checks usually include:

```bash
cd src-tauri && cargo check
```

Delivery/reliability changes require targeted tests plus source-level audit of invariants; frontend-coupled changes also require frontend build.
