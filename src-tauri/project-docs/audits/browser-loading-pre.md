# Browser/WebView Loading — Pre-Implementation Audit

Date: 2026-07-06

Result: **FAIL — the black windows have a source-proven navigation cause, and setup can advance without successful readiness/send detection.**

## Scope and evidence

The root and nested `AGENTS.md`, `DECISIONS.md`, `PROCESS.md`, `BACKEND.md`,
`ARCHITECTURE.md`, `FRONTEND.md`, `IPC.md`, all five named recent audits, and
every Stage 1 source/configuration file were read completely before this audit
was written. Repository-wide searches covered builders, URLs, window lookup,
creation/navigation, initialization and navigation callbacks, setup/readiness
events, browser diagnostics, `app.emit` calls, blank-window references, and
Tauri capabilities. The installed Tauri 2.11.2 source was also checked to
confirm that `WebviewWindowBuilder::on_page_load`, `WebviewWindow::url`,
`navigate`, `show`, and `set_focus` are available without a dependency change.

The pre-existing dirty worktree contains only the tracked generated binary
`/home/kasun/Music/arena/consensus-arena/src-tauri/target/debug/consensus-arena`.
It must be preserved.

## Creation and navigation findings

### Leader WebView

`create_windows` in
`/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:433`
builds label `arena-leader` with `WebviewUrl::External(about:blank)`, title
`Consensus Arena — Leader`, size 1200×800, and `visible(true)`. It installs the
static `GENERIC_INIT_SCRIPT` and the sync-channel `on_navigation` callback, then
stores the handle in `BrowserState.leader_window` (`:440`–`:475`).

There is no external leader URL in this creation path. During setup,
`run_setup` only evaluates `window.__ca_agentId = ...` in the current document
and then immediately waits for `arena://ready` (`session_runner.rs:27`–`:60`).
It never calls `navigate`. Therefore the leader remains on `about:blank`.

### Shared nav WebView

The same `create_windows` function builds label `arena-nav` with
`WebviewUrl::External(about:blank)`, the same size/visibility/init script, and a
separate sync sender clone (`browser_backend.rs:457`–`:475`). `run_setup` also
does not navigate this window, so every participant setup attempt waits on an
`about:blank` document.

After setup, `inject_and_wait_with_retry` navigates the nav window only when a
saved conversation URL exists (`response_router.rs:1005`–`:1023`). When setup
failed to capture a real conversation, the map entry is absent/`None`, so this
path also leaves the shared window at its existing URL. The older
`inject_to_agent` helper can navigate a model base URL
(`browser_backend.rs:180`–`:205`), but repository search found no live caller.

### Model URLs

The authoritative `AGENTS` table at `browser_backend.rs:11`–`:47` contains:

| agent_id | display name | intended base URL | URL validity |
|---|---|---|---|
| `chatgpt` | ChatGPT | `https://chatgpt.com` | valid absolute HTTPS URL |
| `claude` | Claude | `https://claude.ai` | valid absolute HTTPS URL |
| `gemini` | Gemini | `https://gemini.google.com` | valid absolute HTTPS URL |
| `deepseek` | DeepSeek | `https://chat.deepseek.com` | valid absolute HTTPS URL |
| `qwen` | Qwen | `https://chat.qwen.ai` | valid absolute HTTPS URL |
| `glm` | GLM | `https://chat.z.ai/` | valid absolute HTTPS URL |
| `kimi` | Kimi | `https://www.kimi.com/` | valid absolute HTTPS URL |

The strings themselves are valid. The defect is that setup does not use them.

## Window behavior and lifecycle

- Both AI windows are explicitly visible and use normal 1200×800 sizes. No
  hidden, minimized, transparent, or zero-size construction option explains
  the black result.
- Labels are stable and distinct (`arena-leader`, `arena-nav`), preserving the
  two-WebView architecture. They are not unique per session. No existing-window
  lookup, reuse, close, or replacement exists, so a second session in the same
  process can collide with the still-existing labels and fail window creation.
- Creation and `navigate` errors are frequently discarded with `let _ = ...`
  or `.ok()`: identity eval and navigation in `inject_to_agent`, setup evals,
  participant navigation, prompt eval, and most setup event emissions. There is
  no page-load callback and no visible current URL/title/readiness state.
- The initialization script runs on `about:blank` immediately. Setup assigns
  `window.__ca_agentId` to that document but does not navigate. In the later
  paths that do navigate, assigning a normal JavaScript window property before
  a cross-document navigation does not provide a reliable identity to the next
  document. This is a second readiness risk because the generic script reports
  identity by reading `window.__ca_agentId` at runtime.

## Readiness and setup advancement

- `run_setup` waits up to 30 seconds for a matching `NavEvent::Ready`, but on
  timeout or matching error it emits a generic `boss-message` and `continue`s
  to the next agent (`session_runner.rs:42`–`:70`). It does not fail setup.
- It waits 120 seconds for `SendDetected`. On timeout, the manual
  `setup_agent_sent` flag can substitute for browser send detection; otherwise
  it emits another generic message and continues (`session_runner.rs:123`–`:149`).
- After the loop it unconditionally emits `setup-complete`
  (`session_runner.rs:206`) even when one or every agent failed readiness or
  send detection. Consequently setup can complete without the user pressing
  Send, without `setup-agent-complete` for all selected agents, and with both
  windows still blank.
- The frontend transitions to `running` solely on `setup-complete`
  (`useIpcListeners.ts:33`–`:38`). `PrimingView` also provides an “I pressed
  Send” button that invokes the manual flag (`PrimingView.tsx:10`–`:15`).
- If a model never emits `arena://ready`, the app waits 30 seconds, emits a
  generic login suggestion, skips that model, and can still enter the active
  state. There is no model/window/URL-specific error state.

## Observability and capabilities

- `on_navigation` observes only fake `arena://` signals. Ordinary external
  navigation is allowed but not recorded. There is no `on_page_load` handler,
  load-start/load-finish event, navigation error event, or per-window URL/title
  snapshot.
- JavaScript `console.error`/`window.onerror` are forwarded to tracing through
  `arena://log`, but only to the file log. Browser construction/navigation
  errors are not covered, and the user cannot see them.
- The existing `get_diagnostic_snapshot` reports persistence/configuration and
  memory only (`commands.rs:683`–`:755`). It reports no window existence, URL,
  model phase, readiness, send, response, or browser error data.
- `capabilities/default.json` includes `windows: ["*"]`; the explicit legacy
  model labels are redundant but do not exclude `arena-leader` or `arena-nav`.
  Window creation and navigation occur in trusted Rust, not through frontend
  window IPC permissions. `tauri.conf.json` has no URL allowlist preventing the
  external `WebviewUrl`. Capabilities are therefore not the likely blocker.

## Likely root cause

The primary cause is proven: `create_windows` loads `about:blank`, and the live
setup path never requests navigation to any model URL. The black windows are
therefore consistent with the exact constructed documents, not a memory or
settings failure. Secondary defects are unconditional setup advancement,
discarded navigation/eval errors, unreliable cross-navigation agent identity,
and stale label collision on a later session.

## Minimum coherent change

1. Keep exactly two windows and the static generic initialization script, but
   explicitly prepare and navigate the leader/shared window to each intended
   model URL. Preserve identity across navigation generically and check every
   eval/navigate/show/focus result.
2. Add an `on_page_load` observer and lightweight secret-free diagnostics in
   `BrowserState`, plus a centralized update when `Ready`, `SendDetected`,
   `Response`, or `Done` crosses the existing std→Tokio bridge.
3. Extend the existing JSON-string `get_diagnostic_snapshot` with a
   `browser_diagnostics` object, window-existence booleans, tracked phases,
   URLs, timestamps, and errors. Reuse the current collapsed Settings display.
4. Add/document/listen for `browser-diagnostic`, surfacing important loading,
   readiness, and error phases without recording prompts, cookies, keys, or
   model response text.
5. Make readiness/send timeouts fail setup with model/window/URL-specific
   diagnostics and `boss-message`; emit `setup-complete` only after every
   selected model has produced real `SendDetected` and therefore
   `setup-agent-complete`. The manual confirmation flag must not substitute for
   browser send detection.
6. Close stale `arena-leader`/`arena-nav` windows before constructing a new
   pair so stable labels remain safe across sessions.
