# Pre-Implementation Audit — Multi-Feature Controlled Session

Date: 2026-09-07
Scope: Kimi URL, Prompt system, Dynamic participants, Hackathon teams, Window mockups
Verification: direct source read (no doc trust), git status dirty verified

---

## A. Kimi URL Audit

Canonical required: `https://kimi.ai/`

### Active runtime occurrences (must change)

| # | File | Line | Current value | Purpose | Must change |
|---|------|------|---------------|---------|-------------|
| 1 | `src-tauri/src/browser_backend.rs` | 2728 | `base_url: "https://www.kimi.com/"` | AGENTS registry — sole navigation source for participant window. Authoritative. All navigation, readiness probing, display wiring uses this. | YES — to `https://kimi.ai/` |
| 2 | `src-tauri/src/browser_backend.rs` | 2724 comment | `// D-042: Kimi via Kimi.com` | Comment documenting registry entry | YES — update comment to kimi.ai |
| 3 | `src-tauri/src/browser_backend.rs` | 4376 | `"https://www.kimi.com/"` in test `builtin_registry_has_exactly_seven_participants_unchanged` | Test asserts registry URLs exactly including www.kimi.com; will fail after change if not updated | YES |
| 4 | `src/stores/useAppStore.ts` | 205 | `{ agent_id: 'kimi', base_url: 'https://www.kimi.com/' }` | Frontend offline default registry (used before loadParticipants completes, and as fallback). Must match backend or UI shows stale host before Tauri load. | YES — to `https://kimi.ai/` |
| 5 | `src-tauri/project-docs/ARCHITECTURE.md` | 377 | `| kimi | Kimi | https://www.kimi.com/ |` | Docs table — stale if not updated, could be copied into future impl | YES |
| 6 | `src-tauri/project-docs/BACKEND.md` | 602 & 606 | Same table + conversation URL `https://www.kimi.com/chat/{uuid}` | Same — doc drift risk | YES |
| 7 | `src-tauri/project-docs/CLAUDE-PROJECT-INSTRUCTIONS.md` | 312 | same | same | YES (non-production but listed) |
| 8 | `src-tauri/project-docs/DECISIONS.md` | 288 | D-042 base_url line | Historical decision record — preserve history but add note that canonical changed; leave as historical context? For safety, update comment to note superseded, but keep file as audit history? Resolution: update to kimi.ai with note "superseded 2026-09-07" or leave historical? Pre-audit marks as informational; post-audit must justify if left. For now mark as doc — should be updated to avoid future copy-paste. | YES (doc consistency) |

### Non-runtime / historical occurrences (justify if left)

| File | Line | Value | Notes |
|------|------|-------|-------|
| `src-tauri/project-docs/audits/browser-connected-accounts-pre.md` | 43,55,208,452 | `https://www.kimi.com/` | Pre-audit evidence of bug — historical, must remain as evidence. Justified left. |
| `src-tauri/project-docs/audits/browser-connected-accounts-post.md` | 134,138 | `https://www.kimi.com/` | Post fix description of before-state — historical. Justified left. |
| `src-tauri/project-docs/audits/final-beta-finalization-audit.md` | 29,39,82,177,313 | `https://www.kimi.com/` | Final beta preservation gate docs — historical record of preservation decision. Must remain but annotate supersession. |
| `src-tauri/project-docs/audits/hackathon-reliability-batch-pre/post.md` | etc | `https://www.kimi.com/` | Historical P2 revert record. Justified. |
| `BETA_RELEASE_COMPREHENSIVE_AUDIT.md` | 396 | mentions kimi.ai vs www.kimi.com | Audit note — historical. |
| `src-tauri/src/browser_harness.rs` | 1490-1500 | test uses `"kimi"` agent_id but URL `https://example.com/chat?token=SECRET` — no kimi.com hardcoded remaining after prior fix. Verified clean. |

### Kimi persistence risks

- No duplicate conflicting constants found: AGENTS is single authoritative array in browser_backend.rs; frontend has mirrored literal in useAppStore.ts (two locations — must keep both in sync). No fallback/default path restoring .com beyond these two. No initialization code rewriting URL.
- Search result: `grep -RniE "kimi\.com|www\.kimi\.com" src src-tauri/src` returns exactly 2 active runtime hits (browser_backend.rs:2728,4376 and useAppStore.ts:205). No hidden third registry. `grep -Rni "kimi.ai"` returns 0 currently, confirming bug.
- Centralization recommendation: AGENTS registry is already centralized; frontend mirrors it via loadParticipants (get_participants) after mount, but offline default in useAppStore still holds literal — must be updated to kimi.ai to survive future sessions. Also consider exposing KIMI_BASE_URL const but smallest coherent fix is to update both literals to same canonical.

---

## B. Prompt System Audit

### Storage & ownership

| Layer | File | Keys | Current behavior |
|-------|------|------|------------------|
| DB schema | `src-tauri/src/settings_store.rs` | `brain_system_prompt`, `prompt_leader_priming`, `prompt_participant_priming` + embedded defaults via `include_str!("../../leader_priming.md")` | SettingsStore::new seeds hardened beta prompts on first open; get_prompt_template_with_default falls back to include_str if DB empty; migration version 1 upgrades old factory prompts (header without hardened marker) to new hardened ones. Single authoritative source: repo root *.md files. |
| Commands | `src-tauri/src/commands.rs` | `save_prompt_template(template_name,content)` maps to keys, `get_prompt_template(template_name)` plain string (no JSON parse) | Correct IPC mapping; get_prompt_template returns raw string. |
| UI — Settings | `src/panels/SettingsPanel.tsx` | Leader/Participant template textareas + Save calls `save_prompt_template` | Loads via `get_prompt_template` plain string; saves raw. |
| UI — Setup | `src/components/views/SetupView.tsx` | Agent Brain collapsed fields | Saves via `save_agent_brain_config` (api_key/base_url/model/system_prompt) — does NOT touch priming keys (correct). |
| Session setup | `src-tauri/src/session_runner.rs` lines 511-667 | Loads templates via `get_prompt_template_with_default("prompt_leader_priming")` fallback to `default_leader_priming()` | Performs placeholder replacement then injection via eval script. |

### Placeholder mechanism

Existing placeholders (session_runner.rs:640-655):

- Leader template replacements:
  - `{{participant_count}}` → `other_count` = `agent_ids.len() - 1` (other models count)
  - `{{participant_list_with_display_names}}` → `other_list` = formatted display names of non-leader participants
  - `{{leader_display_name}}` → leader display name
  - `{{full_participant_list_including_leader}}` → full formatted list
  - `{{role}}` → Leader/Critic/etc.

- Participant template replacements:
  - `{{leader_display_name}}` → leader
  - `{{participant_count}}` → total count
  - `{{full_participant_list_including_leader}}` → full list
  - `{{participant_list_with_display_names}}` → other list (same as leader but defined)
  - `{{role}}` → per-participant role

Missing dynamic variables (per Requirement 9):
- `{{project_brief}}` — not currently templated; project context comes via active-turn first prompt (`Consensus Arena active turn 1 ... Project brief:`) and via ContextManager history, not via priming template. Task requires at minimum support for project brief, session type, leader, selected participants, participant identity, panel membership. Currently session_type not injected into priming either (only via SessionType enum for context). Need to add `{{project_brief}}` and `{{session_type}}` placeholders (safe, no dependency).
- `{{session_type}}` — not present.

Both are trivial string replacements consistent with existing architecture (no templating dependency). Current templates (leader_priming.md, participant_priming.md) are already production-grade: 458 lines leader, 204 lines participant, 274 lines agent_system. They are NOT brief — they are hardened beta prompts already covering all Requirement 2/3 bullets (orchestration role, 6 decisions, JSON contract, dynamic routing, AskUser gate, Hackathon 6-condition trigger, etc.). No giant hardcoding needed; existing file-based architecture is correct. Only addition needed is two placeholders + filling.

### Overwrite risk

- No later hardcoded override detected that would overwrite with shorter version. `session_runner.rs:657-667` fallback generic prompt only triggers if priming_raw.trim().is_empty() after replacement — correctly fallback not override. Settings migration preserves user customization via `is_old_leader_factory` check (only migrates old factory headers lacking hardened markers). User-edited prompts remain.

### Prompt file locations

- `agent_system.md` — embedded via include_str, seeded into DB, controls brain decide() via AgentBrain.system_prompt + DECISION_JSON_CONTRACT suffix. Explains all 7 actions with strict field contracts, roster authoritative, cycle caps, hackathon trigger, etc.
- `leader_priming.md` — 458 lines, explains leader owns meeting, fixed roster, runtime state authoritative, 4 capabilities (Route, RouteCompare, AskUser, Hackathon), Phase 1/2 module loop, independent judgment, disagreement handling, quality bar, when to skip discussion, AskUser bar, communication style.
- `participant_priming.md` — 204 lines, establishes participant identity, leader identity, panel membership, review expectations, research honesty conditional on tool availability, Hackathon review, bounded pushback.

All three already satisfy Requirements 8A/B/C content lists. No hardcoded short prompts found.

---

## C. Dynamic Participant Audit

Trace:

```
SetupView selected:Set<string> (default chatgpt,claude,deepseek) → toggle()
  ↓
start() builds agent_ids = participants.map(p=>p.agent_id).filter(id=>selected.has(id)), leader = claude etc., setupOrder=[leader,...ids filtered]
  ↓
invoke start_session {project_brief, session_type, agent_ids, leader_agent_id}
  ↓
commands.rs start_session validates via validate_session_agents against MERGED registry (built-ins + custom), builds SessionConfig {session_id, project_brief, session_type, agent_ids, leader_agent_id}
  ↓
session_runner.rs run_setup iterates setup_order (leader first) — PRIMING
  - For each agent_id resolves via resolve_participant(agent_id,custom) (merged)
  - Builds priming_raw via template + replacement using other_count/other_list/full_list/leader_display
  - Injects via eval, waits for send detection
  ↓
response_router.rs run_agent_loop
  - Builds context string with config.agent_ids.join(", ") and leader_id (dynamic, not all-seven)
  - Calls brain.decide(leader_response, context, memory_context) where context is runtime roster authoritative
  - RouteCompare/Route validated via resolve_selected_agent_id against config.agent_ids (only selected)
```

Hardcoded participant/model list checks:

- `grep -RniE "chatgpt.*claude.*gemini.*deepseek"` — only in AGENTS registry definition (7 entries) — correct as supported registry.
- `grep -Rni "all seven"` — no hardcoded "all seven are participants" logic found in session_runner or response_router. The only place with hard assumption is DECISION_JSON_CONTRACT example list, which correctly says `example: deepseek` and agent_system.md example list is marked "example — runtime Context roster is authoritative". Both are safe.
- `grep -Rni "AGENTS"` — registry definition correctly lists 7, but usage in setup is via selected agent_ids, not AGENTS enumeration.
- Session config validation correctly requires leader in agent_ids, at least 2.

Conclusion: Dynamic participant flow already correct for current session selection. Leader priming already generated from current session's selected IDs (other_count/other_list). No hardcoded all-seven participant list bug currently present. Tested conceptually:

- Session A (claude leader, claude+gemini): other_list = "Gemini", participant_count=1, full_list="Claude and Gemini" — correct, no deepseek/kimi/qwen/glm/chatgpt leaked.
- Session B (claude+gemini+deepseek+kimi): other_list = "Gemini, DeepSeek, and Kimi" — correct.
- Session C leader switch verified via leader_display replacement.

Remaining gap: session_type and project_brief not yet dynamic placeholders (see B).

---

## D. Hackathon Configuration Audit

Trace:

```
SetupView HackathonMode toggle → save_hackathon_config(enabled) → persists groups/models/max_questions/enabled
  ↓
HackathonMiniWindow loadConfig() on open via get_hackathon_config + get_hackathon_run_state
  ↓
team data model:
  HackathonConfig { groups: GroupConfig{id,name,model_ids,selected}, models: ModelConfig{id,model_name,base_url,api_key,group_id}, max_questions, enabled }
  ↓
model membership: model.group_id + group.model_ids array — dual source must stay consistent (validated in HackathonConfig::validate)
  ↓
drag/drop handlers: dragging {groupId,modelId,idx}, dragOver {groupId, idx}
  ↓
handleDragReorder(sourceGroupId,sourceIdx,targetGroupId,targetIdx)
  - if cross-team: remove modelId from source model_ids, splice into target at clamped idx, update model's group_id, persist via save_hackathon_config
  - if same-team: splice within array, persist
  ↓
persist() → invoke save_hackathon_config {config} → SettingsStore::save_hackathon_config (JSON) → reload via get_hackathon_config
  ↓
Send Invitation → invoke send_hackathon_invitations → backend fan-out health-check (15s timeout) per model, updates ParticipantRunStatus pending→confirmed/failed, emits hackathon-invitation-update, then hackathon-group-status + hackathon-invitations-complete, sorts responders float top via sort_by_responder_status
  ↓
team card rendering: displayGroups maps groups → participants = models.filter(group_id) but iterates cfg.model_ids for order — NOT run order (see ordering gap below)
```

Data structures: canonical state is HackathonConfig persisted under key `hackathon_config` in settings.db; transient run state HackathonRunState held in memory (hackathon_run Arc<Mutex>). No parallel team-management system exists — single canonical flow.

State management mechanism: React Zustand store hackathonConfig/hackathonRun + local useState for dragOver/dragging + persist() round-trip.

Drag/drop implementation audit:

- Existing: HTML5 draggable on GripVertical handle (`draggable` true), onDragStart sets dataTransfer, sets dragging state; onDragOver prevents default and sets dragOver, onDrop calls handleDragReorder. Cross-team logic exists (sourceGroupId !== targetGroupId branch). Same-team branch handles deterministic splice (no duplicate). Invalid drops safely no-op (missing group/idx guard). React state/store updates via persist reload.
- Gaps to verify: target team identifiable? Yes via cfg.id of drop row. Cross-team move updates canonical state? Yes (model.group_id + group.model_ids). Duplicate prevention? Yes (filter + splice). Stale visual? No — persist reloads full config from DB and setHackathonConfig. Persisted config uses updated membership? Yes via save_hackathon_config. Invalid drops no-op? Yes guards. Same-team no duplicate? Yes (idx check). However dragging onto another participant inside a team: currently inserts at that participant's index (targetIdx = idx) — deterministic but shifts after removal accounting? Cross-team case clamps targetIdx, same-team splices after removal — correct. HTML5/native pointer should be used — it is (draggable + drag events), not needing extra dependency.
- Important: The current implementation already has `draggable` on handle only, but container row has onDragOver/onDrop — works. Need to verify target team drop when dragging over empty team (no models) — there is a separate bottom drop zone at end of list (height 10px) but only when dragging.groupId === cfg.id — limited to same-group drop at end; cross-team drop onto empty team's bottom zone not covered if empty. Gap: empty team has no rows, so no drop target except the small end-zone which is conditioned on dragging.groupId === cfg.id (same group). For cross-team into empty, no drop target would fire. Should add universal empty-team drop zone.

Invitation/response-state audit:

- Authoritative response state: HackathonRunState.groups[].participants[].status (pending/confirmed/failed) + consultation_count, plus GroupRunStatus (Pending/Running/Completed/Locked). Determined when each model's health-check call returns (15s timeout, all in parallel join_set). Confirmed if API call success, failed otherwise. Sorting after all invitations via sort_by_responder_status: confirmed float top preserving order, zero responders → locked.
- Frontend sorting handling: useIpcListeners hackathon-invitation-update updates participant status in store but comment "Re-sort responders float top is handled by backend; here we could re-sort locally but rely on next group-status". Later hackathon-group-status and invitations-complete refetch full run state (which is sorted). So eventual order after all invitations is sorted, but incremental per-model update does not reorder until group-status / invitations-complete. Could cause temporary unsorted but eventual sorted.
- Ordering gap (Requirement 4): team card/model list must update ordering after Send Invitation to show responded at top. Current rendering uses cfg.model_ids order, not run model_ids_ordered. So even after sorting run state, the visual order won't reflect it because it maps cfg.model_ids (config order) not runGroup.model_ids_ordered (sorted). Need to fix displayGroups to use runGroup's sorted order when run exists, falling back to cfg order.
- No mutation of canonical source unless intended — backend sorts run's model_ids_ordered only, not persisted config's groups model_ids (config remains original order). Is this intended? Requirement says responded float top preserving order — backend does that on run. But if user expects persisted membership order to also reorder, not needed; visual should reflect run order. Need to decide: requirement example before/after shows within same Team A after invitation responders move top — that is run order sorting, not persisted config reorder. So display fix is sufficient without mutating config.
- Async response updates reorder correctly? After sorting, reordered participants vec matches sorted_ids — stable ordering among same status preserved via sort_by_responder_status's stable partition.

---

## E. Window Mockup Audit

Actual user-visible application surfaces (desktop-oriented):

### Application UI views (React, mounted in App.tsx main-shell)

1. **Empty/New Session** — `EmptyView.tsx` (hello animation, suggestions, InputBar). Matches preview.html #v-empty.
2. **Setup** — `SetupView.tsx` (project brief, session type seg, participants pcards, leader select, Agent Brain collapsible, Hackathon Mode toggle + configure button, Start button). Matches preview.html #v-setup.
3. **Priming** — `PrimingView.tsx` (Preparing your panel, prog bar, plist prows done/cur/wait, info-box). Matches preview.html #v-priming.
4. **Active Session** — `ActiveView.tsx` (blueprint sections, stline + sdrawer, InputBar Send→Stop). Matches preview.html #v-active.
5. **Settings panel** — `SettingsPanel.tsx` (sp drawer, sp-backdrop, Connected Accounts, Agent Brain / Fallback / Secondary, System Prompts, Appearance themes, About). Matches preview.html #sp.
6. **Sidebar** — `Sidebar.tsx` (sb-logo, sb-new, recover-card, sb-scroll si list, sb-foot connected dots, acct). Persistent, but part of shell.
7. **Topbar** — `Topbar.tsx` (tb-left with toggle + title, tb-right badge/download). Shared across views.

### Overlays / Modals (mounted at App root)

8. **AskUser** — `AskUserPopup.tsx` (ov ask, question, options 2-4, allow_custom free-text + Send, Escape/backdrop → "Cancelled"). Matches preview.html #ov-askuser.
9. **CAPTCHA/Verification overlay** — `CaptchaOverlay.tsx` (ov captcha, verification required, Resume). Matches preview.html #ov-captcha.
10. **Rate-limit overlay** — `RateLimitOverlay.tsx` (ov ratelimit, hit rate limit, 4 options). Matches preview.html #ov-ratelimit.
11. **Debug panel** — `DebugPanel.tsx` (dev only, Ctrl+Shift+D, circular 200 buffer, tag filter). Not in preview.html screenshot bar but exists.
12. **Hackathon mini-window** — `HackathonMiniWindow.tsx` (hk-modal overlay, groups columns, hk-row drag handle, reorder arrows, three-dot menu, hk-popup add model / new team, footer Cancel/Send invitations/Go, rounds control). Standalone preview at `src-tauri/project-docs/mockup/hackathon-mini-window.html` already exists as separate mockup, but must be verified complete vs production component.
13. **Toast** — `Toast.tsx` (bottom centered, 9100). Matches preview.html .toast.

### Tauri desktop-level windows vs WebViews vs overlays

- **Application UI window**: single Tauri WebViewWindow hosting React app (main window). All above are views/overlays within it, not separate OS windows.
- **Model WebViews**: maximum two WebViews — leader window (persistent) + shared nav window (navigating). These are Tauri WebViewWindows managed by browser_backend.rs, not React views. Not represented in preview.html mockups (they are external chat site loads).
- **Overlays/modals**: z-index stacked: toast 9100, rate-limit 9200, captcha 9300, askuser 9999, hackathon 9400, debug panel fixed. Settings panel uses backdrop + drawer at z-index 40.
- **Distinction**: Task's "window mockup" = each distinct UI surface/state the user sees in the main app window, plus dedicated dialogs. Model WebViews are external sites and not cloned (would be inaccurate).

### Existing mockup files

- `src-tauri/project-docs/mockup/preview.html` — canonical shell mockup covering Empty, Setup, Priming, Active, Settings, CAPTCHA, Rate-limit, AskUser, Toast, Sidebar, Topbar, themes Blue/Light/Dark, with real CSS tokens and JS for view switching. Provides visual ground truth.
- `src-tauri/project-docs/mockup/hackathon-mini-window.html` — standalone mockup for Hackathon configure modal (3 teams Falcon/Orbit/Vega, live/pending/dead states, hk-rounds, popups). Must verify fidelity vs current production component (e.g., drag handle, responded sorting, pending spinner, degraded locked).

### Missing / required mockups

Per audit, the app has at least 7 distinct React views/states plus 4 overlays plus Hackathon. Preview.html already composites them via view toggles, but Requirement 5 demands separate standalone HTML files per window in `mockup/windows/` with descriptive filenames, desktop-oriented 1440×1000, self-contained, no CDN dependency (or local fonts). Need to generate:

- empty.html (or app-shell with empty state)
- setup.html
- priming.html
- active.html
- settings.html (or settings-drawer.html)
- ask-user.html
- captcha-overlay.html
- rate-limit-overlay.html
- debug-panel.html
- hackathon-config.html (verify vs existing hackathon-mini-window.html — either promote existing or regenerate under windows/)
- Possibly topbar/sidebar variants implicit in each.

Existing preview.html already represents shell; should not be overwritten. New directory `src-tauri/project-docs/mockup/windows/` required per Requirement 14.

CSS/fonts: Must use local Inter/JetBrains Mono assets under public/fonts, not CDN. Preview.html currently uses CDN (fonts.googleapis.com, cdnjs lottie, unpkg lucide) — new mockups must be self-contained without external dependency, using local font references where practical.

---

## Plan Before Editing — Implementation Summary

1. Kimi URL files: `src-tauri/src/browser_backend.rs`, `src/stores/useAppStore.ts`, test expectation at same file, plus docs `src-tauri/project-docs/ARCHITECTURE.md`, `BACKEND.md`. Also ensure no stale fallback — verify via grep pre/post.
2. Prompt files: Add `{{project_brief}}` and `{{session_type}}` placeholders to `leader_priming.md` / `participant_priming.md` near top, and extend `session_runner.rs` placeholder replacements to fill them (plus verify participant template). No need to rewrite hardened content — preserve user customization.
3. Dynamic participant flow: Already correct; ensure new placeholders also dynamic per session; verify no hardcoded all-model branch.
4. Hackathon files: `src/components/hackathon/HackathonMiniWindow.tsx` — fix responders-float-top ordering (displayGroups sort via runGroup.model_ids_ordered), add robust cross-team empty drop handling, verify drag persistence correct.
5. Invitation ordering: same file fix ensures responded models move to top with stable ordering, async updates re-fetch run state sorts correctly.
6. Mockup files: create dir `src-tauri/project-docs/mockup/windows/` and generate standalone desktop HTMLs for each window discovered (empty, setup, priming, active, settings, ask-user, captcha, rate-limit, debug, hackathon-config). Clone actual UI faithfully, self-contained, no CDN.
7. IPC changes: none required for Kimi/prompt/hackathon (existing commands already snake_case correct). New placeholders need no IPC change.
8. Persistence changes: hackathon config persists via settings_store; Kimi change persists via code constant (no DB persistence for Kimi). Prompt new placeholders persisted via settings template string — no schema change.
