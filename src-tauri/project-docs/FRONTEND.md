# Consensus Arena — Frontend

## Status: FULLY BUILT AND REDESIGNED FROM preview.html

All 4 views, Zustand store, IPC listener hook, Settings panel with Project
Context and `MemoryPanel`, Sidebar with full session CRUD, AskUser popup,
CAPTCHA/rate-limit overlays, Toast, DebugPanel, and a shared Topbar component
all exist as real, working files.
This was confirmed by directly reading all real source files under src/ and
a live `npm run tauri dev` run showing a fully interactive app -- this
status line previously claimed otherwise for an extended period and was
only caught because the user's own screenshots directly contradicted it.
The preview.html redesign has replaced the previous production React
presentation while preserving the established Tauri IPC wiring.

Current implemented design reference:
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
Do not replace this frontend design without explicit approval.

Current execution method: Codex CLI reads real source, applies scoped edits,
and runs verification directly. Source-over-document and audit-before-edit
rules remain mandatory.

---

## Known Limits

1. The hello uses preview.html's actual Bézier path and transforms in an
   inline SVG/CSS port. It does not load the mockup's CDN Lottie runtime.
   Compared with byte-for-byte Lottie playback, variable stroke width and
   the full 17-stop gradient are simplified.
2. The historical Light-theme rectangle report is not a currently
   reproduced defect. The html/#root/body height-chain fix remains present;
   reopen the issue only with a current reproducible screenshot.

---

## Design Philosophy

Match the implemented preview.html reference. The interface disappears.
You notice the blueprint being built, not the chrome around it.

**Not monochromatic.** Default theme is a real blue color system (see
index.css's :root block) -- matches the mockup's own default
(`<html data-theme="blue">`), including a gradient logo wordmark and a
gradient "hello" empty-state animation. An earlier version of this
document said "Monochromatic only: black, white, and grey" -- that
predated the currently-approved mockup and does not apply. If the mockup
and a written design principle ever conflict again, the mockup wins;
correct the document, don't follow the document over the mockup.

Minimal. Professional. Every element earns its place.
Nothing decorative. Nothing that exists to show off features.

**The user hired a consulting firm. They don't watch the consultants argue.
They receive the deliverable.**

Additional UI elements (AskUser popup, debug panel) must match the existing
design language exactly -- same typography, same spacing, same color system.
No new design patterns introduced without approval.

---

## Tech Stack

- React + TypeScript
- Tailwind CSS (installed -- tailwind.config.js at root)
- shadcn/ui was scaffolded early on (components.json at root,
  src/lib/utils.ts present) but is **not the actual styling approach in
  current use** -- real components use inline `style={{}}` objects keyed
  to CSS variables defined in index.css, not shadcn primitives or Tailwind
  utility classes directly on JSX elements
- Lucide icons
- Tauri IPC via @tauri-apps/api
- Native memory backup/restore dialogs via @tauri-apps/plugin-dialog
- Zustand (state management)
- **Inter + JetBrains Mono variable fonts**, loaded via local `@font-face`
  in index.css from `/fonts/inter-variable.woff2` and
  `/fonts/jetbrains-mono-variable.woff2`.
  NOT DM Sans/DM Mono, and NOT loaded from Google Fonts CDN at runtime --
  a packaged Tauri build has no guaranteed network access. index.html
  previously had a dead Google Fonts `<link>` referencing DM Sans/DM
  Mono -- removed once found, since nothing in the app actually used those
  fonts; every component consumes Inter via CSS variables. Both local files
  and their built-output copies have been verified.

---

## Theme System

Three themes using CSS variables (index.css implements this -- coordinate
any change with lib/theme.ts's Theme type, which must stay in sync):
- `data-theme` unset = **Blue (default)** -- matches the mockup's own
  default (`<html data-theme="blue">`)
- `data-theme="light"` = Light
- `data-theme="dark"` = Dark

Key variables: `--bg`, `--surface`, `--text`, `--text-secondary`, `--border`,
`--accent-500`, `--font`, `--font-mono`, `--sidebar-width`, `--topbar-height`

There is no Gray theme reachable from the UI. An earlier version of this
document said "Light / Gray / Dark" -- that was never actually implemented
in any real component; SettingsPanel.tsx's three buttons are Blue/Light/
Dark.

**Not monochromatic** -- see Design Philosophy above.

---

## Layout Structure

```
┌─────────────┬──────────────────────────────────────┐
│             │  Topbar (collapse toggle + title)    │
│  Sidebar    ├──────────────────────────────────────│
│  (collapsible)│  Main Content Area                 │
│             │  (scrollable)                        │
│             │──────────────────────────────────────│
│             │  Live Status Label                   │
│             │  Input Bar                           │
└─────────────┴──────────────────────────────────────┘
```

No right panel. No blueprint panel separate from main content.
The main content area IS the blueprint being built.

A shared `components/layout/Topbar.tsx` renders across all 4 views
(Empty/Setup/Priming/Active), replacing what used to be 4 duplicated
inline topbar blocks. It exposes `title`, an optional `titleBadge` (used
by ActiveView for its Session Active/Complete badge), and an optional
`right` slot (used by ActiveView for the Download button). It also owns
the sidebar-collapse toggle button (matches mockup's `.ic-btn`/`panel-left`
icon), wired to a `sidebarCollapsed` boolean + `toggleSidebar()` action in
the Zustand store.

---

## Left Sidebar

Follows ChatGPT's sidebar pattern, now collapsible via the Topbar's toggle
button -- matches the mockup's `.sidebar.closed` state exactly
(width -> 0, opacity -> 0, border removed) rather than a hard
`display: none`, so the transition animates smoothly.

Elements (top to bottom):
- App name / logo (gradient wordmark)
- New Session button
- Session history list
  - Each session: title derived from project brief
  - Three-dot menu: Rename, Delete, Export blueprint, Session details --
    **all four backed by real commands** (delete_session, rename_session,
    get_session_details, export_blueprint with session_id honoured)
- Connected Models indicator (small status dots per model)
- Account icon -> opens Settings popover

---

## Main Content Area

### Blueprint sections
Rendered as markdown via ReactMarkdown, using the `.bp-markdown` typography
classes in index.css (merged in from what used to be a dead, never-imported
App.css during the mockup-matching batch). Each section has:
- Section heading
- Content body
- Copy button (top-right corner, appears on hover)

### Empty State
Dependency-free SVG/CSS port of preview.html's embedded hello path, plus:
```
Your panel is assembled. Brief them and step back.
```

### Download Button
Appears when at least one blueprint section exists. Lives in the shared
Topbar's `right` slot during an active session (see Layout Structure
above), not a standalone fixed-position element.
Downloads complete blueprint as .md file.

---

## Live Status Label

Single line fixed at bottom of content area, above the input bar.
Implemented in ActiveView.tsx, matching the mockup's `.stline`/`.sdrawer`
pattern.

```
● Claude is reviewing the database architecture proposal...
```

- Small animated dot indicator (subtle pulse)
- Plain text description of current activity
- Clickable -- expands to show full current model response in a drawer
- Fed by: agent-state-change, agent-routing, boss-message events

---

## Input Bar

Fixed at bottom. Matches ChatGPT input bar exactly.
Implemented in `components/shared/InputBar.tsx` with its own
JS-driven `autoResize()` textarea logic (deliberately not using the
`.ca-textarea` CSS `field-sizing` class also present in index.css, to
avoid the two resize mechanisms conflicting).

- Text input -- grows with content up to 4 lines, then scrolls
- Send button converts to Stop button when session is active
  - Send: invokes start_session (idle) or user_input (active)
  - Stop: invokes abort_session
- File attachment button (disabled until implemented)
- The mockup-only Templates button is intentionally absent.

---

## Setup Screen

Replaces main content area on New Session click (not a modal).
Implemented in SetupView.tsx.

### Fields:

**Project Brief** -- large textarea, required

**Session Type** -- segmented control:
Architecture / MVP / API Design / Security Review / Custom

**Participants** -- card grid for each available model:
chatgpt, claude, gemini, deepseek, qwen, glm, kimi
Each card: service name, connection status dot, toggle on/off.
Minimum 2 selected.

**Leader** -- dropdown, default: first selected model

**Agent Brain** (collapsible, collapsed by default):
- API Base URL
- API Key (masked)
- Model Name
- System Prompt (expandable textarea -- shows default, fully editable)

Loads previously-saved config via get_agent_brain_config and
get_agent_health on mount -- both of these are JSON-string-returning
commands and must be `JSON.parse()`'d (see IPC Wiring section below;
this was previously a real bug here, since fixed).

**Start Session button** -- disabled until all required fields complete.
On click: invokes save_agent_brain_config then start_session.

---

## Setup Progress Screen

Replaces setup form after Start Session. Implemented in PrimingView.tsx.

```
Preparing your expert panel...

✓ Claude — ready
→ DeepSeek — waiting for you to press Send...
  Gemini — waiting
```

Listens for: setup-agent-ready, setup-agent-complete, setup-complete.
On setup-complete: transitions to active session view.

---

## Active Session View

Implemented in ActiveView.tsx. Main content area shows blueprint sections
as they appear. Live status label shows current activity. Input bar:
Send button is now Stop button. Topbar carries the Session Active/Complete
badge and Download button (see Layout Structure above).

---

## Settings Popover

Implemented in SettingsPanel.tsx. Triggered from account icon at bottom of
sidebar.

### Sections:

**Connected Accounts** -- health/status rows for all 7 models

**Agent Brain** -- API Base URL, API Key, Model Name, System Prompt.
Save invokes save_agent_brain_config.

**Fallback Brain** (new) -- API Base URL, API Key, Model Name (no system
prompt -- reuses the primary's). Used automatically if the primary agent
brain's API call fails. Save invokes save_fallback_brain_config.

**Secondary Brain** -- API Base URL, API Key, Model Name, System Prompt.
Takes over after repeated primary failures. Save invokes
save_secondary_brain_config.

**System Prompts** -- Leader Priming Template, Participant Priming Template.
Save invokes save_prompt_template.

**Project Context** -- follows System Prompts. It loads and saves through
`get_project_config`/`save_project_config`. This user-confirmed context is
hard-pinned and always included in the agent brain's bounded memory context.

**Memory** -- `MemoryPanel` shows health, can repair the FTS index, export a
backup, restore a backup, inspect stored project facts, and clear project
memory. Native export/restore file selection uses
`@tauri-apps/plugin-dialog`.

**Appearance** -- Theme selector: **Blue / Light / Dark** (not
"Light / Gray / Dark" -- see Theme System above)

**About** -- Version, links.

There is no Performance section in the current Settings panel.

---

## Overlays and Notifications

**CAPTCHA overlay** -- captcha-detected event:
Blurred background, centered card. z-index 9300 (highest of the three
overlay components -- see z-index note below).
"[Model] needs verification. Complete it in the model's window, then click Resume."
Resume invokes captcha_resolved(agent_id).

**Rate limit notification** -- rate-limit-reached event:
Centered modal overlay (not inline in status area, despite an earlier
version of this doc describing it that way -- confirmed via direct read
of RateLimitOverlay.tsx). z-index 9200.
"[Model] has hit its rate limit. Estimated reset: N minutes."
Options: Wait / Continue without / Use lighter model / Skip this model
Each invokes rate_limit_decision(agent_id, decision).

**AskUser popup** -- agent-ask-user event:
Modal overlay with blurred background. Centered card.
Components:
- Header: small icon + "Agent · Needs Input" label
- Question text (prominent)
- 2-4 option buttons (full width, left-aligned text)
- Optional free-text input + Send button (only if allow_custom is true)
Behaviour:
- Renders on top of all other UI -- highest z-index of all overlays
- Blocks all other interaction until answered
- On button click: invoke provide_user_answer(answer), close popup
- On custom button or Enter: invoke provide_user_answer(trimmed answer), close popup
- On Escape or backdrop click: invoke provide_user_answer("Cancelled"), close popup
- Must NEVER be dismissable without sending an answer -- backend channel will hang
Mounted once in App.tsx root. Listens for agent-ask-user event.
State managed in Zustand: askUserPending: AskUserPayload | null

**Toast** -- e.g. "Progress saved" after session-checkpoint, general
confirmation toasts. z-index 9100 (lowest of the three overlay
components, so CAPTCHA and rate-limit modals always appear above it).

**Session complete state** -- session-complete event:
Status label: "Session complete". Download button prominent. Stop -> inactive.

---

## Debug Panel (Development Only)

Mounted once in App.tsx. Renders ONLY when import.meta.env.DEV is true.
Toggle: Ctrl+Shift+D.
Position: fixed bottom-right, 500x340px, semi-transparent dark surface.

Listens for debug-log Tauri event (development-only -- NOT in IPC.md).
Circular buffer: last 200 entries.
Each row: [HH:MM:SS] [TAG] message in monospace font.
Filter input: substring match on tag.
Clear button resets buffer.
Color: error = red text, info = default, debug = muted.

**Never renders in production builds.**

---

## Zustand State Shape

```typescript
interface AppState {
    // Session
    sessionStatus: 'idle' | 'setup' | 'priming' | 'running' | 'paused' | 'complete' | 'ended'
    setupProgress: string[]

    // Blueprint
    blueprintSections: BlueprintSection[]

    // Live status
    liveStatusText: string
    liveStatusExpanded: boolean

    // AskUser
    askUserPending: AskUserPayload | null   // non-null when agent-ask-user fires

    // Settings
    agentBrainConfig: AgentBrainConfig | null
    settingsOpen: boolean

    // Sidebar (new)
    sidebarCollapsed: boolean

    // UI
    selectedSessionId: string | null
    recoveryState: { available: boolean; session_id: string } | null
    captchaPending: { agent_id: string } | null
    rateLimitPending: { agent_id: string; estimated_reset_mins: number } | null
    toasts: ToastMessage[]
}

interface BlueprintSection {
    id: string
    title: string
    content: string
    status: 'draft' | 'agreed' | 'negotiation' | 'disputed'
}

interface AgentBrainConfig {
    api_key: string
    base_url: string
    model: string
    system_prompt: string
}

interface AskUserPayload {
    question: string
    options: string[]
    allow_custom: boolean
}

interface ToastMessage {
    id: string
    text: string
}
```

(Actual field list above reflects a direct read of useAppStore.ts as of
the mockup-matching batch -- an earlier version of this document was
missing settingsOpen, sidebarCollapsed, recoveryState, captchaPending,
rateLimitPending, and toasts entirely.)

---

## IPC Wiring — Zero Mock Data

All state comes from real Tauri events. No mock data in production components.

### CRITICAL: several commands return JSON-serialized strings, not objects

`get_agent_brain_config`, `get_agent_health`, `get_fallback_brain_config`,
`get_secondary_brain_config`, `get_session_list`, `get_session_details`, and
`get_recovery_state` all return `Result<String, String>` from the backend,
where the String is `serde_json::to_string(&value)` -- a JSON-serialized
string, NOT an already-parsed object. Every frontend `invoke<T>()` call
against these MUST `JSON.parse()` the result before using it as `T`.

This exact bug shipped at least four separate times across different files
(Sidebar.tsx, SettingsPanel.tsx x2 call sites, SetupView.tsx x2 call sites)
before being caught by systematically grepping every `invoke<T>()` call in
the frontend against commands.rs's real return types. `get_prompt_template`
and `export_blueprint` are the known exceptions -- they correctly return
plain strings directly, not JSON-wrapped, and should NOT be JSON.parse()'d.

Memory collection/struct commands follow the same rule:
`get_project_memory`, `get_global_memory`, `get_open_questions`,
`get_model_strengths`, `get_memory_health`, and `get_patterns` return JSON
strings and must be parsed. `get_project_config` is a plain string and must not
be parsed.

When adding any new `invoke<T>()` call: check the real Rust command's
actual return type in commands.rs first. Do not assume based on what
"seems like" it should return.

### Events to listen for (exact names from IPC.md):
- `session-status` → update sessionStatus
- `setup-agent-ready` → update setup progress indicator
- `setup-agent-complete` → mark model as ready in setup progress
- `setup-complete` → transition priming → active session
- `agent-state-change` → update liveStatusText
- `agent-routing` → update liveStatusText
- `boss-message` → update liveStatusText
- `blueprint-section-added` → append new section
- `blueprint-update` → upsert section
- `agent-ask-user` → set askUserPending in store
- `captcha-detected` → show CAPTCHA overlay
- `rate-limit-reached` → show rate limit notification
- `session-checkpoint` → show "Progress saved" toast
- `session-complete` → show completion state
- `memory-updated` → refresh relevant memory data (`memory_type`, `trigger`)
- `memory-health-warning` → show a warning (`text`, `fts_needs_repair`)

### Commands to invoke (exact names from IPC.md):
- `start_session` -- Start Session button
- `abort_session` -- Stop button
- `user_input` -- Send during active session
- `provide_user_answer` -- AskUser popup answer (ALWAYS called on close)
- `captcha_resolved` -- Resume after CAPTCHA
- `rate_limit_decision` -- Rate limit option selection
- `setup_agent_sent` -- Send during priming
- `save_agent_brain_config` -- Save in Agent Brain settings
- `get_agent_brain_config` -- Settings popover open, Setup screen load (JSON string -- parse it)
- `save_secondary_brain_config` -- Save secondary brain
- `get_secondary_brain_config` -- Settings popover open (JSON string -- parse it)
- `save_fallback_brain_config` (new) -- Save fallback brain
- `get_fallback_brain_config` (new) -- Settings popover open (JSON string -- parse it)
- `save_prompt_template` -- Save in System Prompts settings
- `get_prompt_template` -- Settings popover open (plain string, do NOT parse)
- `get_session_list` -- Sidebar load (JSON string -- parse it)
- `export_blueprint` -- Download button; now accepts an optional session_id
  so exporting a specific past session from sidebar history actually
  exports that session, not whatever's currently active (returns a plain
  file-path string, do NOT parse)
- `get_agent_health` -- Connected models indicator (JSON string -- parse it)
- `delete_session` (new) -- Sidebar three-dot menu Delete
- `rename_session` (new) -- Sidebar three-dot menu Rename
- `get_session_details` (new) -- Sidebar three-dot menu Session details (JSON string -- parse it)
- `get_recovery_state` -- App startup check (JSON string -- parse it)
- `recover_session` -- Recovery banner "Recover" button
- `get_project_memory` -- JSON string; parse it
- `get_global_memory` -- JSON string; parse it
- `clear_project_memory` -- void
- `get_open_questions` -- JSON string; parse it
- `get_model_strengths` -- JSON string; parse it
- `save_project_config` -- void
- `get_project_config` -- plain string; do NOT parse
- `get_memory_health` -- JSON string; parse it
- `repair_memory_index` -- void
- `get_patterns` -- JSON string; parse it
- `export_memory` -- void
- `restore_memory` -- void

### All listen() calls must be cleaned up on component unmount.

---

## Redesign Verification Record

- Tracked frontend diff against HEAD: 19 files, 404 insertions, 4,324 deletions.
- Templates audit: PASS; no production source renders a Templates control.
- Seven-model registry/use: PASS.
- AskUser option/custom/Escape/backdrop command paths: PASS.
- JSON-string versus plain-string command handling: PASS for all current
  frontend call sites.
- `npm run build`: PASS in the latest direct run (1,707 modules transformed).
- `cargo check`: recorded PASS for the completed backend/redesign state; not
  rerun by the documentation-only D-057 update.
- `git diff --check`: PASS for the documentation update.
- Current headless visual smoke test: PASS at 1440×1000. The earlier reported
  1440×960 result was not rerun during the documentation task.
- Local font files now resolve cleanly; the latest build did not emit the
  earlier missing-font warnings.

See `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/preview-redesign-post.md`.

---

## File Structure (actual, as built)

Root entry file: `/home/kasun/Music/arena/consensus-arena/index.html` loads
`/src/main.tsx` and contains no Google Fonts/CDN reference.

```
src/
├── index.css                — design system CSS variables + animations
│                              (rebuilt during mockup-matching batch --
│                              coordinate changes with lib/theme.ts)
├── vite-env.d.ts            — Vite client type reference (added to this
│                              list after an independent Cline audit found
│                              it was a real file missing from this
│                              listing — see DECISIONS.md D-056)
├── main.tsx                 — entry point
├── App.tsx                  — root, mounts IPC listeners, AskUserPopup,
│                              DebugPanel, theme restore
├── lib/
│   ├── utils.ts             — cn() utility
│   ├── theme.ts             — Theme type ('blue'|'light'|'dark') +
│   │                          applyTheme/loadStoredTheme/storeTheme
│   ├── tauri.ts             — safeInvoke/safeListen wrapper (prevents
│   │                          @tauri-apps/api throwing synchronously
│   │                          outside the Tauri webview)
│   └── agents.ts            — AGENT_IDS, AGENT_DISPLAY_NAMES, displayName()
├── stores/
│   └── useAppStore.ts       — Zustand store (see Zustand State Shape above)
├── hooks/
│   └── useIpcListeners.ts   — all Tauri event listeners with cleanup
├── components/
│   ├── layout/
│   │   ├── Sidebar.tsx      — collapse-aware
│   │   └── Topbar.tsx       — shared across all 4 views
│   ├── views/
│   │   ├── EmptyView.tsx
│   │   ├── SetupView.tsx
│   │   ├── PrimingView.tsx
│   │   └── ActiveView.tsx
│   ├── overlays/
│   │   ├── CaptchaOverlay.tsx
│   │   ├── RateLimitOverlay.tsx
│   │   └── AskUserPopup.tsx
│   └── shared/
│       ├── InputBar.tsx
│       ├── Toast.tsx
│       └── DebugPanel.tsx  (dev only)
└── panels/
    ├── SettingsPanel.tsx    — brains, prompts, Project Context, themes
    └── MemoryPanel.tsx      — memory health, facts, repair, backup/restore
```

`App.css` (a real but never-imported stylesheet) existed alongside
index.css for an unknown period and was removed during the mockup-matching
batch after its two genuinely useful class families (narrow-scrollbar
variants, `.bp-markdown` typography, `.ca-textarea`) were merged directly
into index.css.

---

## What Is NOT in This Interface

- No model response columns side by side
- No consensus meter
- No turn counter or round counter
- No debate cards
- No war-room monitoring dashboard
- No separate blueprint right panel
- No pause/resume buttons separate from send/stop
- No feature announcements on the home screen
- No onboarding flow
- No tooltips explaining features
- No Templates input button (intentionally removed from the mockup port)

(Note: "No multi-color brand theming" from an earlier version of this
list has been removed -- the approved mockup's default theme IS a real
blue color system; see Design Philosophy above. This list item was
correct under the old monochrome instruction and is not correct now.)
