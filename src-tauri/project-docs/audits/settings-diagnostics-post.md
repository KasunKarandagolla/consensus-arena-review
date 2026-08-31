# Settings Diagnostics — Post-Implementation Audit

Date: 2026-07-06

Result: PASS — no FAIL items.

## Required checks

- PASS — Agent Brain save failures show the command name and exact redacted
  backend/Tauri error in a seven-second toast and in the collapsed, selectable,
  copyable “Last settings error” area beside Agent Brain. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:154`,
  `:218`, and `:219`.
- PASS — Fallback Brain save failures use the same exact redacted path and
  render beside Fallback Brain. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:223`–`:225`.
- PASS — Secondary Brain save failures use the same exact redacted path and
  render beside Secondary Brain. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:229`–`:231`.
- PASS — Project Context save failures use the same command-specific redacted
  path and render beside Project Context. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:176`
  and `:239`–`:241`.
- PASS — `safeInvoke` still returns `tauriInvoke` directly and therefore
  preserves the original rejection. `errorToString`, `redactSecrets`, and
  `buildCommandErrorMessage` format a separate display boundary without
  changing `safeInvoke`'s return type or behavior. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/lib/tauri.ts:14`–`:47` and
  `:55`–`:66`.
- PASS — API keys and secrets are excluded from the snapshot, and both backend
  diagnostic logging/error mapping and frontend display apply redaction.
  Snapshot configuration values are reduced to booleans before serialization.
  Evidence: `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:14`–`:50`,
  `:683`–`:755`, and
  `/home/kasun/Music/arena/consensus-arena/src/lib/tauri.ts:29`–`:47`.
- PASS — `save_agent_brain_config` now validates the base URL/model before
  construction and maps validation, fallback reads, and settings writes with
  the required stage-specific text. The final Tokio mutex assignment cannot
  return a lock error; it remains after all fallible operations, so AppState is
  unchanged on every save failure. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:372`–`:443`.
- PASS — Fallback and secondary saves validate before writing and map their
  persistence failures with command-stage text. All-empty fallback still
  clears the fallback. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:500`–`:536`
  and `:570`–`:621`.
- PASS — The source-proven Tauri argument-case defect is fixed for all touched
  settings save/read paths with `rename_all = "snake_case"`, matching IPC.md
  and existing callers. This includes primary/fallback/secondary, prompts,
  and Project Context.
- PASS — `get_diagnostic_snapshot` exists, returns a JSON-serialized string,
  is registered, and is documented with the required `JSON.parse` note.
  Evidence: `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:683`–`:755`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs:118`, and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md:123`–`:129`.
- PASS — Settings parses the diagnostic JSON string and displays it collapsed
  by default as selectable/copyable text. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:179`–`:188`
  and `:243`–`:251`.
- PASS — `debug-log` remains development-only/internal. No new emitter was
  added and IPC.md does not document it.
- PASS — No dependency was added.

## Verification

- PASS — `cargo check`: exit 0. It finished the dev profile and reported the
  same 43 non-fatal warnings present at baseline.
- PASS — `npm run build`: exit 0. The script ran `tsc && vite build`.
- PASS — `git diff --check`: no output.
- PASS — command parity: 39 `#[tauri::command]` definitions and 39
  `generate_handler!` registrations.
- PASS — required secret grep found two classified matches only: the new
  frontend `Bearer` redaction regex and the existing agent-brain Authorization
  header. It found no `api_key`/`apiKey` value passed to `addToast`.
- PASS — required `unwrap()/expect(` grep over `commands.rs`,
  `settings_store.rs`, and `agent_brain.rs` returned no matches.
- PASS — required `blocking_lock()` grep over `src-tauri/src` returned no
  matches.
- PASS — JSON parse rules are correct: the new snapshot is parsed; existing
  JSON configuration results remain parsed; prompt and Project Context plain
  strings remain unparsed.

## Root cause finding

The first Agent Brain save failed before the Rust command body ran. Tauri
2.6.2's command macro defaults Rust multiword argument identifiers to camelCase,
so the unannotated command expected `apiKey`, `baseUrl`, and `systemPrompt`.
IPC.md and `SettingsPanel` correctly sent the project's authoritative
snake_case keys. Adding `rename_all = "snake_case"` to the touched commands
aligns the real generated command wrapper with the documented contract.
