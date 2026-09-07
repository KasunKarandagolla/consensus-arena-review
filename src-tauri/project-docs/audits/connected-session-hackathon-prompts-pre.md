# Pre-Audit: Connected Accounts / Session Lifecycle / Hackathon DnD / Prompt Integrity
Date: 2026-09-07  (one-session controlled repair)
Repo: /home/kasun/Music/arena/consensus-arena
Dirty baseline: 31 files changed (5890 insertions, 1270 deletions) — do not reset

## 1. Connected Accounts Architecture (current real source)

### Browser window creation
`src-tauri/src/browser_backend.rs` + `src-tauri/src/commands.rs:launch_connected_account`
- Max 2 WebViews: `LEADER_WINDOW_LABEL = arena-leader`, `NAV_WINDOW_LABEL = arena-nav` (browser_backend.rs:14-15)
- `launch_connected_account` reuses the shared nav window for every Connected Accounts open, never creates a third. It validates `session_active == false` (commands.rs:1588-1593), busy guard via `connected_account_busy_until`, destroys stale nav window, calls `ensure_nav_window` + `navigate_agent_window`.
- `start_session` creates both windows via `create_windows` (commands.rs:253-261). `session_runner::run_setup` then navigates each agent in `setup_order` through that window.
- Memory model: leader persistent + 1 shared navigating participant (see ARCHITECTURE.md Memory Model ~1.68GB). Two-window invariant enforced by `NewWindowResponse::Deny` except OAuth popup (browser_backend.rs:3311-3353).

### WebView count enforcement
- `make_new_window_handler` denies all non-OAuth new windows to preserve two-WebView limit. OAuth popup for `accounts.google.com` is allowed (temporary) — introduced to fix Claude login via Google.
- `launch_connected_account` explicitly destroys stale nav window before recreating so `on_navigation` closure captures the live sender.
- No code path creates a third persistent window.

### Navigation callbacks
- `on_navigation` via `make_nav_closure` (browser_backend.rs:3497-3520): captures only `tx: SyncSender<NavEvent>`, uses `std::sync::mpsc`, not tokio. Agent identity from URL path segments (`arena://host/...`) via `parse_arena_signal`. Verified: no `blocking_lock()` in callback, no tokio mpsc.
- `handle_page_load` (browser_backend.rs:3192-3309): re-sets `window.__ca_agentId`, records `record_navigation` with cause correlation (`arena_requested` vs `page_initiated`), harness emits `NavigationStarted/NavigationFinished/UrlChanged/DocumentLoaded`, updates `last_navigation_url` when real external URL.
- `BrowserDiagnostics::record_navigation` (browser_backend.rs:939-1043) correlates pending arena navigations within 5s; warns on navigation.

### GENERIC_INIT_SCRIPT
- `pub const GENERIC_INIT_SCRIPT: &str` at browser_backend.rs:5347 is static, generic across all 7 models. Runtime identity via `window.__ca_agentId` (and fallback `window.name`). Verified: contains fixed selectors for both textarea and contenteditable, does not branch per agent beyond generic input detection. Tests at 4072-4140 assert required markers and forbid agent-specific strings.
- Includesconsole diagnostics bridge, lifecycle history handlers, safe DOM forensics, action attribution, `arena://ready` / `arena://sent` / `arena://response` / `arena://done` etc. No closure captures agent id.

### arena:// handling
`handle_arena_url` (browser_backend.rs:3521-3867) parses host-based signals:
- `arena://ready/<id>` / `error-<id>` -> Ready/Error
- `arena://response/<id>/<turn>/<text>` , `arena://done/<id>/<turn>` , `arena://sent/<id>` , `arena://setup-response` , `arena://prompt-injection/...` (10 args), `arena://active-submit` , `arena://send-probe` (6 or 13 args), `arena://challenge/<id>/<indicator>`, `arena://unshowable`, `arena://log`, `arena://console/...` (6 args), `arena://lifecycle`, `arena://dom`, `arena://action`, `arena://ua` .
- Unknown signals -> `UnsupportedNavigation` with redacted URL.
- `send_nav_event` uses `try_send`, logs dropped events, bounded 256.

### Browser state
`BrowserState` (browser_backend.rs:2933-2974): `leader_window`, `nav_window`, `conversation_urls`, `nav_tx`, `diagnostics`, `pending_sends`, `captcha_resolved`, `cooldowns`, `active_turn`, `connected_account_busy_until`. `select_window(is_leader)` picks correct window. All injection via `inject_to_window` (lock-safe; caller drops lock first) and `inject_to_agent` (setup path).

### Cookie/session persistence
- `SessionVault` (session_vault.rs): SQLite + ring AES-256-GCM, `save_conversation_url`, `delete_session_urls`. Cookies table deliberately NOT deleted on `delete_session` (D-046) — keyed by agent_id, survive session deletes.
- `launch_connected_account` uses same `navigate_agent_window` helper, so same cookie jar as session. WebView persistence is per-window; leader window never cleared during session; nav window reuses conversation URLs.
- `context_manager` history, `blueprint_store` and `transcript_store` now file-backed; `run_blocking` for all.

### Model registry
- Backend: `AGENTS: &[AgentConfig]` (browser_backend.rs:2692-2730) — 7 static entries: chatgpt, claude, gemini, deepseek, qwen, glm, kimi. `get_agent_config`, `resolve_participant`, `merged_participants`, `resolve_display_name` form unified registry with persisted `CustomParticipant` (P1-P3).
- Frontend: `src/lib/agents.ts` AGENT_IDS same 7, `AGENT_DISPLAY_NAMES`, `src/stores/useAppStore.ts:198-206` default participants same 7, runtime `loadParticipants()` merges `get_participants` JSON string.
- No duplicate competing URL constants besides historical audits.

### Model URLs (current HEAD, actual file)
- chatgpt: `https://chatgpt.com`
- claude: `https://claude.ai`  (correct)
- gemini: `https://gemini.google.com` (correct)
- deepseek: `https://chat.deepseek.com`
- qwen: `https://chat.qwen.ai`
- glm: `https://chat.z.ai/`
- kimi: `https://kimi.ai/` — canonical, verified `grep -rn www.kimi.com` on `src`/`src-tauri/src` returns 0; only historical audit md files contain legacy www.kimi.com. AGENTS test at 4376 asserts `https://kimi.ai/`. useAppStore kimi `https://kimi.ai/`. No kimi.com regression in runtime.
- Finding: Kimi URL is CURRENTLY correct (not regressed) in dirty worktree — unlike earlier audit states where it was www.kimi.com. The task's §18 repeats regression protection; current state is PASS but needs kept.

### Claude navigation/injection flow
- URL: `https://claude.ai` via `navigate_agent_window` -> `window.navigate`.
- Generic init script handles contenteditable detection (ExecCommand for Claude). No model-specific injection branch in GENERIC_INIT_SCRIPT; per-model selectors are generic.
- Challenge handling: `NavEvent::ChallengeDetected` emits `captcha-detected`, `boss-message`; `wait_for_setup_ready` waits 600s after `captcha_required` for `ResumeRequested` or `Ready`. `send_probe` with `page_health_hint` containing cloudflare/captcha emits `CloudflareDetected/CaptchaDetected`. No bypass — legitimate page controls remain.
- Previous observed Cloudflare 401 / `brunhild.challenges.cloudflare.com Network unreachable` — not caused by our navigation interception per `make_nav_closure` allowing http/https, and `GENERIC_INIT_SCRIPT` not blocking challenge iframe resources. Isolated to external network condition; no code blocks subresources.

### Gemini navigation/injection flow
- URL: `https://gemini.google.com` successfully loads. Observed 401s are `play.google.com/log` telemetry with `SAPISIDHASH` — not auth endpoints. Generic script does not interfere with Google login UI. Navigation interception allows http/https; no redirect loop detected in current `navigate_agent_window` + `handle_page_load`.
- CSP warning `manifest-src` is benign (Chrome warning, not causal).

### Kimi navigation/injection flow
- Uses Lexical contenteditable. Input: `div.chat-input-editor[contenteditable="true"]`, Send: `div.send-button-container`. Injection path `execCommand('insertText')` with fallback `textContent`. Conversation URL `https://kimi.ai/chat/{uuid}` via window.url after priming.
- No stale fallback.

## 2. Session Lifecycle State

### Backend state machine
- `AppState::session_active: Arc<AtomicBool>` (orchestrator.rs:160) — IMP-3 concurrency guard. Set true at `start_session` via `compare_exchange(false,true)` (commands.rs:152-156); cleared on `abort_session` (840-841), on spawned task exit (error or complete) unless `Paused`, on `resume_session` reconstruct. Also `orchestrator.status: OrchestratorStatus::Idle|Preparing|Setup|Requirements|Running|Paused|Complete|Ended` (orchestrator.rs:22-31).
- `start_session` creates `SessionConfig` with `session_id=Uuid::v4()`, stores to `transcript_store`, `context_manager`, `orchestrator.current_session`, emits `session-status: setup` (commands.rs:285-296), spawns loop that runs `run_setup` then `run_debate`.
- `run_setup` loops setup_order, `wait_for_setup_ready` with 100s timeout + 600s challenge resume.
- `response_router::run_agent_loop` drives `Route/Blueprint/Continue/Complete/RouteCompare/AskUser/Hackathon`, checkpointing via `session_vault`/`blueprint_store`/`memory_store` with bounded memory injection.
- Settings guard: `launch_connected_account` checks `session_active` (commands.rs:2588-2593). `delete_session` refuses if session_active true for that id, `restore_memory` refuses if active.

### Frontend state
- `useAppStore.sessionStatus: 'idle'|'setup'|'priming'|'requirements'|'running'|'paused'|'complete'|'ended'` (useAppStore.ts:122). `sessionAgentIds`, `setupBrief`, `setupReadyAgentId`, etc.
- `App.tsx` routes: idle->EmptyView, setup->SetupView, priming/requirements->PrimingView, running/paused/complete/ended->ActiveView. No view for "draft without backend session".
- `Sidebar.newSession()` at `src/components/layout/Sidebar.tsx:29`:
  ```
  function newSession(){ clearSessionState(); setSelectedSessionId(null); setSessionStatus('setup') }
  ```
  This immediately sets `sessionStatus='setup'` without calling backend `start_session`. That is the bug: SettingsPanel's guard at SettingsPanel.tsx:433:
  ```
  const isActiveSession = sessionStatus==='running'||'priming'||'setup'||'requirements'||'paused'
  ```
  So after clicking New Session but before Start Session, the UI thinks a session is active and refuses Connected Accounts. Backend `session_active` is still false, but frontend falsely blocks Settings. Same `isActiveSession` check in Sidebar etc.
- `SetupView.start()` (SetupView.tsx:38-39) is the real `start_session` invoke; only there does backend become active. So distinction "draft vs active" is not modeled on frontend.
- Expected fix: New Session should set a distinct draft state (e.g., 'setup' remains but not counted as active for Settings) OR introduce 'draft' / keep 'idle' until real start. Spec §11-12 say: New Session = setup screen, no active session, Settings accessible; Start Session = real active. Current code violates §12. Affects Settings access, Connected Accounts launch, no false close warning.

### Settings access guards
- Backend: `launch_connected_account` checks `session_active` (commands.rs:2588). Also checked on every readiness wait. Frontend: SettingsPanel `isActiveSession` includes 'setup' — over-broad.
- Correct behavior after fix: `isActiveSession` should exclude 'setup' (draft) and only include priming/running/requirements/paused after real start. Or backend's session_active remains the source of truth and frontend's Connected Accounts launch should surface backend error without pre-blocking UI when still draft.

### Also inspected: current git diff for session changes shows no prior fix for this; Sidebar change is still required.

## 3. Hackathon Team State

### Config persistence
- `src-tauri/src/hackathon.rs:41-49` `HackathonConfig` with groups/models/max_questions/enabled persisted via `settings_store::hackathon_config` key (settings_store.rs:368-387). Validation ensures group names unique, model base_url http(s), api_key non-empty, group membership consistent.
- `AppState.hackathon_run / hackathon_run_id / hackathon_cancel / checkpoint` transient run state.

### Drag/drop implementation (current real code)
- File: `src/components/hackathon/HackathonMiniWindow.tsx` (861 lines)
- State: `dragging:{groupId,modelId,idx}`, `dragOver:{groupId,idx}`, `displayGroups` computed with `runGroup.model_ids_ordered` after invitations.
- Handlers:
  - `handleReorder` (294-318) for arrow buttons: swaps `displayedIds`, persists via `persist` (save_hackathon_config).
  - `handleDragReorder(sourceGroupId, sourceModelId, targetGroupId, targetVisualIdx)` (320-387): handles both intra-team reorder and cross-team move. For cross-team: removes from source `model_ids` filter, inserts into target at mapped insertion index based on `tgtDisplay` visual position, checks `tgtGroup.model_ids.includes(sourceModelId)` to avoid duplicates, updates `models` `group_id` for moved model, calls `persist`. For intra-team: maps visual indices correctly, handles shift after splice.
- Events: per-row `hk-drag-handle` draggable true, `onDragStart` sets dragging, `onDragEnd` clears. Per-row `onDragOver`/`onDrop` + column body `onDragOver`/`onDrop` + trailing drop zone. Empty column path supports drop onto column with 0 participants.
- Verified: supports cross-team repeatedly, both directions, preserves identity, no duplicate due to `includes` guard, updates UI immediately via `setHackathonConfig` after `persist` round-trip (re-fetch `get_hackathon_config`). Leader/team semantics not mutated beyond membership; leader derived as first in `model_ids_ordered` after sort.

### Current pre-audit findings: cross-team DnD
- Existing `handleDragReorder` already implements required 1-16 (§20): visually permits, identifies participant, identifies destination, removes/adds, avoids duplicates, preserves identity/response state (no reset), updates UI, preserves team semantics, works repeatedly both directions, no orphan/loss.
- However deep audit needed: does `persist` preserve `api_key`? It does via `apiKeyMap` spread and `models.map(m=>{...api_key: apiKeyMap[m.id]??''})` plus edited model's explicit `api_key` retained. Group `selected` flag not mutated on move. Good.
- Remaining risk: drag handle is the only draggable element (`draggable` on handle not whole row) — works but not whole-card drag; not a functional failure. Events use `displayedIds` so after invitation responders float top, DnD inserts at visual index and persists that visual order as new `model_ids` (intent replaces config order) — matches spec "Responders float top preserving order" but user reordering after invitations corrects to visual order.
- No backend state corruption observed; `HackathonConfig::validate` allows move.

### Invitation/response state
- `send_hackathon_invitations` (commands.rs:3148-3471): fan-out health-check (15s timeout) to each model in selected groups, `call_hackathon_model` with `build_invitation_prompt()` ("Reply with exactly: OK"), emits `hackathon-invitation-update` per model (`confirmed`/`failed`), after all tasks sorts via `sort_by_responder_status` so responders float top, selects `leader_id = first confirmed`, sets zero-responder groups to `Locked` (`selected` becomes inactive in UI `locked = runGroup.status==='locked'`), emits `hackathon-group-status` and `hackathon-invitations-complete`.
- Participants have `status: Pending|Confirmed|Failed`, `consultation_count`. `GroupRunStatus: Pending|Running|Completed|Failed|Locked`.
- Frontend `HackathonMiniWindow.displayGroups` (74-110) derives `status/countStr/showNote/locked` from `runGroup`; filters `model_ids_ordered` as source of truth after invitations.

### Invitation → responded models default selected (§21)
- Current frontend: `selectedParticipants: Set<string>` state (HackathonMiniWindow.tsx:17) default logic in `useEffect` (58-68):
  ```
  if(!hackathonRun) return
  const confirmedIds = groups.flatMap(...)
  if(confirmedIds.length===0) return
  setSelectedParticipants(prev=>{ if(prev.size===0) return new Set(confirmedIds); ...})
  ```
  This only auto-fills when prev empty. After `send_hackathon_invitations` the run updates, first `hackathonRun` with confirmed will fill. But if user had prior selection, it preserves. Requirement: responded models must become selected by default; only models that actually responded (Confirmed) should be auto-selected. Current logic satisfies happy path but does NOT handle case where `hackathon-invitations-complete` arrives and confirmed id is new but `prev.size !==0` (user had some selection) — it does not add new confirmed. Also per-group selection derived from `selectedParticipants` set; `handleGo` validates that each selected group has at least one selected responder if confirmed exist.
- Need to harden: when `hackathonRun` updates, ensure every newly `Confirmed` id is added to `selectedParticipants` by default unless already present; `Failed` stays not selected. No manual CSS class; state is `selectedParticipants`.
- Also need sorting state-driven per §23: response arrival -> participant response status changes (backend `ParticipantRunStatus::Confirmed`) -> selection derivation/update -> React render. Currently honors that via `hackathon-invitation-update` handler in `useIpcListeners.ts:285-302` (updates local HackathonRun copy per message) + backend sorting. No DOM-only sorting.

### Participant ordering (§23)
- Backend `sort_by_responder_status` (hackathon.rs:410-424) partitions confirmed first preserving order. Frontend `displayGroups` uses `runGroup.model_ids_ordered` when present. Deterministic, state-driven.

## 4. Prompt System

### Prompt loading / persistence
- Canonical files at repo root (3 authorative, per §4-5):
  - `leader_priming.md` (463 lines) header `template_name: 'leader_priming'` + `---` + body with `{{project_brief}} {{session_type}} {{participant_count}} {{participant_list_with_display_names}}` (+ `{{leader_display_name}}`, `{{full_participant_list_including_leader}}`, `{{role}}` used in code — placeholders per header docs).
  - `agent_system.md` (274 lines) header `agent_system` + body with strict field contracts (`continue`/`complete` alone, `hackathon` only task_brief 1-2000, roster is authoritative, etc.)
  - `participant_priming.md` (207 lines) header + body with `{{leader_display_name}} {{participant_count}} {{full_participant_list_including_leader}} {{project_brief}} {{session_type}}`
- SettingsStore (settings_store.rs:9-35) embeds each file via `include_str!("../../leader_priming.md")`, strips header up to `---` via `extract_prompt_body`, exposes `default_leader_priming/participant/agent_system`. Seed on `SettingsStore::new` (88-106) when key missing/empty; migrate old factory defaults when old header lacks hardened markers (116-162, version 2). `get_prompt_template_with_default` (230-241) returns persisted value if non-empty else canonical body. `get_agent_brain_config` also falls back to defaults (207-214). `save_prompt_template` (1571-1589 commands.rs) maps template_name to key. So persistence mechanism already exists per §5 requirements.
- Current `SettingsPanel.tsx` loads via `get_prompt_template` plain string (not JSON parse) at 221 lines 221-231, displays persisted value; saves via `save_prompt_template`.
- Agents brain construction: `commands::save_agent_brain_config` constructs `AgentBrain::new` with `system_prompt` from persisted `brain_system_prompt`, persists to DB, attaches fallback, updates `AppState.agent_brain`. Runtime `response_router::run_agent_loop` uses `brain.build_effective_system_prompt(memory_context)` + context string + decides via primary/fallback/secondary. So path `canonical file -> persistence (settings.db) -> retrieval -> AgentBrain -> API request` exists, with fallback. Verified: no duplicate competing hardcoded prompt definition besides legacy `context_manager.rs:52-86` `build_prompt_for_agent` which is dead-simple old-style prompt (role + history) NOT used by response_router's agent brain path — but context_manager is still used for participant injection via `session_runner` priming (which uses the hardened templates). The old helper is not invoked by orchestrator loop; not a secret replacement but dead code risk.
- Requirement honored: existing user-customized prompt preserved — `seed` and `migrate` only seed when empty / old factory detected, not when custom with marker present. So §5 largely implemented.

### Prompt interpolation / template construction / leader & participant priming
- `session_runner::run_setup` (493-679) does live interpolation:
  ```
  leader_display = display_name_for(leader_agent_id)
  participant_count_total = agent_ids.len()
  other_count = agent_ids.len()-1
  other_display_names = filter leader
  full_display_names = all ids display names
  session_type_str = Architecture/MVP/API Design/Security Review/Custom
  ```
  For leader (649-657):
  ```
  t.replace("{{participant_count}}", other_count)
  t.replace("{{participant_list_with_display_names}}", other_list)
  t.replace("{{leader_display_name}}", leader_display)
  t.replace("{{full_participant_list_including_leader}}", full_list)
  t.replace("{{project_brief}}", project_brief)
  t.replace("{{session_type}}", session_type_str)
  t.replace("{{role}}", role)
  ```
  For participant (658-666) similar but `participant_count = total`, plus `participant_list_with_display_names = other_list`.
- This is dynamic per §6: reflects actual `sessionAgentIds`/`leader_agent_id`/`project_brief`/`session_type`/`role`. No hardcoded seven-model list. Template retains complete finalized prompt ideas; only placeholders replaced. Confirmed: the finalized `leader_priming.md` contains narrative sections (Who is in this session, Runtime state authoritative, What tools, etc.) and placeholders above; code replaces exactly those. No summarization.
- Need to verify that `{{leader_model}}` style from task example is not used; actual placeholders are `{{participant_count}}`, `{{participant_list_with_display_names}}`, `{{project_brief}}`, `{{session_type}}`, `{{leader_display_name}}`, `{{full_participant_list_including_leader}}` — matches files. No additional faithful substitution needed.

### Where selected participant IDs originate
- Frontend `SetupView` (SetupView.tsx:17 `selected: Set<string>` default chatgpt,claude,deepseek) -> `start()` collects `ids = participants.map(...).filter(selected)` -> invokes `start_session { project_brief, session_type, agent_ids: ids, leader_agent_id: leader }` (39 lines). Also `Sidebar.newSession` clears `sessionAgentIds`.
- Backend `start_session` param `agent_ids: Vec<String>` validated via `validate_session_agents` against merged registry (103-128 commands.rs), persisted as `SessionConfig.agent_ids`. Also stored to `BrowserDiagnostics.metadata.selected_agent_ids`, `SessionConfig.setup_order`. All downstream (browser windows, priming, diagnostics, hackathon) derive from this vector; no hardcoded 7 used.
- `useAppStore.sessionAgentIds` set after successful `start_session` to setupOrder. Also `useIpcListeners` listens `session-status` to set `sessionAgentIds` from `setup_order`/`selected_agent_ids`. So participant source is user config, not registry.

### Where leader ID originates
- Frontend dropdown bound to `leader` state, filtered to selected participants, default `'claude'`, corrected by `useEffect` when leader not in selected. Backend `leader_agent_id: String` validated to be within `agent_ids`. `SessionConfig.leader_agent_id` authoritative. Priming `leader_display` derived from it.
- `BrowserState.leader_agent_id` updated on creation.

### Every place participant model names could be hardcoded (search results)
- Backend registry `AGENTS` (browser_backend.rs:2692) — allowed, contains all supported models.
- Tests `builtin_registry_has_exactly_seven_participants_unchanged` (browser_backend.rs:4356) asserts exactly those 7 — allowed.
- Frontend `AGENT_IDS`, `AGENT_DISPLAY_NAMES` (lib/agents.ts) — allowed registry.
- `useAppStore` default participants 7 — allowed offline fallback.
- `browser_harness.rs` harness operation id helpers etc — allow listing.
- No runtime priming code contains hardcoded participant list; verified `session_runner.rs` uses `config.agent_ids` only. `agent_system.md` example `claude|chatgpt|gemini|deepseek|qwen|glm|kimi` is an example, not authoritative, and `agent_brain.rs DECISION_JSON_CONTRACT` lists same as example context — not used for roster validation; roster comes from runtime context. Audit confirmed no prompt builder hardcodes seven as participant context.

### Every place system/priming prompts are generated or overwritten
- Generation: `settings_store.rs` include_str + extract + seed/migrate + get_with_default. `session_runner::run_setup` interpolates template at injection time. `response_router` uses `brain_system_prompt` + `DECISION_JSON_CONTRACT` + memory_context concatenated before every `decide`. No other overwrite. `get_agent_brain_config` returns `system_prompt` + `leader_priming_prompt` + `participant_priming_prompt` together. `save_agent_brain_config` overwrites 4 keys atomically.
- Potential risk: `context_manager.rs:52-86 build_prompt_for_agent` still exists as an old simple prompt builder but not invoked by `response_router` (which builds context string manually). Confirm no call site remaining besides potential legacy use.

## 5. Three External Prompt Files — Current State

### leader_priming.md (/home/kasun/Music/arena/consensus-arena/leader_priming.md)
- 463 lines, NOT truncated. Contains: leadership framing, Project brief placeholders, Who is in this session ({{participant_count}} + {{participant_list_with_display_names}}), roster fixed entire session, Runtime state authoritative (do not invent), What tools (Route/RouteCompare/AskUser/Hackathon with strict triggers + task_brief structure PROBLEM STATEMENT/CONSTRAINTS/REQUIRED REPORT STRUCTURE), Phase 1 clearing ambiguity (5 steps), Phase 2 module-by-module (6 steps), Module completion (6 conditions, clean pass valid), Global completion (5 verifications), Independent judgment (hypothesis->exposure->synthesis + consultation self-check), Handling disagreement (bounded pushback, leader not exempt), Quality bar, When to skip, Ask User bar, Communication style, Time and rigor (no time limit). All sections present. Placeholders preserve full logic; no summary.

### agent_system.md
- 274 lines. Non-participant orchestration brain. Authoritative context list, Output contract JSON shape + strict per-action field contract (route target_model+prompt, route_compare models+prompt, blueprint section_title+content, ask_user question+options 2-4+allow_custom true, continue {}, complete {}, hackathon task_brief 1-2000 with 3 sections). Roster authoritative (example list not authority). Classification rules 1-12 (module cycles are route not route_compare, continue carries no prompt, complete global, relitigation flag, blueprint needs no open dissent, silence not resolution, ask_user gate, hackathon brief complete, do not hackathon during open Phase 1, advisory, soft cycle cap 6, never fabricate). Malformed fallback. No loss.

### participant_priming.md
- 207 lines. Reviewer role with {{leader_display_name}} {{participant_count}} {{full_participant_list_including_leader}} {{project_brief}} {{session_type}}. No fixed role, repeat visits about demonstrated relevance, Hackathon review guidance, Runtime context authoritative lists, What you will be shown (design + dispute log), Expected review (Accept/Dispute with replacement/Counter-question + Don't manufacture), Research honesty (only browse if tool explicitly provided), Hackathon result evaluation (5 constraint checks), Do not accept, Leader not exempt, Bounded pushback, Ambiguity test, Product-vision, Style concise.

All three are authoritative, complete, contain repetitive safeguards intentionally. They are embedded via include_str! and seeded — not summarized.

## 6. Hackathon specifics (pre-audit deep)

### Cross-team drag/drop requirement gaps
Current implementation passes most checks but two hardening gaps:
- §21: responded models default selected — current effect only covers first run where prev size 0; subsequent invitation waves with new confirmed ids not added. Needs to merge all confirmed into selectedParticipants automatically (while keeping failed not selected).
- §20 cross-team drag/drop preservation of response state: moving a participant between teams currently preserves their status in `hackathon_run`? Actually `handleDragReorder` persists `HackathonConfig` (selected state of models) but does NOT mutate `hackathonRun` transient state; after moving a confirmed participant to another team, the run's `GroupRunState.participants` still lists them in old group until next `get_hackathon_run_state` refresh? The handler only mutates config, not run. Since `run` is not persisted until next invitation, moving after invitations could drift run vs config membership. Requirement says underlying app state must change — current `persist` changes persisted config, not run transient; but run is derived from config at invitation time. Cross-team move after invitations should either update run immediately or be allowed only before invitations? Need to ensure run's participant membership updates on move if a run exists — or sort safety: forbid move during active run. Report as design decision.

### Ordering & selection
- Already state-driven via backend sorting. Frontend re-renders from that. No DOM-only movement.

## 7. Kimi Regression Check (pre state)
- Runtime `AGENTS kimi https://kimi.ai/` — canonical. Frontend unified `https://kimi.ai/`. No `www.kimi.com` or `kimi.com` in src/src-tauri/src runtime. Docs `src-tauri/project-docs/BACKEND.md:602` still lists `https://kimi.ai/` (correct) + ARCHITECTURE 377 also  correct. Only stale mentions in audit history remain and are justified.

## 8. Claude / Gemini Diagnostic (pre)
- Claude URL correct, challenge handling implemented, Cloudflare `brunhild.challenges.cloudflare.com Network unreachable` is external environment, not our blocking — our WebView does not block iframe / challenge resources (CSP below enforcement). We allow `http|https|about|blob|data`, no custom headers that break Cloudflare, no User-Agent spoof beyond `CHROME_USER_AGENT` (126 Chrome Windows) which is legitimate. Our `GENERIC_INIT_SCRIPT` is static and does not touch `challenges.cloudflare.com`. Conclusion: if fix needed, it is not to bypass Cloudflare but to verify not blocking subresources; no intervention beyond confirming.
- Gemini page loads; 401s are telemetry `play.google.com/log` + `SAPISIDHASH`; not blocking authentication. Cookies/storage preserved via same WebView jar; login UI usable. `manifest-src` CSP warning benign.

## 9. Browser Architecture Safety (pre)
- No `blocking_lock()` in async/on_navigation (verified grep 0). Uses `std::sync::mpsc::sync_channel(256)` (browser_backend.rs:13) not tokio. `make_nav_closure` captures only tx, agent id via URL segments. `GENERIC_INIT_SCRIPT` static generic via `window.__ca_agentId`. Event names verified against IPC.md (14+ events). IPC args snake_case rename_all on commands. No new unwrap/expect in inspected commands.

## 10. Immediate Repairs Needed (grouped)
A. Connected Accounts: Kimi URL already correct — keep; diagnose Claude/Gemini confirm no regression fix needed beyond documentation.
B. Session lifecycle: Fix frontend Settings guard to not treat New Session draft as active session (Sidebar.newSession + SettingsPanel.isActiveSession + any other sessionActive checks).
C. Hackathon: Harden responded-models default selected (merge confirmed), ensure drag/drop perf fix already present is not broken, verify no duplicate regression tests missing.
D. Prompt system: Embedding + persistence already correct — integrate hardening already present but add regression invariant tests for hardcoded list prevention and Kimi URL invariant if missing.
E. Regression protection: Greps for kimi.com, hardcoded lists, short fallback prompts, session checks already mostly clean — add post-audit verification.
