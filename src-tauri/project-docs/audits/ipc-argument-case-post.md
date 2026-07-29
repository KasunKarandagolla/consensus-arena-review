# Tauri IPC Argument Case — Post-Implementation Audit

Date: 2026-07-06

Result: PASS — no FAIL items.

## Contract checks

- PASS — `start_session` accepts the documented snake_case keys
  `project_brief`, `session_type`, `agent_ids`, and `leader_agent_id` through
  `#[tauri::command(rename_all = "snake_case")]`. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:77`–`:85`
  and
  `/home/kasun/Music/arena/consensus-arena/src/components/views/SetupView.tsx:33`.
- PASS — every frontend invoke payload matches the corresponding Rust public
  argument and IPC.md key. The complete call-site inventory includes static
  calls plus the four Settings commands dispatched through the shared dynamic
  save helper. No frontend payload key required correction.
- PASS — all 39 command names are identical across command definitions,
  `generate_handler!`, and IPC.md. The measured counts are 39 definitions,
  39 registrations, and 39 documented commands, with empty set differences.
  Registration evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs:89`–`:144`.
- PASS — commands documented but intentionally unused by the current frontend
  remain valid public IPC: `pause_session`, `resume_session`, `get_transcript`,
  `get_global_memory`, `get_open_questions`, `get_model_strengths`, and
  `get_patterns`.
- PASS — IPC.md already documented the intended snake_case contract and needed
  no change in this pass.

## Multiword argument protection

- PASS — all 23 commands with at least one multiword public argument now use
  `rename_all = "snake_case"`: seven settings/Project Context commands were
  already protected and 16 commands were fixed in this pass.
- PASS — the 16 fixed commands are `start_session`, `setup_agent_sent`,
  `captcha_resolved`, `rate_limit_decision`, `export_blueprint`,
  `delete_session`, `rename_session`, `get_session_details`, `recover_session`,
  `get_project_memory`, `clear_project_memory`, `get_open_questions`,
  `get_model_strengths`, `get_patterns`, `export_memory`, and `restore_memory`.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:77`,
  `:446`, `:456`, `:466`, `:819`, `:904`, `:972`, `:1012`, `:1098`,
  `:1137`, `:1166`, `:1180`, `:1195`, `:1266`, `:1281`, and `:1295`.
- PASS — the project-wide convention and future RISK-IPCPARSE audit
  requirement are recorded in
  `/home/kasun/Music/arena/consensus-arena/AGENTS.md:30`–`:32` and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/DECISIONS.md:100`–`:107`.

## Diagnostics and serialization safeguards

- PASS — `SetupView` now reports `start_session` failures through the existing
  redacted helper, preserving the command name and exact redacted Tauri/backend
  detail in both the form and seven-second toast. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/components/views/SetupView.tsx:3`
  and `:34`, with the unchanged helper at
  `/home/kasun/Music/arena/consensus-arena/src/lib/tauri.ts:14`–`:47`.
- PASS — settings diagnostics remain intact: the redaction/error helpers,
  command-specific Settings error reporting, and diagnostic snapshot flow were
  not removed or bypassed. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/lib/tauri.ts:14`–`:47`,
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:151`–`:188`,
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:699`–`:755`.
- PASS — JSON-string commands used by the frontend are still parsed. Evidence:
  brain/health/session/diagnostic parsing in
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:108`–`:113`,
  `:127`, and `:183`; memory parsing in
  `/home/kasun/Music/arena/consensus-arena/src/panels/MemoryPanel.tsx:42` and
  `:59`; recovery parsing in
  `/home/kasun/Music/arena/consensus-arena/src/App.tsx:32`–`:35`; and setup
  parsing in
  `/home/kasun/Music/arena/consensus-arena/src/components/views/SetupView.tsx:23`–`:25`.
- PASS — plain-string commands remain unparsed: `get_prompt_template` and
  `get_project_config` in
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:104`–`:105`
  and `:136`, and `export_blueprint` in
  `/home/kasun/Music/arena/consensus-arena/src/components/views/ActiveView.tsx:21`
  and
  `/home/kasun/Music/arena/consensus-arena/src/components/layout/Sidebar.tsx:25`.

## Verification

- PASS — baseline `cargo check`: exit 0; 43 existing non-fatal warnings;
  finished the dev profile in 25.17 seconds.
- PASS — baseline `npm run build`: exit 0; 1,709 modules transformed; built in
  1 minute 5 seconds.
- PASS — final `cargo check`: exit 0; the same 43 non-fatal warnings; finished
  the dev profile in 1 minute 5 seconds.
- PASS — final `npm run build`: exit 0; `tsc && vite build`, 1,709 modules
  transformed, built in 1 minute 31 seconds.
- PASS — `git diff --check`: exit 0 with no output. The separately checked
  pre-existing staged diff also passed `git diff --cached --check` with no
  output.
- PASS — required `blocking_lock()` search returned no matches.
- PASS — required `memory_store.lock().await` search returned no matches.
- PASS — no dependency, package manifest, lockfile, Tauri capability, command
  name, event payload, memory architecture, or session architecture change was
  made. Required verification regenerated tracked/untracked `dist` and
  `src-tauri/target` outputs already present in the dirty worktree; they were
  preserved rather than reverted.

## Root cause resolution

Tauri 2.6.2 generated camelCase wrapper keys for every unannotated multiword
Rust argument. Thus `start_session` expected `projectBrief` even though IPC.md
and `SetupView` correctly sent `project_brief`. Explicit snake_case command
annotations now make the generated backend wrappers honor the authoritative
IPC/frontend keys across the entire command surface.
