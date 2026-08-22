# Consensus Arena — Decisions & Context Handoff

## Purpose

This file captures every decision made during development.
It exists to preserve full context when starting a new chat session.
**Read this file completely before making any suggestions or writing any code.**

---

## What This Programme Is

A native Tauri 2.0 desktop application that orchestrates multiple AI models
(via their personal web interfaces — no paid API keys) to produce verified
project blueprints. A separate AI agent brain (via user-configured
OpenAI-compatible API) watches the leader model's responses and routes
communication between models autonomously.

**The key insight:** The leader runs the meeting. The agent brain follows
the leader's decisions using its own AI intelligence. The user watches
the blueprint being built section by section.

---

## Current State Summary

**Backend: FULLY COMPLETE, including the D-035-D-042 batch, the
triage-fix-plan.md Tasks 1-12 batches (internally called Batch A/B/C), and
the Phase 1 memory system (D-058).**
- cargo check: 0 errors
- 22 modules registered in main.rs (21 original + new db_helpers.rs)
- 38 commands implemented in commands.rs, all 38 registered in
  generate_handler! (was 24 registered until an independent Cline audit
  found pause_session and resume_session existed as working functions but
  were never added to the registration list — see D-056 below; fixed)
- 16 AppState fields (the prior 14 plus `memory_store` and
  `last_memory_health`)
- AgentDecision enum has all 6 variants live: Route, Blueprint, Continue,
  Complete, RouteCompare, AskUser — none are commented-out/pending
- GLM and Kimi both fully implemented as participant models
- Debug logging (tracing + tracing-subscriber, file-backed rolling log) implemented
- Agent brain fallback retry (D-038) and secondary brain (D-039) implemented
- Session CRUD (delete/rename/get_session_details) implemented with a
  3-store cascade delete (transcript + blueprint + saved conversation URLs;
  cookies deliberately untouched -- see D-046 below)
- TranscriptStore/BlueprintStore/SessionVault switched from
  tokio::sync::Mutex to std::sync::Mutex, with all DB calls now routed
  through a new db_helpers::run_blocking() (spawn_blocking + retry/backoff)
  -- see D-047
- Settings and blueprint stores persist to disk (not memory); TranscriptStore
  also now file-backed (was previously in-memory -- see D-048)
- Lock-safe injection via inject_to_window pattern
- Atomic save_agent_brain_config implemented
- All IPC event payloads match IPC.md exactly
- Phase 1 memory persists six normal tables plus an external-content FTS5
  index in `app_data_dir/memory.db`, with provenance, hard-pinned Project
  Context, bounded brain context injection, reliability tracking, health/
  repair, and export/restore with automatic pre-restore backup
- Phase 1 Memory was checkpointed at commit `f0847c0`. Its post-audit has no
  FAIL items: `cargo check` and `npm run build` passed; the SQLite smoke test
  confirmed the required tables, WAL, and `user_version=1`; and forbidden
  searches found no `memory_store.lock().await` or `blocking_lock()` matches.

**Frontend: FULLY BUILT AND REDESIGNED FROM
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.**
- The previous wired React presentation was replaced with the preview.html
  shell, themes, collapsible sidebar, four views, settings panel, overlays,
  toasts, status drawer, and empty-state presentation while preserving the
  existing backend IPC contract.
- Setup, sidebar health, Settings connected accounts, priming, and active
  status UI use the shared seven-model registry: chatgpt, claude, gemini,
  deepseek, qwen, glm, and kimi.
- Settings includes primary, fallback, and secondary brain configuration.
- AskUser option, custom-submit, Escape, and backdrop paths all converge on
  `provide_user_answer`; dismiss paths send `"Cancelled"`.
- The input's mockup-only Templates button was intentionally omitted. The
  disabled attachment button remains.
- The hello now uses the mockup's actual Bézier path and transforms in a
  dependency-free inline SVG/CSS draw animation. It is not the earlier
  Inter-font compromise and does not load Lottie at runtime. It is not a
  byte-for-byte Lottie renderer: the source's variable stroke width and full
  gradient-stop set are simplified.
- Local variable Inter and JetBrains Mono assets now exist at
  `/home/kasun/Music/arena/consensus-arena/public/fonts/` and are referenced
  by the production `@font-face` rules. No font CDN is used.
- See D-057 and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/preview-redesign-post.md`.

**Next steps:**
1. Exercise Route, RouteCompare, Blueprint, AskUser, memory export, and memory
   restore during a real interactive Tauri session.
2. Then start Phase 2 Skills planning and implementation.
3. Keep agent-brain JSON response-format testing open (O-004) unless it is
   directly verified.

---

## Decisions Made — Backend

### D-059: Tauri Command Arguments Follow the Snake-Case IPC Contract

Tauri 2.6.2 defaults multiword Rust command arguments to camelCase in the
generated command wrapper. Because IPC.md and the frontend use snake_case,
every command with a multiword public argument must declare
`#[tauri::command(rename_all = "snake_case")]`. RISK-IPCPARSE / IPC audits
must compare Rust signatures, macro argument case, IPC.md payloads, and all
frontend invoke objects together. Single-word arguments are unaffected.

### D-058: Phase 1 Memory System DONE
Checkpoint: `f0847c0`.

`memory_store.rs` owns a low-RAM, file-backed SQLite database at
`app_data_dir/memory.db`. It contains six normal tables — `session_memory`,
`project_memory`, `global_memory`, `open_questions`, `model_reliability`, and
`pattern_memory` — plus the external-content `project_memory_fts` FTS5 table.
Memory context is bounded and prioritized. Project Context is high-importance,
hard-pinned, and always injected into the agent brain. Provenance is recorded
through `source_agent` and `source_type`.

Route and RouteCompare record routing facts and model-reliability adoption
checks. AskUser confirmed answers are stored as `user`/`confirmed`. Blueprint
finalization and completion summaries also feed memory. Export and restore are
implemented, with an automatic pre-restore backup; health checks and FTS repair
are available. The frontend implements `MemoryPanel` and a Project Context
section. All async memory DB access uses `db_helpers::run_blocking()` around a
`std::sync::Mutex`, and memory errors inside `response_router` remain non-fatal.

The post-audit passed with no FAIL items. `cargo check`, `npm run build`, the
SQLite schema smoke test (required tables, WAL, `user_version=1`), and the
forbidden lock searches all passed. The remaining manual check is a real Tauri
session covering Route, RouteCompare, Blueprint, AskUser, export, and restore.

### D-001: Single Window Architecture DONE
Two WebView windows only: one persistent leader window, one shared
navigating window. Never more than 2 windows. Memory constraint:
total must stay under 2GB on a 4GB/Celeron machine.
Implemented in browser_backend.rs.

### D-002: arena:// Protocol for JS-to-Rust IPC DONE
Fake URL navigation intercepted by on_navigation callback.
Below CSP enforcement. Proven working. Do not replace.
Implemented in browser_backend.rs.

### D-003: No Round-Robin Turn Order DONE
Replaced hardcoded round-robin in session_runner.rs with dynamic
agent-driven loop. run_debate now delegates entirely to
response_router::run_agent_loop().

### D-004: Agent Brain as Configurable API Client DONE
Separate AI model accessed via OpenAI-compatible API.
Not a meeting participant. User configures: API key, base URL,
model name, system prompt. Implemented in agent_brain.rs.

### D-005: Agent Brain Uses AI Intelligence for Routing DONE
No pattern matching. No rigid leader output format required.
Brain reads natural language, returns structured JSON decision.
AgentDecision enum: Route / Blueprint / Continue / Complete / RouteCompare / AskUser.
Implemented in agent_brain.rs. Uses #[serde(rename_all = "snake_case")]
(not "lowercase" as an earlier version of this doc said) -- this matters
because RouteCompare maps to "route_compare" and AskUser maps to "ask_user",
which specifically require snake_case, not simple lowercasing, to match
the JSON contract.

### D-006: Dynamic Session Loop DONE
Leader speaks -> Brain reads -> Brain decides -> App acts -> Repeat.
Implemented in response_router.rs. Called from session_runner::run_debate().

### D-007: Settings Store Persists to Disk DONE
User-configurable agent brain settings persisted in SQLite on disk.
Implemented in settings_store.rs.
Database path: app_data_dir/settings.db (resolved at startup in main.rs).
Keys: brain_api_key, brain_base_url, brain_model, brain_system_prompt,
      prompt_leader_priming, prompt_participant_priming,
      brain_fallback_api_key, brain_fallback_base_url, brain_fallback_model,
      brain2_api_key, brain2_base_url, brain2_model, brain2_system_prompt,
      last_session_id, session_complete.

### D-008: System Prompts Are Fully Customizable DONE (backend + UI)
Backend storage complete via settings_store.rs and save_prompt_template command.
Frontend settings UI (SettingsPanel.tsx) implements this -- Leader Priming
Template and Participant Priming Template fields with Save.

### D-009: Blueprint Built Section by Section DONE
Agent brain identifies finalized sections, extracts title+content,
pushes to main window via blueprint-section-added event,
saves to blueprint_store.rs via upsert_section().
Blueprint database path: app_data_dir/blueprint.db.
Implemented in response_router.rs.

### D-010: Session Termination DONE
Two conditions: AgentDecision::Complete OR user presses Stop
(abort_session command). No fixed iteration cap in normal flow.
Implemented in response_router.rs and commands.rs.

### D-011: Lock-Safe Injection via inject_to_window DONE
browser_backend.rs exposes two injection functions:
- inject_to_agent(&BrowserState, ...) -- used by session_runner setup phase
- inject_to_window(WebviewWindow, ...) -- used by response_router
response_router extracts window handle from BrowserState in a scoped block,
drops the lock, then calls inject_to_window with the cloned handle.
This ensures browser_state lock is never held across an await point.
RISK-BLOCKING: fully resolved.

### D-012: save_agent_brain_config is Atomic DONE
Operation order: construct AgentBrain first -> save to DB -> update AppState.
If construction fails, nothing is saved. If DB save fails, AppState is
not updated. No partial state possible. Implemented in commands.rs.

### D-013: AgentBrain::new() is Fallible DONE
Returns Result<Self, AgentError>.
reqwest::Client::builder().build() error mapped to AgentError::NetworkError.
All call sites handle the Result with .map_err(|e| e.to_string())?.
AgentBrain now also carries an optional fallback config (D-038) and a
without_fallback() builder method to explicitly clear it.

### D-014: JSON Fence Fallback in agent_brain.rs DONE
After code fence stripping, finds first '{' via json_start index approach.
Handles prose-prefixed responses (e.g. "Here is my decision: {...}").
If no '{' found, returns AgentError::NetworkError -- explicit failure,
not silent garbage parsing.

### D-015: NavEvent Channel Buffer 256 DONE
std::sync::mpsc::sync_channel(256) used for nav events.
Dropped events logged with eprintln! rather than silently discarded.

### D-016: wait_for_response Handles All NavEvent Variants DONE
Matches: Response (text), Done (long response complete), Error (agent error).
Wildcard arm silently skips Ready and SendDetected.
Timeout handled via Err(_elapsed) match arm -- never uses ? on timeout.
Checks BOTH agent_id AND turn number before accepting any response.

### D-035: RouteCompare AgentDecision Variant DONE
RouteCompare { models: Vec<String>, prompt: String } -- implemented in
agent_brain.rs. response_router.rs has a RouteCompare arm that routes to
each listed model in sequence, collects all responses, and returns a
combined "[X said: ...][Y said: ...]" block to the leader.

### D-036: GLM Participant (chat.z.ai) DONE
agent_id: glm | display_name: GLM | base_url: https://chat.z.ai/
Input: #chat-input (textarea) | Send: #send-message-button
Svelte framework -- .svelte-* hash classes ignored, ID selectors used.

### D-037: Custom Model Addition -- REMOVED FROM SCOPE
This feature was explicitly removed. Not to be implemented.

### D-038: Agent Brain API Fallback DONE
settings_store.rs has brain_fallback_api_key/base_url/model fields plus
get/save_fallback_brain_config methods. agent_brain.rs's decide() retries
once with a fallback client on primary failure, surfacing the original
error if the fallback also fails. commands.rs has save_fallback_brain_config
(new, Task 5/HIGH-3 -- previously the storage/retry logic existed but had
no command reaching it) and get_fallback_brain_config. SettingsPanel.tsx
has a "Fallback brain" UI section (3 fields -- no system_prompt, since the
fallback always reuses the primary's).

### D-039: Secondary API Provider DONE
AppState has agent_brain_2: Arc<Mutex<Option<AgentBrain>>>.
commands.rs has save_secondary_brain_config / get_secondary_brain_config.
response_router.rs's run_agent_loop switches permanently to agent_brain_2
after 3 consecutive primary decide() failures (brain_fail_count on AppState,
an AtomicU32), resetting to 0 on any success before that threshold.

### D-040: Debug Logging Tier 1 + Tier 2 DONE
Tier 1 (Rust tracing): tracing + tracing-subscriber (env-filter) added to
Cargo.toml. main.rs's .setup() closure initializes a daily rolling
file-backed subscriber (tracing_appender), default filter "debug",
overridable via RUST_LOG. 7 log points implemented across
response_router.rs/agent_brain.rs as originally spec'd.
Tier 2 (WebView error forwarding): out of scope for this doc's current
verification pass -- GENERIC_INIT_SCRIPT's console.error override was not
directly re-confirmed in the most recent read-through; verify against
browser_backend.rs directly if this becomes relevant to a task.
Tier 3 (DebugPanel.tsx, frontend): DONE -- Ctrl+Shift+D toggle, circular
200-entry buffer, tag filter, dev-only via import.meta.env.DEV guard.

### D-041: AskUser Interactive Popup DONE
AgentDecision::AskUser { question, options, allow_custom } -- implemented.
AppState has ask_user_tx: Arc<Mutex<Option<oneshot::Sender<String>>>>.
response_router.rs's AskUser arm: creates oneshot channel, stores tx,
emits agent-ask-user, awaits rx (loop suspended, no spin), injects answer
as context to leader on receipt. provide_user_answer command uses
.take() to atomically clear the Option and prevent double-send
(RISK-ASKCHANNEL resolved). Frontend: AskUserPopup.tsx mounted once at
App.tsx root, calls provide_user_answer on every close path including
Escape/backdrop click (RISK-ASKDISMISS resolved).

### D-042: Kimi Participant (www.kimi.com) DONE
agent_id: kimi | display_name: Kimi | base_url: https://www.kimi.com/
Input: div.chat-input-editor[contenteditable="true"] (Lexical editor,
execCommand injection path) | Send: div.send-button-container
Vue.js framework -- data-v-* attributes ignored, class selectors used.

---

## Decisions Made — Batch A/B/C (triage-fix-plan.md Tasks 1-12)

These decisions came from a consolidated triage document synthesized from
nine prior Cline audit sessions, executed as three delivery batches.

### D-046: Session Delete Cascade Excludes Cookies DONE
delete_session (Task 3/CRIT-4) cascades across TranscriptStore (turns +
session row), BlueprintStore (sections), and SessionVault (saved
conversation URLs only). Deliberately does NOT touch SessionVault's
cookies table -- cookies are keyed by agent_id (the user's login state
with that model's website), not by session, and must survive deleting any
number of sessions. Also refuses to delete the currently-active session
(checked via session_active AtomicBool + orchestrator's current_session).

### D-047: DB Access via db_helpers::run_blocking (Task 9/HIGH-5,HIGH-6) DONE
New module db_helpers.rs. TranscriptStore, BlueprintStore, and
SessionVault switched from Arc<tokio::sync::Mutex<_>> to
Arc<std::sync::Mutex<_>> specifically so their synchronous rusqlite calls
run inside tokio::task::spawn_blocking (off the async runtime thread) via
a shared run_blocking() helper, with a 3-attempt retry/backoff (50ms x
attempt) for transient failures. settings_store.rs was deliberately NOT
converted -- out of this task's scope; its reads are tiny single-key
lookups on the hot path of nearly every command, and converting it is a
separate, larger pass.

### D-048: TranscriptStore File-Backed, Not In-Memory (Task 7/CRIT-5) DONE
AppState::new() now takes a data_dir and derives settings.db, blueprint.db,
and transcript.db as three independent paths (previously blueprint.db was
derived by string-replacing "settings.db" in the settings path -- silently
broke if that filename ever changed). TranscriptStore uses the file-backed
open() constructor, not the in-memory new().
SessionVault remains in-memory (SessionVault::new()) -- this was never
flagged by any audit and is explicitly out of scope; do not "fix" this
without it being separately requested.

### D-049: Shared Topbar.tsx Extracted (Batch D) DONE
Previously EmptyView/SetupView/PrimingView/ActiveView each duplicated
near-identical inline topbar markup. Extracted to
components/layout/Topbar.tsx with title / titleBadge / right slot props
-- completes what FRONTEND.md's file structure had listed as a target
("Topbar.tsx (if needed by mockup)") but never actually built as a
separate file until this batch.

### D-050: Mockup Is Ground Truth, Design Is Not Monochromatic (Batch D) DONE
An earlier version of this project's docs said "Monochromatic only" -- this
predated the currently implemented mockup
(`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`), which has
a real blue color system as its default theme (<html data-theme="blue">),
a gradient logo wordmark, and a gradient "hello" animation. User confirmed
explicitly: mockup is ground truth, the monochrome instruction is stale.
index.css was rebuilt with :root as the mockup's blue palette directly,
plus [data-theme="light"] and [data-theme="dark"] blocks matching the
mockup's other two theme states. Roughly 1,180 lines of dead legacy CSS
from an earlier, explicitly-rejected three-column/debate-card/
consensus-meter/command-palette design were removed -- confirmed dead via
direct grep of every className used in every real .tsx file, not assumed.

### D-051: Sidebar Collapse + Session Badge (Batch D) DONE
useAppStore.ts gained sidebarCollapsed: boolean + toggleSidebar(). Sidebar.tsx
is now collapse-aware (width goes to 0, opacity goes to 0, border removed --
matches mockup's .sidebar.closed exactly). App.tsx's <main> marginLeft
responds in step. ActiveView.tsx's Topbar usage carries a titleBadge
(Session Active/Complete) and right slot (Download button), preserved from
the pre-batch inline version.

### D-052: html/#root/body Height Chain Fix (Batch D-2) DONE
A visible mismatched-background rendering artifact (a rectangle that didn't
match the rest of the window) was traced to document.body collapsing to
height:1px -- confirmed via document.body.getBoundingClientRect() in
devtools. Root cause: html had no height rule and #root (the div React
mounts <App/> into, per main.tsx) had zero CSS applied to it anywhere --
body{height:100vh} alone does nothing if intermediate ancestors don't also
participate in the height chain. Fixed: html{height:100%},
#root{height:100%;display:flex}, body switched from height:100vh to
height:100% plus width:100%.
The issue has not been reproduced in the current redesign smoke test.
Treat it as historical unless a fresh current screenshot reproduces it.

### D-053: Hello Animation Ported from preview.html (Batch D through preview redesign) DONE
Earlier attempts used an illegible hand-drawn approximation, a platform-
dependent cursive fallback, and skewed Inter. Those are historical and are
not the current implementation. `src/components/views/EmptyView.tsx` now
contains an inline SVG built from preview.html's actual Bézier vertices,
control points, layer/group transforms, and gradient colors;
`src/index.css` supplies the repeating draw animation. This preserves the
recognizable embedded hello without adding the mockup's CDN-hosted Lottie
runtime. It is a deliberate dependency-free port, not byte-for-byte Lottie
playback: the Lottie source animates stroke width and contains a denser
17-stop gradient while the React SVG uses a fixed stroke and fewer stops.

### D-054: index.html -- Dead Google Fonts CDN, Wrong Font Family (Batch D-2) DONE
index.html (project root -- NOT under src/, easy to miss when grepping
.tsx/.ts/.css files, and was in fact missed for several rounds of
debugging before being directly requested and read) was loading DM
Sans/DM Mono from fonts.googleapis.com/fonts.gstatic.com via a
link rel="stylesheet", while every real component actually uses
Inter/JetBrains Mono via index.css's --font/--font-mono variables. Fixed:
removed the preconnect + stylesheet link tags entirely. index.css's
local @font-face rules are the only font source now. A later typography
completion copied verified variable font assets to
`/home/kasun/Music/arena/consensus-arena/public/fonts/inter-variable.woff2`
and `.../jetbrains-mono-variable.woff2`; production and built-output paths
were verified directly.

### D-056 (new): Independent Cline Audit Confirmed Docs Largely Accurate, Found One Real Code Bug DONE
After the Batch D-D3 doc corrections were written, the user (correctly)
did not trust Claude's own self-report of accuracy and had Cline perform
an independent, read-only audit -- comparing 22 specific factual claims in
the updated docs against the real codebase, plus an open-ended scan for
undocumented reality. Result: 19-21 of 22 claims confirmed correct
(depending on how partial matches are counted), 3 discrepancies found:
  1. AppState has 14 fields, not 13 -- a genuine miscount in the docs,
     corrected.
  2. commands.rs defines 26 #[tauri::command] functions, but main.rs's
     generate_handler! only registered 24 of them. Diffing the two lists
     directly (not trusting either count in isolation) identified the
     exact two missing: pause_session and resume_session. Both were
     fully implemented, working functions -- they updated
     OrchestratorStatus and emitted session-status correctly -- but were
     simply never added to the registration list, meaning the frontend
     could never actually invoke them. This is a genuine, previously
     undetected bug, not a doc-only issue. FIXED in main.rs.
  3. FRONTEND.md's file-structure listing was missing three real files:
     lib/tauri.ts, lib/agents.ts, vite-env.d.ts. Doc-only omission,
     corrected.
This is a good example of why an independent audit step matters even
after Claude has already "verified" its own work by reading source files
-- reading files correctly and then writing an accurate summary of what
was read are two different steps, and an off-by-one miscount or an
unregistered-but-defined function are exactly the kind of small, easy-to-
miss errors that a second, independent pass catches. Going forward, any
claim about "N fields" or "N commands" should be verified by an actual
count command (grep -c, or equivalent), not by incrementing a running
mental tally while reading through a file.

### D-057: preview.html Frontend Redesign Port Completed DONE
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`
is now the current implemented frontend design reference. The production
React frontend replaces the previous visual implementation while retaining
the existing Zustand state and Tauri command/event wiring. The port covers
the shell, Blue/Light/Dark themes, collapsible sidebar, Empty/Setup/Priming/
Active views, settings, overlays, toasts, live-status drawer, and embedded
hello path.

The mockup's Templates input button was intentionally not ported. The input
contains only the disabled attachment control and Send/Stop control. The
seven-model registry is shared by setup, sidebar health, Settings connected
accounts, priming, and active-status UI. Settings exposes primary, fallback,
and secondary brain configuration. Rate-limit handling includes `lighter`.
Priming renders `sessionAgentIds` selected during setup rather than a
hardcoded model list. AskUser option click, custom submit (button or Enter),
Escape, and backdrop dismissal all call `provide_user_answer` through one
guarded function.

Direct source checks confirmed JSON-string results are parsed for the
configuration, health, session-list/details, and recovery commands used by
the frontend; plain `get_prompt_template` and `export_blueprint` results are
not parsed. The tracked frontend diff against HEAD is exactly 19 files,
404 insertions, and 4,324 deletions. The current aggregate worktree also
contains earlier backend batch changes, so Git alone cannot prove the
historical claim that the redesign changed no backend source; no backend or
dependency files were edited by this documentation task.

Recorded verification status:
- `npm run build`: PASS. The most recent direct build transformed 1,707
  modules and completed successfully; after local fonts were added it emitted
  no missing-font warnings.
- `cargo check`: previously recorded PASS for the completed backend/redesign
  state; not rerun during the documentation-only D-057 update.
- `git diff --check`: PASS for this documentation update.
- Templates source/DOM audit: PASS.
- Headless visual smoke test: PASS at 1440×1000 for the current Settings,
  sidebar, input, suggestions, and hello presentation. The previously
  reported 1440×960 run was not independently reproduced in this doc task.

Detailed evidence is in
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/preview-redesign-post.md`.

---

## Deferred Items — Fix Before Release (Not Blocking Frontend)

### DEF-001: run_debate Holds agent_brain Lock for Entire Session RESOLVED
session_runner.rs's run_debate now clones the AgentBrain out of the lock
before starting the session loop (all fields are cheap to clone --
reqwest::Client is Arc-backed, rest is String). agent_brain lock is free
for the entire duration of a session, so save_agent_brain_config can be
called mid-session without deadlock.

### DEF-002: AgentBrainConfig Built with Empty Priming Prompts -- NOT A REAL BUG
Investigated during Batch A/C planning: the actual save_agent_brain_config
command never constructs an AgentBrainConfig struct with priming-prompt
fields at all -- it writes 4 specific settings keys directly
(brain_api_key/base_url/model/system_prompt) and never touches
prompt_leader_priming/prompt_participant_priming, which are independently
written by save_prompt_template and independently read back by
get_agent_brain_config. Nothing was ever being silently wiped. This item
was a stale finding from the original audit, confirmed via direct code
read, not fixed because there was nothing to fix.

### DEF-003: unwrap_or_default on Settings DB Reads -- NOT A REAL BUG
Investigated during Batch A/C planning: SettingsStore::get() already
distinguishes a missing key (Ok(None)) from a real DB failure
(Err(SqliteError) leading to Err(AgentError::DatabaseError)) correctly.
Every commands.rs call site does .map_err(...)? BEFORE
.unwrap_or_default(), so a genuine DB error already short-circuits via ?
and never reaches the default -- the only thing unwrap_or_default()
collapses is a legitimately unset key, which is correct behaviour (e.g.
"no fallback configured" should mean empty string, not an error).
Confirmed via direct code read, not fixed because there was nothing to fix.

---

## Decisions Made — Frontend

### D-017: Complete Frontend Rewrite Required DONE (historical)
Current frontend (Aider prototype) rated 3/10 at the time. Wrong IPC names.
Mock data everywhere. Wrong component structure. Full rewrite was
performed -- this decision is historical context, not a pending task.

### D-018: Design Philosophy — See D-050
Superseded by D-050 above. The "ChatGPT-inspired, monochromatic" framing
from this decision predates the currently-approved mockup and no longer
applies to color usage -- the sidebar/topbar/settings *structure* being
ChatGPT-like is still accurate, but "monochromatic only" is not.

### D-019: Main Chat Area Shows Blueprint Only DONE
No individual model responses in main window.
Only finalized agreed content from agent brain Blueprint decisions appears.
Sections appear progressively. Rendered as markdown. Implemented in
ActiveView.tsx.

### D-020: Live Status Label DONE
Single line fixed at bottom of content area, above input bar. Expandable
drawer implemented in ActiveView.tsx, matching the mockup's .stline/.sdrawer
pattern -- fed by agent-state-change/agent-routing/boss-message events.

### D-021: Left Sidebar — ChatGPT Pattern DONE
Session history list. Three-dot menu per session: rename, delete, export,
session details -- all four backed by real commands (Task 3 batch).
New Session button at top. Account/settings icon at bottom. Connected
models indicator above account icon. Sidebar collapse added in Batch D
(see D-051), not part of the original decision but consistent with it.

### D-022: No Separate Pause/Resume Buttons DONE
Send button converts to Stop button when session is active. Implemented
in InputBar.tsx / ActiveView.tsx.

### D-023: Blueprint Utility Buttons DONE
Copy button per section (hover reveal). Download button for full
blueprint (markdown), now part of the shared Topbar's right slot (see
D-049/D-051) rather than a standalone inline button.

### D-024: Settings via Account Icon DONE
Account icon at bottom of sidebar opens settings popover.
Sections: Connected Accounts, Agent Brain config, Fallback Brain config
(new -- Task 5/HIGH-3), System Prompts, Appearance, About. ("Performance"
section from the original decision was not confirmed present in the most
recent direct read of SettingsPanel.tsx -- verify directly if this becomes
relevant, don't assume either way.)

### D-025: Tech Stack for Frontend — See Tech Stack section, updated
React + TypeScript, Tailwind CSS, Lucide icons, @tauri-apps/api, Zustand.
shadcn/ui was scaffolded in Session 1 but is not the actual styling
approach in current use -- real components use inline style={{}} objects
keyed to CSS variables in index.css, not shadcn primitives.

### D-026: UI Design — preview.html Port DONE, See D-050/D-053/D-057
Current design reference:
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
The production React port is complete. Any later replacement of this design
requires explicit approval.

### D-027: Blueprint Display Format DONE
Main chat area only -- no separate right panel. Blueprint renders as live
markdown directly in main content area, using ReactMarkdown in
ActiveView.tsx with the .bp-markdown typography classes (merged into
index.css from the previously-dead App.css during Batch D).

### D-043: Frontend Delivery Method — Historical, superseded by Codex CLI
Complete-file delivery was used through Batch D-3. Codex CLI is now the
current execution and verification tool: it reads real source, makes scoped
edits, and runs the relevant checks directly. Audit-before-edit and
source-over-document rules remain mandatory.

### D-044: Frontend Sessions Collapsed to 3 DONE (historical)
This decision described the original 3-session plan for the initial
frontend build. All 3 sessions' worth of work is complete, plus the
subsequent Batch A-D pixel-match/bugfix rounds. This decision is now
historical context only.

---

## Decisions Made — Process

### D-028: Execution Tool — Codex CLI CURRENT
The original Cline workflow and later complete-file-delivery workflow are
historical. Codex CLI is now the execution and independent verification
tool. It reads current source, applies scoped edits, and runs checks
directly. Cline remains legacy/infrastructure-only.

### D-029: Nemotron-Optimized Cline Prompt Format (historical)
Pattern developed after Nemotron was found incompatible. Superseded by
later direct delivery and now by the Codex CLI workflow in D-028.

### D-030: Cline Model — Gemini 2.5 Pro via Google AI Studio
NVIDIA Nemotron Super incompatible (thinking mode breaks tool-call parser).
Mistral-Nemotron too slow for multi-phase prompts.
Confirmed working: Gemini 2.5 Pro via Google AI Studio (free tier).
Only relevant if Cline is used for an infrastructure task -- see D-028.

### D-031: All Cline Prompts Must Use Absolute Paths
Lesson from file structure collapse. Still applies if Cline is used.

### D-032: Git Checkpoint After Clean Verification and User Review
After checks pass and the user approves the diff, run:
  git add -A && git commit -m "checkpoint: [description]"
Do not commit before review when the user explicitly reserves that decision.

### D-033: Audit-Then-Implement, Never Patch Without Auditing DONE
No code changes without a pre-audit reading the affected files first.
For current Codex work, Codex reads all affected files completely before
editing and independently runs the relevant post-change checks.

### D-034: Pre-populate Known Bugs in Audit Prompts
Audit post-reports must carry forward all previously logged bugs.

### D-045: Audit Evidence Is Task-Proportional CURRENT
The mandatory read phase is the pre-audit. Post-change evidence may be a
formal audit document or direct build/check/source results proportional to
risk. Never claim verification without recording the actual evidence.

### D-055 (new): Verify "Current State" Claims Against Real Files, Not Just Docs
This exact set of project docs described the frontend as unbuilt and
several implementation sessions as pending for an extended period after they had
already happened -- undetected until the user's own description/screenshots
of a working app directly contradicted it. Going forward: when a task
depends on whether something is "done"/"pending"/"built" and there's any
doubt, run a real check (cargo check + direct file read for backend; a
real find over src/ plus direct reads for frontend; don't forget
index.html at the project root, which is easy to miss since it isn't
matched by a .tsx/.ts/.css-only find and was in fact missed for several
debugging rounds before being directly requested) rather than trusting a
doc's status line at face value.

---

## Open Items

### O-001: Frontend Structure RESOLVED (Session 1, historical)
package.json, tsconfig, vite.config.ts all at project root. Build passes.

### O-002: Frontend Design RESOLVED, see D-057
preview.html is ported into production React. The Light-theme rectangle is
not a current verified defect; reopen it only with a reproducible current
screenshot.

### O-003: shadcn/ui Installation RESOLVED (Session 1, historical)
Scaffolded but not the actual styling approach in current use (see D-025).

### O-004: Agent Brain Response Format Testing — STILL OPEN
Default system prompt defined, including AskUser/RouteCompare action
types. Real-world JSON output reliability testing across all 7 models has
not been confirmed as complete -- treat as still open unless directly
verified.

### O-005: Backend Batch Implementation RESOLVED
All D-035 through D-042 implemented, confirmed via direct code read.

### O-006: Formal preview-redesign audit RESOLVED
See `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/preview-redesign-post.md`.

---

## Non-Negotiable Constraints

- Zero paid API keys for model participation -- all via user's personal web accounts
- 4GB RAM / Celeron -- total memory under 2GB at all times
- Maximum 2 WebViews simultaneously
- Native desktop only -- Tauri 2.0
- No blocking_lock() in async code or on_navigation
- No .unwrap() or .expect() in production paths
- All IPC event names must match IPC.md exactly
- on_navigation closure captures only nav_tx sender
- GENERIC_INIT_SCRIPT remains a static constant -- never agent-specific
- Agent identity always from window.__ca_agentId -- never captured by closure value
- All Cline prompts must use absolute file paths (when Cline is used -- see D-028)
- Git checkpoint after clean verification and user approval
- debug-log event NOT added to IPC.md (development-only event)
- AskUserState must be registered with .manage() before any command using it
- Any new command returning a struct/collection must return
  serde_json::to_string(&value), and every frontend caller must
  JSON.parse() the result -- this exact bug (calling invoke<T>() and using
  the raw JSON string as if it were already T) shipped at least four
  separate times (Sidebar.tsx, SettingsPanel.tsx x2 calls, SetupView.tsx x2
  calls) before being systematically caught by grepping every invoke<T>()
  call against commands.rs's real return types. get_prompt_template and
  export_blueprint are the known exceptions that correctly return plain
  strings, not JSON.
