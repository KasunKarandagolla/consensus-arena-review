# Consensus Arena — Architecture

## Product Identity

A native Tauri 2.0 desktop application that orchestrates an expert panel of AI
models to produce verified, stress-tested project blueprints. The user describes
a project. A designated leader model runs the meeting autonomously, consulting
other models as it decides, until a complete blueprint is produced section by section.

**Not a round-robin debate tool. Not a monitoring dashboard. An autonomous
consulting engagement run by AI.**

Zero paid API keys for model participation — all AI via the user's personal
free web accounts, automated through browser injection.

---

## Core Concept

The leader model runs the meeting like a senior consultant. It decides:
- When to consult another model
- What to ask them
- When a section is finalized
- When the blueprint is complete
- When to ask the user a clarifying question (AskUser)

A separate AI agent brain (not a participant) watches every leader response
and acts on those decisions. The user watches the blueprint being built in
real time. They do not watch the models argue.

---

## The Agent Brain

A separate AI model accessed via OpenAI-compatible HTTP API.
This is NOT one of the meeting participants. This is the orchestration intelligence.

**Configured by user:**
- API key
- Base URL (OpenAI-compatible — works with DeepSeek, Gemini, Ollama, any compatible endpoint)
- Model name
- System prompt (fully customizable — this defines all agent behaviour)
- Optional fallback model (D-038 — retries once on failure; implemented,
  including a UI section in SettingsPanel.tsx)
- Optional secondary brain (D-039 — alternative orchestrator, switched to
  automatically after 3 consecutive primary failures — implemented)

**What the agent brain does:**
Reads every leader response using its own AI intelligence and decides:
1. Does the leader want input from another model? → Who? What exact prompt?
2. Does the leader want side-by-side responses from multiple models? (RouteCompare)
3. Has the leader finalized a section? → Extract title + content, push to blueprint
4. Does the session need user input to continue? (AskUser)
5. Is the meeting complete? → Signal session end

No pattern matching. No rigid output format required from the leader.
The brain reads natural language and acts.

**AgentDecision enum — all 6 variants implemented, none pending:**
```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentDecision {
    Route { target_model: String, prompt: String },
    Blueprint { section_title: String, section_content: String },
    Continue,
    Complete,
    RouteCompare { models: Vec<String>, prompt: String },
    AskUser { question: String, options: Vec<String>, allow_custom: bool },
}
```

Note: `rename_all = "snake_case"` — an earlier version of this document
said `"lowercase"`. This matters specifically for `RouteCompare` (→
`"route_compare"`) and `AskUser` (→ `"ask_user"`), which need snake_case
word-splitting behaviour, not simple lowercasing of an already-single word.

**Default agent system prompt (stored in settings_store, fully customizable):**
```
You are an orchestration agent managing an expert panel discussion.
Your job is to read the leader's responses and decide what action to take next.

You must respond in JSON with this structure:
{
  "action": "route" | "route_compare" | "blueprint" | "ask_user" | "continue" | "complete",
  "target_model": "claude|chatgpt|gemini|deepseek|qwen|glm|kimi" (only if action is route),
  "models": ["model1", "model2"] (only if action is route_compare),
  "prompt": "exact prompt to inject" (only if action is route or route_compare),
  "section_title": "title" (only if action is blueprint),
  "section_content": "exact finalized text" (only if action is blueprint),
  "question": "short question for user" (only if action is ask_user),
  "options": ["option1", "option2"] (only if action is ask_user, 2-4 items),
  "allow_custom": false (only if action is ask_user)
}

Rules:
- If the leader is asking for input from a specific participant, action is "route"
- If the leader wants to compare responses from multiple participants, action is "route_compare"
- If the leader has produced a finalized section ready for the blueprint, action is "blueprint"
- If the session is genuinely ambiguous and user input would change the approach, action is "ask_user"
- If the leader is still working and needs no routing, action is "continue"
- If the leader signals the blueprint is complete, action is "complete"
- Use ask_user sparingly — only when the answer materially changes direction
- Always include "Your choice" as one of the ask_user options
```

---

## Session Flow

### Phase 1 — Session Setup (Frontend)

User configures via the Setup screen:
- Project brief (free text)
- Session type (Architecture / MVP / API Design / Security Review / Custom)
- Leader model (selected from authenticated models)
- Participating models (toggled on/off) — includes GLM and Kimi (both fully
  implemented, not pending)
- Agent brain (API key, base URL, model name, system prompt — collapsible section)

Start Session button disabled until: brief entered + 2+ models + leader chosen + brain configured.

### Phase 2 — Model Priming (run_setup in session_runner.rs)

App opens each selected model's window one at a time.
For each model:
1. Inject role-priming prompt into input field via eval()
2. User reviews prompt in the model window and presses Send
3. App detects Send via arena://sent/{agent_id} signal
4. Saves conversation URL (via db_helpers::run_blocking, since SessionVault
   is now Arc<std::sync::Mutex<_>> — see Task 9 in BACKEND.md), emits
   setup-agent-complete, moves to next model

All priming prompts are fully customizable via settings.

### Phase 3 — Autonomous Session Loop (run_agent_loop in response_router.rs)

```
Leader speaks freely in leader window
        ↓
wait_for_response() captures response
        ↓
brain.decide(leader_response, context, memory_context) → AgentDecision
   (or agent_brain_2.decide() if 3+ consecutive primary failures — IMP-10)
        ↓
Route        → emit agent-routing
               inject to participant → wait for response
               inject "[Response from X]: ..." to leader
               loop continues
        ↓
RouteCompare → route to each model in sequence
               collect all responses
               inject combined "[X said: ...][Y said: ...]" to leader
               loop continues
        ↓
Blueprint    → save section to blueprint_store (via db_helpers::run_blocking)
               emit blueprint-section-added to frontend
               inject acknowledgement to leader
               loop continues
        ↓
AskUser      → store oneshot tx in ask_user_tx
               emit agent-ask-user to frontend
               await rx (loop suspended — no spin)
               receive answer from provide_user_answer command
               inject answer as context to leader
               loop continues
        ↓
Continue     → emit boss-message status
               inject "Please continue." to leader
               loop continues
        ↓
Complete     → mark session_complete=true in settings_store (for recovery)
               emit session-complete
               return Ok(()) — loop exits
        ↓
User Stop    → abort_session command → loop exits
```

When memory context is available, `AgentBrain::decide` appends it to the
effective system prompt used by primary, fallback, and secondary brains. The
context is injected into the orchestration brain only, never directly into a
participant WebView model. Project Context is hard-pinned so it is always
included within the bounded memory context.

### Phase 4 — Blueprint Export

User downloads complete blueprint as markdown file, from the shared Topbar's
`right` slot (during an active session) or from a specific past session via
the Sidebar's three-dot menu (export_blueprint now accepts an optional
session_id — HIGH-8 — so exporting a past session actually exports that
session, not whatever happens to be currently active).
Copy button per section. Download button for full document.

### Phase 5 — Session Recovery (on app startup)

get_recovery_state checks whether a previous session was started but never
reached Complete. If so, the frontend shows a recovery banner; clicking
Recover calls recover_session, which re-emits blueprint-section-added for
every already-agreed section of that session — it does NOT restart the
autonomous session loop, only replays the partial blueprint so the user can
see what existed before the interruption.

---

## Memory Model

| Component | RAM |
|-----------|-----|
| OS baseline (Linux Lite) | ~800MB |
| Tauri app + React UI | ~150MB |
| Leader WebView (always alive) | ~350MB |
| One non-leader WebView (active turn) | ~350MB |
| SQLite + Rust state | ~30MB |
| **Total** | **~1.68GB** |

Maximum 2 WebViews active simultaneously. Hard constraint — never exceeded.
Leader window never closed during session.
Non-leader models share one navigating window using saved conversation URLs.

---

## Browser Automation Mechanism

### arena:// Protocol

JavaScript → Rust communication via fake URL navigation intercepted by
Tauri's on_navigation callback. Operates below CSP enforcement — works
on all AI web interfaces without modification.

```
Ready signal:    arena://ready/{agent_id}
Error signal:    arena://ready/error-{agent_id}
Response signal: arena://response/{agent_id}/{turn}/{url-encoded-text}
Done signal:     arena://done/{agent_id}/{turn}
Send detected:   arena://sent/{agent_id}
Log signal:      arena://log/{level}/{url-encoded-message}
                 STATUS UNCONFIRMED — this was spec'd as part of D-040
                 Tier 2 but was not directly re-confirmed present in the
                 most recent read of browser_backend.rs/on_navigation.
                 Verify directly against the real file before assuming
                 either way; don't treat this doc's "implemented" claims
                 elsewhere as covering this specific piece.
```

### Window Architecture

**Leader window:** Created at session start. Never navigated away. Always loaded.
Receives prompts via inject_to_window with wait_ready=false (already loaded).

**Nav window:** One shared window. Navigates to each non-leader model as needed.
Receives prompts via inject_to_window with wait_ready=true.
Returns to existing conversations using saved conversation URLs.

### Injection Architecture

Two injection functions in browser_backend.rs:

**inject_to_agent(&BrowserState, agent_id, is_leader, prompt, turn, nav_rx)**
- Used by session_runner.rs setup phase only

**inject_to_window(WebviewWindow, agent_id, prompt, turn, nav_rx, wait_ready)**
- Used by response_router.rs — takes cloned WebviewWindow handle, no lock required

### GENERIC_INIT_SCRIPT

- pub const &str in browser_backend.rs — static, never modified, never agent-specific
- Generic across ALL agents including GLM and Kimi
- Detects input type at runtime: textarea (value injection) vs contenteditable
  (execCommand injection for Kimi/Lexical) — both paths in same script
- Polls for input field, signals arena://ready/{agent_id} when found
- Implements send detection via polling
- Sets window.__ca_lastResponse for long response capture
- console.error override / window.onerror → arena://log/error/... —
  STATUS UNCONFIRMED, see arena:// Protocol section above
- NEVER captures agent_id by closure value — always reads window.__ca_agentId

### on_navigation Closure Rules (NEVER VIOLATE)

- Captures ONLY tx: std::sync::mpsc::SyncSender<NavEvent>
- Uses std::sync::mpsc — NOT tokio::sync::mpsc
- Uses tx.clone() inside closure
- No blocking_lock() calls anywhere
- Agent identity always from URL path segments

---

## State Architecture

### AppState — 16 fields, all implemented

```rust
pub struct AppState {
    pub orchestrator:     Arc<Mutex<Orchestrator>>,
    // Task 9: std::sync::Mutex, not tokio::sync::Mutex — see BACKEND.md
    pub transcript_store: Arc<std::sync::Mutex<TranscriptStore>>,
    pub token_budget:     Arc<Mutex<TokenBudget>>,
    pub session_vault:    Arc<std::sync::Mutex<SessionVault>>,
    pub browser_state:    Arc<Mutex<BrowserState>>,
    pub context_manager:  Arc<Mutex<ContextManager>>,
    pub blueprint_store:  Arc<std::sync::Mutex<BlueprintStore>>,
    pub settings_store:   Arc<Mutex<SettingsStore>>,  // NOT converted, see BACKEND.md
    pub agent_brain:      Arc<Mutex<Option<AgentBrain>>>,
    pub ask_user_tx:      Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    pub agent_brain_2:    Arc<Mutex<Option<AgentBrain>>>,
    pub session_active:   Arc<AtomicBool>,   // IMP-3 concurrency guard
    pub model_health:     Arc<Mutex<HashMap<String, ModelHealth>>>,  // IMP-5
    pub brain_fail_count: Arc<AtomicU32>,    // IMP-10
    pub memory_store:     Arc<std::sync::Mutex<MemoryStore>>,
    pub last_memory_health: MemoryHealth,
}
```

See BACKEND.md's AppState section for the full field-by-field rationale,
including which three fields' lock TYPE changed (not their storage
backend) as part of Task 9.

### Lock Safety Rules

All async functions that acquire AppState locks must follow this pattern:

```rust
// CORRECT — lock dropped before await
let data = {
    let guard = state.some_field.lock().await;
    guard.clone_needed_data()
}; // lock drops at closing brace
async_function(data).await?; // no lock held here
```

For the four `std::sync::Mutex`-wrapped fields (transcript_store,
blueprint_store, session_vault, memory_store), the equivalent-but-different correct
pattern is to route the whole lock+call sequence through
`db_helpers::run_blocking()`, which executes entirely inside
`tokio::task::spawn_blocking` — see BACKEND.md's db_helpers.rs section.

---

## Frontend Architecture

React + TypeScript + Tailwind CSS + Zustand + Lucide icons. shadcn/ui was
scaffolded early on but is not the actual styling approach in current use
(see FRONTEND.md's Tech Stack section).

Current implemented design reference:
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.
The production React frontend ports that design's shell, Blue/Light/Dark
themes, collapsible sidebar, Empty/Setup/Priming/Active views, settings,
overlays, toasts, live-status drawer, and hello path. The previous visual
implementation has been replaced; existing backend command/event wiring is
preserved. Do not replace this design without explicit approval.

Main window shows blueprint sections only — no model responses.
Live status label at bottom (expandable drawer).
Left sidebar: session history + three-dot menus (rename/export/details/
delete, all backed by real commands), collapsible via a shared Topbar
component.
Settings via account icon popover, including primary, fallback, and
secondary brain sections plus all-seven-model health rows.
AskUser popup: modal overlay, appears when agent-ask-user fires, blocks all
interaction, and invokes `provide_user_answer` for option click, custom
submit, Escape, and backdrop dismissal.
Priming renders the selected `sessionAgentIds`, not a hardcoded model list.
The mockup's Templates input button is intentionally absent.

The hello is a dependency-free SVG/CSS port of preview.html's actual Bézier
path and transforms. It does not load Lottie at runtime and simplifies the
source's variable stroke width and full gradient-stop set.

See FRONTEND.md for complete specification.

---

## Supported Models

| agent_id | Display Name | Base URL | Input Type | Notes |
|----------|-------------|----------|------------|-------|
| chatgpt  | ChatGPT     | https://chatgpt.com | textarea | Original 5 |
| claude   | Claude      | https://claude.ai | contenteditable | |
| gemini   | Gemini      | https://gemini.google.com | textarea | |
| deepseek | DeepSeek    | https://chat.deepseek.com | textarea | |
| qwen     | Qwen        | https://chat.qwen.ai | textarea | |
| glm      | GLM         | https://chat.z.ai/ | textarea (#chat-input) | Implemented (D-036) |
| kimi     | Kimi        | https://www.kimi.com/ | Lexical contenteditable | Implemented (D-042) |

All 7 models are fully implemented — none are pending. An earlier version
of this table said "Pending D-036"/"Pending D-042" for GLM/Kimi
respectively.

---

## Memory System

Phase 1 is implemented as a low-RAM local SQLite system at
`app_data_dir/memory.db`. It carries cross-session decisions, facts, open
questions, model strengths/reliability, and reusable patterns. Six normal
tables plus an external-content FTS5 table provide bounded retrieval without a
separate service. Records retain provenance through `source_agent` and
`source_type`; selection prioritizes pinned, important, and relevant context.
Project Context is hard-pinned. Router reads and writes
handle memory errors locally, so memory degradation does not abort a session.

## Future Extensibility and Roadmap

### Phase 1 — Memory

Implemented. The agent remembers decisions from previous sessions, what worked,
what failed, and what each model is good at. Before this, every session started
blank.

### Phase 2 — Skills

Planned. The agent loads a specialist role depending on what is needed —
Security Auditor, Database Designer, MVP Scope Cutter, and others. Inspired by
gstack-style `SKILL.md` architecture and systems like OpenClaw, the brain
becomes the relevant specialist instead of using one flat prompt.

### Phase 3 — Tools + MCP

Planned. The agent can search the web, read project files, and fetch
documentation. MCP servers can connect it to GitHub, Slack, the filesystem, and
other services.

### Phase 4 — OpenCode Integration

Planned. Once the expert panel agrees on a blueprint section, the agent
delegates implementation to OpenCode CLI. OpenCode writes code, runs tests,
commits, and returns results to the leader, which decides the next step. The app
then both designs and builds.

### Phase 5 — Better System Prompts

Planned. Structured, layered prompts make the brain more reliable and
predictable through examples, trigger words, and XML structure.

### Phase 6 — Self-Improvement

Planned. After every session, the agent records what it learned. Skills improve
over time, and a weekly curator reviews and promotes patterns. The system gets
smarter with use, similar to Hermes-style learning.
