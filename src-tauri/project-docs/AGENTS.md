# AGENTS.md — Consensus Arena

This file is read by Codex CLI before it works on this project. It exists so
every session inherits the same non-negotiable rules automatically, instead
of them needing to be repeated by hand in every prompt.

Consensus Arena is a native Tauri 2.0 desktop app (Rust backend,
React/TypeScript frontend) that orchestrates multiple AI models through an
autonomous leader-driven expert panel to produce project blueprints. Zero
paid API keys for model participation — all AI access via the user's
personal web accounts, automated through browser injection.

---

## Working rules (non-negotiable)

- **Read before writing.** Always read a file completely before editing it.
  Do not guess at a file's current contents from memory, from a doc's
  description of it, or from a previous session's summary — read it fresh
  every time. This project has been burned repeatedly by project docs that
  described a stale version of the codebase; the actual source files are
  the only ground truth.
- **Small, contained changes.** Prefer the smallest change that correctly
  solves the task. Do not refactor unrelated code while fixing something
  else.
- **Always show a plan before editing**, unless explicitly told to proceed
  directly. After editing, always show `/diff` and a plain-English summary
  of what changed and why.
- **Ask before installing any new package or dependency** (npm or cargo).
  Do not silently add a dependency to solve a problem that has a
  no-new-dependency solution.
- **Ask before deleting any file.** Confirm it's genuinely dead code first
  (e.g. grep for every reference to it) rather than assuming.
- **After any change, run the real verification command** — `cargo check`
  for backend changes, `npm run build` for frontend changes — and report
  the actual output. Do not claim something works without having run it.
- **Never claim a task is "done" or "fixed" without independent
  verification** (a clean build, a passing check, or a direct read
  confirming the change is present). This project has repeatedly had
  claims of completion that turned out to be wrong when actually checked —
  treat every "this should work now" as unverified until proven.

---

## Project-specific technical constraints (never violate)

- 4GB RAM / Celeron machine — total memory footprint must stay under 2GB
  at all times. Do not suggest heavy dependencies or approaches without
  considering this.
- Maximum 2 WebViews simultaneously in the Tauri app (one persistent
  leader window, one shared navigating window). Never suggest a third.
- No `blocking_lock()` in async Rust code or inside `on_navigation` closures
  — causes UI freeze on WebKitGTK.
- No `tokio::sync::mpsc` inside `on_navigation` — it's a synchronous
  callback; must use `std::sync::mpsc`.
- Phase 1 Memory is implemented. `MemoryStore` is protected by
  `std::sync::Mutex`; every async access must use
  `db_helpers::run_blocking()`. Never use `memory_store.lock().await`, and
  keep memory failures inside `response_router` non-fatal.
- No `.unwrap()` or `.expect()` in any Rust code path reachable during a
  live session. Acceptable only in test code, compile-time constants, and
  `.setup()` closure startup (unrecoverable init failures).
- Every `app.emit()` event name and payload field name must match
  `IPC.md` exactly — check the doc before adding or changing an event.
- `GENERIC_INIT_SCRIPT` (in `browser_backend.rs`) must remain a single
  static `&str` constant, generic across all 7 supported AI models — never
  agent-specific branching baked into it.
- Agent identity inside browser-injected JS always comes from
  `window.__ca_agentId`, read at runtime — never captured by closure value
  or hardcoded.
- `provide_user_answer` must be called on every path that closes the
  AskUser popup (button click, Escape key, backdrop click) — the backend
  channel hangs forever otherwise.
- **RISK-IPCPARSE:** Several Tauri commands return `serde_json::to_string(&value)`
  — a JSON-serialized STRING, not a parsed object (e.g.
  `get_agent_brain_config`, `get_agent_health`, `get_fallback_brain_config`,
  `get_secondary_brain_config`, `get_session_list`, `get_session_details`,
  `get_recovery_state`, `get_transcript`). Every frontend `invoke<T>()` call
  against these MUST `JSON.parse()` the result before using it as `T`. This
  exact bug has shipped at least four separate times in this project
  before being caught. `get_prompt_template` and `export_blueprint` are the
  known exceptions — they return plain strings, do NOT JSON.parse() those.
  Before writing any new `invoke<T>()` call, check the real Rust command's
  actual return type in `commands.rs` first.
- Any new Tauri command that returns a struct or collection must return it
  via `serde_json::to_string(&value)`, matching the existing pattern — and
  the frontend caller must `JSON.parse()` it.

---

## Design ground truth

- The current implemented visual reference is
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
  Do not replace the current frontend design without explicit approval. If
  a written description conflicts with real source, source wins and the doc
  must be corrected.
- Default theme is a real blue color system (`index.css`'s `:root` block),
  not monochromatic. Themes are Blue (default) / Light / Dark. There is no
  Gray theme.
- Fonts are Inter + JetBrains Mono via local `@font-face` in `index.css` —
  never load fonts from a CDN at runtime (the packaged Tauri app has no
  guaranteed network access).
- Real components use inline `style={{}}` objects keyed to CSS variables in
  `index.css` — not shadcn/ui primitives, not Tailwind utility classes
  directly on JSX elements, even though both are installed in the project.

---

## Project commands

- Backend check: `cd src-tauri && cargo check`
- Frontend build: `cd src && npm run build`
- Dev run (visual testing): `npm run tauri dev` (from project root)
- Release build: `npm run tauri build` (from project root — takes ~17-18
  min on this hardware; only run after `npm run tauri dev` has confirmed
  the change works)

---

## Where to look first

- `src-tauri/src/commands.rs` — every Tauri command's real signature and
  real return type (source of truth for RISK-IPCPARSE)
- `src-tauri/src/orchestrator.rs` — the real `AppState` struct
- `src/index.css` — the real, current design tokens (`:root` and
  `[data-theme]` blocks)
- `index.html` (project root, NOT under `src/`) — easy to forget this file
  exists since it isn't matched by a `.tsx`/`.ts`/`.css`-only search; it has
  held real bugs before (a stale font CDN reference) that no amount of
  reading component files would have surfaced
- `src-tauri/project-docs/audits/` — permanent history of what's been
  verified and when; never delete anything here

---

## What NOT to do

- Do not trust a project doc's "status" claims (e.g. "Sessions 2-4
  pending," "N fields," "N commands") without directly counting/reading
  the real source. These docs have been stale and self-contradictory
  multiple times in this project's history.
- Do not add features not explicitly asked for, even if they seem like an
  obvious improvement.
- The hello is already implemented as a dependency-free SVG/CSS port of
  preview.html's path. Do not replace it or add the CDN Lottie runtime
  without explicit approval.
