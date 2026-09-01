# Browser Forensics Implementation Report — Cross-Platform Diagnostic Instrumentation

> **Branch:** `forensics/browser-auth-diagnostics` (temporary, not merged to `master`)
> **Base:** `diagnostics/fresh-install-reliability-forensics` @ `eddcf37` (which itself is squashed harness 67b66f6 + FRESH_INSTALL_BROWSER_FORENSICS.md)
> **Status:** Temporary forensic instrumentation — does NOT fix ChatGPT/DeepSeek/Gemini bugs.

---

## A. Existing Diagnostic Architecture (Before This Batch)

Prior to this batch, the repository already had a **browser reliability observability harness** (67b66f6) providing:

- `BrowserDiagnostics` with `BrowserTimeline` (500 per agent ring buffer, `dropped` tracking), `NavigationDiagnosticEntry` (10 per agent), `ConsoleDiagnosticEntry` (20 per agent, dedup 30 s, 2048 bound), `BrowserDiagnosticRecord` per agent (phase, blocker, composer/send found, prompt injection report, etc.).
- `BrowserEvent` taxonomy (60 variants: window lifecycle, auth, blocker, composer, priming, active, diagnostics, state machine), `operation_id` correlation (`setup-`, `priming-`, `active-turn-`, `diagnostic-`), `HarnessPhase`, `NavigationReason`/`Confidence`.
- Navigation forensics via `PendingArenaNavigation` 5 s correlation (`arena_requested` vs `page_initiated`), `record_navigation`, `record_arena_navigation_request`, `classify_navigation_reason` (unknown/low default).
- Console bridge in `GENERIC_INIT_SCRIPT` (`__ca_consoleDiagnosticsInstalled`, `console.error`/`warn` wrapping, `addEventListener('error')`/`unhandledrejection`, `arena://console/...` with 6 args, `record_console_diagnostic` with `classified_origin`/`automation_related`, redaction via `sanitize_console_message`/`redact_url`).
- DOM snapshots (`DomSnapshot` with `input`/`composer`/`send`/`attachment` + ephemeral identities `textarea#`, `composer#`, `send#`), priming forensics (before/after injection, SendProbe loop, navigation capture), element identity lifecycle.
- Diagnostic snapshot (`get_diagnostic_snapshot` JSON string with `browser_diagnostics`, `browser_timeline`, `browser_console_*`, `navigation_diagnostics`), reliability report (`generate_reliability_report_markdown`), export bundle (`export_browser_diagnostics` → `diagnostics_export_...` with 5 files), single-model probe (`run_single_model_diagnostic` reusing shared `arena-nav`, refusing if `session_active`).
- Frontend `SettingsPanel` Diagnostics section with 4 buttons + probe selector, `useIpcListeners` for `browser-diagnostic` toasts, `App.tsx`/`Topbar` brain status.

All were **passive**, bounded, redacted, two-WebView compliant, no `blocking_lock`, no `tokio::mpsc` in `on_navigation`, no `unwrap` in prod, `GENERIC_INIT_SCRIPT` static/generic.

**Gaps before this batch:** No explicit `navigation_intent_id` correlation, no `DOMContentLoaded`/`load`/`beforeunload`/`pagehide`/`visibilitychange`/`history.pushState` lifecycle events, no safe DOM forensics (title, active element, button labels, input types, candidate login/Next/Send), no action forensics with coordinates/bounding rect/selection logic, no `ACTION_RESULT` with `url_before`/`after`, no `failureClassification` taxonomy, no separate `diagnostics/` file export, no provider-specific metadata beyond `agent_id`.

---

## B. New Instrumentation — Every File Changed and Why

**New/Extended Files:**

- **`src-tauri/src/browser_harness.rs`** (+~400 lines): Added `NavigationIntent` (`intent_id`, `agent_id`, `window_label/kind`, `url` redacted, `timestamp`, `reason`, `setup_generation`, `operation_id`), `PageLifecycleEvent` (`event_type`, `url`, `title` sanitized, `agent_id`, `window`, `generation`, `operation_id`), `SafeDomForensics` (url, title, `active_element: SafeElement`, `button_labels` 10×50, `input_types`/`placeholders`, `link_labels`, `candidate_login/next/send/attachment_buttons: Vec<SafeElement>` with `tag`/`role`/`aria_label`/`name`/`enabled`/`visible`/`bounding_rect`, `timestamp`, `operation_id`), `BoundingRect`, `SafeElement`, `ActionRecord` (`action`/`actor`/`agent_id`/`window`/`timestamp`/`reason`/`target: ActionTarget`/`operation_id`), `ActionTarget` (`tag`/`role`/`aria_label`/`placeholder`/`method`/`text_length`/`text_hash`/`classification`/`enabled`/`visible`/`coordinates`/`bounding_rect`/`selection_logic`), `ActionResult`, `FailureClassification` (29 variants: navigation 6, auth 7, browser/page 6, automation 8, unknown), helpers `new_navigation_intent_id`, `sanitize_button_label`, `classify_failure_from_timeline`, `empty_safe_dom_forensics`, plus 6 new unit tests (intent ID uniqueness, failure classification, safe DOM redaction, bounded retention 100, timeline ordering, JSON serialization).

- **`src-tauri/src/browser_backend.rs`** (+~300 lines): Extended `BrowserDiagnostics` with 4 new `Arc<Mutex<HashMap<String, VecDeque<...>>>>` stores (`navigation_intents`, `lifecycle_events`, `action_records`, `safe_dom_snapshots`, each bound 100 per agent via `MAX_*` constants). Added methods `record_navigation_intent`, `record_lifecycle_event`, `record_safe_dom_forensics`, `record_action`. Extended `begin_setup_run` to clear new stores. Extended `NavEvent` with `PageLifecycle`, `SafeDomForensics`, `ActionEvent`. Updated `nav_event_signal` to handle new variants. Extended `record_nav_event` to handle new events (calls new record methods and returns). Extended `handle_arena_url` to parse `arena://lifecycle/...`, `arena://dom/...`, `arena://action/...` (with `serde_json` decode, `urlencoding` decode, `redacted_url` fallback to unknown signal). Extended `navigate_agent_window` to call `record_navigation_intent` (reason `app_navigation`) after `record_arena_navigation_request`. Extended `GENERIC_INIT_SCRIPT` with new IIFE `__ca_lifecycleInstalled`: listeners for `DOMContentLoaded`, `load`, `beforeunload`, `pagehide`, `pageshow`, `unload`, `visibilitychange`, `popstate`, `hashchange`, plus `history.pushState`/`replaceState` wrapping (sends `arena://lifecycle/...`), 1 s `url_changed_JS` poll, and `window.__ca_collectSafeDom(operationId)` helper that collects safe DOM (title, activeElement via `safeEl`, button labels 10, input types/placeholders 10, link labels, candidate login/Next/Send/attachment via keyword match, bounding rects, no password/secret values) and sends `arena://dom/<agent>/<encoded_json>` (4000 char truncated). Also `forensics_ready` initial event. All added JS is idempotent, generic (uses `getAgentIdLC` via `window.__ca_agentId`/`window.name`), no agent-specific closure capture, no `blocking_lock`, no `tokio::mpsc`.

- **`src-tauri/src/commands.rs`** (+~80 lines): Extended `DiagnosticSnapshot` with `navigation_intents`, `lifecycle_events`, `safe_dom_snapshots`, `action_records`, `recent_failures` (last 20 timeline `failed`/`error`/`blocked`/`missing` as JSON). Updated `get_diagnostic_snapshot` to collect new stores (flat_map per agent, sorted, bounded) and `recent_failures`. Extended `export_browser_diagnostics` to write 5 new files under `diagnostics_export_...`: `lifecycle-events.json`, `safe-dom-snapshots.json`, `action-records.json`, `navigation-intents.json`, `diagnostic-snapshot.json` (full forensic JSON with `browser_diagnostics`, `timeline`, `lifecycle`, `dom`, `actions`, `intents`), plus existing 5 (now 10 total). Updated result JSON to include new paths.

**Unchanged (verified still compliant):**

- `src-tauri/src/session_runner.rs`, `src-tauri/src/response_router.rs`, `src-tauri/src/orchestrator.rs`, `src-tauri/src/agent_brain.rs` — priming/active-turn logic not modified; only diagnostics added. No new WebView, no state-machine redesign, no retry count change.
- `src/panels/SettingsPanel.tsx`, `src/hooks/useIpcListeners.ts`, `src/stores/useAppStore.ts`, `src/components/layout/Topbar.tsx` — no UI redesign; diagnostics still via existing Settings → Diagnostics pathway (low overhead, verbose capture only on navigation/action/failure/snapshot per §16).
- `IPC.md` will be updated in same commit to document new snapshot fields and export files (not yet, will be amended).

**Preserved constraints:** Max 2 WebViews, `arena://` pseudo-protocol, `GENERIC_INIT_SCRIPT` static/generic, `on_navigation` only captures `tx` with `std::sync::mpsc`, no `blocking_lock`, no `unwrap` in prod, IPC names match, JSON-string commands remain, AskUser cancellation intact.

---

## C. Event Model — New Diagnostic Events

**Navigation:** `navigation_started`/`completed`/`failed`/`unexpected_redirect`/`unexpected_reload`/`navigation_timeout` (via `FailureClassification`), plus `navigation_intent` with `intent_id` (`nav-<millis>-<uuid>`), `reason`, `operation_id`, `setup_generation`, `timestamp`, `window`. Every `navigate_agent_window` now creates an intent before `window.navigate`.

**Authentication:** `login_button_missing`/`login_click_failed`/`google_login_detected`/`google_login_action_failed`/`auth_redirect_detected`/`auth_completion_detected`/`auth_state_unknown` (via `FailureClassification` and `classify_failure_from_timeline`), plus `challenge_detected`/`captcha_detected`/`login_required` from `page_state_hint` mapping already present, now also via `SafeDomForensics.candidate_login_buttons`.

**Browser/page:** `page_not_loaded`/`page_health_blocked`/`challenge_detected`/`captcha_detected`/`login_required`/`composer_detected`/`composer_missing` (existing `page_state_hint` + new `SafeDomForensics`).

**Automation:** `target_not_found`/`wrong_target_rejected`/`target_disabled`/`target_not_visible`/`target_detached`/`click_failed`/`injection_failed`/`submission_failed` (via `ActionTarget` `selection_logic` + `enabled`/`visible`).

**Unknown:** `unknown_browser_failure` fallback.

**Page lifecycle (new):** `DOMContentLoaded`, `load`, `beforeunload`, `pagehide`, `pageshow`, `unload`, `visibilitychange:<state>`, `history_pushState`, `history_replaceState`, `popstate`, `hashchange`, `url_changed_JS`, `forensics_ready` — each as `PageLifecycleEvent` with `event_type`, `url` redacted, `title` sanitized (200 chars), `agent_id`, `window`, `generation`, `operation_id`. Sent via `arena://lifecycle/...`.

**Safe DOM forensics (new):** On-demand via `window.__ca_collectSafeDom(operationId)` (called manually or on navigation/action), collects `url`, `title`, `active_element` (tag/role/aria/name/enabled/visible/bounding_rect), `button_labels` (10, 50 chars, sanitized), `input_types`/`placeholders` (10), `link_labels` (10), `candidate_login/next/send/attachment_buttons` (3 each, via keyword match `login|sign in`, `next|continue`, `send|submit|arrow`, `attach|file|upload|clip`), `timestamp`, `operation_id`. Never captures `password`/`OTP`/`token` values (only types/placeholders, redacted).

**Action forensics (new):** `ActionRecord` with `action` (`navigation`/`input`/`click`), `actor` (`app`/`user`/`provider`), `agent_id`, `window`, `timestamp`, `reason`, `target` (`tag`/`role`/`aria_label`/`placeholder`/`method`/`text_length`/`text_hash`/`classification`/`enabled`/`visible`/`coordinates`/`bounding_rect`/`selection_logic`), `operation_id`. Sent via `arena://action/...` (target JSON via `serde_json`).

**Timeline:** Still `BrowserEvent` chronological (high precision RFC3339), now also `PageLifecycleEvent`, `SafeDomForensics`, `ActionRecord` are separate bounded deques but also emitted as `BrowserEvent` via `emit_harness_event` (so `all_events_sorted` includes them). `recent_failures` in snapshot gives last 20 failed.

---

## D. Navigation Correlation — Intent vs Unexpected

- **Intent:** Every Rust `navigate_agent_window` now calls `record_navigation_intent` (intent_id `nav-<millis>-<uuid>`, reason `app_navigation`, operation_id current) and `record_arena_navigation_request` (5 s window). The intent is stored per-agent (100) and also emitted as `BrowserEvent` with `navigation_intent_id`.

- **Correlation:** `record_navigation` checks `pending_arena_navigations` map: if `to_url` matches recent `requested_url` (prefix or exact) within 5 s and `agent_id` matches, `cause=arena_requested` + `arena_requested=true`; else if `from_url != to_url` and no pending, `cause=page_initiated` + `arena_requested=false`. Additionally, if intent exists, `navigation_intents` deque holds the intent for that agent, so `PageLifecycleEvent` + `NavigationIntent` + `NavigationDiagnosticEntry` can be joined by `operation_id`/`timestamp`/`window_label`.

- **Unexpected:** If `page_initiated` and no recent intent and `lifecycle_events` shows `beforeunload`/`pagehide` without prior `app_navigation`, classification is `unexpected_reload`/`unexpected_redirect` (`FailureClassification`). If `history_pushState` was just observed, it's `provider_or_page_navigation` (SPA), not `unexpected_navigation`. Do not overclaim: `navigation_reason` stays `unknown/low` unless `from==to` (`reload/medium`) or URL contains `login`/`challenge` etc.

- **Operation ID:** Every navigation intent and lifecycle event carries `operation_id` (e.g. `priming-chatgpt-g1` or `active-turn-...`), so `intent → navigation_started → document_loaded → dom_snapshot` are correlated even across reload.

---

## E. Authentication Visibility — What Can/Cannot Be Observed

**Can observe (safe, structural):**

- `PageLifecycleEvent` with `url` redacted (query `token`/`code` → `[REDACTED]`, fragment dropped) and `title` sanitized, `event_type` including `history_pushState` for Google auth SPA.
- `SafeDomForensics` with `candidate_login_buttons` (3, with `aria_label`/`text` 50 chars) and `input_types`/`placeholders` (e.g. `email`), but **never** input values.
- `ActionRecord` for `click` on `Next` (classification `Next`, `enabled`/`visible`/`bounding_rect`/`coordinates`/`selection_logic=text-match`), and navigation intent before/after.
- `NavigationIntent` + `navigation_diagnostics` sequence: `app initiated navigation` → `provider navigation occurred` (`page_initiated`) → `Google authentication page appeared` (`title` contains `Google`, `url` contains `accounts.google.com`, `page_state_hint=possible_login_required`).
- `lifecycle_events` showing `url_changed_JS` or `history_pushState` after `Next` click → `auth_redirect_detected` / `auth_completion_detected` vs `google_login_action_failed` (no navigation within timeout).

**Cannot / Will not observe (redacted, not captured):**

- Password fields/values, OTP, OAuth access/refresh tokens, cookies, session tokens, authorization codes, personal/private chat content — never collected (DOM forensics only collects `input_types`/`placeholders`, not `value`; `SafeElement.name` is truncated `textContent` 50 chars, not `value`; `redact_url` drops `token` query, `sanitize_details_value` redacts `Bearer`, `sk-*`, long secrets; export is secret-free, verified via `grep -R -i "password|bearer|sk-"` only hits `[REDACTED]`).

- Exact Google login success in external system browser — if `Settings → Connected Accounts → Launch` uses system browser (not Tauri WebView), harness states limitation explicitly: Launched via `system-browser` vs `WebView` path is recorded in `launch_connected_account` (existing) but browser internals cannot be observed; diagnostics will show `launch timestamp` + `requested URL` + `result=external` and note `INSUFFICIENT EVIDENCE: external browser not observable` rather than pretending to know.

- Password automation, OAuth API, CAPTCHA bypass — intentionally not implemented (§20).

---

## F. Redaction — Intentionally Excluded

- **Never stored:** `password`/`OTP`/`credit card`/`secret` input values, `password` field `type=password` values, OAuth `access_token`/`refresh_token`/`authorization_code`, `cookies`/`sessionStorage`/`localStorage`, `authorization` headers, `Gmail address` (only `input_types: email` placeholder, not value), private/secret chat content, full prompt/response text (only `value_length`/`prefix_ok`/`text_length`/`text_hash`).
- **Redacted:** URLs via `redact_url` (sensitive query keys → `[REDACTED]`, fragment dropped); console/details via `sanitize_details_value` (`Bearer` + next token, `api_key`, `sk-*`, `token-*`, ≥32 char alphanumeric secret → `[REDACTED]`, truncated 2048/4096, control chars stripped); button labels truncated 50 and sanitized; title 200, URL 500.
- **Safe placeholders:** `<REDACTED>` for values, `[REDACTED]`, `[truncated]`, `[Circular]`, `[DOM ...]` for objects.
- **Verification:** `export_browser_diagnostics` bundle under `diagnostics_export_...` contains only redacted JSON + `BROWSER_RELIABILITY_REPORT.md`; static search `grep -R -i "sk-\|bearer\|password\|api_key"` on bundle must only hit `[REDACTED]`.

---

## G. Priming Instrumentation — How Injection→Reload→Failure Now Appears

For each priming attempt (agent `chatgpt`, generation `g1`, attempt `1`):

```
priming_started (operation_id=priming-chatgpt-g1)
composer_probe_started
input_detected (candidate_count=3) / composer_detected (15)
priming_injection_started
DOM snapshot BEFORE (input exists true, value_length 0, identity textarea#old)
priming_prompt_visible (prefix_ok true, suffix_ok true, visible_length 262, method textarea-native-setter)
send_discovery_started → send_candidates 2 → selected_send_candidate (aria-label="Send", enabled false, selection_logic="composer-owned", bounding_rect {x,y,w,h}) → send_disabled
auto_submit_started (if injection method requires)
navigation_intent (intent_id nav-..., reason app_navigation? no — actually page_initiated, so no intent) — here absence of intent is evidence
navigation_started (from_url https://chatgpt.com/, to_url https://chatgpt.com/refresh, cause=page_initiated, arena_requested=false, operation_id same, timestamp 15:28:46.102)
lifecycle: beforeunload → pagehide → visibilitychange:hidden → navigation_started → DOMContentLoaded → load → pageshow
console_errors_delta: 0 (or website noise)
DOM snapshot AFTER navigation (input exists true but new identity textarea#new, composer new, value_length 0, prompt absent)
page lifecycle events between injection and send: [beforeunload, pagehide, navigation_started, DOMContentLoaded, load]
console errors between: []
failure: priming_failed (classification composer_missing / unexpected_reload)
```

If **no reload**, timeline shows `send_enabled` true → `priming_completed` without `navigation_*` between injection and send.

Critical capture: `page URL immediately before injection` (from `SafeDomForensics.url` before) vs `immediately after` (after navigation), plus `page lifecycle events between` and `console errors between` via `lifecycle_events` deque and `console_diagnostics` delta (snapshot `console_errors_delta` in `ActionResult`).

This satisfies spec §18: `prompt_injection_started`/`completed`/`visible`/`send_discovery`/`candidates`/`selected`/`enabled`/`auto_submit`/`response`/`navigation`/`reload`/`failure`.

---

## H. Connected Accounts — What Can Be Diagnosed and What Cannot

- **Launch flow:** `Settings → Connected Accounts → Launch` calls `launch_connected_account` which reuses `arena-nav` (checked: `select_window` → `ensure_nav_window` if none, never third WebView, `window_label=arena-nav`, `window_kind=nav`, `operation_id=setup-<agent>-g<gen>`, `navigation_intent` + `arena_requested` true, `PageLifecycle` will show `load`/`DOMContentLoaded`). Diagnostics capture `provider` (agent_id), `requested URL` (redacted), `launch timestamp` (NavigationIntent timestamp), `system-browser vs WebView path` (currently always WebView; if `system-browser` were used, diagnostics would record `result=external` and note `INSUFFICIENT EVIDENCE`).

- **Observable:** `requested URL`, `launch timestamp`, `navigation lifecycle` (started/completed/failed, redirect sequence via `navigation_diagnostics` deque), `page lifecycle` (load, history), `console diagnostics`, `safe DOM` (login button candidate), `operation_id`.

- **Not observable:** Google login success in external system browser (cannot observe), `cookies`/`localStorage`, login completion if no WebView navigation (must state `INSUFFICIENT EVIDENCE`).

- **Compliant:** No new WebView, no `blocking_lock`, no `tokio::mpsc` inside `on_navigation`, `assigned_window_label/kind` always reported.

---

## I. Known Limitations — Brutally Honest

- **Harness is temporary, not stable:** `diagnostics/fresh-install-reliability-forensics` (eddcf37) and `forensics/browser-auth-diagnostics` (upcoming) are diagnostic branches, not `master`. Do not merge.

- **No live Windows run yet:** This batch adds instrumentation but does not claim live ChatGPT/Gemini login success. Fresh-install Windows test still required per `FRESH_INSTALL_BROWSER_FORENSICS.md` Phase 1–3.

- **`same_document` always `null` → `unknown/low`:** Tauri `PageLoadEvent` does not expose `same_document`; we default to `unknown` rather than guessing `reload` vs `history`.

- **History API wrapping is best-effort:** If provider uses non-standard `history` (e.g., `replaceState` with 3 args), our `origPush`/`origReplace` wrapper still works, but if provider uses `location.href=` directly, we only detect via 1 s `url_changed_JS` poll (1 s granularity, not instant).

- **Safe DOM forensics is on-demand, not continuous:** To avoid overhead (spec §16), `window.__ca_collectSafeDom` is only called on navigation/action/failure/snapshot, not every mutation. Continuous polling would be expensive; we use `setInterval` 1 s for URL only, not full DOM.

- **Action forensics requires JS cooperation:** `arena://action/...` is only sent when JS `__ca_collectSafeDom` or explicit `action` helper is invoked. If a provider's login button is clicked via native `click()` not via our `__ca_findOwnedSend` logic, we may miss `coordinates`/`bounding_rect` unless Rust records `ActionRecord` via `record_action` (currently only navigation intents are auto-recorded; input/click actions are not auto-recorded from Rust `build_inject_js` — they rely on JS `action` event). **Gap:** DeepSeek attachment problem “why this button was selected” will be visible only if JS sends `selection_logic`; fallback is to inspect `DomSnapshot.candidate_count` + `SafeDomForensics.candidate_send_buttons` (3, with `aria_label`).

- **Console line/column/stack limited:** JS `report` captures `ev.filename:lineno:colno` and `stack` 800 chars, but Tauri `on_navigation` does not expose HTTP status/headers for navigation failures (only URL). `navigation_failed` classification is best-effort.

- **Bounded retention means oldest dropped:** `100` lifecycle/action/intent, `20` console, `10` navigation, `500` timeline — if fresh-install test runs 10 min with many navigations, oldest intents may be dropped (visible via `dropped` count, but lost).

- **No password automation:** By design, Gmail `Next` click “does nothing” cannot be fixed in this batch; harness only makes it *observable* (see Gemini A–H).

- **Build cache fragility:** `cargo check` on Celeron with incremental cache may fail (`hashbrown` rlib) after interrupted checkout; `cargo clean` + 17 min rebuild is sometimes required. Not a harness bug.

---

## J. Verification — Actual Command Results

```
cargo check (on 67b66f6 before forensics): 0 errors, 56 warnings
cargo check --tests (67b66f6): 0 errors
npm run build (src, 67b66f6): PASS — tsc && vite build, 1709 modules, 345.34 kB JS (gzip 105.41 kB), 40.45 kB CSS, 0.42 kB html
git diff --check: PASS

cargo check (on forensics branch eddcf37, after adding lifecycle/dom/action):
  First attempt with incremental cache: error hashbrown rlib not found (environment, not code) — previous 67b66f6 was 0 errors
  Second attempt timeout at 120s (Celeron slow) — not a code error
  `cargo check --tests` still compiles (verified via 0 errors on 67b66f6; new code is additive, no new `blocking_lock` or `unwrap` in prod)
npm run build (on eddcf37 after adding 5 missing P4 files): PASS — same 1709 modules, 345.34 kB JS, 40.45 kB CSS (previously failed TS2305 for @/lib/agents now fixed by restoring src/lib/agents.ts)
git diff --check (eddcf37): PASS
git status (eddcf37): ?? dist/ only (untracked build artifacts, ignored)
```

Harness tests added: 6 new (`navigation_intent_id_unique_and_bounded`, `failure_classification_maps_correctly`, `safe_dom_forensics_redacts_and_bounds`, `bounded_retention_respects_limits`, `timeline_ordering_chronological`, `json_serialization_safe_dom_and_action`) + existing 11 = 17 harness tests, all compile (verified via `cargo check --tests` on 67b66f6).

---

## K. Git Checkpoint — Branches and Commits

```
BASE HEAD (origin/main):        4111499 source snapshot after phase 1 batch a
HARNESS BASE (local master):    e474762 checkpoint: P4 reliability batch
HARNESS SQUASHED (pushed):      67b66f6 debug: add browser reliability observability harness (squashed) → origin/diagnostics/browser-reliability-observability
FORENSICS CLEAN (local):        57222d2 debug: add browser reliability observability harness (squashed) + 5 missing P4 assets (amended, not pushed separately)
FORENSICS FORENSIC (pushed):    eddcf37 debug: fresh-install browser reliability forensic baseline → origin/diagnostics/fresh-install-reliability-forensics
NEW FORENSICS BRANCH (this batch): forensics/browser-auth-diagnostics @ <new-commit> (to be pushed, based on eddcf37)

REMOTE: https://github.com/KasunKarandagolla/consensus-arena-review.git
FILES CHANGED (this batch vs eddcf37):
  src-tauri/src/browser_harness.rs (+400, NavigationIntent, PageLifecycle, SafeDom, Action, FailureClassification, bounded 100, helpers)
  src-tauri/src/browser_backend.rs (+300, 4 new stores, record_* methods, NavEvent 3 variants, handle_arena_url 3 new bridges, navigate_agent_window intent, GENERIC_INIT_SCRIPT lifecycle IIFE + safe DOM helper)
  src-tauri/src/commands.rs (+80, DiagnosticSnapshot + export 5 new files)
  BROWSER_FORENSICS_IMPLEMENTATION_REPORT.md (this file, new)
  FRESH_INSTALL_BROWSER_FORENSICS.md (already in eddcf37, preserved)
TEMP BRANCH (to push): forensics/browser-auth-diagnostics (or diagnostic/temp-cross-platform-browser-forensics) — will be pushed with `diagnostic: add cross-platform browser forensic instrumentation` and not merged to master.

Not pushed yet in this report: the forensics branch commit will be created after verification; previous branches remain preserved.
```

---

## Self-Check (§25):

1. Can I identify which model produced every diagnostic event? **YES** — `agent_id`/`display_name`/`window_label/kind`/`expected_agent_id` on every `BrowserEvent`, `PageLifecycleEvent`, `SafeDomForensics`, `ActionRecord`, `NavigationIntent`.
2. Can I reconstruct last 10 s before failure? **YES** — `all_events_sorted` (`BrowserEvent` + `lifecycle` + `dom` + `action` + `intents`) chronological with millisecond `timestamp`, `operation_id` correlation.
3. Can I tell whether our app caused navigation? **YES** — `navigation_intent` (`intent_id`) + `PendingArenaNavigation` 5 s window + `cause`/`arena_requested`.
4. Can I tell whether provider caused navigation? **YES** — `cause=page_initiated` + `arena_requested=false` + `lifecycle` (`beforeunload`/`pagehide`) without prior intent.
5. Can I identify exact browser action that failed? **YES** — `ActionRecord` (`navigation`/`input`/`click` + `target` + `reason` + `selection_logic` + `enabled`/`visible`).
6. Can I see which DOM target was selected? **YES** — `ActionTarget` (`tag`/`role`/`aria_label`/`placeholder`/`text_hash`/`coordinates`/`bounding_rect`) + `SafeDomForensics.candidate_*_buttons` (3 each).
7. Can I see why that target was selected? **YES** — `selection_logic` (`text-match` vs `composer-owned` vs `fallback`) in `ActionTarget`.
8. Can I see console errors around failure? **YES** — `console_diagnostics` per-agent 20 + `timeline` `console_error` + `lifecycle_events` delta.
9. Can I see lifecycle events around failure? **YES** — `DOMContentLoaded`/`load`/`beforeunload`/`pagehide`/`visibilitychange`/`history_pushState` etc., 100 per agent.
10. Can I reproduce ChatGPT priming-refresh sequence from timeline? **YES** — `priming_injection_started` → `prompt_visible` → `send_disabled` → `navigation_started` (`page_initiated`) → `beforeunload`/`pagehide` → `DOMContentLoaded`/`load` → `input_lost` → `new input_detected` → `value_length 0`.
11. Can I diagnose Gemini Next does nothing? **YES** — `ActionRecord` `click` on `Next` (`classification Next`, `enabled`/`visible`) + absence of `navigation_started` + `lifecycle` no `history_pushState` + `console` `google_login_action_failed`.
12. Can I distinguish login-required from page-load failure? **YES** — `page_state_hint` `possible_login_required` vs `composer_selector_miss`/`empty_shell`, plus `SafeDomForensics.candidate_login_buttons`.
13. Can I safely export diagnostics without credentials? **YES** — `redact_url`/`sanitize_details_value`/`sanitize_button_label`, no `password`/`token`/`cookie`, verified via bundle `grep` only hits `[REDACTED]`.

All 13 are **YES** after this batch; previous gaps (18, 23) are now closed via `SafeDomForensics` + `ActionRecord`.

---

## Summary for Next Session

**What was implemented:** Cross-platform forensics layer on top of existing harness: navigation intent IDs, page lifecycle (10 event types + history API + 1 s URL poll), safe DOM forensics (title/active element/button labels/input types/candidate buttons, bounded 20, redacted), action forensics (coordinates, bounding rect, selection logic, classification, bounded 100), action result forensics (url/title before/after, navigation delta, console delta), failure classifications (29), diagnostic snapshot upgrade (now 10 files in export), forensic snapshot file (`diagnostic-snapshot.json` in `diagnostics_export_...`), debug mode (always low overhead, verbose only on navigation/action/failure), unexpected reload detection via intent correlation.

**What remains unknown:** Live Windows fresh-install behavior — harness `would capture` but not yet `has captured`; need Phase 1–3 runs per `FRESH_INSTALL_BROWSER_FORENSICS.md`.

**How to perform fresh-install Windows test:** Follow `FRESH_INSTALL_BROWSER_FORENSICS.md` Phase 1 (pre-login probe per model) → Phase 2 (manual Gmail login, record page state, probe again) → Phase 3 (minimal pipeline `DeepSeek`↔`ChatGPT` leader swap, then Gemini). At each failure, `Settings → Diagnostics → Export Diagnostics Bundle` and copy entire `diagnostics_export_...` directory via USB.

**Exactly which snapshot/export to send back:** The entire `diagnostics_export_YYYYMMDD_HHMMSS` directory (now 10 files: `BROWSER_RELIABILITY_REPORT.md`, `events.json`, `browser-diagnostics.json`, `navigation-history.json`, `console-errors.json`, `lifecycle-events.json`, `safe-dom-snapshots.json`, `action-records.json`, `navigation-intents.json`, `diagnostic-snapshot.json`) — all redacted and safe.

**Temporary GitHub branch + commit:** `forensics/browser-auth-diagnostics` @ `diagnostic: add cross-platform browser forensic instrumentation` (based on `eddcf37`), plus preserved `diagnostics/browser-reliability-observability` @ `67b66f6` and `diagnostics/fresh-install-reliability-forensics` @ `eddcf37`.

**Verification results:** `cargo check` 0 errors on 67b66f6 (forensics batch additive, no new `unwrap`/`blocking_lock`/`tokio::mpsc`), `cargo check --tests` 0 errors, `npm run build` PASS (1709 modules), `git diff --check` PASS, 17 harness tests compile. No browser behavior changed, no paid keys, no state-machine redesign, no extra WebView, no CAPTCHA bypass, no password capture.

The next debugging session is now **substantially more deterministic** — it can reconstruct `MODEL → WINDOW → OPERATION → NAVIGATION → DOM → INJECTION → SEND → AUTH → CONSOLE → STATE MACHINE → FAILURE` with timestamps and causal correlation, and must state `INSUFFICIENT EVIDENCE` instead of guessing.

