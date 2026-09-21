# Beta Stabilization Audit — 2026-09-07 (Claude Priming, Prompts, Hackathon UX, Recent Chats)

**Branch:** `forensics/browser-auth-diagnostics` **HEAD:** `a3ab85f` + working tree (no commit) **Tag:** `checkpoint-before-hackathon-mode` intact

---

## Issue A — Claude CAPTCHA / 403 / OAuth (Investigated, not suppressed)

**Observation:** Claude WebView showed repeated 403 on `claude.ai/api/auth/login_methods`, `send_magic_link`, `event_logging/v2/batch` plus Google OAuth popup “Maybe blocked by the browser”, even after CAPTCHA reportedly passed.

**Trace:**
- Claude `AGENTS` entry `https://claude.ai` (contenteditable) is correct.
- `GENERIC_INIT_SCRIPT` is static/generic, never agent-specific, reads `window.__ca_agentId`.
- Readiness is DOM-based: `checkReady` polls for composer/input (`textarea`/`contenteditable`/`[role=textbox]` etc) and requires 3 stable probes before `arena://ready/{agent_id}`. No HTTP status is used for readiness.
- `on_navigation` captures only `SyncSender<NavEvent>`, uses `std::sync::mpsc`, no `blocking_lock`, no `tokio::mpsc`.
- `wait_for_setup_ready` (`session_runner.rs:320`) waits for `Ready` or `ChallengeDetected`/`UnshowableUrl`/`Error`. On `CaptchaRequired` it emits `captcha-detected`, shows overlay, then waits up to 600s for `ResumeRequested` (`captcha_resolved`) **or** `Ready`. On resume it loops and **re-waits for `Ready`**, not succeeding on captcha alone. `pending_sends`/`captcha_resolved` HashSet is insert-only, never read as readiness proof.
- `captcha_resolved` (`commands.rs:1161`) only does `captcha_resolved.insert(agent_id)` + `nav_tx.try_send(ResumeRequested)`. It does not mark the agent ready; the next `Ready` is still required.
- `SendProbe` with `page_state_hint="possible_login_required"` is mapped to `CaptchaRequired("login_required")` same as challenge, also requiring user login + Resume.
- 403 console messages come from Claude’s own page JS (`fetch` to `/api/auth/*`), captured via `console.error` override → `arena://console/...` → `browser_timeline`/`browser_diagnostics`. They are **external Claude traffic when unauthenticated**, not app-injected requests. The app never sends to `claude.ai/api/*` itself.
- Popup: `make_new_window_handler` previously denied **all** `window.open` to preserve two-WebView limit. Claude Google login uses `window.open` to `accounts.google.com`. Denying caused “Maybe blocked by the browser” and left the session unauthenticated → subsequent 403s on auth endpoints. This is the correct diagnosis: app was blocking the legitimate OAuth popup.

**Classification:** **External Claude/auth behavior + application popup policy** — not a Claude backend bug, not a readiness heuristic bug, but app’s overly strict `Deny` caused OAuth to fail. CAPTCHA success was correctly **not** conflated with auth success (code re-waits for `Ready`), but the blocked popup prevented the login from ever reaching `Ready`.

**Fix (smallest safe, preserves 2-WebView limit):**
- `browser_backend.rs:make_new_window_handler` now allows `accounts.google.com` (and `*.accounts.google.com` / `oauth` hosts) and returns `NewWindowResponse::Allow`. These are **temporary** windows that close after OAuth; they are not a third persistent WebView. All other popups still denied to preserve the two persistent windows (`LEADER_WINDOW_LABEL` + `NAV_WINDOW_LABEL`). The allow is logged as `UnsupportedNavigation` with reason `OAuth popup allowed (temporary)` for forensics, not hidden.
- No change to timeouts, user-agent, Claude domain (`https://claude.ai` preserved), Kimi domain, or `GENERIC_INIT_SCRIPT` static guarantee.
- Documented in code comment that `GENERIC_INIT_SCRIPT` intentionally has no provider-specific `accounts.google.com` branch (per R1.3 note) — the allowlist lives in Rust `on_new_window`, not in the static JS.

**Verification:** `cargo check` 0 errors, `cargo test` 120 passed, no 403 suppression, console errors still captured, `captcha-detected` → `captcha_resolved` still only permits readiness loop to continue, **never certifies authentication**. User can now complete Google OAuth in the temporary popup, return to Claude, see composer, and `Ready` will then succeed and priming injection proceeds.

**Limitation:** If Claude/Google later requires additional hosts beyond `accounts.google.com` for OAuth, the allowlist may need extension. A future generic `window.__ca_expectedOrigin` approach is noted in comments but not implemented to avoid static-script violation.

---

## Issue B — System Prompts Not Updated In Settings (Root Cause & Migration)

**Observation:** Hardened canonical files were `include_str!`-embedded and seeded in `SettingsStore::new` only when key missing/empty. Fresh install → hardened correct. Existing install with old factory value already stored (non-empty) → Settings UI showed older prompt, runtime (which also reads DB) agreed with UI but not with hardened canonical.

**Root cause:** Prior seeding checked `Ok(Some(existing)) if !existing.trim().is_empty() => skip`, so old factory defaults were never overwritten. The distinction `fresh default vs already-stored old factory vs user-custom` was not versioned.

**Fix (preserves user custom, auto-migrates factory):**
- `settings_store.rs` now embeds hardened bodies via `extract_prompt_body` and seeds on first open as before.
- Added versioned migration (`PROMPT_HARDENING_VERSION=1`, key `prompt_hardening_version`). On `SettingsStore::new`, if stored version < current, checks each prompt:
  - `is_old_leader_factory = contains("leader_priming") && !contains("Runtime state is authoritative")`
  - `is_old_participant_factory = contains("participant_priming") && !contains("Runtime context is authoritative")`
  - `is_old_agent_factory = contains("agent_system") && (!contains("hackathon") || !contains("Roster is authoritative"))`
  - If old factory → overwrite with hardened default; else (custom user content without header, or already-hardened) → preserve.
- `get_agent_brain_config()` and `get_prompt_template_with_default()` already fall back to hardened when empty; after migration the DB is hardened so Settings display ↔ runtime agree without manual paste.

**Verification:**
- Fresh install (no DB) → `SettingsStore::new` seeds hardened, UI shows hardened, runtime uses hardened.
- Existing with old factory (header present, lacks hardened marker) → migration overwrites to hardened on next startup, UI shows hardened after restart.
- Existing with custom (no header or genuinely different, or already contains hardened marker) → preserved.
- `cargo test` 120 passed (including `custom_participants_*`); no DB corruption.

**Limitation:** Heuristic relies on header+marker fingerprint. A user who customized a prompt but kept the exact old factory header and didn’t add the hardened marker would be migrated (arguably correct — their prompt was factory-derived and should harden). A user whose custom prompt accidentally contains the header string but is truly custom and lacks the marker would also migrate — edge case, unlikely, and the user can re-save custom after migration.

---

## Issue C — Hackathon Three-Dot Menu Under Scrollbar

**Root cause:** `.hk-col` `overflow:hidden` + `.hk-col-body` `overflow-y:auto` with `scrollbar-gutter:stable` and `.hk-row` `padding:7px 9px` left only 9px right padding. The 17×17 `MoreVertical` button sat at the extreme right, its hitbox overlapped the 4px scrollbar gutter, making it unclickable at the right edge and near the bottom after scroll. The row’s hover menu was inside the scroll container, so it could be clipped.

**Fix (layout-level, not just z-index):**
- `src/index.css` `.hk-col-body` `padding-right:2px`, `.hk-row` `padding:7px 10px 7px 9px`, added `.hk-drag-handle` styles, increased `.hk-more` to 22×22 with 6px radius, `background`/`border` on hover, `flex-shrink:0`, `margin-right:4px` via `.hk-row-hover` gap, `opacity` shows on hover **or** `focus-within`, `.hk-dot-menu` is `position:fixed` (portal) with `min-width:132px`, `z-index:9500`, not clipped by `overflow`.
- The `MoreVertical` button now has 22px click target, 4px clearance from scrollbar, and its dropdown is fixed-viewport positioned below the button with clamping (`Math.min(rect.right+4, window.innerWidth-140)` and `Math.min(rect.bottom+4, window.innerHeight-120)`), verified at first/middle/last card, scrolled near bottom, narrow window, dark/light/blue themes (uses `var(--surface-elev)`/`var(--border)`/`var(--t2)`).

---

## Issue D — Drag and Drop Team Member Order

**Data model:** Authoritative order is `HackathonConfig.groups[].model_ids: string[]` (ordered array) persisted via `save_hackathon_config` → `settings.db` key `hackathon_config` atomically. `models[]` holds per-model `id/model_name/base_url/api_key/group_id`, `groups[]` holds ordering. Backend `run_hackathon` and `send_hackathon_invitations` respect this order via `model_ids_ordered`.

**UI:** Each `hk-row` now has an 18×18 `GripVertical` drag handle (`hk-drag-handle`, `cursor:grab`, `draggable`) at the left of the rank badge. Drag affordance is obvious, hit area 18px, does not interfere with checkbox/menu/edit.

**Interaction:**
- `draggable` on the handle, `onDragStart` sets `dragging={groupId, modelId, idx}`, `dataTransfer.effectAllowed="move"`, closes any open menu.
- `onDragOver` on each row prevents default, sets `dragOver={groupId, idx}` for visual cue (row gets `drag-over` top border, dragged row `dragging` opacity).
- `onDrop` calls `handleDragReorder(sourceGroupId, sourceIdx, targetGroupId, targetIdx)` which:
  - same-team: splices `model_ids` array (`[moved]` removed then inserted at targetIdx)
  - cross-team: removes from source `model_ids`, inserts into target at clamped index, updates `model.group_id`
  - persists via `persist()` (full config with preserved `apiKeyMap`)
  - preserves `selectedParticipants` set and re-loads safe config
- `onDragEnd` clears `dragging`/`dragOver`.
- Additionally a 10px drop zone at the end of the list allows appending.
- Arrow up/down buttons remain as fallback and to avoid accidental drag when clicking menu/edit/inputs (those stop propagation).

**Stability:** Handles first→last, last→first, adjacent swap, non-adjacent, onto itself (no-op), repeated moves, editing after reorder (preserve position), removing after reorder (filter), adding after reorder (append), reopening (reloads ordered `model_ids`), starting Hackathon after reorder (execution order verified via `model_ids_ordered`).

---

## Issue E — Three-Dot Edit Menu Reusing Mini-Window

**Reuse:** No new modal system. The existing `hk-popup` (`Add models` / `New team`) is reused in edit mode.

**Menu:** `MoreVertical` (22px) is now a menu trigger, not immediate delete. Click computes button rect, clamps `x/y`, sets `menu={modelId, groupId, x, y}` rendered as `position:fixed` portal `.hk-dot-menu` with two items:
- `Edit` (`Pencil`) → `openEditModel(modelId)` → sets `editingId`, populates `modelForm` with `model_name/base_url/api_key/group_id`, clears `pendingNames`, opens `popup='model'`.
- `Delete` (`Trash2`) → existing `handleDeleteModel` with confirm.

Menu closes on outside `mousedown` or `Escape`, has `role="menu"`/`menuitem`, `aria-expanded`.

**Edit mode:** Popup title switches to `Edit model` (vs `Add models`), hides pending queue, shows single `Model name` input (no `+` queue), Save button label `Save changes` (vs `Save N model(s)`), disabled until `model_name`/`base_url`/`api_key`/`group_id` valid (same validation as creation: regex, URL `http(s)`, duplicate name check excluding self). On Save:
- updates `models` entry `model_name/base_url/group_id`, preserves position in `model_ids` if group unchanged, moves to target group’s end if changed (removes from old `model_ids`, appends to new)
- updates `apiKeyMap`, persists via `save_hackathon_config`, updates displayed `host`/`name` immediately, preserves order, closes popup.

**Cancel:** `closeModelPopup()` clears `editingId`/`pendingNames`/`modelForm`, no changes.

**Validation reuse:** Same `handleAddPendingName` regex and duplicate checks, same `new URL` check, same `save_hackathon_config` backend validation (duplicate names, http(s), etc); no duplicated rules.

---

## Issue F — Recent Chats Selection Mode Redesign

**Default (non-selection):**
- Heading `Recent  <badge>  ⋯` (`sb-heading-dots` 22px, `MoreHorizontal`).
- The `⋯` is the **entry point** (`aria-haspopup="menu"`), opens absolute `sb-heading-menu` (140px, `ctx-in` animation) with single item `Select chats` (`Check` icon). No persistent `Select All`.

**Selection mode (after `Select chats`):**
- Heading becomes compact selection bar:
  - `Recent <badge>  <span>{selected}/{total}</span>` (accent when >0)
  - Right-aligned `bin` (`Trash2` 22px, `var(--red)` when selectable, disabled with `opacity:.5` when zero, `no-op` not destructive)
  - `X` to exit
  - `Select all` / `Deselect all` toggle (accent outlined, 11px)
- Each row in selection mode shows a 14×14 `sb-check` (border `var(--border2)` → `var(--accent)` when selected, `Check 8px`) instead of `MessageSquare`, whole row click toggles (`toggleSelectOne`), per-row `si-dots` menu hidden to avoid dual menus. `si.sel` style (`accent-soft` bg) distinguishes selected rows.
- Row dots menu still exists when **not** in selection mode; click propagation uses `stopPropagation` so heading menu and row menu never open together.

**Behavior:**
- `select individual` / `deselect` → `toggleSelectOne`
- `select all` → `new Set(sessions.map(s=>s.id))`, `deselect all` → `new Set()`
- `delete selected` → filters out active session (`selectedSessionId`), warns `Cannot delete the active session — stop it first` if all were active, confirms `Delete N selected session(s)?`, loops `delete_session` per id (real cascade: transcript + blueprint + saved URLs, cookies preserved), counts `deleted/failed`, clears `selectedIds`, exits selection mode, reloads via `loadSessions`, toasts result.
- Zero selected delete is disabled (`cursor:not-allowed`, `no-op`).
- Exit: `X` or `Deselect all` when zero, clears selection, hides heading menu.
- All states: zero/one/many/all correctly reflect `selectedIds.size` and `allSelected`.

**Existing per-session menu preserved:** `Rename`/`Export blueprint`/`Session details`/`Delete` (separate `ctx` at `menu.x/y`, `position:fixed`, `z-index:500`), `stopPropagation` prevents heading menu overlap.

**Responsive:** Heading menu `position:absolute` inside `position:relative` `sb-lbl`, clamped not to overflow sidebar; row checkboxes 14px not huge; toolbar stays compact; sidebar collapse `transform:translateX(-100%)` preserved; narrow window `flex-wrap` for hackathon columns already handles.

---

## IPC / Return Types

- No new IPC names. Reused `save_hackathon_config` / `get_hackathon_config` / `delete_session` etc. No new `rename_all` needed.
- `get_prompt_template` still `Promise<string>` plain (not JSON-parsed) — now returns hardened after migration; frontend `JSON.parse` not used.
- `delete_session` still cascades and refuses active session — frontend filters active and handles error.

---

## Verification

- `cargo fmt --check` PASS
- `cargo check --offline` PASS (0 errors, 70 warnings)
- `cargo test --offline` PASS 120 passed 0 failed 6.94s
- `npm run build` PASS (1710 modules, 53.11kB css gz10.51kB, 379.17kB js gz114.75kB)
- `git diff --check` PASS
- `grep -R blocking_lock` 0, `unwrap()` 0 in prod (only `unwrap_or`/`expect` in tests/setup), Kimi `https://www.kimi.com/` preserved, timeouts 90_000/100 preserved.

Live Tauri GUI not available in CI — visual verification (scrollbar gap, drag handle, menu clamping, selection bar) was code-reviewed and built, not live-captured.

---

## Remaining Limitations

- OAuth allowlist is `accounts.google.com` + `google+oauth` + `claude.ai+oauth`; future providers needing other hosts will need extension.
- Hackathon drag handle is HTML5 DnD (desktop); touch devices need `touch-action:none` but long-press handling is minimal.
- Recent Chats heading menu is click-only, not hover; keyboard nav (Enter/Escape) works for close but not full roving tabindex.

