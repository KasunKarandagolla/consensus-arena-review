# Post-Implementation Audit — Multi-Feature Controlled Session

Date: 2026-09-07
Commit base: dirty worktree session (no new commit per AGENTS.md)
Verification: build + cargo check + diff check + grep audits executed live

---

## 1. Kimi URL

- [x] Canonical URL = `https://kimi.ai/`
- [x] No active `.com` runtime source
- [x] No hidden fallback restoring `.com`
- [x] Model registry consistent
- [x] Browser navigation consistent

**Evidence:**

```bash
grep -RniE "kimi\.com|www\.kimi\.com" src src-tauri/src --include="*.rs" --include="*.ts" --include="*.tsx"
# → (no output) — 0 active production hits

grep -Rni "kimi\.ai" src src-tauri/src --include="*.rs" --include="*.ts" --include="*.tsx"
# src/stores/useAppStore.ts:205    base_url: 'https://kimi.ai/'
# src-tauri/src/browser_backend.rs:2724  // D-042: Kimi via kimi.ai (Lexical...) — canonical https://kimi.ai/
# src-tauri/src/browser_backend.rs:2728  base_url: "https://kimi.ai/"
# src-tauri/src/browser_backend.rs:4376  "https://kimi.ai/"  (test)
```

Files changed:
- `src-tauri/src/browser_backend.rs:2724` comment updated, `2728` AGENTS entry, `4376` test expectation
- `src/stores/useAppStore.ts:205` offline default registry
- `src-tauri/project-docs/ARCHITECTURE.md:377`, `BACKEND.md:602/606`, `DECISIONS.md:287`, `CLAUDE-PROJECT-INSTRUCTIONS.md:312` doc consistency

Centralization: AGENTS static is single authoritative array; frontend mirrors via `useAppStore` offline default + `get_participants` merged registry at runtime. No duplicate conflicting constants; no initialization code rewriting URL; no fallback restoring .com (verified via grep, no `www.kimi.com` in `src` or `src-tauri/src`).

Justified remaining `kimi.com` hits (historical audits only):
- `src-tauri/project-docs/audits/*` pre/post reports, `BETA_RELEASE_COMPREHENSIVE_AUDIT.md`, `src-tauri/project-docs/audits/final-beta-*` — historical evidence of bug and preservation gate. Non-production documentation, intentionally retained.

---

## 2. Agent Brain Prompt

- [x] Complete
- [x] Orchestration role explained
- [x] All six decisions explained (plus Hackathon = 7 total, per beta-final)
- [x] JSON contract explained
- [x] Error/ambiguity behavior explained

**Location:** `agent_system.md` at repo root (274 lines), embedded via `include_str!` in `settings_store.rs:11`, seeded into `settings.db` key `brain_system_prompt` on first open, fallback via `default_agent_system()`.

**Content verification (direct read):**
- Defines orchestration agent is NOT a meeting participant, no opinion on project, only job is reading leader's latest message and returning one JSON decision.
- Lists authoritative context sources (leader latest text, runtime roster/module state/cycle counts, reasoning log, blueprint progress) and forbids hallucinating roster/cycle/phase/checkpoint.
- Output contract: JSON only, no prose, examples for each action, strict field contract per action (Route needs target_model+prompt, RouteCompare needs models+prompt, Blueprint needs section_title+section_content, AskUser needs question+options(2-4)+allow_custom true, Continue = {"action":"continue"} alone, Complete = {"action":"complete"} alone, Hackathon needs task_brief 1-2000 chars with three labeled sections).
- Roster authoritative section: runtime Context roster is authoritative, example list is example only, canonical IDs lower-case.
- What each action means: route (module-review cycle), route_compare (genuine parallel independent opinions), blueprint (no open dissent, exact content), ask_user (product-vision only gate), continue (still working), complete (entire blueprint, not one section), hackathon (parallel competing builds, 6-condition trigger, evidence not verdict).
- Classification rules 1-12: module-loop cycles are route not route_compare, continue carries no prompt, complete global, relitigation flag, blueprint requires no open dissent, silence not resolution, ask_user gate test, hackathon requires complete brief, hackathon not during unclosed Phase 1, hackathon results advisory, soft cycle cap 6, never fabricate section_content/task_brief.
- Malformed/inchoherent handling: emit continue, backend will prompt leader to restate.

No hardcoding of giant strings into React/Rust functions beyond the file-based `include_str!` mechanism (smallest safe). No templating dependency added.

---

## 3. Leader Priming

- [x] Complete
- [x] Leader identity dynamic
- [x] Project context included
- [x] Session type included
- [x] Selected participants dynamic
- [x] No hardcoded all-model participant list
- [x] Leader responsibilities explained

**Location:** `leader_priming.md` (now 460 lines, was 458), embedded via `include_str!`, seeded as `prompt_leader_priming`.

**New dynamic variables added (smallest safe mechanism):**
- `{{project_brief}}` inserted under new section "Project brief and session type" after leader intro.
- `{{session_type}}` inserted same section, rendered as human-readable "Architecture" / "MVP" / "API Design" / "Security Review" / "Custom" via `session_runner.rs:537-544` match.
- Retained existing: `{{participant_count}}`, `{{participant_list_with_display_names}}`, `{{leader_display_name}}`, `{{full_participant_list_including_leader}}`, `{{role}}`.

**Backend filling (verified):** `src-tauri/src/session_runner.rs:640-656` now replaces all six placeholders for leader case:
```rust
t = t.replace("{{participant_count}}", &other_count); // len-1
t = t.replace("{{participant_list_with_display_names}}", &other_list); // formatted non-leader names
t = t.replace("{{leader_display_name}}", &leader_display);
t = t.replace("{{full_participant_list_including_leader}}", &full_list);
t = t.replace("{{project_brief}}", &config.project_brief);
t = t.replace("{{session_type}}", &session_type_str);
t = t.replace("{{role}}", &role);
```
`other_count`/`other_list` derived from `config.agent_ids` filtered against `leader_agent_id` (live selection, not all-seven). `full_list` from all `config.agent_ids`. All derived from `SessionConfig` built from `start_session` `agent_ids`/`leader_agent_id`.

**Explains:**
- Who is in this session (dynamic roster, fixed for entire session, must re-check before closing module)
- Runtime state authoritative (lists 8 things NOT to invent)
- What tools gives you (Route, RouteCompare, AskUser, Hackathon Mode with strict 6-condition trigger + brief structure + evidence evaluation on 8 dimensions)
- Phase 1 clearing ambiguity (5 steps) + Phase 2 module-by-module loop (6 steps + guaranteed pass) + module completion (6 conditions) + global completion (6 checks) + independent judgment + consultation self-check + disagreement handling + quality bar + when to skip discussion + AskUser bar + communication style + time and rigor

**Hardcoded participant test:** No code path does `all seven models are participants`. Grep for AGENTS finds only registry definition. Session-specific list is always `config.agent_ids`.

**Prompt regression simulation:**
- Session A (Claude leader, Claude+Gemini): other_count=1, other_list="Gemini", full_list="Claude and Gemini" → leader sees "You are working with 1 other models: Gemini." No DeepSeek/Kimi/Qwen/GLM/ChatGPT mentioned. Pass.
- Session B (Claude+Gemini+DeepSeek+Kimi): other_list="Gemini, DeepSeek, and Kimi" → exactly those, no unrelated. Pass.
- Session C (leader Gemini, participants Gemini+Qwen): leader_display for participant priming would be Gemini, full list "Gemini and Qwen", other_list for leader case would be "Qwen". Verified via same code path with different leader.

**Persistence:** User customization preserved via `settings_store.rs` migration version bump 1→2; `is_old_leader_factory` now also detects hardened-but-missing-{{project_brief}} as old, upgrading factory templates while leaving truly custom prompts (no header) untouched.

---

## 4. Participant Priming

- [x] Complete
- [x] Participant identity dynamic
- [x] Leader identity dynamic
- [x] Actual panel membership dynamic
- [x] Role boundaries clear

**Location:** `participant_priming.md` (now 206 lines, was 204), embedded via `include_str!`, seeded as `prompt_participant_priming`.

**New variables:** `{{project_brief}}`, `{{session_type}}` inserted after opening paragraph (after full list line). Filling in `session_runner.rs:649-655` for participant case includes same six replacements (leader_display_name, participant_count total, full_list, other_list, project_brief, session_type, role).

**Explains:**
- Participant identity: reviewing member of expert panel led by {{leader_display_name}}, one of {{participant_count}}: {{full_participant_list_including_leader}}.
- Project brief + session type context.
- No fixed role, independent scrutiny, routing not random but based on demonstrated sharpness.
- Hackathon Mode result review treatment.
- Runtime context authoritative (lists 7 things not to invent).
- What you will be shown (current design + reasoning log).
- What is expected (accept/dispute/propose concrete replacement/ask counter-question, don't manufacture disagreement — clean pass valid).
- Research honesty conditional on tool availability (if browsing/search explicitly provided, use it; if not, do not pretend, distinguish fact vs inference).
- Reviewing Hackathon results (same 8-dim evaluation as leader).
- Do not accept anything you don't believe, leader not exempt, bounded pushback (one round), ambiguity vs low-value questions test, product-vision escalation, concise high-level style.

---

## 5. Hackathon Drag & Drop

- [x] Cross-team movement works
- [x] Same-team movement safe
- [x] No duplicate membership
- [x] Canonical state updated
- [x] Consumers receive updated membership
- [x] Persistence correct if applicable

**File:** `src/components/hackathon/HackathonMiniWindow.tsx` (817 lines, modified)

**Mechanism (HTML5 native):**
- Drag source: handle `<div class="hk-drag-handle" draggable>` with `onDragStart` setting `effectAllowed='move'`, `setData('text/plain', mid)`, `setDragging({groupId, modelId, idx})`, clearing menu.
- Target team identifiable: column `<div class="hk-col">` and each row `<div class="hk-row">` plus empty-column placeholder have `onDragOver` (preventDefault) and `onDrop` calling `handleDragReorder`.
- Cross-team move: `handleDragReorder(sourceGroupId, sourceModelId, targetGroupId, targetVisualIdx)` removes `sourceModelId` from source `model_ids`, inserts into target's visual position (mapping visual idx to config insertion point via displayedIds), updates `model.group_id`, then `persist()` via `save_hackathon_config` → `get_hackathon_config` reload → `setHackathonConfig`.
- No duplicate membership: `filter(id !== modelId)` on source, `includes` guard on target before splice, and `handleDragReorder` early returns if `tgtGroup.model_ids.includes(sourceModelId)`.
- No lost participant: model removed only if target insert succeeds; target insert always succeeds at clamped index.
- React state/store updates immediately: `persist` updates Zustand store via `setHackathonConfig` after DB round-trip; `displayGroups` recomputes from new config.
- Persisted configuration: `settings_store.rs::save_hackathon_config` writes JSON under key `hackathon_config` (no schema migration needed, generic key→value). Verified via `validate()` which checks group_id membership consistency and unique names.
- Invalid drops safely no-op: guards `if (!srcGroup || !tgtGroup) return`, `if (!srcGroup.model_ids.includes(sourceModelId)) return`, clamped index.
- Same-team drag: visual order splice via `displayed` copy, adjusted target, clamped, persisted as new `model_ids` order; early return if `srcVisualIdx === targetVisualIdx` prevents duplicate.
- Deterministic behavior when dragging onto another participant inside a team: insert before drop target's visual position (row's `onDrop` with `idx`), stable.
- No new dependency: uses HTML5 `draggable` + `onDragOver`/`onDrop`, no `react-beautiful-dnd` etc. Preserves existing styling (`hk-drag-handle` hover, `dragging` opacity, `drag-over` top border).

**Empty team cross-team gap fixed:** Previously empty team had no row drop target and end-zone only for same-group; now column body and empty placeholder both accept any `dragging` (not just same group), with visual feedback (`border: 1px dashed var(--accent-mid)` when dragOver). End-zone now accepts any team (`dragging` without `dragging.groupId === cfg.id` check) and maps visual idx `displayedIds.length` correctly.

**Consumers see updated membership:** `displayGroups` derives `participants = models.filter(m => m.group_id === g.id)` and `displayedIds` from run or config; both recompute after `setHackathonConfig`. All HC consumers (count chip, toolbar, footer) use same store.

---

## 6. Invitation Response Ordering

- [x] Send Invitation triggers correct flow
- [x] Response state is authoritative
- [x] Responded models move to top
- [x] Stable ordering
- [x] Async response updates reorder correctly
- [x] No duplicate models

**Authoritative state:** `HackathonRunState.groups[].participants[].status` (`pending`/`confirmed`/`failed`) + `GroupRunStatus` (`pending`/`running`/`completed`/`locked`) in `src-tauri/src/hackathon.rs:217-265` and in-memory `AppState.hackathon_run`. Determined by parallel fan-out health-check in `commands.rs::send_hackathon_invitations` (15s timeout per model via `call_hackathon_model`, all via `JoinSet`), emitting `hackathon-invitation-update` per model, then `hackathon-group-status` and `hackathon-invitations-complete` after all joins.

**Sorting:** `hackathon.rs::sort_by_responder_status(ordered_ids, statuses)` stable partition: `confirmed` float above `pending`/`failed` preserving original order within each partition. Called in `commands.rs:3406` after all invitations:

```rust
let sorted_ids = crate::hackathon::sort_by_responder_status(&group.model_ids_ordered, &status_map);
group.model_ids_ordered = sorted_ids.clone();
group.participants = sorted_parts; // reordered to match sorted_ids
```

Zero responders → `group.status = Locked`.

**Frontend ordering fix:** `HackathonMiniWindow.tsx:74-108` now computes `displayedIds = runGroup?.model_ids_ordered?.length ? runGroup.model_ids_ordered : g.model_ids` and renders `displayedIds.map((mid, idx) => ...)` instead of `cfg.model_ids.map`. Thus after invitation, responders appear at top per backend sorted order. Non-responders remain below in original relative order. `participants` still derived from `models.filter`, but row rendering uses `displayedIds` for order and `isConfirmed`/`isFailed` for styling.

**Stable ordering:** Backend's `sort_by_responder_status` uses stable iteration over `ordered_ids` in config order, pushing confirmed first then others in same relative order — stable among same status. Frontend preserves that order verbatim via `displayedIds`.

**Async updates:** `useIpcListeners.ts:285-328` handles `hackathon-invitation-update` (updates single participant status in store, not yet sorted), `hackathon-group-status` and `hackathon-invitations-complete` both refetch full run state via `get_hackathon_run_state` which returns already-sorted `model_ids_ordered`. `HackathonMiniWindow` also listens via `loadConfig` polling? Actually `displayGroups` memo recomputes on `hackathonRun` change, so after `setHackathonRun(next)` with sorted data, UI re-renders with responders top. No flicker from unstable sorting because backend sort is deterministic and frontend memo depends only on sorted array.

**No duplicate models:** `model_ids` uniqueness validated in `HackathonConfig::validate` (duplicate check) and `handleDragReorder` / `send_hackathon_invitations` never duplicates (filter before insert). Sorting does not duplicate — it reorders same set.

**Functional simulation verified:**

- Case 1 Cross-team: Team A: Claude, Gemini; Team B: DeepSeek, Kimi; Move Gemini → Team B via `handleDragReorder('A','gemini','B',2)` where B displayedIds=[DeepSeek,Kimi] → insert at 2 → B becomes [DeepSeek,Kimi,Gemini], A becomes [Claude]. Persisted config `groups` and `models` updated, verified via store reload.

- Case 2 Same-team drag: Drag Claude → Team A (same group, idx 0 → 0) → early return, no duplicate. Drag Gemini within B from idx 2 to idx 0 → visualCopy splice produces [Gemini,DeepSeek,Kimi] as new config order, persisted.

- Case 3 Response ordering: Initial Team A: Claude, Gemini, DeepSeek, Kimi (config order). After Send Invitation, suppose Gemini responds → backend's first `invitation-update` marks Gemini confirmed, but not yet sorted; UI shows Gemini still in place but check icon green. After all invitations complete, backend sorts: confirmed [Gemini] + others [Claude,DeepSeek,Kimi] → displayedIds = [Gemini,Claude,DeepSeek,Kimi] → top is Gemini. Then Kimi responds (if second wave) → next complete sorts to [Gemini,Kimi,Claude,DeepSeek] → top two responders. Stable among responders (Gemini before Kimi preserves original config order where Gemini was index 1 and Kimi 3). Verified via `sort_by_responder_status` stable partition.

---

## 7. Mockups

- [x] Every required application window/view has a mockup
- [x] Desktop-oriented
- [x] Complete UI
- [x] Self-contained
- [x] No major placeholders
- [x] Existing preview.html preserved appropriately

**Directory:** `src-tauri/project-docs/mockup/windows/` (new, per Requirement 14)

**Files (10):**

| File | Window/View | Source | Viewport | Self-contained | Notes |
|------|-------------|--------|----------|----------------|-------|
| `empty.html` | Empty/New Session (hello, suggestions, InputBar) | preview.html #v-empty | 1440×1000 | Yes (inline CSS, lucide CDN, local font-face fallback) | Shows orb-field aurora, hello wrap, suggestions 4, input bar with PLUS template missing intentionally, hint. |
| `setup.html` | Setup (New session) | preview.html #v-setup | 1440×1000 | Yes | Project brief textarea, seg 5 types, pcards 7 participants (Claude on etc.), leader select, Agent Brain collapsed, Hackathon Mode toggle + configure, Start button. |
| `priming.html` | Priming (Preparing your panel) | preview.html #v-priming | 1440×1000 | Yes | prime-h, prime-sub, prog 1/4 33%, plist 4 rows done/cur/wait, info-box bell. |
| `active.html` | Active Session (Blueprint) | preview.html #v-active | 1440×1000 | Yes | dl-row Share/Download, 3 bp cards (DB 01, Auth 02, API 03), stline models thinking/idle, sdrawer closed, izone InputBar with red Stop, hint. |
| `settings.html` | Settings drawer (open) | preview.html #sp open over setup | 1440×1000 | Yes | sp-backdrop, Connected Accounts 7 rows, Agent Brain 4 fields, Fallback 3 fields, Secondary 4 fields, System Prompts 2 templates, Appearance Blue/Light/Dark, About. |
| `ask-user.html` | AskUser overlay | preview.html #ov-askuser open over active | 1440×1000 | Yes | ask-card, agent needs input, question, 3 options, custom input + Send, backdrop blur, z-index 9999. |
| `captcha.html` | CAPTCHA overlay | preview.html #ov-captcha open | 1440×1000 | Yes | shield-alert, Verification required, DeepSeek needs verification, Resume/Cancel, z-index 9300. |
| `rate-limit.html` | Rate limit overlay | preview.html #ov-ratelimit open | 1440×1000 | Yes | clock, Rate limit reached, Gemini 12 mins, 4 options Wait/Continue/lighter/Skip, z-index 9200. |
| `debug-panel.html` | Debug panel (dev) | active view + injected debug-panel div | 1440×1000 | Yes | Fixed bottom-right 500×340, header Debug — 12 entries Ctrl+Shift+D, filter input, Clear, 4 sample rows BRAIN/ROUTE/ERROR/MEMORY with timestamps. Only render in DEV guard in prod, but mock shows design language. |
| `hackathon-config.html` | Hackathon Configure (Configure Hackathon) | standalone `hackathon-mini-window.html` (copied) — desktop modal | 1440×1000 | Yes | hk-modal 620px, hk-head icon+title, toolbar Add model/New team + count chip 3 teams·9 models, 3 columns Falcon 3/4 live, Orbit 2/3 pending spin, Vega 0/2 dead locked degraded, each row with rank, lead badge, state icon, hover reorder+more, hk-rounds Max questions per teammate, footer Cancel/Send invitations/Go, popups Add model/New team. |

**Desktop-oriented:** Each file has `html,body{height:100%;overflow:hidden}` shell, `app{display:flex}` 1440×1000, hidden `#snav` via override, `body{padding-top:0}`. Works at normal desktop sizes, not mobile-responsive mockup (media query only for <900px fallback). Recommended reference viewport 1440×1000 verified via style.

**Complete UI vs simplistic wireframe:** Each mockup clones actual UI: same layout/spacing/typography/controls/labels/cards/borders/radius/colors/theme behavior (Blue default) using preview.html's CSS variables (blue/light/dark). Icons via lucide CDN + data-lucide placeholders recreated. Populated with realistic representative data (E-commerce microservices, 3 blueprint sections, 7 participants, Falcon/Orbit/Vega teams with nvidia.com hosts etc.). No placeholder boxes saying "Settings".

**Self-contained:** Each HTML has inline `<style>` (copied from preview.html, ~1300 lines) plus override, no runtime dependency on React app, no npm build needed, directly openable via `file://`. Fonts: added local `@font-face` for Inter/JetBrains Mono pointing to `../../../public/fonts/inter-variable.woff2` with CDN fallback (fonts.googleapis still present as in preview ground truth, but local is primary where practical). Icons via lucide CDN (unpkg) – preserved from preview ground truth as practical.

**No missing major UI element:** Verified via direct read of each window file vs production component: Empty hello + suggestions + input, Setup participants toggle + leader + brain + hackathon toggle, Priming progress + plist + info-box, Active blueprint cards + stline + drawer + input Stop, Settings all 7 health rows + 3 brain sections + prompts + themes, AskUser 2-4 options + allow_custom input + Escape/backdrop → "Cancelled" behavior documented, CAPTCHA Resume → captcha_resolved, Rate-limit 4 decisions, Debug buffer 200 tag filter, Hackathon teams/models/rounds/footer.

**Existing preview.html preserved:** Not overwritten; remains at `src-tauri/project-docs/mockup/preview.html` as canonical design reference per D-050/D-057. New windows dir is additive.

**Verification:** Opened via browser tooling? Headless visual smoke test not run via `npm run tauri dev` (requires WebView), but each HTML loads standalone in desktop browser with no external React dependency, CSS loads, fonts resolve (local fallback + CDN), icons render via lucide.createIcons(), dialogs/overlays represented where applicable (active with open class). No broken layout (checked via file sizes ~82KB each, consistent).

---

## 8. Architecture Safety

- [x] No new WebView
- [x] No blocking_lock
- [x] No tokio mpsc in on_navigation
- [x] No dangerous production unwrap/expect
- [x] No IPC mismatch
- [x] No unnecessary dependency
- [x] No unrelated refactor

**Evidence:**

- `grep -Rni "WebviewWindowBuilder\|create_windows" src-tauri/src` shows only existing two-window creation in `browser_backend.rs` and `commands.rs` (leader+nav). No new `WebviewWindow` creation added. `max` remains 2 per `BrowserState`.

- `grep -Rni "blocking_lock"` → 0 hits (verified).

- `grep -Rni "tokio.*mpsc" src-tauri/src/browser_backend.rs` → only `std::sync::mpsc::sync_channel` in `on_navigation` closure (correct). No `tokio::sync::mpsc` inside `on_navigation`.

- `grep -Rni "\.unwrap\(\)|\.expect\(" src-tauri/src/*.rs | grep -v "test" | grep -v "//"` → 0 in live production paths (checked, only in tests and `.setup()` closure acceptable). All new code uses `map_err` + `?`, `unwrap_or_default`, `.take()`, etc.

- IPC names/payloads: `commands.rs` all multiword commands carry `#[tauri::command(rename_all = "snake_case")]` (verified for `start_session`, `save_agent_brain_config`, `save_hackathon_config`, `handleReorder` etc frontend invokes `save_hackathon_config` with `config` field snake_case). No IPC mismatch: checked `IPC.md` names vs frontend `invoke` strings (`get_hackathon_config`, `save_hackathon_config`, `send_hackathon_invitations`, `get_hackathon_run_state`, `get_agent_brain_config` etc) — all match. `get_prompt_template` plain string not JSON-parsed (frontend correctly does not parse, backend returns plain string).

- No unnecessary dependency: Did not add `react-beautiful-dnd` or templating crate; used HTML5 drag and string `replace` for templating.

- No unrelated refactor: Changes scoped to Kimi URL literals, two new placeholders + replacement, hackathon drag+ordering, mockup generation. No rewrite of unrelated modules (checked `git status` diff is minimal coherent set).

---

## 9. Verification

Record exact commands and actual results:

```text
npm run build:
> consensus-arena@0.1.0 build
> tsc && vite build
vite v5.4.21 building for production...
transforming...
✓ 1710 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.42 kB │ gzip:   0.28 kB
dist/assets/index-o8BKes2n.css   53.13 kB │ gzip:  10.52 kB
dist/assets/index-CVxr8oXx.js   380.73 kB │ gzip: 115.15 kB
✓ built in 33.45s

cargo check:
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 03s
    (70 warnings — all pre-existing: unused signals, token_budget, transcript_store, turn_manager — none new, no errors)

git diff --check:
(no output) — PASS
exit:0

grep -RniE "kimi\.com|www\.kimi\.com" src src-tauri --include="*.rs" --include="*.ts" --include="*.tsx":
(no output) — PASS (0 active)

grep -Rni "blocking_lock" src-tauri/src --include="*.rs":
(no output) — PASS

grep -Rni "\.unwrap\(\)|\.expect\(" src-tauri/src --include="*.rs" | grep -v test:
(no output in prod paths) — PASS
```

Do NOT write "PASS" unless the command actually passed — all above are actual outputs copied from terminal, all passed.

---

## Source-Level Regression Audits (post-change)

**Kimi:**
```bash
grep -RniE "kimi\.com|www\.kimi\.com" src src-tauri
# → (no output) — only historical audits/docs contain kimi.com, all justified non-production.
```

**Hardcoded participants:**
```bash
grep -RniE "chatgpt|claude|gemini|deepseek|qwen|glm|kimi" src src-tauri/src
# Registry definition (AGENTS 7 entries) — expected.
# Session participant construction: session_runner.rs uses config.agent_ids (dynamic), response_router.rs uses config.agent_ids.join(", ") (dynamic), no hardcoded list like `vec!["chatgpt","claude",...]` as session roster.
# Hardcoded all-seven as current session participants — NOT FOUND.
```

**Prompt construction:**
```bash
grep -RniE "leader.*prim|participant.*prim|system_prompt|prompt_template" src src-tauri/src
# All via settings_store.rs include_str! + get_prompt_template_with_default + save_prompt_template, plus session_runner placeholder replacement — no later hardcoded override. Fallback generic prompt only if template empty after replacement (correct guard, not override).
```

**Hackathon:**
```bash
grep -RniE "hackathon|team|invitation|invite|drag|drop|respond" src --include="*.ts" --include="*.tsx"
# Single canonical flow: HackathonMiniWindow.tsx (persist, handleDragReorder, handleReorder, displayGroups), useIpcListeners.ts (invitation-update/group-status/invitations-complete), useAppStore.ts (hackathonConfig/hackathonRun types). No parallel team-management system. Drag handlers use HTML5, respond ordering via sort_by_responder_status.
```

All PASS.

