# Phase 1 Memory System — Pre-Implementation Audit

Date: 2026-07-06

## Gate result

PASS. The live source matches the prerequisite state in
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/PHASE1_MEMORY_v10_FINAL.md`:
26 `#[tauri::command]` functions are defined and all 26 are registered, `AppState`
has 14 fields, `db_helpers::run_blocking()` exists, and the memory module is still
a scaffold. Implementation may continue after the required clean baseline checks.

The specification refers to `/home/kasun/Music/arena/consensus-arena/IPC.md`, but
that path does not exist in the live worktree. Project rules identify
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md` as the
authoritative contract, so that existing file is the implementation target.

The v10 spec requires `MemoryStore::get_global_memory()` and a matching command,
but does not spell out its SQL. The smallest contract-consistent implementation is
to return all `global_memory` rows ordered by provenance-independent importance,
mention count, recency, then creation time.

## Current source state

- Command count: 26 (`#[tauri::command]` in
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs`).
- `generate_handler!` registration count: 26 in
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs`.
- `AppState` fields (14): `orchestrator`, `transcript_store`, `token_budget`,
  `session_vault`, `browser_state`, `context_manager`, `blueprint_store`,
  `settings_store`, `agent_brain`, `ask_user_tx`, `agent_brain_2`,
  `session_active`, `model_health`, and `brain_fail_count`.
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/memory_store.rs` is a
  stub: it has no connection or schema and only placeholder summary/transcript/
  reliability methods.
- All three current `AgentBrain::decide` calls are in
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/response_router.rs`.
- No current memory event emissions or `MemoryPanel.tsx` exist.
- No current `tauri-plugin-dialog` or `@tauri-apps/plugin-dialog` dependency or
  import exists.

## Files that must change

- `/home/kasun/Music/arena/consensus-arena/src-tauri/Cargo.toml`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/Cargo.lock` (generated lock update)
- `/home/kasun/Music/arena/consensus-arena/package.json`
- `/home/kasun/Music/arena/consensus-arena/package-lock.json` (generated lock update)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/capabilities/default.json`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/memory_store.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/orchestrator.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/agent_brain.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/response_router.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md`
- `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx`
- `/home/kasun/Music/arena/consensus-arena/src/panels/MemoryPanel.tsx` (new)
- `/home/kasun/Music/arena/consensus-arena/src/hooks/useIpcListeners.ts`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/DECISIONS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/phase1-memory-post.md` (new)

## Dependency decision

Dependency changes are required. `rusqlite` is present with only `bundled`, so
the required `backup` feature must be enabled. Native save/open dialogs are an
explicit frontend requirement and no dialog plugin exists, so
`tauri-plugin-dialog = "2"` and `@tauri-apps/plugin-dialog = "^2"` must be added.
Tauri 2 capability enforcement also requires `dialog:default` in
`/home/kasun/Music/arena/consensus-arena/src-tauri/capabilities/default.json`.

## Frontend state decision

`/home/kasun/Music/arena/consensus-arena/src/stores/useAppStore.ts` does not need
changes. Its existing `setupBrief` is the current active/new-session project
brief used by the setup and active views. Memory health can remain local to
`MemoryPanel`, while `memory-health-warning` can use the existing `addToast`
action. No duplicate `currentProjectBrief` or global health field is necessary.

## IPC parse audit

Current JSON-string callers for brain configs, model health, session list/details,
recovery state are parsed with `JSON.parse()`. Current plain-string callers for
`get_prompt_template` and `export_blueprint` are not parsed. New collection/struct
memory commands must preserve this split: project/global memory, open questions,
model strengths, health, and patterns require `JSON.parse()`; `get_project_config`
is a plain string and must not be parsed.

## Named risk checklist (pre-change)

- RISK-BLOCKING: PASS — no `blocking_lock()` exists; synchronous stores use
  `db_helpers::run_blocking()` from async paths.
- RISK-CHANNEL: PASS — `on_navigation` uses `std::sync::mpsc`; Tokio mpsc is
  used only outside the synchronous navigation closure.
- RISK-EVENTMATCH: PASS — existing emitted event fields match the authoritative
  IPC contract; new memory events remain to be added to both sides.
- RISK-UNWRAP: PASS for affected live-session paths — existing raw unwrap/expect
  findings are startup/in-memory constructor patterns, not the router path.
- RISK-STALERESPONSE: PASS — `wait_for_response` checks both agent id and turn.
- RISK-INITSCRIPT: PASS — `GENERIC_INIT_SCRIPT` is one static generic constant.
- RISK-NAVCLOSURE: PASS — `make_nav_closure` captures only the sync sender;
  agent identity is derived from URL/runtime state.
- RISK-ASKCHANNEL: PASS — `provide_user_answer` uses `take()` before send.
- RISK-ASKDISMISS: PASS — option/custom/Escape/backdrop paths call
  `provide_user_answer`, with dismissal sending `"Cancelled"`.
- RISK-IPCPARSE: PASS for existing call sites; new memory callers must follow
  the parse split documented above.

