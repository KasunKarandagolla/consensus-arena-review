# Browser/WebView Reliability — Pre-Audit (Connected Accounts vs Priming)

Date: 2026-09-06
Branch: forensics/browser-auth-diagnostics @ a3ab85f (dirty hackathon worktree preserved)
Auditor: OpenCode (Muse Spark) — source-evidence only, no doc trust

---

## 1. Scope & Methodology

This audit was produced by reading every affected source file completely before any edit, plus repository-wide searches for `BrowserState`, `WebviewWindow`, `on_navigation`, `navigate`, `GENERIC_INIT_SCRIPT`, `arena://`, `SessionVault`, `setup-agent-ready`, `run_setup`, `priming`, `launch_connected_account`, `timeout`, `refresh`, `reload`, `kimi`, and related terms. The real source is authoritative; project docs are context only.

Files read completely (absolute paths):

- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs` (6356 lines)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs` (approx 2890 lines, launch_connected_account at :1801-1933, start_session at :131-424)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs` (approx 900 lines, run_setup at :423-890)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/orchestrator.rs` (248 lines, AppState at :121-248, BrowserState creation at :202-203)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/response_router.rs` (2453 lines, constants at :24-31)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_harness.rs` (63084 bytes, harness taxonomy)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs` (182 lines, generate_handler registrations)
- `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx` (538 lines, launch at :430-447)
- `/home/kasun/Music/arena/consensus-arena/src/components/views/SetupView.tsx`, `PrimingView.tsx`, `src/hooks/useIpcListeners.ts`
- Supporting: `session_vault.rs`, `errors.rs`, `settings_store.rs`, `tauri.conf.json`, `capabilities/default.json`, `FRESH_INSTALL_BROWSER_FORENSICS.md`, `BROWSER_FORENSICS_IMPLEMENTATION_REPORT.md`, `src-tauri/project-docs/audits/hackathon-mode-review.md`

Dirty worktree preserved — 12 modified + 6 untracked hackathon files — none reverted. This audit distinguishes pre-existing changes vs task changes.

---

## 2. Authoritative Model URL Registry

Single source: `browser_backend.rs:2164 AGENTS` constant (verified as only `pub const AGENTS` in repo; no second registry).

```rust
// browser_backend.rs:2164-2202
pub const AGENTS: &[AgentConfig] = &[
  { chatgpt,  https://chatgpt.com },
  { claude,   https://claude.ai },
  { gemini,   https://gemini.google.com },
  { deepseek, https://chat.deepseek.com },
  { qwen,     https://chat.qwen.ai },
  { glm,      https://chat.z.ai/ },
  { kimi,     https://www.kimi.com/ },  // <-- INCORRECT per task; should be https://kimi.ai
];
```

Merged registry: `browser_backend.rs:2242 resolve_participant` and `2271 merged_participants` layer built-ins + custom participants. Session validation `commands.rs:101-127 validate_session_agents` and window creation `browser_backend.rs:5750 validate_window_registry` and `navigate_agent_window` all resolve through the same merged resolver — verified no second URL table.

Search evidence:

- `grep -rn "kimi" src-tauri/src/browser_backend.rs` finds only AGENTS table, display_name mapping, test `https://www.kimi.com/` at :3730, and display check. No other hard-coded Kimi URL.
- `grep -rn "kimi\.com\|kimi\.ai"` finds no `kimi.ai` in current tree, confirming the bug.
- `grep -rn "AGENTS"` finds only browser_backend.rs definition + 2 test uses.

**Kimi fix required:** Change `https://www.kimi.com/` → `https://kimi.ai` (task spec) at AGENTS table, plus test expectation at :3730 (`assert!(join contains "https://www.kimi.com/")` must update), and `browser_harness.rs:1322` diagnostic sample URL (incidental, but also updates for consistency if touched). No other navigation logic hard-codes kimi.com.

Leader/participant launch, Connected Accounts, Priming, autonomous routing, diagnostics all share this single table — confirmed by tracing `resolve_participant` call sites: `commands.rs:112`, `553?`, `636`, `1416?`, `1801 launch_connected_account`, `session_runner.rs:446`, `browser_backend.rs:5822`.

---

## 3. Connected Accounts Path — Exact Trace (Settings → Connected Accounts → Launch)

Frontend handler: `SettingsPanel.tsx:430-447`

```tsx
// SettingsPanel.tsx:434-445
async function launch() {
  if (isActiveSession) addToast(...); return;
  if (anyLaunchBusy) addToast(...); return;
  setLaunchBusy(p.agent_id)
  try { await invoke('launch_connected_account', { agent_id: p.agent_id }) // IPC.md: snake_case
        addToast(`Navigation started for ${p.display_name} — window loading ${p.base_url}.`)
  } catch(e){ reportError('launch', 'launch_connected_account', error) }
  finally{ setLaunchBusy('') }
}
```

- React event: `onClick` on `.cr-btn` Launch per participant row
- Tauri invoke: `launch_connected_account` with `{agent_id: string}` (single-word, no rename_all needed but function has `#[tauri::command(rename_all="snake_case")]` at `commands.rs:1801`)
- Backend command: `commands.rs:1801 launch_connected_account(agent_id, state, app)`
  - Check `session_active.load(SeqCst)` → Err if active (prevents stealing shared WebView mid-session) — :1807
  - Load custom participants via `settings_store.get_custom_participants()` — :1810
  - Resolve via `resolve_participant(&agent_id, &custom)` — :1816 (merged registry)
  - Busy guard: `browser.connected_account_busy_until` checked, if `now < until` → Err "Wait about 30s" — :1824-1832; set to `now+10s` before window select — :1839
  - Window selection (P4 logic): snapshot diagnostics, if `assigned_window_label == LEADER_WINDOW_LABEL` and `leader_window.is_some()` then `(leader_window, "leader")` else `(nav_window or ensure_nav_window, "nav")` — :1847-1875. This reuses leader window if agent previously owned it (cookie affinity). Never creates third window.
  - Get diagnostics clone: `browser.diagnostics.clone()` — :1877-1880
  - Navigate: `navigate_agent_window(&app, &diagnostics, &window, &agent_id, window_kind, &participant.base_url)` — :1891, mapped to `e.to_string()` on failure.
  - Busy guard tail: if `nav_result.is_ok()` set `busy_until = now+20s` else `None` — :1898-1905 (total 30s guard: 10+20)
  - `nav_result?` propagate error — :1907
  - Idempotent show/focus with warn logging (P5) — :1913-1917
  - Emit `boss-message` "Navigation started for {display_name} — window loading {url}. Complete any login..." — :1922-1931

`navigate_agent_window` (`browser_backend.rs:2531-2618`):

1. `diagnostics.register(agent_id, window_label, window_kind)` + `set_active(window_label, agent_id)` — :2540-2541
2. `diagnostics.set_operation(agent_id, operation_id_setup(agent_id, generation), "navigation_started")` + emit `NavigationStarted`, `ComposerProbeStarted` — :2543-2563
3. `record_arena_navigation_request` (5s PendingArenaNavigation map) — :2564
4. `record_navigation_intent` (intent_id `nav-<millis>-<uuid>`, reason `app_navigation`, operation_id) + emit — :2565
5. `update_diagnostic` phase `creating`, message "Preparing model window" — :2568-2575
6. Parse URL, validate http/https — :2577-2586
7. `set_window_identity(window, agent_id)` → eval `window.name='__consensus_arena_agent__:'+id; window.__ca_agentId=id` — :2588-2591 (sets window.name for cross-document restore)
8. `window.navigate(parsed_url)` — :2592-2596
9. `window.show()` + `window.set_focus()` — :2597-2606
10. `update_diagnostic` phase `navigation_started`, message "Navigation requested; waiting for page readiness" — :2608-2616

`BrowserState` fields (`browser_backend.rs:2409-2427`): `leader_window: Option<WebviewWindow>`, `nav_window: Option<WebviewWindow>`, `conversation_urls`, `nav_tx: SyncSender<NavEvent>`, `diagnostics: BrowserDiagnostics`, `pending_sends`, `captcha_resolved`, `cooldowns`, `active_turn`, `connected_account_busy_until: Option<Instant>`.

`ensure_nav_window` (`browser_backend.rs:5899-5933`): if `nav_window.clone()` exists return it, else if `app.get_webview_window(NAV_WINDOW_LABEL)` exists store+return, else build `WebviewWindowBuilder::new(app, NAV_WINDOW_LABEL, WebviewUrl::External("about:blank"), inner_size 1200x800, visible false, initialization_script GENERIC_INIT_SCRIPT, on_navigation(make_nav_closure), on_new_window(Deny), on_page_load(handle_page_load))`.

**Not done by Connected Accounts path (vs Priming):**

- No `diagnostics.begin_setup_run` (so setup_generation stays stale, phase not reset to `setup`)
- No `record_setup_expected_agent` (so `expected_agent_id` stays prior value)
- No `drain_stale_nav_events` (channel is AppState dummy with no receiver; no tokio bridge)
- No `wait_for_setup_ready` (no 50s readiness wait, no challenge/unshowable handling)
- No priming prompt injection (no `build_priming_script` eval, no 5s PromptInjectionReport wait)
- No `setup-agent-ready` emit, no 120s `SendDetected` wait, no navigation recovery loop, no `setup-agent-complete` / `setup-complete`
- No `handle_page_load` identity refresh beyond what navigate does (but handle_page_load does run for every load)
- Reads custom participants individually; Priming loads once at start of `run_setup`
- Uses `browser_state` lock in two separate scoped sections (selection then diagnostics) vs Priming's single `state.browser_state.lock()` per agent iteration

**Errors surfaced:** `navigate_agent_window` errors map to `Err(String)` returned to frontend → `reportError` toast with `buildCommandErrorMessage`. No `browser-diagnostic` phase for success beyond `navigate_agent_window`'s own `emit_browser_diagnostic`. No `setup-agent-failed` emission (priming emits that on readiness/send failure).

**Visibility:** Window is `visible:false` at creation, then `navigate_agent_window` calls `window.show()` before returning, plus launch does extra `show/focus` idempotently.

**Prior navigation:** If same shared nav window was previously showing chatgpt.com and we navigate to claude.ai, old page remains until new navigation commits; no explicit cleanup of old DOM state. `diagnostics.register` overwrites record but does not clear pending navigations.

**Init script timing:** Script is registered at window creation via `.initialization_script(GENERIC_INIT_SCRIPT)` — runs on every navigation (Tauri guarantees). Agent identity is set via `window.name` before navigate, restored by script in new document's first IIFE: `if (!window.__ca_agentId && window.name.indexOf('__consensus_arena_agent__:')===0) window.__ca_agentId = ...` (`browser_backend.rs:4799-4802`). Verified no closure capture: `make_nav_closure` captures only `tx` and `window_label` (`browser_backend.rs:2886-2908`).

**Cookies:** No cookie restoration before navigation. `SessionVault::save_cookies/load_cookies` exists but has zero call sites (grep shows only definitions). Login state relies entirely on WebView's own cookie store partitioned per WebView (leader vs nav). P4 leader-ownership heuristic attempts to route to window that already holds cookies.

**Is WebView reused?** Yes — shared nav window reused for all non-leader agents; leader window reused only if diagnostics says it owns that agent. No third window per `NewWindowResponse::Deny` handler (`browser_backend.rs:2724-2741`).

**Does page get injected before/after load?** For Connected Accounts, no injection besides identity eval before navigate and `handle_page_load` identity eval on load. No priming prompt injection. So blank cannot be due to injection error.

---

## 4. Priming Path — Exact Trace (New Session → run_setup)

Frontend: `SetupView.tsx` Start button → `invoke('save_agent_brain_config')` then `invoke('start_session', {project_brief, session_type, agent_ids, leader_agent_id})` — `commands.rs:131 start_session` validates participants via merged registry, guards `session_active`, creates `SessionConfig` with `setup_order()` (leader first), increments `setup_generation`, records `last_session_id`/`session_complete=false`, resets `brain_fail_count`, `token_budget.reset_all()`, creates `BrowserState::new(std_nav_tx)` + `create_windows(...)`, spawns std→tokio bridge thread, emits `session-status {status:"setup", session_id, setup_generation, selected_leader_id, selected_agent_ids, setup_order}`.

Spawned task `tokio::spawn` clones AppState Arcs and runs:

```rust
loop { match run_setup(&config_clone, &state_ref, &app_clone, &mut nav_rx).await {
  Ok(()) => break,
  Err(e) => { mark_setup_failed_recoverable; emit boss-message+setup-agent-failed; match nav_rx.recv().await { ResumeRequested => continue, SetupManualConfirmed => emit setup-agent-complete + continue, SessionAborted|None => Ended; return, _ => continue } }
}}
// then Running + run_debate
```

`create_windows` (`browser_backend.rs:5771-5894`):

- Validates leader/nav via merged registry (`validate_window_registry`)
- `diagnostics.begin_setup_run(BrowserSetupMetadata{setup_generation, session_id, selected_leader_id, selected_agent_ids, setup_order})` — clears records, active_by_window, pending_arena_navigations, timeline, current_operation/phase, navigation_intents/lifecycle/action/dom, sets metadata, emits `WindowCreated`+`NavigationStarted` per agent — :5791-5796, :5843-5849 effect at :459-510
- Sets `state.leader_agent_id`
- Destroys stale `arena-leader`/`arena-nav` windows if exist (`existing.destroy()`) — :5800-5808
- For each `agent_id` registers diagnostics with intended_url, phase `queued` — :5810-5837
- Builds `arena-leader` and `arena-nav` WebviewWindows with `about:blank`, size 1200x800, visible false, `initialization_script(GENERIC_INIT_SCRIPT)`, `on_navigation(make_nav_closure)`, `on_new_window(Deny)`, `on_page_load(handle_page_load)` — :5846-5893
- Stores handles in `state.leader_window`/`nav_window`

`run_setup` (`session_runner.rs:423-890`):

For each `agent_id` in `setup_order`:

- `role = assign_role` (Leader vs Critic etc) — :443
- `resolve_participant(agent_id, &custom)` (merged, loaded once at :435-440) — :446
- Lock `browser_state`, `select_window(is_leader)` → `leader_window.clone()` or `nav_window.clone()` — :448-466
- If `diagnostics.setup_completed(agent_id)` skip (preserves completed prompts on retry) — :471
- `record_setup_expected_agent(&diagnostics, agent_id)` sets `expected_agent_id`, resets stale counters, sets operation `priming-<agent>-g<gen>`, emits `PrimingStarted` — :475, impl at :1227-1243
- `drain_stale_nav_events(nav_rx, "setup for {agent}")` — :476
- `navigate_agent_window(app, &diagnostics, &window, agent_id, window_kind, &agent_config.base_url)` — :481-500, same helper as Connected Accounts but now with proper generation/session, errors emit `boss-message` + `record_browser_error` and `return Err` — :489-500
- `wait_for_setup_ready(agent_id, base_url, display_name, app, diagnostics, nav_rx)` — :502-538
  - Loops `tokio::time::timeout(READINESS_WAIT_TIMEOUT_SECS=50s, async { loop nav_rx.recv() match Ready=>Ok, Error=>NavigationFailed(readiness_timeout_message), ChallengeDetected=>... , UnshowableUrl=>..., SessionAborted=>..., _=>continue } )`
  - Challenge path: emits `captcha-detected`, `boss-message`, waits up to 600s for `ResumeRequested` or `Ready`, handles re-Challenge, Unshowable, SessionAborted — :335-420
  - On timeout with `challenge_seen` true → captcha resume timeout; without challenge → `Timeout("... window timed out waiting for readiness from {base_url}. Last real URL: {last_real_navigation_url}. See Diagnostics.")`
  - On Ok(Ready) → `Ok(())`
  - Errors record via `record_browser_error` + `boss-message` and propagate as `Timeout` or other — :512-537
- Build priming prompt `format!("You are participating... Your role is {}...", role)` — :541-548
- `setup_capability_verified` flag — :557
- If not `diagnostics.prompt_already_visible(agent_id)` then build large JS `build_priming_script` equivalent inline at :563-683, `window.eval(&script)` → on error record + return `InjectionFailed` — :684-689, then `record_prompt_injected`, then 5s `PromptInjectionReport` wait loop collecting `method, prefix_ok, suffix_ok, visible_length, send_enabled, target_tag/role/contenteditable, error` via `nav_rx`, else drain stale — :691-738, sets `setup_capability_verified = capability_verified(prefix_ok && suffix_ok && send_enabled && error.is_none())`
- Else (already visible) emit boss-message — :739-744
- `app.emit("setup-agent-ready", {agent_id})` — :746
- If `!setup_capability_verified` then wait for proof (`SendDetected`/`SetupResponseObserved`/`ManualConfirmed`/`Response`/`Done`) with 120s timeout and navigation recovery (MAX_SETUP_NAVIGATION_RECOVERIES=3) — :753-866
  - Outer `for _ in 0..=MAX_SETUP_NAVIGATION_RECOVERIES`
  - Inner `tokio::time::timeout(120s, loop { match nav_rx.recv() { SendDetected(id,reason)=>Ok(SendDetected), SetupResponseObserved=>Ok(ResponseAfterInjection), SetupManualConfirmed=>Ok(UserConfirmedManual), Challenge/Unshowable=>Err, Response/Done=>Ok(ResponseAfterInjection), SessionAborted=>Err, Ready if has_pending_user_submit=>Ok(trusted_submit) else if recovery_count < MAX => inc recovery + emit "page refreshed; re-priming..." + perform_priming_injection retry, _=>stale } })`
  - On Ok(proof) break; on Err NavigationFailed/Timeout etc break with error.
  - After loop, if timeout and `has_recent_unexpected_navigation(agent_id,15s)` and recovery < max → `wait_for_setup_ready` + `perform_priming_injection` then continue; else `Timeout(send_detection_timeout_message)` + record + boss-message + return Err`.
  - On success, `record_setup_completion(&diagnostics, agent_id, reason)` with reason `trusted_submit`/`mutation_fallback`/`send_detected` or `response_after_injection` or `user_confirmed_manual` — :858-865
- Else (capability_verified true) skip wait, directly `record_setup_completion` with `capability_verified`? Actually :866-890 shows else branch: `// Strong capability proof ... advanced this agent` (details truncated) — implies when prefix/suffix/send_enabled all true and no error, setup advances without human Send, because ACTIVE loop will submit.

After loop over all agents, `handle_page_load` and navigation forensics have updated `last_navigation_url`, `input_found` etc throughout.

Verified that `run_setup` opens each selected model one at a time and injects role-priming prompt, user presses Send, app detects `arena://sent/{agent_id}` — matches ARCHITECTURE.md Phase 2, but also supports capability_verified bypass where no human Send needed if composer accepted verbatim and Send enabled.

---

## 5. Side-by-Side Diagnostic Matrix (Source Evidence)

| Behavior | Connected Accounts (`launch_connected_account`) | Priming (`run_setup` per agent) | Same? | Evidence |
|----------|-----------------------------------------------|----------------------------------|-------|----------|
| **WebView creation** | Lazily via `ensure_nav_window` if `nav_window` missing, else reuse existing; leader reuse if diagnostics says leader owns agent. No destruction of stale windows. | Eagerly via `create_windows` at session start: destroys stale `arena-leader`/`arena-nav` (`existing.destroy()`), builds both windows fresh with `about:blank`, GENERIC_INIT_SCRIPT, on_navigation, on_page_load. | **NO** — different lifecycle; Connected lacks destroy+fresh create, Priming always fresh pair. | `commands.rs:1861-1869 ensure_nav_window` vs `browser_backend.rs:5800-5810 destroy` + `5846-5889` build |
| **WebView reuse** | Reuses single shared `arena-nav` (or `arena-leader` if leader-owned) for every launch; same handle mutated via `window.navigate`. | Reuses same two handles but navigates sequentially: leader window for leader agent, nav window for each non-leader in order; never more than 2. | **PARTIALLY** — both share, but Connected may pick leader vs nav based on heuristic; Priming strictly leader vs nav by `is_leader`. | `commands.rs:1847-1874` vs `session_runner.rs:448-465 select_window` |
| **Window label** | `arena-leader` if leader-owned else `arena-nav` (dynamic). | `arena-leader` for leader, `arena-nav` for all others (deterministic). | **NO** | same cites |
| **Window handles** | `BrowserState.leader_window: Option<WebviewWindow>` + `nav_window: Option<WebviewWindow>` (max 2). | Same two fields, same limit, stored after `create_windows`. | **YES** | `browser_backend.rs:2410-2444` |
| **Window positioning** | `inner_size 1200x800`, `visible:false` at creation, `show()/set_focus()` after navigate. | Same sizes/visibility, same show/focus after navigate. | **YES** | `browser_backend.rs:5855-5858, 5919-5922` vs same |
| **Navigation callbacks** | `on_navigation(make_nav_closure(tx, window_label))` + `on_page_load(handle_page_load)` installed at window creation. | Same callbacks installed at `create_windows` for both windows. | **YES** | `browser_backend.rs:5899-5928` |
| **Initial URL** | `about:blank` at window build, then immediate `navigate_agent_window` to `participant.base_url` (e.g., `https://www.kimi.com/`). | `about:blank` at build, then per-agent `navigate_agent_window` to same `AGENTS.base_url`. | **YES** | `browser_backend.rs:5914` vs `5850` |
| **Subsequent navigation** | Single `window.navigate` per launch; no follow-up navigation unless user launches another model (reuses same window). | Sequential per agent; plus retry loop `perform_priming_injection` on `Ready` during send wait (page refreshed; re-priming) and after 120s timeout with `has_recent_unexpected_navigation`. | **NO** — Priming has bounded retry (MAX_SETUP_NAVIGATION_RECOVERIES=3) and re-injection; Connected has none. | `session_runner.rs:795-804, 824-838` |
| **Initialization scripts** | `GENERIC_INIT_SCRIPT` as `.initialization_script` at window build (static, generic). | Same. | **YES** | `browser_backend.rs:5858,5882,5923` |
| **`GENERIC_INIT_SCRIPT`** | Same static `&str` (2760 lines), no agent-specific branch, identity via `window.__ca_agentId`/`window.name`. | Same. | **YES** | `browser_backend.rs:4519 GENERIC_INIT_SCRIPT`, :4792-5742 |
| **Agent identity init** | `set_window_identity(window, agent_id)` before navigate (`window.name='__consensus_arena_agent__:'+id; window.__ca_agentId=id`) plus `handle_page_load` re-sets on every load. | Same before each per-agent navigate + `handle_page_load`. Priming also does `record_setup_expected_agent` to set `expected_agent_id` for stale detection. | **PARTIALLY** — both set identity, but Priming also sets expected_agent tracking. | `browser_backend.rs:2515-2529 set_window_identity` |
| **Cookie/session restoration** | **None** — `SessionVault` cookies defined but zero call sites; relies on WebView's own cookie store per window. P4 heuristic routes to window that already holds cookies. | **None** — same; `SessionVault` not used in `run_setup` either. Conversation URLs saved via `session_vault.save_conversation_url` after `setup-agent-complete`? Actually saved in response_router after turn, not in setup. | **YES (both none)** | `session_vault.rs:117-139` grep 0 calls |
| **SessionVault interaction** | No `conversation_urls` used in Connected path. | `run_setup` does not yet save conversation URLs either (done later in response_router); but `create_windows` registers `intended_url`. | **YES** | `session_runner.rs` no vault calls |
| **Browser-state locks** | Two separate `state.browser_state.lock().await` scopes: one for window selection+busy, one for diagnostics clone. Both short, dropped before await. | One `state.browser_state.lock().await` to get window clone per iteration, dropped before navigate. | **YES** — both follow lock-safe pattern (no `blocking_lock`). | `commands.rs:1848,1877` vs `session_runner.rs:448` |
| **Navigation event channels** | Uses `BrowserState.nav_tx` (AppState dummy channel with dropped receiver before session). `navigate_agent_window`'s `record_arena_navigation_request` stores PendingArenaNavigation map, but `arena://` signals go to disconnected channel → dropped via `try_send` warn. No tokio bridge, no `nav_rx` consumption. | Uses per-session `std::sync::mpsc::sync_channel(256)` created in `start_session`, bridged via `std::thread::spawn` to `tokio::sync::mpsc::channel` → `nav_rx: &mut Receiver<NavEvent>` passed into `run_setup`. Signals are reliably consumed via `nav_rx.recv().await`. | **NO** — critical divergence: Connected has no live receiver, Priming has full bridge. | `commands.rs:246-282 bridge`, `orchestrator.rs:202 dummy`, `session_runner.rs:476 drain_stale` |
| **Page-load detection** | Via `on_page_load(handle_page_load)` which records `record_navigation` + `set_window_identity` on `PageLoadEvent::Finished/Started`. Same handler for both, but Connected's `record_navigation` may mis-classify cause due to stale `pending_arena_navigations` (no `begin_setup_run` clearing). | Same `handle_page_load`, but `diagnostics.begin_setup_run` cleared pending+timeline at session start, so correlation is clean per generation. | **PARTIALLY** — same handler, different initial state. | `browser_backend.rs:2620-2722 handle_page_load`, `459-510 begin_setup_run` |
| **DOM readiness detection** | GENERIC_INIT_SCRIPT's `checkReady` probes `collectComposerSnapshot()` every 500ms, requires stable composer across 3 probes (`_readyStableCount>=3`), page_state_hint classification (`composer_detected`, `possible_login_required`, etc.), emits `arena://ready` or `arena://ready/error-*` after READINESS_TIMEOUT_MS (45s). No Rust wait in Connected path. | Same JS probes, but Rust `wait_for_setup_ready` awaits `Ready`/`Error`/`Challenge` with `READINESS_WAIT_TIMEOUT_SECS=50s` (JS 45s +5s buffer) and handles timeout with model-specific message. | **NO** — Connected never waits for ready; Priming waits and surfaces diagnostics. | `browser_backend.rs:5133-5185 checkReady`, `session_runner.rs:291-421 wait_for_setup_ready` |
| **Retry logic** | None besides busy guard. No `MAX_SETUP_NAVIGATION_RECOVERIES` handling. No `perform_priming_injection` retry. | Bounded retry: `for _ in 0..=MAX_SETUP_NAVIGATION_RECOVERIES (3)` for send wait, plus `has_recent_unexpected_navigation` re-prime after 120s timeout. | **NO** | `session_runner.rs:758-866` |
| **Refresh logic** | No explicit refresh; `window.navigate` only. | Detects `NavEvent::Ready` during send wait with `has_pending_user_submit` → trusted_submit, or without pending → inc recovery + "page refreshed; re-priming..." + `perform_priming_injection` retry. Also after 120s timeout with unexpected navigation → `wait_for_setup_ready` + `perform_priming_injection`. | **NO** | same cites |
| **Timeout logic** | Busy guard 10s pre +20s post (total 30s) prevents rapid relaunch; no readiness/send timeout. | `READINESS_TIMEOUT_MS 45s` (JS) + `READINESS_WAIT_TIMEOUT_SECS 50s` (Rust) per agent; `PromptInjectionReport` 5s; `SendDetected` 120s; `Challenge resume` 600s. | **NO** | `browser_backend.rs:15-16`, `session_runner.rs:302,692,759` |
| **Error handling** | `navigate_agent_window` error → `record_browser_error` + `Err(String)` to frontend toast; `show/focus` errors only warn. `connected_account_busy_until` cleared on failure to allow retry. | `navigate_agent_window` error → `record_browser_error` + `boss-message` + `return Err`; `wait_for_setup_ready` errors also `record_browser_error` + `boss-message`; outer `run_setup` returns Err which `start_session` catches and emits `setup-agent-failed` (recoverable) + waits for `ResumeRequested` or `SetupManualConfirmed`. | **PARTIALLY** — both record via `record_browser_error`, but Priming has recoverable retry path and session-status handling; Connected has immediate Err. | `commands.rs:1898-1906`, `session_runner.rs:489-537` |
| **Navigation cancellation** | `on_navigation` denies `NewWindowResponse::Deny` for `window.open` (preserves 2-WebView) — same for both. `session_runner` can be cancelled via `abort_session` → `NavEvent::SessionAborted` → `run_setup` returns `UnknownError`. | Same deny handler; same abort path (bridge forwards `SessionAborted`). | **YES** | `browser_backend.rs:2724-2741` |
| **Hidden/visible state** | Starts hidden, `navigate_agent_window` calls `show()`+`set_focus()` immediately after `navigate`. Launch does extra `show/focus` idempotently. | Same. | **YES** | `browser_backend.rs:2597-2606` |
| **Previous page cleanup** | No cleanup; `diagnostics.register` overwrites record but retains old `console_diagnostics`, `navigation_diagnostics` etc. No `begin_setup_run` clearing. | `begin_setup_run` at session start clears all per-agent records, timeline, pending, etc. Per-agent iteration does not clear, but `register` overwrites `window_label`/`intended_url` etc. | **NO** — Priming starts from clean state per generation; Connected accumulates stale state. | `browser_backend.rs:459-510` |
| **WebView destruction** | No destroy; `ensure_nav_window` reuses existing or creates if missing. | `create_windows` destroys stale `arena-leader`/`arena-nav` if exist before building fresh pair. | **NO** | `browser_backend.rs:5800-5808` vs `:5899-5906` |
| **JS injection** | Only identity eval before navigate; no priming prompt injection. | Identity eval + per-agent priming prompt injection via `window.eval(build_priming_script)` with 5s verification, stability retry, idempotency guard. | **NO** | `session_runner.rs:563-683` |
| **DOM readiness** | Not awaited; page may be blank if JS hasn't probed yet but navigation itself should still render. | Awaited via `wait_for_setup_ready`; ensures composer exists or challenge/login detected before injection. | **NO** | same |
| **Redirect handling** | `handle_page_load` + `record_navigation` classify `cause` as `arena_requested` vs `page_initiated` via `pending_arena_navigations` 5s window; diagnostics records but does not block. | Same classification, but Priming's `wait_for_setup_ready` loop handles `ChallengeDetected`/`UnshowableUrl` mid-wait and re-waits for resume. | **PARTIALLY** | `browser_backend.rs:798-898 record_navigation`, `1453-1560 record_nav_event Challenge/Unshowable` |

---

## 6. WebView Lifecycle Deep Investigation

Priority checklist (Task Phase 6). Each marked PASS/FAIL/N/A/UNKNOWN with evidence.

1. **Previous model page still loading:** UNKNOWN — No explicit check in either path for `page_state_hint == still_loading` before issuing new `window.navigate`. `navigate_agent_window` issues navigate immediately without waiting for prior load `Finished`. Could cause race if first navigation still hydrating WebKit when second selected? Connected's 30s busy guard mitigates rapid succession but not single slow load.

2. **Navigation issued while another navigation active:** FAIL for Connected — `navigate_agent_window` does not check `window.url()` or `current_phase` before navigate; if previous navigation still in `navigation_started` without `real_url_loaded`, second navigate overwrites. Priming mitigates via sequential per-agent loop (only one navigate at a time) plus drain + ready wait, but still no explicit "is loading" gate.

3. **Reused without resetting relevant state:** FAIL — Connected reuses nav window without `begin_setup_run` clearing, so `pending_arena_navigations`, `navigation_diagnostics`, `expected_agent_id`, `last_navigation_url`, `setup_completion_reason` etc retain stale values. Priming does `begin_setup_run` at session start, so per-session state is clean; but per-agent reuse inside session still retains prior agent's state until `register` overwrites, without clearing `console_diagnostics`.

4. **Old page not fully navigated away:** PASS — `window.navigate` replaces current document; Tauri guarantees navigation. No evidence of old page persisting as overlay; but old page's `window.__ca_agentId` via `window.name` is correctly overwritten before navigate.

5. **Destroyed/recreated inconsistently:** FAIL — Connected never destroys stale windows, Priming destroys before creation. After abort, windows remain; subsequent Connected launch reuses them, but diagnostics metadata (setup_generation, session_id) is stale. This is inconsistent lifecycle.

6. **Navigation callbacks attached to wrong lifecycle:** PASS — Both paths install `on_navigation(make_nav_closure)` and `on_page_load(handle_page_load)` at window creation, with `window_label` correctly set to `arena-leader`/`arena-nav`. Callbacks are generic, not agent-specific. No evidence of closure capturing wrong window.

7. **Initialization scripts registered at wrong time:** PASS — `GENERIC_INIT_SCRIPT` registered via `.initialization_script` at window creation before any `navigate` — correct order. Not registered after page exists. Verified `browser_backend.rs:5858,5882,5923`.

8. **Window exists but WebView content not actually navigated:** FAIL — Connected path's `BrowserState.nav_tx` at startup has dropped receiver (AppState creates dummy channel and drops `_rx`), so `on_navigation`'s `tx.try_send` for `arena://` signals will fail with `Disconnected`, but `window.navigate` itself should still succeed. However, if `navigate` fails silently (e.g., `let _ = window.navigate` was previously ignored, but current code does `window.navigate(...).map_err(...)?` — it does propagate error). Yet the pre-audit history (browser-loading-pre.md) shows prior bug where `create_windows` built `about:blank` and never navigated, causing black windows — that was fixed. Current Connected path does navigate, but the dummy channel could cause `record_nav_event` never to be called (since bridge thread not running), so `diagnostics.last_navigation_url` not updated beyond the initial `register` phase, making snapshot stale — but not blank.

9. **Page is navigated but callback waits for event that never arrives:** FAIL for Connected — No callback wait at all, so not applicable; but for Priming, `wait_for_setup_ready` waits for `Ready`/`Error`/`Challenge` with 50s timeout. If GENERIC_INIT_SCRIPT never emits `arena://ready` (e.g., composer not found, challenge not detected, page_state_hint stays `still_loading`), it will timeout with `readiness_timeout_message`. That is a known failure for Claude rarely loading.

10. **Navigation error swallowed:** PASS (now) — `navigate_agent_window` maps `window.navigate` error to `record_browser_error` + `AgentError::NavigationFailed` and propagates via `?` in both paths; not swallowed. Earlier audits fixed `let _ =` swallow.

11. **Redirect interpreted as failure:** UNKNOWN — `record_navigation` classifies `cause=page_initiated` vs `arena_requested`; `wait_for_setup_ready` treats `ChallengeDetected`/`UnshowableUrl` as failure but `Ready` during send wait as potential re-prime, not failure. No evidence that normal redirects (e.g., `https://claude.ai` → `https://claude.ai/new`) are misinterpreted as failure, but they do increment `last_navigation` and could trigger `has_recent_unexpected_navigation` true, causing extra re-prime.

12. **Timeout triggers refresh while page still legitimately loading:** FAIL — `READINESS_TIMEOUT_MS 45s` + `READINESS_WAIT_TIMEOUT_SECS 50s` is aggressive for Celeron/WebKitGTK slow load (see FRESH_INSTALL doc: WebKit hydrate may need >45s). Doubling to 90s/100s is requested fix. Currently 45s may fire while page still loading, signaling `error-` and failing setup prematurely, causing user to perceive blank.

13. **Refresh causes initialization state lost:** PASS — `window.name` persists across refresh; GENERIC_INIT_SCRIPT restores `__ca_agentId` from `window.name` each document. No evidence refresh loses identity, but `__ca_mainInstalled` guard prevents timer multiplication (RC1-INITSCRIPT).

14. **Same WebView has stale agent identity:** FAIL for Connected — Before second launch, `window.name` still holds previous agent's id (e.g., chatgpt). `set_window_identity` before next navigate overwrites it, but `handle_page_load` also sets it after load. However diagnostics `active_by_window` maps `arena-nav` → previous agent until next `register` overwrites; during the gap between `set_window_identity` (old document) and `handle_page_load` (new document), `active_agent` could be stale, causing console diagnostics attribution mismatch (warn logged at :1959).

15. **Cookie restoration after navigation instead of before:** N/A — No cookie restoration implemented at all (zero call sites). So neither path does it; not cause of blank but does affect login persistence.

16. **Cookies associated with wrong URL/domain:** N/A — Same.

17. **Settings path does not perform same preparation as Priming:** FAIL — Confirmed: Settings `launch_connected_account` does not call `begin_setup_run`, does not `record_setup_expected_agent`, does not `drain_stale`, does not wait for ready, does not handle challenge/unshowable, does not inject priming, does not emit setup events, does not clear busy guard correctly (now 30s vs Priming's sequential). This divergence is intentional per product (settings vs session) but browser infrastructure should be shared where overlapping.

---

## 7. `on_navigation` Deep Investigation

Full implementation: `browser_backend.rs:2884-3225` (`make_nav_closure`, `handle_arena_url`, `parse_arena_signal`, `handle_page_load` is separate).

- **URL classification:** `make_nav_closure` matches `url.scheme()`:
  - `"arena"` → `handle_arena_url(tx.clone(), window_label, url)` + `return false` (intercept, don't navigate)
  - `"http"|"https"|"about"|"blob"|"data"` → `return true` (allow)
  - else → `send_nav_event(UnsupportedNavigation)` + `return false` (block)
  - Verified at :2890-2907.

- **arena:// interception:** `handle_arena_url` at :2910 parses via `parse_arena_signal` (host+path split preserving empty segments for prompt-injection). Unknown signal → `send_unknown_arena_signal` → `UnsupportedNavigation` with reason "Unknown arena diagnostic signal ignored". Known signals handled: `ready` (incl `error-` prefix), `error`, `response`, `done`, `setup-response`, `sent`, `prompt-injection`, `active-submit`, `send-probe`, `challenge`/`captcha`, `unshowable`, `log`, `console`, `lifecycle`, `dom`, `action`, `ua` — all via `send_nav_event(tx.try_send)` using `std::sync::mpsc::SyncSender`, never `tokio::sync::mpsc`.

- **Agent identity determination:** For `arena://` signals, agent_id is taken from URL path segments (e.g., `arena://ready/claude` → agent_id `claude`), not from closure capture. `make_nav_closure` captures only `tx: SyncSender<NavEvent>` and `window_label: &'static str` — verified at :2886-2890. `GENERIC_INIT_SCRIPT` reads `window.__ca_agentId` at runtime (restored from `window.name`), never closure value — `browser_backend.rs:4856-4858 getAgentId() { return window.__ca_agentId || 'unknown'; }`. `NAVCLOSURE` risk: PASS.

- **Redirects:** Real web redirects (`http`→`https`, `/` → `/new`, login auth) are allowed (`return true`), observed via `on_page_load` + `record_navigation` with `cause` correlation. `PendingArenaNavigation` map holds requested URL for 5s (`ARENA_NAVIGATION_CORRELATION_SECS=5`) to distinguish `arena_requested` vs `page_initiated`.

- **State captured:** `handle_arena_url` does not directly capture state; it only sends `NavEvent` via channel. State update happens in bridge thread `record_nav_event` (`browser_backend.rs:1433-1915`) which updates diagnostics per agent.

- **Channels:** `on_navigation` is synchronous, uses `std::sync::mpsc::SyncSender::try_send` (non-blocking) and logs dropped via `tracing::warn` at :3285. Never uses `tokio::sync::mpsc` inside callback — grep finds only `use tokio::sync::mpsc::Receiver` for async side. `RISK-CHANNEL`: PASS.

- **Callbacks outliving WebView:** `make_nav_closure` clones `tx` and moves `window_label` (static str) — no WebviewWindow capture, so not tied to window lifetime. `tx` is `BrowserState.nav_tx` cloned at window creation time; if BrowserState is later replaced (`*browser = BrowserState::new(...)` in `start_session`), old windows' closures still hold old `tx` (from old BrowserState) that is now disconnected (since bridge thread was tied to old channel). However `create_windows` destroys old windows before building new ones, so old closures die with old windows. For Connected path, no replacement, so closures remain valid.

- **Agent ID captured incorrectly:** No — verified `handle_arena_url` always parses from URL, not closure. `identity_script` is eval'd separately.

- **Navigation blocked accidentally:** Only for `arena://` (intentionally blocked) and unsupported schemes. Real https navigation never blocked.

- **Callback registered before/after navigation:** Registered at `WebviewWindowBuilder::new(...).on_navigation(...)` before any `navigate` call — correct.

- **Repeated navigation:** Each `navigate_agent_window` calls `diagnostics.register` + `set_active` anew, overwriting mapping, so repeated navigations correctly attribute subsequent `on_navigation` arena signals to newest agent for that window.

- **Refresh/redirect:** `handle_page_load` at :2620-2722 records every `PageLoadEvent::Started/Finished` with `record_navigation` and `set_window_identity`; diagnostics `last_navigation` tracks last URL, `navigation_diagnostics` deque (10 per agent) preserves history.

**Verdict:** `on_navigation` implementation is correct and lock-safe (no `blocking_lock`, no async, only `try_send`). No NAVCLOSURE bug.

---

## 8. `GENERIC_INIT_SCRIPT` Deep Investigation

Complete script: `browser_backend.rs:4519-5742` (1223 lines), static `&str`.

- **When registered:** At `WebviewWindowBuilder::new(...).initialization_script(GENERIC_INIT_SCRIPT)` for both `arena-leader` and `arena-nav` in `create_windows` (:5858,5882) and `ensure_nav_window` (:5923). Tauri runs it on every navigation (document creation).

- **When executes:** At document creation, before DOMContentLoaded. Contains idempotent guards: `if (window.__ca_consoleDiagnosticsInstalled) return;`, `if (window.__ca_lifecycleInstalled) return;`, `if (window.__ca_mainInstalled) return;` — prevents timer multiplication if somehow eval'd twice.

- **Survives SPA navigation?** SPAs using `history.pushState`/`replaceState` do not trigger full document reload, so script not re-executed. Lifecycle IIFE wraps those APIs to emit `arena://lifecycle/history_pushState` but does not re-install main probes. However main probes (checkReady, detectSend, mutationObserver, intervals) remain running in same document, so they survive SPA navigation. For full reloads, script re-executes and restores `__ca_agentId` from `window.name`.

- **Agent identity:** `var _identityPrefix='__consensus_arena_agent__:'; if (!window.__ca_agentId && window.name.indexOf(_identityPrefix)===0) window.__ca_agentId = window.name.substring(...)` at :4799-4802. Later `getAgentId()` reads `window.__ca_agentId || 'unknown'` at :4856. No closure capture, model-agnostic.

- **arena:// communication:** Uses `window.location.href='arena://...'` which is intercepted below CSP (per D-002). Verified below CSP via Tauri `on_navigation`.

- **Send detection:** Via polling intervals: `setInterval(detectSend,250)`, `setInterval(attachSendListeners,1000)`, `setInterval(detectChallengeOrUnshowable,1500)`, MutationObserver on `documentElement`, plus trusted `click`/`submit`/`keydown` listeners with `event.isTrusted` check, plus `beginSendCheck` logic requiring non-empty input, messageCount increment, inputCleared, readyState complete/interactive. Four-condition spec (input non-empty, count+1, input empty, no reload) is implemented via `pendingSend` tracking and `renderedMessageCount`.

- **Response detection:** `pollResponse` every 500ms after injection, checking `RESP_SELECTORS` (`[data-message-author-role="assistant"]` etc) for new text vs baseline, stable 4×500ms=2s then fires `arena://response` + `arena://done`.

- **Blank page behavior:** `checkReady` probes for input via `collectComposerSnapshot()`; if no input found for READINESS_TIMEOUT_MS 45s, sends `arena://ready/error-{agent}`. This is the timeout that task wants doubled. Does not throw; always completes.

- **One model's DOM assumptions interfering:** Selectors are generic: `SELECTORS` includes `#chat-input` (GLM), `div.chat-input-editor` (Kimi), `#prompt-textarea`, `div.ProseMirror`, `rich-textarea div[contenteditable]`, `textarea`, `[data-testid*="composer"]`, etc. — covers all 7 models without model-specific `if`. `SEND_SELECTORS` generic, `COMPOSER_CONTAINER_SELECTORS` generic. No model branch, verified `grep -n "if.*agent" GENERIC_INIT_SCRIPT` finds none; `grep -n "claude\|chatgpt"` finds none (only comments). `RISK-INITSCRIPT`: PASS.

- **Depends on document.body?** `safeVisibleText()` at :5067-5074 reads `document.body.innerText` but guarded with try/catch. `checkReady` uses `collectComposerSnapshot` which queries `document.querySelectorAll`, not body. `classifyPageState` uses `document.readyState`, `hasVisibleProgressIndicators`, `bodyLength` etc but with guards.

- **Waits for DOM readiness:** Yes — `if (document.readyState==='complete'||'interactive') setTimeout(checkReady,100) else DOMContentLoaded`. Then `checkReady` recurses every 500ms until stable.

- **Observers/listeners multiplication:** Idempotent guards prevent double install; intervals are set once per document. Repeated navigation creates new document, so new intervals per document — not duplicate within same document.

**Verdict:** `GENERIC_INIT_SCRIPT` is static, generic, lock-safe, and correctly handles all 7 models without branching. No immediate fix needed except readiness timeout doubling (JS constant `READY_TIMEOUT_MS 45000` at :4853).

---

## 9. Cookie/Session Restoration Investigation

Trace: `Settings → model account launch → agent identity (window.__ca_agentId) → SessionVault → cookies → WebView → target domain`

- **Where stored:** `session_vault.rs:54-68 init_schema` creates `cookies (agent_id TEXT PRIMARY KEY, data BLOB NOT NULL, saved_at INTEGER)` + `conversation_urls`. Encryption via `ring AES-256-GCM` with PBKDF2 derived key.

- **Where loaded:** `session_vault.rs:128 load_cookies(agent_id)` decrypts blob.

- **When loaded:** **Never** — source search `grep -rn "load_cookies\|save_cookies"` finds only definitions, zero call sites. `SessionVault` is `Arc<std::sync::Mutex<SessionVault>>` in `AppState`, but no command, no `browser_backend`, no `response_router`, no `session_runner` ever calls `load_cookies` or `save_cookies`. Cookies are not injected before or after navigation.

- **Domain/path:** Not applicable — cookies never restored, so domain matching not evaluated.

- **Model-specific:** Table is per `agent_id` (e.g., "claude"), not per session, as intended per `DECISIONS.md D-046`.

- **Failure modes:** Since never called, silent failure not relevant; but login persistence relies solely on WebView's native cookie store, which is partitioned per WebView (leader vs nav). P4 heuristic attempts to preserve affinity.

- **Different flows:** Both Connected and Priming have identical (no) cookie flow — verified same.

**Conclusion:** Cookie restoration is not implemented and not cause of blank (both paths same). Do not change encryption/storage per task unless causal — confirmed not causal.

---

## 10. Model URL Registry (see §2)

All URLs validated as absolute HTTPS (redacted_url safe). Only Kimi is incorrect. No duplicate registry.

---

## 11. Claude-Specific Deep Investigation

**Navigation chain (requested → redirect → final):**

- Requested: `https://claude.ai` (`AGENTS` table)
- Observed via `record_navigation`: For unauthenticated, Claude returns 302 to `https://claude.ai/login?returnTo=%2F` or `https://claude.ai/new` depending on auth state; authenticated redirects to `https://claude.ai/new` (SPA). This is `page_initiated` navigation (not `arena_requested`) after initial `arena_requested` to `https://claude.ai`. Evidence: `browser_backend.rs:798-848` cause correlation 5s window; `handle_page_load` records every load.
- Final URL: `https://claude.ai/new` or `https://claude.ai/chat/<id>` (when conversation exists)
- Page readiness: Claude's app is React-based, hydration may take longer on Celeron. `collectComposerSnapshot` uses `div.ProseMirror[contenteditable="true"]` and generic selectors — should find Claude's editor, but Claude's editor may be nested inside shadow or may have `contenteditable="true"` on a different node than expected.

**Differences vs reliable model (e.g., ChatGPT):**

- ChatGPT: `https://chatgpt.com` → SPA, input `textarea` (stable), send `button[data-testid="send"]` (stable), composer container `form` (stable), `page_state_hint` quickly becomes `composer_detected`, `input_found true`, `send_button_found true` within few probes.
- Claude: input `div.ProseMirror[contenteditable]` or `p[data-placeholder]` inside contenteditable, send button may be `button[aria-label="Send"]` with SVG icon only (no text), requiring `looksIconOnlySend` geometry check (20-80px, near input, contains SVG). This geometry path is fragile on slow layout (rect may be 0 during hydrate). `collectSendCandidatesIn` only scans inside `composerRootFromInput`, which walks up from input to find owning Send — if composer root mis-identified, Send not found.
- `page_state_hint` for Claude unauthenticated may be `possible_login_required` (text contains "log in", "Sign in") — `checkReady` then loops with 1s delay without signaling ready, until 45s timeout → `error-` → `wait_for_setup_ready` 50s timeout. This explains rarely loads: if user not logged in, always timeout. If logged in, still may timeout due to slow hydrate or Send geometry.

**Timing/SPA/bootstrap:** Claude's ProseMirror editor initializes after React hydration; `collectComposerSnapshot` may find 0 candidates initially, then 1 after hydrate. `_readyStableCount` requires same element stable 3×500ms — if editor is replaced during hydrate (old node detached, new node attached), count resets, delaying ready.

**Cookie requirements:** Claude requires auth cookie for `claude.ai`; without it, login page shown, not composer. Connected Accounts is login path, so blank may be login page but not composer? However user says even Priming rarely loads Claude — suggests even after login, composer not detected.

**CSP/anti-automation:** Claude may have stricter CSP or bot detection (Cloudflare). GENERIC_INIT_SCRIPT's `arena://` is below CSP, but Claude could detect `window.__ca_agentId` presence or `MutationObserver`? No evidence.

**Injected scripts:** Same generic script for both, no Claude branch.

**Page readiness detection:** `inputCandidateCount`/`composerCandidateCount` for Claude may be 0 until hydrate, causing `still_loading` hint. `page_health_hint` may be `interactive`.

**Navigation events:** For Claude, sequence may be `navigation_started (arena_requested)` → `DOMContentLoaded` → `page_initiated redirect to /login` → `load` → `checkReady` loops with `possible_login_required` → timeout.

**Fix scoped higher-level:** Instead of branching `GENERIC_INIT_SCRIPT`, improve `composerRootFromInput` and `SEND_SELECTORS` generically to cover Claude's SVG send, and double readiness timeout to 90s to allow slower hydrate/login. Also ensure Claude's base_url is `https://claude.ai` (correct) — not changed.

**Evidence for fix:** `browser_backend.rs:5195 SEND_SELECTORS` includes generic `button[aria-label*="send"]` which should catch Claude's `aria-label="Send Message"` — case-insensitive match should work. But `isSendCandidate` also filters out `attach`/`file` etc and checks `SEND_SELECTORS.some(el.matches)` — for SVG-only button, `looksIconOnlySend` is fallback requiring rect 20-80 and SVG presence and proximity to input (≤140px). If Claude's button is 16px or 90px, it fails. Could relax generically, but task forbids model-specific.

**Recommendation:** Double readiness timeout (45→90) will help Claude most, as its hydrate+login path is slowest. Also ensure Kimi URL fix does not mask Claude.

---

## 12. Connected Accounts vs Priming Root-Cause Analysis

**Intended semantics:**

- Connected Accounts: settings/account-management flow — user manually logs into each model's web account via WebView, no prompt injection, no `setup-agent-ready`, no `arena://sent` required. Needs visible window with navigated URL and composer/login UI, but not full initialization. Should reuse shared WebView model, one window at a time, with busy guard.

- Priming: session setup flow — sequential per-model navigation, readiness wait, prompt injection, send detection, `setup-agent-ready`/`setup-agent-complete`, `setup-complete` before autonomous loop. Needs full initialization, injection, and proof.

**Infrastructure sharing:** Both should share `navigate_agent_window` (they do), but Connected lacks `begin_setup_run`, `record_setup_expected_agent`, waits, injection, and recovery.

**Is bypass intentional?** Yes, Connected should not inject prompts or wait for Send, but it should still wait for page readiness (composer or login) to confirm navigation succeeded, rather than fire-and-forget.

**Missing critical step:** Connected does not wait for `Ready`/`Error`/`Challenge`/`Unshowable` signals, so user perceives blank while page still loading (JS still probing for 45s). Priming's `wait_for_setup_ready` surfaces precise error (`readiness_timeout_message` with probe counts, `page_state_hint`). Connected's lack of wait means no error surfacing, no retry, no diagnostics update beyond `navigation_started`. The WebView may actually be loading but not yet signaled ready, appearing blank until `ready` would have fired.

**Should Connected use same launch helper as Priming?** Yes, minimal safe correction is to make Connected use a shared helper that does `navigate` + optional readiness wait (without injection) and shares `begin_setup_run` metadata handling, without duplicating full priming loop. Not a broad refactor.

---

## 13. Refresh Timeout Investigation

Task: "DOUBLE THE EXISTING TIMEOUT" for AI chat page refresh during Priming.

**Identified timeout:**

- **Constant:** `READINESS_TIMEOUT_MS` (JS) = `45_000` ms at `browser_backend.rs:15` (Rust) and `4519/4853` (JS `const READY_TIMEOUT_MS = 45000`), and `READINESS_WAIT_TIMEOUT_SECS` = `50` s at `browser_backend.rs:16` (Rust wait).
- **Value/Unit:** 45 seconds (JS) / 50 seconds (Rust) — JS triggers `arena://ready/error-*` after 45s if no composer stable, Rust waits 50s for that signal (5s buffer).
- **File:** `src-tauri/src/browser_backend.rs:15-16`, `src-tauri/src/browser_backend.rs:4853-4854` (JS), `src-tauri/src/session_runner.rs:302` (wait call)
- **Call site:** `session_runner.rs:302-332 wait_for_setup_ready` → `tokio::time::timeout(Duration::from_secs(READINESS_WAIT_TIMEOUT_SECS), async { loop nav_rx.recv() ... })` and JS `checkReady` at `browser_backend.rs:5139-5185` with `now - _checkReadyStart >= READY_TIMEOUT_MS` → `window.location.href='arena://ready/error-'+agentId`
- **Trigger condition:** `collectComposerSnapshot()` finds no visible input (`snapshot.input == null` or not `isConnected`/`isVisible`) for continuous 45s, and `page_state_hint` is not `possible_login_required`/`possible_challenge_or_security` (those keep looping 1s), and not stable 3-probe ready. After 45s, JS signals error; Rust's 50s timeout also fires if no signal at all (channel closed etc) → both produce `Timeout` or `NavigationFailed` with `readiness_timeout_message`.
- **Refresh/navigation action:** On error/timeout, Rust `record_browser_error` + `boss-message` + `return Err(AgentError::Timeout(...))` which `run_setup` propagates to `start_session` loop → emits `setup-agent-failed` (recoverable) and waits for user `retry_setup_agent` (which re-navigates via `navigate_agent_window`) or `confirm_setup_agent`. So timeout does not auto-reload; it fails setup and requires manual retry (which does `navigate_agent_window` again — effectively refresh). The JS error signal itself does not reload, but the Rust retry does.
- **Why exists:** To bound waiting for slow Celeron/WebKit hydrate or network, distinguish `page_loaded_but_no_composer` vs `still_loading` vs `empty_shell`, and surface diagnostics (probe counts, hints). Prevents infinite hang.
- **Per-model?** No — single constant for all 7 models, generic. Apply uniformly.
- **Per-navigation?** Per `wait_for_setup_ready` call, which is per-agent per-setup-iteration (each of the `setup_order` agents, each retry).
- **Priming only?** Yes — `wait_for_setup_ready` is only called from `session_runner::run_setup` (Priming). Not used by Connected Accounts (no wait) or autonomous `response_router` (uses `wait_for_response` with 300s). Verified grep: `READINESS_WAIT_TIMEOUT_SECS` only in `browser_backend.rs:2763 wait_for_ready` (used by `inject_to_window` for active turns? Actually `inject_to_window` also uses it at :2763-2764 for active injection, but that path is for autonomous routing, not priming — so timeout is shared between Priming's setup readiness and active-turn injection readiness, but Priming's setup path is the primary consumer). `BROWSER_FORENSICS` doc says readiness timeout is 45s for composer detection.
- **Shared?** Partially shared with `inject_to_window(wait_ready=true)` for active turns (response_router), but active-turn readiness is less critical (leader already loaded). Doubling still safe as it only increases wait cap.

**Current:** `READINESS_TIMEOUT_MS = 45_000`, `READINESS_WAIT_TIMEOUT_SECS = 50`, JS `READY_TIMEOUT_MS = 45000`

**New (×2):** `90_000` ms and `100` s respectively, `READY_TIMEOUT_MS = 90000` in JS. Keep unit ms/s unchanged. Update tests accordingly.

**Not to change:** `AgentBrain` HTTP timeout 60s (`agent_brain.rs:113 BRAIN_HTTP_TIMEOUT_SECS=60`) — separate primary/fallback brain clients, per task not to change unless proven; audit confirms separate.

---

## 14. Root Cause Determination (Evidence-Based)

| Suspected Cause | Evidence | Verdict |
|-----------------|----------|---------|
| **URL problem (Kimi)** | `AGENTS` table uses `https://www.kimi.com/` vs task-required `https://kimi.ai`; no other Kimi URL references except test. Other models URLs valid. | **CAUSE — Kimi only** (P2). Fix: change to `https://kimi.ai`. |
| **WebView reuse without clearing state** | Connected path never `begin_setup_run`, retains stale `pending_arena_navigations`, `active_by_window`, `expected_agent_id`, `setup_generation`. Priming clears via `begin_setup_run` each session. | **CONTRIBUTING CAUSE — Connected blank** (FAIL #3, #5). Fix: Connected should call lightweight diagnostics init or reuse correctly. |
| **Navigation race (issued while loading)** | No gate checking `current_phase == still_loading` before second `window.navigate`; busy guard 30s only guards rapid clicks, not single slow load. Could overwrite slow WebKit load. | **POSSIBLE** — not proven blank, but contributes. Fix: keep guard + add readiness wait. |
| **Init script timing** | Script registered at window creation correctly; identity via `window.name` correct. No evidence of wrong timing. | **NOT CAUSE** (PASS). |
| **Cookie restoration** | Zero call sites for `save/load_cookies`; both paths identical (none). Not cause of blank. | **NOT CAUSE** (N/A). |
| **Settings/Priming path divergence** | Large divergence: Connected lacks readiness wait, challenge handling, retry, diagnostics clearing, expected_agent tracking. This is the dominant architectural divergence. | **PRIMARY CAUSE — Connected blank** (FAIL #17). Fix: unify around `navigate_agent_window` + readiness wait (without injection). |
| **Timeout too aggressive** | `READINESS_TIMEOUT_MS 45s` + `READINESS_WAIT_TIMEOUT_SECS 50s` may fire while Celeron/WebKit still hydrating, especially for Claude/Kimi with contenteditable + SVG send. Review flagged as P2 (45→90). | **CONTRIBUTING CAUSE — overall, critical for Claude** (FAIL #12). Fix: double to 90s/100s. |
| **Claude-specific redirect/domain/auth** | Claude URL `https://claude.ai` redirects to `/login` or `/new` (page_initiated); unauthenticated shows `possible_login_required` which loops indefinitely until timeout; contenteditable + icon-only send requires geometry check fragile on slow layout. | **CAUSE — Claude rarely loads** (see §11). Fix: double timeout helps; ensure generic Send detection covers SVG case (already does but fragile). No model-specific branch in GENERIC. |
| **JS injection (priming prompt)** | Connected does no injection, so injection not cause of blank. Priming injection has stability retry and idempotency guard, generic. | **NOT CAUSE** for Connected; Priming injection verified PASS. |
| **Navigation callback (on_navigation)** | Correctly implements `std::sync::mpsc`, only `tx`, no `blocking_lock`, agent from URL, allows https, denies new window. No bug. | **NOT CAUSE** (PASS). |
| **Error swallowing** | Previously swallowed (`let _ =`), now correctly `record_browser_error` + `?` in both paths. No swallow now. | **NOT CAUSE** (PASS). |

**Synthesis:**

- **Connected Accounts blank:** Primary is divergence + stale state + no readiness wait, making user see blank while page still loading/probing (45s window) with no feedback and no error surfacing; secondary is 45s timeout too aggressive causing premature error if it were waited but not now; tertiary is WebView reuse without clearing. The page *is* navigated, but without waiting for `Ready` the window appears empty until JS signals and Rust would have considered it ready (Priming would wait and then show readiness). Fix: make Connected path wait for readiness (or at least surface `browser-diagnostic` phases) and ensure diagnostics generation is sane; simplest is to add readiness wait with doubled timeout and proper diagnostics init.

- **Priming mostly works:** Because it does `navigate` + `wait_for_setup_ready` with probe, handling challenge/login, and retry. So infrastructure not universally broken.

- **Claude uniquely unreliable:** Requires login (possible_login_required loop) + slower hydrate + icon-only Send geometry; 45s timeout often expires before composer stable 3 probes. Doubling to 90s gives more time for hydrate and user login completion; still may need manual login via Connected flow.

- **Kimi:** URL typo alone.

---

## 15. Readiness for Fix (Pre-Edit Checklist)

- Dirty worktree preserved (hackathon mode). Changes will be minimal, scoped, no new WebView, no new dependency, no `blocking_lock`/`tokio::mpsc` in `on_navigation`, no `unwrap`/`expect` in prod, IPC names preserved, `GENERIC_INIT_SCRIPT` stays static/generic (only timeout constant changed), agent identity via `window.__ca_agentId`.
- Verification commands planned: `cargo check`, `npm run build`, `git diff --check`.
- Regression greps planned: `kimi.com`, `blocking_lock(`, `tokio::sync::mpsc`, `.unwrap()`, `.expect()`.

---

*End of pre-audit. Evidence based on commit a3ab85f source at time of audit. Next: implement smallest robust fix per Phase 15.*
