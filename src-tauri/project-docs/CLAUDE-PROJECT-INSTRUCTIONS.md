# Consensus Arena — Claude Project Instructions

## IMPORTANT — Read This First Every Session

Before doing anything, read DECISIONS.md completely. It is the primary context file.
Then read whichever files are relevant to the current task (see File Map below).

If you suspect any project file is outdated based on what the user describes — stop immediately:
> "One or more project files may be outdated. Before proceeding, should we update them?"

Never work from stale project files. When a doc's claim and what the user actually
describes/shows (screenshots, terminal output, file contents) disagree, trust what's
real over what's written -- then fix the doc. Claude cannot browse other chat
sessions in this project; it only has direct access to the six project files
(DECISIONS.md, FRONTEND.md, BACKEND.md, ARCHITECTURE.md, IPC.md, PROCESS.md) via
its file tools, plus whatever the user shares directly in the current conversation.
"Cross-check with recent chats" is not something Claude can do on its own -- if
asked to verify against other sessions, Claude should say so plainly and ask the
user to supply whatever specific content needs checking.

---

## What This Project Is

Consensus Arena is a native Tauri 2.0 desktop application (Rust backend, React/TypeScript
frontend) that orchestrates multiple AI models through an autonomous leader-driven expert
panel to produce verified project blueprints.

Zero paid API keys for model participation -- all AI via the user's personal free web
accounts automated through browser injection. A separate configurable AI brain
(OpenAI-compatible API) acts as the orchestration agent.

---

## Current State

**Backend: Complete through Phase 1 Memory, checkpoint `f0847c0`.**
- The Phase 1 post-audit recorded successful `cargo check` and frontend
  `npm run build`, with no FAIL items.
- As of that post-audit, 38 commands are defined and all 38 registered.
  `AppState` has 16 fields, including `memory_store` and
  `last_memory_health`.
- RouteCompare, AskUser, GLM, Kimi, debug logging (tracing +
  tracing-subscriber), agent-brain fallback retry, secondary brain provider
  -- all implemented, not pending
- Session CRUD (delete_session, rename_session, get_session_details),
  spawn_blocking-wrapped DB access via a new db_helpers.rs, file-backed
  transcript persistence, and export_blueprint session_id honouring -- all
  implemented as of the Task 3/9/10 batch
- Phase 1 Memory is implemented at `app_data_dir/memory.db`: six normal
  tables plus FTS5, bounded/provenanced brain context, hard-pinned Project
  Context, reliability tracking, health/repair, and export/restore with a
  pre-restore backup. The SQLite smoke test confirmed required tables, WAL,
  and `user_version=1`; forbidden lock searches were empty.
- Do NOT rewrite any backend file without reading it completely first

**Frontend: FULLY BUILT AND REDESIGNED FROM preview.html.**
- All 4 views (Empty, Setup, Priming, Active), Zustand store, IPC listener
  hook, Settings panel (with a Fallback Brain section), Sidebar with full
  session CRUD, AskUser popup, CAPTCHA/rate-limit overlays, Toast,
  DebugPanel, and a shared Topbar component all exist as real, working
  files -- confirmed by direct reads of all real source files under src/
  and a live npm run tauri dev run
- Current implemented design reference:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
  The previous visual implementation has been replaced without changing the
  established backend IPC contract. Do not replace this design without
  explicit approval.
- The port includes the shell, Blue/Light/Dark themes, collapsible sidebar,
  all four views, primary/fallback/secondary settings, overlays, toasts,
  status drawer, and embedded hello path. The Templates input button is
  intentionally absent.
- The hello uses preview.html's actual Bézier path and transforms in inline
  SVG/CSS, not the obsolete Inter-font approximation. It is dependency-free
  rather than byte-for-byte Lottie playback; variable stroke width and the
  full gradient-stop set are simplified.
- Local variable Inter and JetBrains Mono files are present and verified;
  no Google Fonts runtime dependency remains.
- The historical Light-theme rectangle report is not currently reproduced.
  The height-chain fix remains; require a fresh current screenshot before
  reopening it.

**In flight:** The only remaining Phase 1 check is an interactive Tauri session
covering Route, RouteCompare, Blueprint, AskUser, memory export, and memory
restore. After that, the next implementation phase is Phase 2 Skills. Keep
O-004 agent-brain JSON response-format testing open unless directly verified.

## Roadmap

1. **Phase 1 — Memory: Implemented.** The agent remembers prior decisions,
   successes, failures, and model strengths; sessions no longer start blank.
2. **Phase 2 — Skills: Planned.** Load task-specific specialists such as
   Security Auditor, Database Designer, or MVP Scope Cutter using a
   gstack-style `SKILL.md` approach inspired by systems like OpenClaw.
3. **Phase 3 — Tools + MCP: Planned.** Add web search, project-file and
   documentation access, plus MCP connections to GitHub, Slack, the filesystem,
   and other services.
4. **Phase 4 — OpenCode Integration: Planned.** Delegate agreed blueprint
   sections to OpenCode CLI for implementation, tests, commits, and results
   returned to the leader for the next decision.
5. **Phase 5 — Better System Prompts: Planned.** Use structured, layered prompts
   with examples, trigger words, and XML structure for more predictable output.
6. **Phase 6 — Self-Improvement: Planned.** Record session learning, improve
   skills over time, and use a weekly curator to promote patterns, similar to
   Hermes-style learning.

---

## File Map — Load Only What You Need

| Task | Load These Files |
|------|-----------------|
| Starting any new session | DECISIONS.md — always first |
| Understanding full system | ARCHITECTURE.md + DECISIONS.md |
| Backend work | BACKEND.md + ARCHITECTURE.md + PROCESS.md |
| Frontend work | FRONTEND.md + IPC.md + PROCESS.md |
| IPC wiring | IPC.md + BACKEND.md |
| Any coding task | PROCESS.md + relevant files above |
| Architecture decisions | All files |

---

## How Work Gets Done — Updated Method

### Roles
- **Claude:** Designs everything. Reads source files. Delivers complete replacement files
  OR precise specifications for Codex CLI to execute. Makes all technical decisions
  autonomously.
- **User:** Runs Codex CLI sessions (workspace-write + on-request approval — see
  PROCESS.md's Codex patch). For non-Codex tasks: uploads current source files,
  places delivered files at specified paths. Says go/stop at major decision
  points only.
- **Codex CLI:** Primary execution and verification tool. Unlike Cline, it
  has real independent file/terminal
  access — reads actual current file state itself, runs cargo check/npm run
  build itself, reports real output. See PROCESS.md's Codex patch and the
  project's AGENTS.md for full detail. workspace-write + on-request approval
  is the default; never danger-full-access on this project.
- **Cline:** Legacy — infrastructure-only (npm install, initial scaffolding) if
  Codex CLI isn't in use. NOT used for coding frontend or backend logic.
- **Gemma 4 31B:** Audits only when Cline executes code. Largely superseded by
  Codex CLI's own independent verification capability.

### Historical Direct File Delivery Method

This was used before Codex CLI became the current execution tool. It is
retained only as delivery history, not as the default workflow.

```
User uploads current source files
        ↓
Claude reads ALL affected files completely
        ↓
Claude writes complete replacement files
(no partial code, no diffs, no "add this function")
        ↓
User places files at specified absolute paths
        ↓
User runs cargo check (backend) or npm run build (frontend)
        ↓
User reports result — Claude fixes any issues
        ↓
Git checkpoint
```

**Every file Claude delivers must be:**
- Complete — the entire file, not a fragment
- Drop-in ready — user places it and it works
- Error-free — Claude is responsible for correctness
- Consistent — all changes to one file done simultaneously

### File Batching Rule

All changes to the same file are delivered simultaneously.
If a feature touches files A, B, and C — all three are delivered together.
Never two separate deliveries for the same file.

### Verifying "current state" claims before trusting them

This exact doc, and DECISIONS.md/FRONTEND.md/BACKEND.md/ARCHITECTURE.md,
all carried obsolete incomplete-frontend, AppState-count, and model-support
claims for an extended
period after those things had actually shipped. This was only caught
because the user's own description/screenshots of a working app directly
contradicted the docs -- not because anything internal flagged the drift.
When a task depends on whether something is "done," "pending," or "built,"
and there's any doubt, prefer a real check over trusting a doc's status line:

- Backend: cargo check and a direct read of the specific file in question
- Frontend: find src -type f \( -name "*.tsx" -o -name "*.ts" -o -name
  "*.css" \) for the real file list, then read the specific files relevant
  to the task -- not just the ones a doc happens to mention
- Also check index.html at the project root when investigating anything
  font/asset/CDN-related -- it is easy to forget this file exists since it
  isn't a .tsx/.ts/.css file and won't show up in a frontend-only
  find command; it was missed for several rounds of debugging in this
  exact project because of this, and turned out to hold a real, relevant
  bug (a stale Google Fonts CDN reference).
- Claude cannot browse other Claude Project chat sessions to "cross-check"
  against them -- only the six project files above and whatever the current
  conversation contains are actually accessible. If asked to verify against
  other chats, say this plainly rather than guessing.

This doesn't replace reading DECISIONS.md first — it's an added step
when a doc's claim and the user's description of what they're looking at
don't match.

---

## Named Risks — Check Every Implementation

**RISK-BLOCKING:** `blocking_lock()` inside `on_navigation` or any async fn → UI freeze

**RISK-CHANNEL:** `tokio::sync::mpsc` inside `on_navigation` → must use `std::sync::mpsc`

**RISK-EVENTMATCH:** backend emits field X, frontend reads field Y → check every
app.emit() against IPC.md. Field names must match exactly.

**RISK-UNWRAP:** `.unwrap()` or `.expect()` in any production path → silent panic

**RISK-STALERESPONSE:** response from turn N captured as turn N+1 → check both
agent_id AND turn number in wait_for_response

**RISK-INITSCRIPT:** GENERIC_INIT_SCRIPT hardcoded per agent → must be static constant,
generic across all agents including GLM and Kimi

**RISK-NAVCLOSURE:** on_navigation captures agent_id by value → must read from URL
or window.__ca_agentId

**RISK-ASKCHANNEL:** ask_user_tx oneshot sender not cleared after use → must call
.take() to clear Option after sending, preventing double-send panic

**RISK-ASKDISMISS:** AskUser popup closed without calling provide_user_answer →
backend channel hangs forever. Frontend MUST call provide_user_answer("Cancelled")
on every popup close path including Escape key and backdrop click.

**RISK-IPCPARSE (new):** Several backend commands (get_agent_brain_config,
get_agent_health, get_fallback_brain_config, get_secondary_brain_config,
get_session_list, get_session_details, get_recovery_state, get_transcript)
return serde_json::to_string(&value) — a JSON-serialized STRING, not a
parsed object. Frontend invoke<T>() calls against these must JSON.parse()
the result before using it as T. This exact bug shipped at least four
separate times across different files (Sidebar.tsx, SettingsPanel.tsx x2
call sites, SetupView.tsx x2 call sites) before being systematically
caught by grepping every invoke<T>() call in the frontend against
commands.rs's real return types. get_prompt_template and export_blueprint
are the known exceptions — they return plain strings directly, not
JSON-wrapped. When adding any new invoke<T>() call: check the real Rust
command's actual return type in commands.rs first, don't assume based on
what "seems like" it should return.

**RISK-MEMORYLOCK:** `MemoryStore` uses `std::sync::Mutex`; async access must
go through `db_helpers::run_blocking()`. Never use
`memory_store.lock().await`. Memory failures inside `response_router` must
remain non-fatal.

---

## Core Constraints — Never Violate

- Zero paid API keys for model participation
- 4GB RAM / Celeron — total memory under 2GB at all times
- Maximum 2 WebViews simultaneously
- Native desktop only — Tauri 2.0
- No `blocking_lock()` in async code or on_navigation
- No `.unwrap()` or `.expect()` in production paths
- All IPC names must match IPC.md exactly
- on_navigation closure captures only nav_tx
- GENERIC_INIT_SCRIPT remains a static constant — never agent-specific
- Agent identity always from window.__ca_agentId — never captured by closure value
- debug-log event NOT added to IPC.md (development-only)
- AskUserState registered with .manage() before any command using it
- provide_user_answer ALWAYS called when AskUser popup closes — no exceptions
- Any new command returning a struct/collection must return it as a JSON
  string (serde_json::to_string), and every frontend caller must
  JSON.parse() it — see RISK-IPCPARSE above

---

## Tech Stack

**Backend:** Rust, Tauri 2.0, tokio (full), rusqlite (bundled + backup),
tauri-plugin-dialog, serde/serde_json, ring,
reqwest (json feature), uuid (v4), chrono (serde), urlencoding, tracing +
tracing-subscriber (env-filter) — implemented, not pending.

**Frontend:** React, TypeScript, Tailwind CSS, Zustand, Lucide icons,
@tauri-apps/api, @tauri-apps/plugin-dialog. Fonts are Inter + JetBrains Mono, loaded via local
@font-face in index.css pointing at /fonts/inter-variable.woff2 and
/fonts/jetbrains-mono-variable.woff2 — NOT DM Sans/DM Mono, and NOT loaded from
Google Fonts CDN at runtime. A packaged Tauri build has no guaranteed
network access; index.html previously had a dead Google Fonts <link>
for DM Sans/DM Mono (removed once found — nothing in the app actually
used those fonts, every component consumes Inter via CSS variables).
shadcn/ui was scaffolded early on but is **not** the actual styling
approach in current use — real components use inline style={{}} objects
keyed to CSS variables defined in index.css, not shadcn primitives or
Tailwind utility classes directly on JSX elements.

---

## Supported Models

| agent_id | Display | URL | Input type |
|----------|---------|-----|------------|
| chatgpt | ChatGPT | https://chatgpt.com | textarea |
| claude | Claude | https://claude.ai | contenteditable |
| gemini | Gemini | https://gemini.google.com | textarea |
| deepseek | DeepSeek | https://chat.deepseek.com | textarea |
| qwen | Qwen | https://chat.qwen.ai | textarea |
| glm | GLM | https://chat.z.ai/ | textarea (#chat-input, #send-message-button) |
| kimi | Kimi | https://www.kimi.com/ | Lexical contenteditable (execCommand injection) |

All 7 fully implemented — none pending.

---

## UI Design Principles

- Current implemented design reference is
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
  Source wins if it differs from prose. Do not replace the current frontend
  design without explicit approval.
- Not monochromatic. Default theme is a real blue color system (see
  index.css's :root block) — matches the mockup's own default
  (<html data-theme="blue">), including a gradient logo wordmark and a
  gradient "hello" empty-state animation. "Monochromatic only" was a stale
  instruction from an earlier design phase that predated the
  currently-approved mockup; it does not apply and should not be
  reintroduced.
- Themes: Blue (default) / Light / Dark — matches the mockup's three
  [data-theme] blocks. There is no Gray theme reachable from the UI. Blue
  is the default (an unset data-theme attribute resolves to :root's blue
  palette directly), not a secondary option alongside a separate
  monochrome default.
- Left sidebar: session history + three-dot menus (rename/export/details/
  delete — all backed by real commands), collapsible via a topbar toggle
  button (matches mockup's .sidebar.closed state)
- Shared components/layout/Topbar.tsx — extracted from what used to be 4
  duplicated inline topbar blocks; supports title, titleBadge (Session
  Active/Complete badge), and right (Download button) slots
- Main area: blueprint sections only — rendered markdown
- Send button converts to Stop when session active
- Account icon → settings popover
- Live status label at bottom — expandable drawer
- AskUser popup: modal overlay, blocks all interaction, ALWAYS sends answer on close
- Debug panel: dev only, Ctrl+Shift+D, never in production builds

### Hello implementation note

The current inline SVG/CSS uses preview.html's real path data. It does not
load Lottie and simplifies variable stroke width and gradient density.

---

## Non-Negotiable Architecture Decisions

Do not reopen these. Decided after extensive discussion.

- Single navigating WebView + persistent leader WebView (2 windows max)
- Dynamic agent-driven loop — not round-robin
- Leader runs meeting autonomously — agent brain follows leader's decisions
- Agent brain uses AI intelligence for routing — no pattern matching
- arena:// pseudo-protocol for JS→Rust IPC
- GENERIC_INIT_SCRIPT re-runs on every navigation automatically
- System prompts fully customizable — behaviour changed via prompts not code
- Blueprint built section by section as agent brain identifies finalized content
- Main window shows blueprint only — no individual model responses
- AskUser: backend owns the loop, frontend only shows UI, oneshot channel pattern
- D-037 (Custom model addition) permanently removed from scope

---

## Chat Session Management

At significant milestones, Claude notifies the user to start a new chat in this
same Claude project and identifies which project files need updating first.

Significant milestones:
- Backend batch complete
- Each frontend batch complete
- Context approaching limits

---

## Audit File Location

All audit files: src-tauri/project-docs/audits/
Never deleted. Permanent history of what was verified and when.
Most recent Phase 1 implementation audit:
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/phase1-memory-post.md`.
