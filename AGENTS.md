# Consensus Arena — Codex Working Rules

## Read before editing

- Read every affected file completely. Real source is authoritative when a
  document, mockup, or remembered status disagrees.
- Make the smallest scoped change that satisfies the task. Do not refactor
  unrelated code.
- After changes, run the relevant real verification command and report its
  output. Do not claim completion from inspection alone.
- Preserve user changes in the dirty worktree. Never delete files or add a
  dependency without explicit approval.
- Use absolute paths in project documentation and execution instructions.

## Project constraints

- Native Tauri 2.0 desktop app; maximum two WebViews: one persistent leader
  and one shared navigating participant window.
- Model participation uses the user's web accounts, not paid participant APIs.
- Keep total memory below 2 GB on the target 4 GB/Celeron machine.
- No `blocking_lock()` in async Rust or `on_navigation`; navigation uses
  `std::sync::mpsc`, not Tokio mpsc.
- Phase 1 Memory is implemented. `MemoryStore` uses `std::sync::Mutex`, and
  async access must go through `db_helpers::run_blocking`. Never use
  `memory_store.lock().await`; memory failures in `response_router` must remain
  non-fatal.
- No `.unwrap()`/`.expect()` in live production paths.
- IPC names and payload fields must match
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md`.
- Every Tauri command with multiword snake_case IPC arguments must use
  `#[tauri::command(rename_all = "snake_case")]`; verify argument case during
  every RISK-IPCPARSE / IPC audit.
- `GENERIC_INIT_SCRIPT` remains static and generic. Agent identity comes from
  `window.__ca_agentId`, never an agent-specific closure capture.
- Main content is blueprint-first: never show participant chat logs as the
  primary content.

## Frontend ground truth

- Current implemented design reference:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
- Do not replace the current frontend design without explicit approval.
- Themes are Blue, Light, and Dark. The Templates input button is
  intentionally absent.
- Supported participants are chatgpt, claude, gemini, deepseek, qwen, glm,
  and kimi.
- Fonts are local Inter and JetBrains Mono assets under
  `/home/kasun/Music/arena/consensus-arena/public/fonts/`; never add a font CDN.

## Named frontend risks

- AskUser option click, custom submit, Escape, and backdrop dismissal must
  invoke `provide_user_answer`; dismissals send `"Cancelled"` so the backend
  oneshot does not hang.
- Commands returning `serde_json::to_string(...)` must be parsed with
  `JSON.parse()` in the frontend. Current examples include brain configs,
  health, session list/details, recovery state, and transcript.
- Plain-string commands such as `get_prompt_template` and `export_blueprint`
  must not be JSON-parsed.
- All event listeners must clean up on unmount.

## Verification

- Frontend: `cd /home/kasun/Music/arena/consensus-arena/src && npm run build`
- Backend: `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check`
- Every change: `cd /home/kasun/Music/arena/consensus-arena && git diff --check`
- Do not create a git checkpoint unless the user asks after reviewing the diff.
