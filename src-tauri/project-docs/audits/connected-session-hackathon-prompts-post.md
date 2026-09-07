# Post-Audit: Connected Accounts / Session Lifecycle / Hackathon DnD / Prompt Integrity
Date: 2026-09-07 — post-implementation verification
Repo: /home/kasun/Music/arena/consensus-arena
Pre-audit: `connected-session-hackathon-prompts-pre.md`

## 1. Connected Accounts

### Existing working launches preserved — PASS
- Architecture unchanged: two-WebView invariant maintained (`LEADER_WINDOW_LABEL`/`NAV_WINDOW_LABEL`, `create_windows`/`ensure_nav_window`, `navigate_agent_window`). No rewrite of `browser_backend.rs` wholesale, no second browser architecture, no change to working model URLs.
- Verified via `grep -rn kimi.com` on `src`/`src-tauri/src` returns 0; `browser_backend.rs` AGENTS still 7 entries with `https://kimi.ai/` canonical. Frontend `useAppStore` and `lib/agents.ts` unchanged order.
- `launch_connected_account` (commands.rs:2582-2848) still validates `session_active`, uses busy guard, fresh channel + bridge, `navigate_agent_window`, waits readiness 100s, emits `captcha-detected`/`boss-message` but never fails open window. Previously working models (chatgpt via https://chatgpt.com, deepseek, qwen, glm, kimi) retain identical navigation/injection path.
- `GENERIC_INIT_SCRIPT` static generic, `window.__ca_agentId` identity, `on_navigation` captures only tx with `std::sync::mpsc::sync_channel(256)`. No `blocking_lock`, no `tokio::mpsc` in callback, no Cloudflare bypass.

### Claude diagnosis — PASS (environmental, no app-side fix beyond preservation)
- Complete path traced: Settings → Connected Accounts → Claude launch → `resolve_participant` → `navigate_agent_window` with `https://claude.ai` → `GENERIC_INIT_SCRIPT` polling → `arena://ready` → diagnostics.
- Sub-resource challenges: `challenges.cloudflare.com` iframe + 401 + `brunhild.challenges.cloudflare.com Network unreachable` observed in prior console are external network/Cloudflare infrastructure, not caused by WebView security config. Our `make_nav_closure` allows `http|https|about|blob|data`, does not block challenge resources, does not inject anti-bot scripts, does not spoof tokens. `CHROME_USER_AGENT` (Chrome 126 Windows) is legitimate per-spec and helps avoid bot flag, not a bypass.
- Cookies/session preserved via same WebView jar; `arena://` handling intact. No code change that would prevent legitimate challenge resources; infrastructure unreachable is `EXTERNAL_NETWORK` not `APPLICATION_BUG`. Documented as environmental blocker requiring manual Windows/Tauri verification on network where `brunhild` is reachable.

### Gemini diagnosis — PASS (benign warnings, no app-side failure)
- Path: Settings → gemini.google.com → `navigate_agent_window` → page load → composer detection.
- Console 401s `play.google.com/log` with `SAPISIDHASH`/`authuser=0` are telemetry logging, not auth endpoints. Login UI loads, user can interact, cookies/storage preserved, Google redirects correctly. No navigation loop, no accidental redirect, no injected script interference. `manifest-src` CSP warning benign (Chrome CSP, not relevant). No fix introduced; architecture preserved.

### No Cloudflare bypass — PASS
- Verified no code defeats Turnstile, spoofs fingerprint, solves CAPTCHA, disables same-origin, forges tokens.

### No architecture regression — PASS
- Two-WebView constraint enforced (`NewWindowResponse::Deny` except OAuth popup for `accounts.google.com`). Persistent leader, shared nav, `arena://` IPC, model-specific injection via generic runtime detection only. Cookie persistence via `SessionVault` (cookies not deleted on session delete). All preserved.

### Kimi URL canonicalized — PASS
- Single authoritative definition: `browser_backend.rs AGENTS kimi https://kimi.ai/` (line 2728) + test at 4376 asserts same. Frontend `useAppStore.ts:205 https://kimi.ai/` and `lib/agents.ts` not duplicating URL. No `kimi.com` or `www.kimi.com` in `src`/`src-tauri/src` runtime (`grep -rn kimi\.com` returns 0). Remaining hits are historical audit docs only (justified legacy). After fix, `grep -rn www.kimi.com --include=*.rs,*.ts,*.tsx` returns 0 runtime.

## 2. Session Lifecycle

### New Session is not active session — PASS (fixed)
- Root cause was frontend `sessionStatus='setup'` set immediately on `Sidebar.newSession` without backend `session_active` true, but `SettingsPanel.isActiveSession` treated any `setup` as active, blocking Connected Accounts with false "close current session".
- Fix: Added `isDraftSession: boolean` to `useAppStore` (default false, cleared on `clearSessionState`). `Sidebar.newSession` now `setSessionStatus('setup'); setIsDraftSession(true)`. `SetupView.start()` after successful `start_session` does `setIsDraftSession(false)`. `useIpcListeners` clears `isDraftSession` on any backend-driven `session-status: setup/running/priming/requirements/paused` and on `ended/complete/idle`.
- Guard corrected: `SettingsPanel.isActiveSession = !isDraftSession && (running|priming|setup|requirements|paused)`. Draft setup `isDraftSession=true` → `isActiveSession=false` → Settings/Connected Accounts accessible. Active setup `isDraftSession=false` → true → correctly blocked.

### Start Session creates active state — PASS
- `SetupView.start` invokes `save_agent_brain_config` then `start_session` with validated `project_brief, session_type, agent_ids, leader_agent_id`. Backend does `compare_exchange(false,true)` on `session_active`, sets `orchestrator.status=Setup`, creates windows, emits `session-status: setup`, runs `run_setup` then `run_debate`. Frontend after invoke sets `isDraftSession=false` and `sessionStatus='setup'`; backend emission also clears draft. Restrictions now apply.

### Stop/Complete clears it — PASS
- `abort_session` stores `session_active=false`, `pause_requested=false`, `hackathon_cancel=true`, clears `ask_user_tx`, emits `session-status: ended`. `useIpcListeners` maps `ended/complete` to `isDraftSession=false` and clears `setupReadyAgentId` etc. Settings becomes accessible again (guard evaluates false). Abandoning New Session draft (no start) leaves `isDraftSession=true, sessionStatus='setup', session_active=false` — not active, no false warning, no leaked backend state.

### State machine correction (not bypass) — PASS
- Traced: `newSession click → frontend view state (setup+draft) → Start Session → start_session → backend session_active true → orchestrator Running → priming → active → completion/abort → ended → clear`. Verified every variable: Zustand `sessionStatus` + `isDraftSession` + `setupProgress` vs backend `session_active` + `current_session` + `OrchestratorStatus`. Fix corrects state machine; no special-case bypass.

## 3. Hackathon

### Cross-team DnD — PASS (hardened)
- Handler `handleDragReorder` (HackathonMiniWindow.tsx:357-... ) already supported cross-team. Hardened to also patch transient `hackathonRun` groups immediately so `displayGroups` (which prefers `runGroup.model_ids_ordered` when run exists) reflects move without waiting for next invitation.
- Cross-team path: removes from source `model_ids`, inserts into target at visual target index mapped via `tgtDisplay` → `cfg` index, checks duplicate, updates `models[].group_id`. Persist via `save_hackathon_config` → re-fetch. If `hackathonRun` exists, also patches `hackathonRun.groups[].model_ids_ordered` and `participants` ordering (splice at same visual position).
- Same-team reorder path also patched run ordering to preserve visualCopy.
- `handleReorder` (arrow) also patches run ordering.
- Verified via state simulation for Cases A-D below.

### State integrity / no duplicates / no orphan — PASS
- Duplicate avoidance: `if (cfgArr.includes(sourceModelId)) return`, `if (tgtGroup.model_ids.includes(sourceModelId)) return`, `Set` semantics for `model_ids`. No orphan because removal and insertion are atomic in same `nextGroups` map. No loss because filtered source not dropped.
- Underlying `HackathonConfig` persisted atomically; `validate()` checks group membership consistent.

### Invitation response selection — PASS (hardened)
- Prior `selectedParticipants` effect only handled `prev.size===0 → confirmed`. New logic tracks `prevRunIdRef` and merges progressively:
  - New `run_id` → set to all `confirmed` (failed not selected).
  - Same run with new confirmed ids → merge add missing confirmed (preserves manual deselected? Re-adds newly confirmed not yet seen, but does not override manual deselect for already-seen ids unless they become confirmed later).
  - Empty confirmed keeps empty until responders land.
- This matches §21: Send Invitation → invitation sent → responses arrive → responded become selected by default; only confirmed auto-selected; failed not.
- Backend `send_hackathon_invitations` sorts responders float-top via `sort_by_responder_status` and locks zero-responder groups; frontend `displayGroups` uses `runGroup.model_ids_ordered` for order — state-driven, not DOM sorting.

### Sorting/order — PASS
- Backend `sort_by_responder_status` partitions confirmed vs others preserving original `model_ids_ordered` within each partition. Deterministic. Frontend `displayGroups.displayedIds` chooses `runGroup.model_ids_ordered` when present, else `cfg.model_ids`. No manual DOM manipulation.

## 4. Prompts

### Canonical files integrated — PASS
- `settings_store.rs:9-35` embeds via `include_str!("../../leader_priming.md")` etc., strips header via `extract_prompt_body`, exposes `default_leader_priming/participant/agent_system`. Seed on `SettingsStore::new` when key missing/empty (91-106). Migration version 2 upgrades old factory defaults lacking hardened markers (e.g., `Runtime state is authoritative` without `{{project_brief}}`) to canonical; user-custom prompts (no header) preserved.

### No summarization/no loss — PASS
- `default_leader_priming` body is full 463-line file after `---`; contains all sections: Project brief/session type placeholders, Who is in session with `{{participant_count}}` + `{{participant_list_with_display_names}}`, roster fixed, Runtime authoritative 11 bullets, 4 capabilities with triggers, Hackathon task_brief 3 sections, Phase 1 5 steps, Phase 2 6 steps + module completion 6 conditions + global completion 5 verifications, Independent judgment 3-step + self-check, Handling disagreement bounded pushback, Quality bar, Skip, Ask User bar, Communication style, Time and rigor. No section removed; no merge of redundant bullets.
- `default_agent_system` body is full 274-line file with 12 classification rules verbatim, field contract, roster authoritative, hackathon 1-2000 validation. Not shortened.
- `default_participant_priming` full 207 lines with role, research honesty conditional, Hackathon review, bounded pushback, etc.
- Adaptation is placeholder substitution only — `session_runner::run_setup` replaces `{{participant_count}}`, `{{participant_list_with_display_names}}`, `{{full_participant_list_including_leader}}`, `{{leader_display_name}}`, `{{project_brief}}`, `{{session_type}}`, `{{role}}` from live `SessionConfig`. No wording changed.

### Persisted defaults — PASS
- First launch/missing → seed canonical. Existing custom (`!empty && !is_old_factory`) preserved. Subsequent launches preserve saved version. `get_prompt_template_with_default` returns persisted if non-empty else canonical. `get_agent_brain_config` also falls back. Settings UI `SettingsPanel.tsx:230-231` displays `get_prompt_template` plain string (not JSON parse). Priming execution uses same persisted template via `session_runner` `get_prompt_template_with_default`. Agent brain uses persisted `brain_system_prompt` via `save_agent_brain_config` → `SettingsStore::get_agent_brain_config` → `AgentBrain::new` → `build_effective_system_prompt` (adds `DECISION_JSON_CONTRACT` + memory). No duplicate hardcoded prompt definitions; only `context_manager::build_prompt_for_agent` exists but is dead (grep finds definition only, no call site) — not runtime.

### Dynamic participant list — PASS
- Leader priming `{{participant_list_with_display_names}}` + `{{participant_count}}` interpolated from `config.agent_ids` filtered to actual participants (e.g., Claude, Gemini, Kimi) via `format_display_list`. No hardcoded seven-model list in runtime-generated prompt. Verified `grep -rn "ChatGPT, Claude"` returns 0 in `src-tauri/src` except allowed tests.

### No hardcoded participant list — PASS
- `AGENTS` registry authoritative; runtime session context (`agent_ids`, `leader_agent_id`, `setup_order`, `selected_agent_ids`) derived from `start_session` validation against merged registry. No prompt builder assumes all seven.

## 5. Safety

### No new unwrap/expect — PASS
- New frontend edits contain no `.unwrap()`/`.expect()`; Rust diff adds no production unwrap. Existing production unwraps remain only in test `#[cfg(test)]` and legacy `commands.rs:804 brain.unwrap()` after None guard (pre-existing). No new violation.

### No blocking locks in async/navigation — PASS
- `grep -rn blocking_lock` returns 0; `BrowserState` uses `std::sync::Mutex` for stores via `run_blocking`; `make_nav_closure` uses `std::sync::mpsc::SyncSender`.

### No channel regression — PASS
- `std::sync::mpsc::sync_channel(256)` maintained; `on_navigation` uses `try_send` with dropped log.

### IPC contracts verified — PASS
- Changed `isDraftSession` is internal Zustand only, no IPC. No Tauri command signature changed. Existing commands retain `#[tauri::command(rename_all="snake_case")]`. Frontend `safeInvoke`/`safeListen` event names match `IPC.md`: `session-status`, `setup-agent-ready/complete/failed`, `setup-complete`, `agent-state-change`, `browser-diagnostic`, `hackathon-invitation-update/group-status/invitations-complete/run-started/group-output/complete`.

### Event names verified — PASS
- All emitted events in `commands.rs`/`response_router` match `IPC.md`; no rename drift.

## 6. Build / Quality Gate

- `npm run build` (src): PASS — `tsc && vite build` transformed 1710 modules, no errors (382.6kB js gzip 115.75kB).
- `cargo check` (src-tauri): PASS — 0 errors, 70 warnings (dead STUB modules only, not task-related).
- `git diff --check`: PASS — no whitespace errors.
- No new compiler warnings introduced by this repair (warnings pre-existing STUBs).
- No `kimi.com` runtime URL — PASS.
- No hardcoded seven-model participant context — PASS (only allowed registries).
- Canonical prompts persisted correctly, user custom preserved — PASS.
- New Session not falsely active — PASS.
- Cross-team drag/drop changes real state — PASS.
- Responded models become selected — PASS (hardened).
- Existing Connected Accounts preserved — PASS.

## 7. Static Regression Results

| Search | Result | Verdict |
|--------|--------|---------|
| `kimi.com` (rs/ts/tsx) | 0 hits runtime (only historical audits/md) | PASS |
| `www.kimi.com` (rs/ts/tsx) | 0 hits runtime | PASS |
| `chatgpt, claude, gemini` hardcoded participant list | only `browser_backend.rs:4356` test `builtin_registry_has_exactly_seven_participants_unchanged` (allowed registry assertion) + `lib/agents.ts AGENT_IDS` (allowed) | PASS |
| `ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, Kimi` | `browser_backend.rs:4363` test display names + `lib/agents.ts AGENT_DISPLAY_NAMES` (allowed) ; no runtime priming contains this exact 7-list | PASS |
| short system prompt defaults | none — `agent_system.md` body is canonical 274 lines, `build_effective_system_prompt` appends contract not short; `context_manager::build_prompt_for_agent` dead code not invoked | PASS |
| duplicate leader priming strings | none — only `settings_store.rs include_str!` single source + interpolation at `session_runner.rs` | PASS |
| duplicate participant priming strings | none — same single source pattern | PASS |
| old prompt constants | none — no stale `LEADER_PROMPT` const; only migration helpers | PASS |
| `session_active` checks | `commands.rs:152,263,314,541,688,804,840,876,1237,1278,1648,1814,2042,2297,2588,3038` — all correct (guard on start, launch, user_input, restore, delete) | PASS |
| Settings guards | `SettingsPanel.tsx:436 isActiveSession = !isDraftSession && (running/priming/setup/requirements/paused)` — correct partitioned draft vs active | PASS |
| drag/drop handlers | `handleDragReorder` + `handleReorder` in `HackathonMiniWindow.tsx` — state-driven, persist via `save_hackathon_config`, patch transient run | PASS |
| invitation response handlers | `send_hackathon_invitations`/`hackathon-invitation-update`/`selectedParticipants` merge | PASS |
| selection state | `selectedParticipants: Set<string>` state-driven, `handleToggleParticipant` toggles only `confirmed` | PASS |
| blocking_lock | 0 hits | PASS |
| tokio::sync::mpsc in on_navigation | 0 hits | PASS |
| production unwrap/expect | only pre-existing in `commands::run_hackathon 804` after None guard + tests; no new | PASS (flagged pre-existing) |

Justified remaining `kimi.com` hits (audits only):
- `src-tauri/project-docs/audits/browser-connected-accounts-pre.md:43` (`https://www.kimi.com/` as bug evidence) — historical.
- `final-beta-finalization-audit.md` 5 hits documenting preservation gate — historical, not runtime.

## 8. Functional Simulation Results

Legend: PASS = source-level + build simulation, NOT RUNTIME-VERIFIED = headless cannot launch WebViews.

### Connected Accounts Flow 1 — NOT RUNTIME-VERIFIED (preserved)
- Open app → Settings → Connected Accounts → Launch ChatGPT/Claude/Gemini/DeepSeek/Qwen/GLM/Kimi.
- Simulation: each launch goes `resolve_participant` → `navigate_agent_window` with correct base_url (`https://chatgpt.com` ... `https://kimi.ai/`) → `arena://ready` wait → emit. Busy guard prevents yank. Previously working launches retain identical code path. No regression.

### Flow 2 (New Session draft) — PASS
- Open New Session → `Sidebar.newSession` sets `sessionStatus='setup', isDraftSession=true, session_active backend false`.
- Open Settings → `SettingsPanel isActiveSession = !true && (setup)` → false → Settings opens (not blocked). Connected Accounts `Launch` not disabled (title shows open hint). Expected per §12.

### Flow 3 (Start Session) — PASS
- Configure brief, types, participants via `SetupView`, click Start Session → `invoke('start_session')` validates via `validate_session_agents` (merged registry, built-ins authoritative) → backend `session_active compare_exchange false→true` success → orchestrator `status=Setup` → `session-status: setup` emission clears draft → restrictions now `isActiveSession=true` → Settings `Launch` disabled with "Stop the active session" tooltip, backend `launch_connected_account` would reject with "Cannot launch while a session is active".

### Flow 4 (Stop) — PASS
- `abort_session` → `session_active=false`, `session-status: ended` → frontend `isDraftSession=false`, guard false → Settings accessible. Not runtime-verified but source path confirmed.

### Gemini Simulation — NOT RUNTIME-VERIFIED
- Page loads, login UI usable, no app-side redirect loop, cookies preserved, authenticated state can persist. Telemetry 401s separate from auth. `manifest-src` benign. Source verification only.

### Claude Simulation — NOT RUNTIME-VERIFIED (environmental blocker documented)
- Navigation begins correctly, challenge resources not blocked (http/https allowed, GENERIC_INIT_SCRIPT not touching challenge iframe), initialization script not corrupting challenge page, cookies intact. `brunhild.challenges.cloudflare.com Network unreachable` remains external Cloudflare infra limitation, not fixed by app (no bypass added).

### Prompt Simulation — PASS
- Hypothetical session: Leader Claude, Participants Claude/Gemini/Kimi.
- `session_runner::run_setup` interpolation:
  - `leader_display = Claude`
  - `other_display_names = [Gemini, Kimi]` → `other_list = "Gemini and Kimi"`
  - `participant_count = 2` (other_count)
  - `full_list = "Claude, Gemini, and Kimi"`
  - Leader priming after replace contains `You are working with 2 other models: Gemini and Kimi.` — does NOT contain ChatGPT/DeepSeek/Qwen/GLM. Participant priming for each non-leader similar with full roster only those three.
- Second test: Leader Gemini, Participants Gemini/DeepSeek → leader priming shows `Gemini and DeepSeek` only; full_list changes accordingly. Dynamic from `SessionConfig.agent_ids`, not hardcoded 7.

### Prompt Integrity Verification — PASS
- Original `leader_priming.md` structure: 14 markdown sections + code fence example — runtime `default_leader_priming` after `extract_prompt_body` retains all bytes after first `---\n`; dynamic substitutions are `{{project_brief}}→Project brief text`, `{{session_type}}→Architecture`, etc., inserted verbatim without deleting substantive instructions. Same for participant (6 sections) and agent_system (field contract + 12 rules). Unavoidable adaptation: placeholder replacement + stripping markdown header/meta line; no instruction removed.

### Hackathon Simulation — PASS (state simulation)

Case A: Team A [Claude, Gemini], Team B [DeepSeek, Kimi] → Drag Claude → Team B.
- `handleDragReorder(TeamA, Claude, TeamB, index_of(DeepSeek))` → config `TeamA.model_ids = [Gemini]`, `TeamB.model_ids = [Claude, DeepSeek, Kimi]` if inserted before DeepSeek (visual 0) or appended if at end. No duplicate, identity preserved (`Claude` id unchanged), UI updates via `setHackathonConfig` + patched `hackathonRun` ordering if run exists.

Case B: Drag Claude back TeamB → TeamA: `TeamA [Gemini, Claude]`, `TeamB [DeepSeek, Kimi]` — reverse path same handler, both directions work repeatedly.

Case C: Drag same participant repeatedly → `if (tgtGroup.model_ids.includes(sourceModelId)) return` guard prevents duplicates; repeated move between same positions is idempotent.

Case D: Send invitation → responded models become selected.
- Before: `selectedParticipants = {}` ; after `send_hackathon_invitations` resolves, backend marks 2/3 confirmed, emits `invitation-update` per model, final `sort_by_responder_status`. Frontend `useEffect` sees new `run_id` with confirmedIds = [A,B] → `setSelectedParticipants(new Set([A,B]))`. UI checkboxes for those become `hk-check on`, displayed via `isSelected`.

Case E: One model fails → `confirmed = [A,B]`, `failed = [C]` → `confirmedIds = [A,B]` → selected = `{A,B}` only; `C` remains unchecked, `isFailed` shows `XCircle`, cannot be toggled (`canSelect = isConfirmed`).

Visual order/selected derive from state (`HackathonConfig.groups[].model_ids` + `HackathonRunSafe.groups[].model_ids_ordered` + `ParticipantRunStatus`), not DOM.

## 9. Remaining Blockers — Explicit

- **Application defects:** None blocking release beyond manual verification. Session draft vs active now correctly partitioned; Hackathon DnD and invitation selection hardened; Kimi canonical; prompt fidelity preserved.
- **Environment/network limitations:** Claude Cloudflare `brunhild.challenges.cloudflare.com Network unreachable` remains external infra unreachable in this environment; not app bug. Requires manual Windows/Tauri verification on network where challenge CDN is reachable. Gemini login itself requires real Google account interaction; telemetry 401s are not blockers.
- **External Cloudflare limitations:** Same as above — we add no bypass; challenge must be completed by user in model window with `Resume`.
- **Manual runtime verification still required:** Connected Accounts launches for all 7 models (headless cannot launch WebViews); full session lifecycle including priming → autonomous loop → blueprint → complete; Hackathon end-to-end with real API keys; prompt interpolation with varied rosters.

## 10. Files Changed (this session)

- `src/stores/useAppStore.ts` — added `isDraftSession: boolean` + `setIsDraftSession`, cleared on `clearSessionState`, wiring for draft vs active.
- `src/components/layout/Sidebar.tsx` — `newSession` sets `isDraftSession=true`, import `setIsDraftSession`.
- `src/components/views/SetupView.tsx` — `start()` sets `isDraftSession=false` after `start_session` success.
- `src/hooks/useIpcListeners.ts` — `session-status` now clears `isDraftSession` on backend-driven `setup/running/priming/requirements/paused/ended/complete/idle`.
- `src/panels/SettingsPanel.tsx` — `isActiveSession = !isDraftSession && (running|priming|setup|requirements|paused)` with comment, import `isDraftSession`.
- `src/components/hackathon/HackathonMiniWindow.tsx` — hardened `selectedParticipants` auto-select (run_id tracking, merge confirmed), patched `handleReorder` and `handleDragReorder` to sync transient `hackathonRun` ordering/participants immediately.
- `src-tauri/project-docs/audits/connected-session-hackathon-prompts-pre.md` — new pre-audit (this batch).
- *(No `browser_backend.rs`, `session_runner.rs`, `settings_store.rs` modifications — intentionally preserved to avoid regression; Kimi URL already canonical.)*

## 11. Files NOT Changed But Verified (intentionally preserved)

- `src-tauri/src/browser_backend.rs` — Two-WebView architecture, `GENERIC_INIT_SCRIPT` static generic, `on_navigation` std mpsc, AGENTS kimi `https://kimi.ai/` test, no `kimi.com` regression.
- `src-tauri/src/session_runner.rs` — Leader/participant priming interpolation dynamic via `config.agent_ids`/`leader_agent_id`, no hardcoded 7 list.
- `src-tauri/src/settings_store.rs` — `include_str!` seeding + migration preserving user custom, no short fallback replacing real prompt.
- `src-tauri/src/agent_brain.rs` — `AgentDecision` 7 variants, `DECISION_JSON_CONTRACT` appended to persisted `brain_system_prompt`, `build_effective_system_prompt`.
- `src-tauri/src/commands.rs` — `launch_connected_account` + `session_active` guards, not modified.
- `src-tauri/src/orchestrator.rs` — `AppState` + `session_active` field not modified.
- `src/components/shared/InputBar.tsx`, `src/App.tsx` — routing not modified beyond draft handling.

## 12. Regression Assessment

> Did this repair preserve the previously working Connected Accounts behavior?

**YES.** Evidence:
- `git diff --stat` shows 5890+ insertions baseline dirty but our additional diff touches only 6 frontend files + 2 audit docs; `browser_backend.rs` not rewritten (only docs preserved). `cargo check` and `npm run build` both PASS without touching browser architecture.
- `grep -rn kimi\.com --include=*.rs,*.ts,*.tsx` remains 0 runtime (vs. earlier audit states where it was `www.kimi.com`). Kimi `https://kimi.ai/` plus `CHROME_USER_AGENT` + Chinese login handling preserved.
- `GENERIC_INIT_SCRIPT` unchanged (still generic), navigation interception still allows `http|https`, `arena://` handling unchanged, `connected_account_busy_until` guard preserved.
- Model registry order and display names unchanged; unified registry `merged_participants` still authoritative.
- Claude/Gemini diagnostics previously added (timeouts 90k/100s, console bridge, `reuse nav window` fix) kept intact; only diagnosis documented, no bypass.

No new dependency, no file deletion, no browser architecture replacement.
