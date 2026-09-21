# Fresh-Install Browser Forensics — Manual Test Procedure (Windows)

> **Purpose:** Produce a **scientifically diagnosable** evidence bundle on a **fresh Windows installation** before any automation fixes.
> Do NOT fix browser bugs in this session. Instrument, run, export, inspect.

This document is the **exact** procedure to be executed on the fresh Windows laptop after installing the harness build from branch `diagnostics/browser-reliability-observability` (67b66f6) or `diagnostics/fresh-install-reliability-forensics`. No developer dependencies are required on Windows beyond the installed Tauri app.

---

## 0. Prerequisites

- **Build:** On Linux dev machine, `npm run build` + `npm run tauri build` from `diagnostics/browser-reliability-observability` (67b66f6). Copy the resulting `.msi`/`.exe` installer to the Windows laptop via USB. **Do not** install via `cargo`/`npm` on Windows.
- **Clean state:** Fresh Windows user profile, no prior `app_data_dir` (`%APPDATA%\com.consensus-arena\` or Tauri's `app_data_dir`). If reinstalling, delete that directory first to guarantee fresh auth state.
- **Accounts:** Have Gmail credentials for ChatGPT, DeepSeek, Gemini (do NOT store them in diagnostics; they will never be captured). Ensure 2FA device is available.
- **Harness build:** Launch the installed app, open `Settings → Diagnostics` — you must see buttons `Show Diagnostic Snapshot`, `Reliability Report`, `Timeline (last 50)`, `Export Diagnostics Bundle`, and `Probe Single Model` + agent selector. If missing, you are not on the harness build.

---

## 1. Phase 1 — Pre-Login Single-Model Probes (No Auth)

**Goal:** Prove harness can distinguish `composer_selector_miss` vs `login_required` vs `challenge` before any login.

For each `ChatGPT` / `DeepSeek` / `Gemini`:

1. Launch app fresh. Do **not** start a session.
2. `Settings → Connected Accounts` — observe dots (expected: `Not checked`/`Not available`).
3. `Settings → Diagnostics → Probe Single Model` — select the model, click `Probe Single Model`. Wait 15–20 s. The probe reuses the shared `arena-nav` window (`window_label=arena-nav`, `window_kind=nav`), never creates a third WebView.
4. Observe result popup: `input_found`, `send_button_found`, `composer_candidate_count`, `page_state_hint`, `page_health_hint`, `last_navigation_url`, `elapsed_ms`. Record manually:
   - `page_state_hint = composer_detected` → likely logged-in or login not required.
   - `possible_login_required` → login page detected.
   - `possible_challenge_or_security` / `cloudflare`/`captcha` → security block.
   - `empty_shell_or_hydration_stuck` / `composer_selector_miss` → selector miss or hydration failure.
5. Immediately `Settings → Diagnostics → Export Diagnostics Bundle` → note `export_dir` path shown (e.g. `%APPDATA%\...\diagnostics_export_20260501_123456\`). **Copy the entire bundle directory** (contains `BROWSER_RELIABILITY_REPORT.md`, `events.json`, `browser-diagnostics.json`, `navigation-history.json`, `console-errors.json`) to USB as `phase1-<agent>-prelogin-<timestamp>/`.
6. Also `Show Diagnostic Snapshot` → `Copy snapshot`, and `Timeline (last 50)` → copy, for cross-check.
7. **Close the probe window** (no session active, just `arena-nav` showing the model URL). Do not start a session.

**Expected artifacts per model:**

- `phase1-chatgpt-prelogin/` — harness should show `operation_id=diagnostic-chatgpt-g<gen>`, `phase=diagnostic`, 5–15 timeline events (`window_created`, `navigation_started`, `document_loaded`, `composer_probe_started`, `input_detected/lost`, `send_detected/lost`, `dom_snapshot`, plus `login_page_detected` if auth required), navigation `cause=arena_requested` for the probe navigation, console diagnostics (website noise vs `arena_*`), and redacted URLs.

**Safety:** Export is secret-free (verified via `redact_url`/`sanitize_details_value`; no prompt/response, cookie, token, password). You may share it.

---

## 2. Phase 2 — Login Forensics (Per Model)

**Goal:** Distinguish login states without automating Gmail; capture structural evidence only.

For each model in order `ChatGPT` → `DeepSeek` → `Gemini` (Gemini is most fragile; do last):

### 2.1 Launch / Login

1. `Settings → Connected Accounts` → for the model, click `Launch`. This reuses `arena-nav` (same architecture as session pipeline: `navigate_agent_window` with `arena_requested`, `operation_id=setup-<agent>-g<gen>` then overwritten to `diagnostic` if probe, but Launch uses `setup` correlation). The window becomes visible for manual login.
2. In the `arena-nav` WebView, perform **manual** Gmail login (enter Gmail, click Next, password, 2FA). **Do not** let the harness auto-type.
3. Observe and record (pen-and-paper or screenshot of the **WebView URL bar**, not the harness):
   - `page loaded?` (did `document_loaded` fire? harness `page_state_hint` moves from `still_loading` → `composer_detected` or `possible_login_required`).
   - `login page?` (harness `login_page_detected`/`login_required` vs `login_state_authenticated`).
   - `button/input detected?` (post-login, `composer_probe_started` should show `input_detected` + `composer_detected`).
   - `navigation?` (did WebView URL change after clicking Next? harness `url_changed` with `from_url`→`to_url`, `cause=page_initiated`).
   - `console errors?` (harness `console_error` with `classified_origin=website` vs `security`).
   - `authentication state?` (`page_state_hint=composer_detected` + `last_blocker=none` + `input_found=true` = authenticated).
   - `challenge?` (`captcha_detected`/`cloudflare_detected` + `last_blocker=captcha_or_challenge` + overlay `CAPTCHA`).
   - `final page state?` (ChatGPT: `https://chatgpt.com/` with composer; DeepSeek: `https://chat.deepseek.com/`; Gemini: `https://gemini.google.com/app`).

### 2.2 Probe After Login

4. Without closing the WebView, return to `Settings → Diagnostics → Probe Single Model` for **same** agent, click `Probe Single Model` again. This re-probes the now-authenticated page (same `arena-nav`, same `window_label`, new `operation_id=diagnostic-<agent>-g<gen>`).
5. Observe new `page_state_hint` → should now be `composer_detected` + `send_detected` if login succeeded; else still `possible_login_required` if login failed.
6. `Export Diagnostics Bundle` → copy as `phase2-<agent>-postlogin-<timestamp>/`.
7. Screenshot the `BROWSER_RELIABILITY_REPORT.md` section for that model (it will show `Current phase`, `Current URL`, `auth/blocker`, `Timeline`, `Navigation`, `Console`, `DOM`).

### 2.3 Gemini Special — Distinguish A–H

For **Gemini** specifically, after entering Gmail and clicking Next, use harness timeline (`get_browser_timeline` via `Timeline (last 50)` or exported `events.json`) to answer:

- **A** Button not discovered? → No `send_detected` / `input_detected` before click, `composer_candidate_count=0`.
- **B** Button discovered but disabled? → `send_detected` with `enabled=false` + `priming_send_disabled`.
- **C** Button clicked? → `send_detected` + `active_submit_attempt` + `active_submit_completed` (for active turns; for login, look for `send_detected` followed by `url_changed`).
- **D** Click produced navigation? → `navigation_started`/`navigation_finished` with `from_url` Gmail → `to_url` Gemini, `cause=page_initiated`, `arena_requested=false`.
- **E** Click produced no navigation? → No `navigation_*` after `send_detected` within 15 s, `page_state_hint` stays `possible_login_required`.
- **F** Console/network error? → `console_error` with `category=console_error`/`javascript_exception`, `classified_origin=website` or `network_blocked`.
- **G** Google challenge? → `challenge_detected`/`captcha_detected`/`cloudflare_detected` + `last_blocker=captcha_or_challenge` + console `security`.
- **H** Unknown? → Timeline shows `unknown/low` confidence and no `login_page_detected` nor `composer_detected`; report `INSUFFICIENT EVIDENCE` rather than guessing.

**Do not fix Gemini** — only export evidence. If timeline cannot distinguish A–H, note the missing signal (e.g. “no `send_snapshot` after click”).

---

## 3. Phase 3 — Real Pipeline (Priming → Active)

**Goal:** Capture the ChatGPT priming failure sequence and cross-model routing with full causal correlation.

### 3.1 Minimal Two-Model Pipeline

1. `New Session` → **Project Brief:** Use a short deterministic brief (save it for reproduction):
   ```
   Build a minimal task tracker API (Node + SQLite). Keep it simple.
   ```
   **Session Type:** `MVP`
   **Participants:** `DeepSeek` + `ChatGPT` (exactly 2, to test shared `arena-nav`).
   **Leader:** `DeepSeek` (first run), then repeat with `ChatGPT` as leader.

2. **Before Start:** `Settings → Diagnostics → Show Diagnostic Snapshot` → copy as `phase3-pre-session-snapshot.json` (proves `setup_generation`, `operation_id=setup-...`).

3. Click `Start Session`. Observe `Priming` view: each agent shows `priming_started` → `composer_probe_started` → `input_detected` → `priming_injection_started` → `priming_prompt_visible` → `send_enabled/disabled`.

4. **Critical:** Do NOT press Send manually unless harness indicates `waiting_user_send` and `send_enabled=false` with `setup_completion_reason` still `None`. If `capability_verified` (prefix_ok && suffix_ok && send_enabled && no error) then priming auto-advances — this is `setup_completion_reason=capability_verified`, not a bug.

5. When `ChatGPT` priming reaches `priming_injection_completed` (check `BROWSER_RELIABILITY_REPORT.md` live via `Reliability Report` button), watch for `Send disabled` vs `Send enabled` and for any `navigation_started` (`page_initiated`) within 10 s after injection. **This is the forensic question:** `Did the page navigate after prompt injection but before Send became enabled?`

6. If pipeline breaks (e.g. `ChatGPT prompt disappears`, `composer_lost`, `input_lost`, `new input_detected` with different `input_identity`), **immediately** (within 10 s) run:
   - `Settings → Diagnostics → Show Diagnostic Snapshot` → copy
   - `Reliability Report` → copy
   - `Timeline (last 50)` → copy
   - `Export Diagnostics Bundle` → copy entire dir as `phase3-failure-deepseek-leader-<timestamp>/`

7. If pipeline succeeds, let it run 2–3 active turns, then `Abort Session` and export as `phase3-success-...`.

### 3.2 Swap Leader

8. Repeat 3.1 with **Leader = ChatGPT**, **Participant = DeepSeek** (same brief). Export as `phase3-failure-chatgpt-leader-...` or `phase3-success-...`.

### 3.3 Include Gemini When Practical

9. If Gemini post-login probe in Phase 2 showed `composer_detected`, repeat with 3 participants (`DeepSeek` + `ChatGPT` + `Gemini`, leader `DeepSeek`). Otherwise document `Gemini not included: login not achieved` and skip.

### 3.4 Capture at Failure

10. At any failure (priming timeout, `setup-agent-failed` overlay, `active_submit_failed`, `response_observed_before_send`, navigation during setup with `setup_navigation_recovery_count` increment), capture:
    - `diagnostic snapshot` (includes `browser_timeline` + `browser_diagnostics` + `navigation_diagnostics` + `console_diagnostics`)
    - `reliability report` (per-model: `Current phase`, `Current URL`, `Current operation`, `Timeline`, `Navigation`, `Console`, `DOM`, `Priming result`, `Submission result`, `Final diagnosis`)
    - `timeline` (`events.json` sorted)
    - `export bundle` (all 5 files)

All bundles are safe to copy to Linux via USB (redacted, no prompt/response, token, cookie).

---

## 4. How to Copy Bundles to Linux

On Windows, each `Export Diagnostics Bundle` creates `%APPDATA%\com.consensus-arena\diagnostics_export_YYYYMMDD_HHMMSS\` containing 5 files. Copy the **entire directory** to USB, then on Linux: `cp -r /media/usb/phase3-* ~/Music/arena/consensus-arena/diagnostics-exports/` and `ls -R`. The `BROWSER_RELIABILITY_REPORT.md` is human-readable; `events.json` is machine-diffable.

---

## 5. What to Inspect on Linux (Post-Copy)

Engineer (or AI) on Linux inspects the GitHub branch `diagnostics/browser-reliability-observability` (67b66f6) and the USB bundles without guessing:

- **Per-agent identity:** `events.json` → every event has `agent_id`, `display_name`, `window_label`, `window_kind`, `session_id`, `setup_generation`, `expected_agent_id`. Verify `deepseek/arena-nav` never misattributed to `chatgpt/arena-leader` (check `active_by_window` mapping and `attribution mismatch` log).
- **Operation correlation:** Filter `events.json` by `operation_id` (`setup-`, `priming-`, `active-turn-`, `diagnostic-`). A priming failure must have `operation_id=priming-chatgpt-g<gen>` for all related `priming_injection_*`, `dom_snapshot`, `navigation_*`.
- **Navigation lifecycle:** In `navigation-history.json` + `events.json`, check `navigation_started` → `navigation_finished` → `document_loaded` → `url_changed` chain, with `from_url`/`to_url`/`navigation_reason`/`confidence`/`cause`/`arena_requested`. `unknown/low` when insufficient evidence — do not invent `login_redirect`.
- **Priming forensics:** Timeline must show `priming_started` → `composer_probe_started` → `input_detected`/`composer_detected`/`send_detected` → `priming_injection_started` → `DOM snapshot BEFORE` → `priming_prompt_visible` → `send_enabled/disabled` → `priming_injection_completed/failed` → `navigation_*` → `input_lost`/`composer_lost` → `new input_detected` → `DOM snapshot AFTER` → `priming_completed/failed`. If `send_disabled` followed by `page_initiated` navigation and `input_identity` change (`textarea#262` → `textarea#0`), report `composer/input lifecycle changed` with navigation evidence.
- **ChatGPT states:** Report distinguishes State A (`input exists, prompt visible, send exists, send disabled`), B (`send becomes enabled`), C (`input disappears, navigation, new input, prompt absent`), D (`injection failed`). These must not collapse to `priming failed`.
- **Console:** `console-errors.json` → each `category`/`severity`/`source`/`classified_origin`/`automation_related`. `segment` metric → `website`, not `arena`.
- **Login:** `browser-diagnostics.json` → `page_state_hint`/`page_health_hint`/`last_blocker` + timeline `login_page_detected`/`login_state_authenticated`/`challenge_detected`. No Gmail/password/token captured (verify via `grep -i "password\|bearer\|sk-\|token"` on bundle — must only hit `[REDACTED]`).
- **Gemini:** Use timeline to answer A–H; if missing, note `MISSING EVIDENCE: no send_snapshot after click`.
- **Connected Accounts:** `Launch` reuses `arena-nav` (`is_active` mapping), `operation_id=setup-<agent>-g<gen>`, same `navigate_agent_window` path as session — verify via `events.json` `window_label`/`operation_id`.
- **Single-model probe:** Should refuse if `session_active`, create/reuse correct WebView, assign `diagnostic-<agent>-g<gen>`, capture readiness/auth/composer/send/navigation/console, produce report, cleanly finish (no `session_active` contaminant).
- **Export safety:** Run `grep -R -i "sk-\|bearer\|password\|api_key" diagnostics-exports/` — only `[REDACTED]` or truncated placeholders should appear. URLs with `token` already redacted.

---

## 6. Known Limitations (Do Not Guess)

- `same_document` is `null` unless WebView exposes it — report `unknown` not `reload`.
- DOM snapshots are metadata (exists/visible/enabled/candidate_count/identity/length), not full dumps — they answer “survived vs recreated” via identity, not pixel.
- Console cannot distinguish every Google challenge variant — stays `unknown/low` when unsure.
- Timeline is bounded 500 per agent (dropped count visible); oldest dropped — check `browser_timeline_dropped`.
- No live WebView on Linux — harness “would capture” the ChatGPT navigation sequence, not live-tested. Fresh Windows run is required for live verdict.

---

## 7. After Export — What Next

Do **not** fix automation on Windows. Copy bundles to Linux, push no code, and open an issue or start a new implementation session referencing:

- GitHub branch `diagnostics/browser-reliability-observability` (harness source) and `diagnostics/fresh-install-reliability-forensics` (this test plan)
- The exact `phase3-failure-*` bundle directory name and `BROWSER_RELIABILITY_REPORT.md` `Final diagnosis` section.

The next implementation session will read this plan + bundles and make the smallest safe patch (e.g. “re-injection after `page_initiated` navigation” or “wait for `send_enabled==true`”) with `cargo check`/`npm run build` verification and a new temporary branch.

---

## Appendix — Quick Command Reference

```bash
# On Linux dev machine (harness branch)
git status --short; git branch --show-current; git rev-parse HEAD
cargo check --tests   # 0 errors
npm run build         # in src/
git diff --check      # no whitespace

# On Windows installed app
Settings → Diagnostics → Probe Single Model → Export Diagnostics Bundle
Settings → Diagnostics → Show Diagnostic Snapshot (JSON parseable)
Settings → Diagnostics → Reliability Report (plain markdown, do NOT JSON.parse)
Settings → Diagnostics → Timeline (last 50) (JSON parseable)
```

All diagnostic bundles are under `%APPDATA%\…\diagnostics_export_*\` and are safe to share.

