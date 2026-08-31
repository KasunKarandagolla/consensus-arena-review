# Tauri IPC Argument Case — Pre-Implementation Audit

Date: 2026-07-06

Result: FAIL — 16 commands with multiword snake_case public arguments are
currently exposed through Tauri's default camelCase argument conversion.

## Scope and method

The root `AGENTS.md`, `DECISIONS.md`, `PROCESS.md`, `BACKEND.md`,
`FRONTEND.md`, `IPC.md`, the three latest named audits, every source file
named in the task, `PrimingView.tsx`, and `App.tsx` were read completely.
Repository-wide searches covered every `#[tauri::command]`,
`generate_handler!`, `safeInvoke`/aliased `invoke`, raw Tauri `invoke`, and
documented IPC command. Rust signatures, IPC.md payloads, and every static or
dynamic frontend call site were then compared directly.

Tauri 2.6.2 command wrappers default Rust argument identifiers to lower
camelCase. Therefore a Rust parameter such as `project_brief` is exposed as
`projectBrief` unless its command uses
`#[tauri::command(rename_all = "snake_case")]`. Single-word parameters are
unchanged by either convention.

## Command contract table

`—` means no public payload. Tauri-managed parameters (`State`, `AppHandle`)
are excluded. “Dynamic” means `SettingsPanel.save()` supplies the named
command and payload at runtime.

| command name | Rust command parameters | multiword parameters yes/no | documented IPC payload keys | frontend payload keys found | backend currently has `rename_all = "snake_case"` | mismatch yes/no | planned fix |
|---|---|---|---|---|---|---|---|
| `start_session` | `project_brief, session_type, agent_ids, leader_agent_id` | yes | same | same (`SetupView`) | no | **yes** | annotate command |
| `pause_session` | — | no | — | unused | no | no | none |
| `resume_session` | — | no | — | unused | no | no | none |
| `abort_session` | — | no | — | — (`InputBar`) | no | no | none |
| `user_input` | `text` | no | `text` | `text` (`InputBar`) | no | no | none |
| `captcha_resolved` | `agent_id` | yes | `agent_id` | `agent_id` (`CaptchaOverlay`) | no | **yes** | annotate command |
| `rate_limit_decision` | `agent_id, decision` | yes | same | same (`RateLimitOverlay`) | no | **yes** | annotate command |
| `setup_agent_sent` | `agent_id` | yes | `agent_id` | `agent_id` (`PrimingView`) | no | **yes** | annotate command |
| `provide_user_answer` | `answer` | no | `answer` | `answer` (`AskUserPopup`) | no | no | none |
| `save_agent_brain_config` | `api_key, base_url, model, system_prompt` | yes | same | same (`SetupView`; dynamic Settings) | yes | no | none |
| `get_agent_brain_config` | — | no | — | — (`SetupView`, Settings) | no | no | none |
| `save_fallback_brain_config` | `api_key, base_url, model` | yes | same | same (dynamic Settings) | yes | no | none |
| `get_fallback_brain_config` | — | no | — | — (Settings) | no | no | none |
| `save_secondary_brain_config` | `api_key, base_url, model, system_prompt` | yes | same | same (dynamic Settings) | yes | no | none |
| `get_secondary_brain_config` | — | no | — | — (Settings) | no | no | none |
| `save_prompt_template` | `template_name, content` | yes | same | same (dynamic Settings) | yes | no | none |
| `get_prompt_template` | `template_name` | yes | `template_name` | `template_name` (Settings) | yes | no | none |
| `get_diagnostic_snapshot` | — | no | — | — (Settings) | no | no | none |
| `get_transcript` | — | no | — | unused | no | no | none |
| `get_session_list` | — | no | — | — (`Sidebar`) | no | no | none |
| `export_blueprint` | `format, session_id` | yes | `format, session_id?` | `format, session_id?` (`ActiveView`, `Sidebar`) | no | **yes** | annotate command |
| `get_agent_health` | — | no | — | — (`SetupView`, `Sidebar`, Settings) | no | no | none |
| `delete_session` | `session_id` | yes | `session_id` | `session_id` (`Sidebar`) | no | **yes** | annotate command |
| `rename_session` | `session_id, title` | yes | same | same (`Sidebar`) | no | **yes** | annotate command |
| `get_session_details` | `session_id` | yes | `session_id` | `session_id` (`Sidebar`, Settings) | no | **yes** | annotate command |
| `get_recovery_state` | — | no | — | — (`App`) | no | no | none |
| `recover_session` | `session_id` | yes | `session_id` | `session_id` (`Sidebar`) | no | **yes** | annotate command |
| `get_project_memory` | `project_brief` | yes | `project_brief` | `project_brief` (`MemoryPanel`) | no | **yes** | annotate command |
| `get_global_memory` | — | no | — | unused | no | no | none |
| `clear_project_memory` | `project_brief` | yes | `project_brief` | `project_brief` (`MemoryPanel`) | no | **yes** | annotate command |
| `get_open_questions` | `project_brief` | yes | `project_brief` | unused | no | **yes** | annotate command |
| `get_model_strengths` | `project_brief` | yes | `project_brief` | unused | no | **yes** | annotate command |
| `save_project_config` | `project_brief, content` | yes | same | same (dynamic Settings) | yes | no | none |
| `get_project_config` | `project_brief` | yes | `project_brief` | `project_brief` (Settings) | yes | no | none |
| `get_memory_health` | — | no | — | — (`MemoryPanel`) | no | no | none |
| `repair_memory_index` | — | no | — | — (`MemoryPanel`) | no | no | none |
| `get_patterns` | `project_brief` | yes | `project_brief` | unused | no | **yes** | annotate command |
| `export_memory` | `destination_path` | yes | `destination_path` | `destination_path` (`MemoryPanel`) | no | **yes** | annotate command |
| `restore_memory` | `source_path` | yes | `source_path` | `source_path` (`MemoryPanel`) | no | **yes** | annotate command |

## Parity and usage findings

- 39 command definitions exist in `src-tauri/src/commands.rs`.
- 39 names are registered in `src-tauri/src/main.rs`.
- IPC.md documents the same 39 names. The three sets have no differences.
- Frontend command names and payload keys match IPC.md at every call site.
  No frontend payload correction is planned.
- Commands intentionally unused by the current frontend are
  `pause_session`, `resume_session`, `get_transcript`, `get_global_memory`,
  `get_open_questions`, `get_model_strengths`, and `get_patterns`.
- The repository search also found the Phase 1 design specification's example
  commands; it is historical design evidence, not executable registration or
  a frontend call site.

## Additional verified risks

- `SetupView` currently preserves the raw start error, but formats it as
  `Failed to start: ...` rather than using the existing redacted
  command-specific diagnostics helper. It will be changed to
  `buildCommandErrorMessage("start_session", error)` so the command name and
  exact redacted backend/Tauri detail are retained.
- JSON-string results remain parsed at every used call site, including brain
  configs, health, session list/details, recovery, memory health/project
  memory, and the diagnostic snapshot.
- Plain-string `get_prompt_template`, `get_project_config`, and
  `export_blueprint` results remain unparsed.
- No event payload mismatch was found; event payloads are outside the required
  implementation change.

## Planned implementation

Add `rename_all = "snake_case"` only to the 16 currently mismatched commands
listed above, update `SetupView` to use the existing redacted command-error
helper, and record the multiword-argument convention in `AGENTS.md` and
`DECISIONS.md`. IPC.md requires no contract correction.
