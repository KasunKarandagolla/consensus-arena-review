# Consensus Arena — Backend Guide

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

### Memory

Phase 1 memory is implemented, not merely a future specification. See `MEMORY.md`.

### Delivery backend

Latest audit establishes backend behavior including:

- clean repository admission;
- app-data worktree creation;
- bounded DSH subprocess;
- acceptance freeze/protected hashes;
- conservative verifier execution;
- bounded repair;
- durable `WaitingForUser` recovery;
- explicit safe Apply.

See `DELIVERY.md`. Exact file/module/command names must come from current source; the supplied audit documents semantics, not a complete symbol table.

Before Delivery admission, the backend checks the external DSH prerequisite
without exposing credentials or mutating the user's system. The current
qualified policy is DSH `0.1.5-rc.1` with a working `headless` profile; the
frontend receives the serialized result of `get_dsh_prerequisite` for a
progressive setup message.

## DB/async rules

- Never hold synchronous DB/file locks across `.await`.
- Use existing `db_helpers::run_blocking` pattern for blocking DB work.
- No `blocking_lock()` in async or browser navigation callbacks.
- Handle poisoned sync mutexes deliberately; current code sometimes recovers via `into_inner()` where appropriate.

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
