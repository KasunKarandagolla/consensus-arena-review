# Settings Diagnostics — Pre-Implementation Audit

Date: 2026-07-06

## Scope and evidence

Read completely before source edits:

- `/home/kasun/Music/arena/consensus-arena/AGENTS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/DECISIONS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/PROCESS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/BACKEND.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/FRONTEND.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/phase1-memory-post.md`
- Every existing Stage 1 backend/frontend file named in the task.

The specified call-site, `safeInvoke`, toast, `debug-log`, app-data path,
database path, and `AgentBrain::new` searches were also completed. The dirty
worktree contains only generated tracked binaries under `src-tauri/target/`;
they predate this pass and must be preserved.

## Current save behavior

`save_agent_brain_config` currently:

1. Receives `api_key`, `base_url`, `model`, and `system_prompt` through Tauri.
2. Constructs an `AgentBrain`; this only builds a reqwest client and does not
   validate that the URL or model is non-empty.
3. Reads the three fallback settings and attaches them only when all are
   non-empty.
4. Writes four primary settings keys in sequence.
5. Updates the live `AppState.agent_brain` only after all prior steps succeed.

The live-state update is atomic relative to save failure, but the four
individual SQLite writes are not wrapped in a database transaction.

## Errors available today

If the command body is entered, it can return an `AgentBrain::new` client-build
error, a fallback settings read error, or any of the four settings write
errors. All are flattened with `e.to_string()` and have no command-stage label.
The Tokio mutex update itself has no fallible lock result.

There is also a proven pre-body error: `#[tauri::command]` defaults command
argument keys to camelCase in Tauri 2.6.2. The macro source initializes
`argument_case` as `ArgumentCase::Camel` and converts Rust identifiers with
`to_lower_camel_case()`. Therefore the backend expects `apiKey`, `baseUrl`,
and `systemPrompt`, while both IPC.md and the frontend send `api_key`,
`base_url`, and `system_prompt`. Tauri rejects the request during argument
deserialization before this command body runs. This is the exact source-proven
cause of the reported first Agent Brain save failure.

The same mismatch affects multiword arguments on the other settings paths,
including fallback/secondary saves, prompt templates, and Project Context.

## Frontend error handling

- `safeInvoke` does not catch, replace, or swallow errors inside Tauri. It
  returns `tauriInvoke` directly, so the original Tauri/backend rejection is
  available to callers.
- `SettingsPanel.save` catches the rejection, writes the raw value to the
  developer console, and replaces it in the UI with the generic toast
  `Save failed`. It therefore hides the diagnostic text from the user.
- Project Context load/save similarly uses generic user-facing failures.
- There is no local “last settings error” state or selectable/copyable error
  display.

## Existing diagnostics and secret risk

`DebugPanel` is development-only, keeps 200 entries, supports filtering, and
listens for the internal `debug-log` event. No backend settings path emits that
event, and the existing rolling tracing logs contain no settings-save failure,
so the panel does not currently help diagnose this failure. `debug-log` must
remain undocumented in IPC.md.

Current save errors do not intentionally contain API keys, and the backend
does not log command arguments. However, forwarding arbitrary backend/Tauri
errors without a redaction boundary would create future risk. The new UI and
diagnostic logging must redact bearer values, `api_key`/`apiKey` fields, known
key/token prefixes, and long secret-like tokens. Diagnostic snapshots must use
configuration booleans only and never include key or prompt values.

## Minimum coherent change

The smallest coherent pass is:

1. Add redacted error-formatting helpers to `src/lib/tauri.ts` without changing
   `safeInvoke`'s return or rejection behavior.
2. Update `SettingsPanel.tsx` save paths to show command-specific redacted
   errors and a collapsed selectable/copyable last-error area.
3. Add `rename_all = "snake_case"` to the affected settings commands so their
   real Tauri argument contract matches IPC.md, plus validation and
   stage-specific error mapping that never logs values.
4. Add a small secret-free `get_diagnostic_snapshot` command, register and
   document its JSON-string result, and expose it in Settings. It is useful at
   low risk because app-data/database presence and configuration state are
   otherwise invisible during runtime diagnosis.
5. Do not change memory/session behavior, dependencies, global events, or the
   existing `debug-log` contract.
