# Browser/WebView Loading — Post-Implementation Audit

Date: 2026-07-06

Result: **PASS — no FAIL items. Interactive account/login verification remains a runtime follow-up.**

## Required browser-loading checks

- **PASS — Leader WebView creation path verified.** `create_windows` validates
  the selected leader, destroys stale stable-label windows, builds exactly one
  hidden `arena-leader` WebView with the generic initialization script and both
  navigation/page-load observers, and navigates/shows/focuses it when it is the
  current setup model. Later leader setup reuses that persistent handle.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:829`–`:935`
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:30`–`:87`.
- **PASS — Shared nav WebView creation/navigation path verified.** Exactly one
  `arena-nav` WebView is built. Setup navigates it sequentially to each
  non-leader base URL, while autonomous routing navigates it to the saved
  conversation URL or a validated base-URL fallback. Every eval, URL parse,
  navigation, show, and focus request is checked and diagnostic on failure.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:367`–`:426`,
  `:899`–`:935`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:30`–`:87`,
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/response_router.rs:1031`–`:1093`.
- **PASS — All seven model base URLs verified.** ChatGPT, Claude, Gemini,
  DeepSeek, Qwen, GLM, and Kimi remain valid absolute HTTPS URLs in the single
  `AGENTS` table. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:222`–`:260`.
- **PASS — `browser-diagnostic` event added and documented.** Backend payload
  fields are exactly `agent_id`, `window_label`, `phase`, `url`, `message`, and
  `error`; IPC documents the same fields; the frontend listener cleans up with
  the existing listener array and only toasts errors. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:99`–`:212`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md:297`–`:307`,
  and
  `/home/kasun/Music/arena/consensus-arena/src/hooks/useIpcListeners.ts:65`–`:85`.
- **PASS — Browser diagnostics snapshot extended and displayed in Settings.**
  The existing JSON-string `get_diagnostic_snapshot` was extended rather than
  adding a command. It includes `leader_window_exists`, `nav_window_exists`,
  and sorted per-agent browser records. The existing caller still uses
  `JSON.parse`, and the whole secret-free snapshot remains collapsed,
  selectable, and copyable. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:692`–`:779`,
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:33`–`:64`,
  `:196`–`:207`, and `:259`–`:268`.
- **PASS — Setup cannot complete without real send detection for every selected
  model.** Readiness timeout/error and send timeout/channel failure now return
  from `run_setup`; `setup-agent-complete` is emitted only after matching
  `NavEvent::SendDetected`; `setup-complete` is after the full successful loop.
  The frontend manual confirmation button was removed, and
  `setup_agent_sent` is documented as non-authoritative. The generic detector
  requires a send click/Enter with non-empty input, unchanged document, cleared
  input, and exactly one newly rendered matching message. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:89`–`:233`,
  `:280`–`:307`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:735`–`:825`,
  `/home/kasun/Music/arena/consensus-arena/src/components/views/PrimingView.tsx:1`–`:18`,
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md:48`–`:55`.
- **PASS — Leader priming response is observable by the autonomous loop.**
  After every agent has really sent its setup prompt, setup installs a
  non-sending monitor on the persistent leader window for turn 1 before
  `setup-complete`. This closes the prior path where the loop could wait for a
  response that no monitor was capturing. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:942`–`:1014`
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:284`–`:307`.
- **PASS — Load/readiness timeouts surface useful diagnostics.** Checked
  navigation emits creating/loading state, `on_page_load` records redirected
  current URLs, the std-to-Tokio bridge records ready/send/response events,
  setup timeouts include model/window URL context in `browser-diagnostic` and
  `boss-message`, and router timeouts update diagnostics. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:367`–`:457`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:190`–`:200`,
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:89`–`:233`,
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/response_router.rs:152`–`:174`,
  `:1100`–`:1167`.
- **PASS — No diagnostic secrets/content.** Records contain model identity,
  window metadata, query/fragment-stripped URLs, phases, timestamps, and
  errors only. Prompts, API keys, cookies, transcript/model response text, and
  project content are never stored or emitted. Page titles are intentionally
  excluded because AI chat titles may derive from prompt/project content.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:13`–`:34`,
  `:109`–`:121`, and `:150`–`:161`.

## Root-cause and loading-fix result

- **PASS — Proven root cause fixed.** Previously both visible windows were
  built at `about:blank`, and `run_setup` never navigated them. Windows are now
  built hidden to avoid blank-window flashes, the current setup model is
  immediately navigated to its real external URL and shown/focused, and every
  subsequent model is handled sequentially so another model's ready event
  cannot be consumed and discarded.
- **PASS — Agent identity survives cross-origin navigation.** The static
  generic script restores `window.__ca_agentId` from a generic `window.name`
  marker written by Rust. It contains no agent-specific branch or closure
  capture. `on_page_load` also refreshes the runtime value on load events.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:349`–`:365`
  and `:648`–`:687`.
- **PASS — Stable label collision handled.** Stale `arena-leader` and
  `arena-nav` windows are destroyed before a new two-window pair is built.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:859`–`:869`.
- **PASS — Capabilities remain non-blocking.** No capability or Tauri config
  change was needed. Existing `windows: ["*"]` covers both stable labels, and
  trusted Rust owns creation/navigation.

## Named risks

- **PASS — RISK-BLOCKING clear.** Required repository grep for
  `blocking_lock()` returned no matches. Diagnostic mutex sections are short
  synchronous `std::sync::Mutex` map operations; no guard crosses an await.
- **PASS — RISK-CHANNEL clear.** The required grep found one
  `use tokio::sync::mpsc::Receiver;` import used by async consumers only.
  `make_nav_closure` still receives/captures only
  `std::sync::mpsc::SyncSender<NavEvent>` and calls `try_send`; no Tokio mpsc is
  inside `on_navigation`. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:563`–`:621`.
- **PASS — RISK-INITSCRIPT clear.** `GENERIC_INIT_SCRIPT` remains one static
  `&str`, with generic selectors and generic identity restoration. No model id,
  URL, or model-specific control branch was added. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:648`–`:827`.
- **PASS — RISK-NAVCLOSURE clear.** `make_nav_closure` captures only its sync
  sender; agent identity is parsed from `arena://` URL segments. Page-load
  observation is a separate generic callback using shared diagnostic state,
  window label, and `window.__ca_agentId`; it does not capture a model id.
- **PASS — Memory constraints clear.** Required grep for
  `memory_store.lock().await` returned no matches. No Agent Brain, Memory, IPC
  argument-case, dependency, or two-WebView architecture change was made.
- **PASS — Live-path panic check.** No raw `.unwrap()` or `.expect()` match was
  introduced in the changed Rust live paths.

## Verification

- **PASS — `cargo check`: exit 0.** Default target finished the dev profile in
  1m24s with 40 non-fatal dead-code/unused warnings. An isolated-target rerun
  also exited 0 after the repository target lock cleared.
- **PASS — `npm run build`: exit 0.** `tsc && vite build`; 1,709 modules
  transformed; final production bundle built in 2m60s.
- **PASS — `git diff --check`: exit 0 with no output.**
- **PASS — required greps:** no `blocking_lock()`; one Tokio mpsc import only,
  outside `on_navigation`; no `memory_store.lock().await`; expected generic
  `window.__ca_agentId` identity assignments/reads only.
