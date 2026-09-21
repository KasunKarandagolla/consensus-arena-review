# Browser Reliability Observability Harness — Temporary Diagnostic Batch

> **Status: diagnostic / investigative. Does NOT fix ChatGPT/DeepSeek/Gemini login, priming, injection or submission bugs.**
> The purpose of this batch is to make the next implementation batch evidence-based.

---

## 1. Why the harness exists

Inconsistent browser failures appear only on fresh installations / fresh authenticated states (especially Windows). Prior to this harness, failures were reported as single strings (“prompt not visible”, “send not detected”) without chronological, per-window evidence. We could not answer:

- Did the page navigate after injection?
- Did the textarea survive or was it recreated?
- Did Send exist but stay disabled?
- Which model/window/operation/phase generated the error?
- Was the error website telemetry, Arena automation, network, auth, or security?

This harness makes the application capable of reconstructing the full causal chain per model WebView:

```
operation start → navigation → DOM state → injection → console error → navigation → composer recreated → send probe → failure
```

---

## 2. Architecture

- **Single new Rust module:** `src-tauri/src/browser_harness.rs` — pure, sync, no WebView, no async lock across await.
- **Two WebViews preserved:** `arena-leader` (persistent) and `arena-nav` (shared). No third window. Diagnostics never create windows.
- **Storage:** `BrowserDiagnostics` (existing) now owns a `BrowserTimeline` (new). Both are `Arc<Mutex<_>>` with `std::sync::Mutex` semantics; all async callers use already-existing `BrowserDiagnostics` handle — no `blocking_lock()` and no `tokio::sync::mpsc` in `on_navigation`.
- **JS bridge:** `GENERIC_INIT_SCRIPT` remains a single static `&str`. It is extended only with idempotent, generic console bridging (`__ca_consoleDiagnosticsInstalled`) and identity helpers. No agent-specific closure capture; identity is always `window.__ca_agentId`.
- **Frontend:** `SettingsPanel.tsx` Diagnostics section reuses the existing `get_diagnostic_snapshot` pathway and adds three harness commands. No new production UI outside the existing debug surface.

```
Rust: BrowserState → BrowserDiagnostics → BrowserTimeline (ring buffer per agent)
                                      ↘ Console diagnostics / Navigation diagnostics (existing, now classified)
JS:   GENERIC_INIT_SCRIPT → arena://console/<agent>/<category>/<severity>/<source>/<msg>/<url>
      → NavEvent::ConsoleDiagnostic → record_console_diagnostic → timeline + classification
```

All prompt/response text is **never** stored — only metadata (`value_length`, `prefix_ok`, `candidate_count`, etc.).

---

## 3. Event taxonomy

Controlled enums (serialized `snake_case`) are used instead of arbitrary strings.

### Window lifecycle
`window_created`, `window_destroyed`, `navigation_started`, `navigation_committed`, `navigation_finished`, `document_loaded`, `dom_content_loaded`, `url_changed`, `page_reloaded`, `document_replaced`

### Authentication
`login_state_unknown`, `login_required`, `login_page_detected`, `login_interaction_started`, `login_interaction_completed`, `login_state_authenticated`, `logout_detected`, `authentication_redirect`, `authentication_failure`

### Browser blockers
`challenge_detected`, `captcha_detected`, `cloudflare_detected`, `security_blocked`, `network_blocked`, `page_health_blocked`

### Composer
`composer_probe_started`, `composer_detected`, `composer_lost`, `input_detected`, `input_lost`, `send_probe_started`, `send_detected`, `send_lost`, `attachment_detected`

### Priming
`priming_started`, `priming_injection_started`, `priming_injection_completed`, `priming_injection_failed`, `priming_prompt_visible`, `priming_send_enabled`, `priming_send_disabled`, `priming_reinjection_started`, `priming_reinjection_completed`, `priming_completed`, `priming_failed`

### Active operation
`active_prompt_injection_started`, `active_prompt_injection_completed`, `active_prompt_injection_failed`, `active_submit_started`, `active_submit_attempt`, `active_submit_completed`, `active_submit_failed`, `response_started`, `response_observed`, `response_completed`

### Diagnostics
`dom_snapshot`, `composer_snapshot`, `send_snapshot`, `console_error`, `console_warning`, `javascript_error`, `unhandled_rejection`, `resource_load_error`, `automation_error`, `arena_protocol_error`

### State machine
`phase_changed`, `state_changed`, `retry_started`, `retry_exhausted`, `stale_signal_detected`, `operation_cancelled`

The harness emits these through `BrowserDiagnostics::emit_harness_event()` at the same call sites that already update `BrowserDiagnosticRecord` — no new timing or retry changes.

---

## 4. Diagnostic lifecycle

### Operation IDs (correlation)

Every important browser operation receives a correlation ID:

- `setup-chatgpt-g1`
- `login-chatgpt-g1`
- `priming-chatgpt-g1`
- `active-turn-chatgpt-g1-t1`
- `submit-deepseek-g1-t2`
- `diagnostic-kimi-g1`

All events generated during that operation carry the same `operation_id`. `BrowserDiagnostics::set_operation()` is called at:

- `record_setup_expected_agent()` → `priming-<agent>-g<gen>`
- `navigate_agent_window()` → `setup-<agent>-g<gen>` (then overwritten by priming)
- `BrowserState::begin_active_turn()` → `active-turn-<agent>-g<gen>-t<turn>`
- Single-model probe → `diagnostic-<agent>-g<gen>`

### Phases

Every event carries `phase`. When unavailable, `phase = unknown` (never invented).

Spec phases: `idle`, `navigation_started`, `login`, `setup`, `composer_detection`, `priming`, `waiting_for_send`, `submitting`, `waiting_for_response`, `response_capture`, `completed`, `failed`, plus extended internal phases (`queued`, `creating`, `loading`, `ready`, `consulting`, `captcha_or_challenge`, `navigation_error`, `unshowable_url`, `error`, `unknown`). `HarnessPhase::from_str()` maps unknown strings to `unknown`.

### Navigation forensics

For every navigation after an operation begins, `record_navigation()` stores:

- `from_url`, `to_url`, `timestamp`, `operation_id`, `phase`, `same_document` (where known), `navigation_reason`, `confidence`, `cause`, `arena_requested`.

`classify_navigation_reason()` returns `unknown/low` unless evidence supports a stronger claim (`reload`, `login_redirect`, `challenge`, `authentication`, `error_page`). `NavigationDiagnosticEntry` keeps `cause = arena_requested | page_initiated | unknown` for exact correlation with `PendingArenaNavigation` (5 s window, URL match).

### Priming diagnostics (ChatGPT sequence)

Around priming injection the harness emits:

- **Before injection:** `priming_injection_started` + `dom_snapshot` (input/composer/send existence)
- **Immediately after:** `dom_snapshot` + `priming_injection_completed/failed` with `prefix_ok`, `suffix_ok`, `visible_length`, `send_enabled`, `target_tag/role/contenteditable`, `method`, `error` + element identities (`input#N`, `composer#N`, `send#N`)
- **During waiting:** repeated `SendProbe` handling emits `composer_probe_started`, `input_detected/lost`, `send_detected/lost`, `dom_snapshot`, `page_state_hint`, `page_health_hint`
- **On navigation/reload:** `navigation_started/finished`, `url_changed`, `document_loaded`, `dom_snapshot`, `page_reloaded` etc. — proving whether prompt survived.

### Sanitized DOM snapshots

```json
{
  "input": { "tag": "TEXTAREA", "exists": true, "visible": true, "value_length": 262 },
  "composer": { "exists": true, "candidate_count": 15 },
  "send": { "exists": true, "candidate_count": 1, "enabled": false, "text": "", "aria_label": "Send" },
  "attachment": { "exists": true, "candidate_count": 1 },
  "input_identity": "textarea#262",
  "composer_identity": "composer#262",
  "send_identity": "send#1"
}
```

Never records prompt text, cookies, tokens, passwords, addresses, storage, or responses.

### Element identity

Ephemeral diagnostic IDs (`textarea#7`, `composer#12`, `button#19`) are emitted in `DomSnapshot`. They are diagnostic-only and never affect application behavior. Comparing `before` vs `after` snapshots tells `same input survived` vs `old input destroyed, new input created`.

### Console + JS capture

`GENERIC_INIT_SCRIPT` now bridges:

- `console.error` / `console.warn` (preserving original via `_ce.apply` / `_cw.apply`, with `addEventListener('error')` and `addEventListener('unhandledrejection')`, not replacing handlers)
- via `arena://console/<agent>/<category>/<severity>/<source>/<msg>/<url>`

Each record: `timestamp`, `agent_id` (authoritative from `window.__ca_agentId` → `active_by_window` mapping, not URL), `window_label`, `window_kind`, `url`, `phase`, `operation_id`, `severity`, `category`, `message`, `stack` (truncated, 800 chars). Categories: `javascript_exception`, `unhandled_rejection`, `console_error`, `console_warning`, `navigation_error`, `automation_error`, `injection_error`, `submission_error`, `challenge_blocker`, `login_blocker`, `diagnostic_bridge_error`.

`classify_console_error()` separates:

- `website_console_error / warning / network / resource` — e.g. `Error sending segment performance metrics TypeError: Load failed` → `website`, `automation_related=false`
- `arena_*` — injection/submission/navigation errors
- `environment` — `diagnostic_bridge_error`
- `authentication` — `login_blocker`
- `security` — `captcha`/`cloudflare`/`challenge`

No secret values are stored (`MAX_CONSOLE_MESSAGE_LENGTH=2048`, truncated with `[truncated]`, dedup within 30 s, bounded to 20 per agent).

### Model identity correctness

Every `BrowserEvent` contains `agent_id`, `display_name`, `window_label`, `window_kind`, `expected_agent_id`. `record_console_diagnostic()` resolves `attributed_agent = active_by_window[window_label] else reported_agent_id`; a mismatch is logged (`[CONSOLE] attribution mismatch`) but never attributes a DeepSeek error to ChatGPT because the nav window changed pages.

---

## 5. Redaction rules

- **URLs:** `redact_url()` parses via `tauri::Url`, redacts query keys containing `token|code|state|auth|key|session|cf_clearance` to `[REDACTED]`, drops fragment. Fallback `prefix?[REDACTED]` for unparsable URLs. `sanitize_url()` is alias.
- **Console/details:** `sanitize_details_value()` redacts `Bearer <token>`, `sk-*`, `token-*`, `api_key/apikey`, and any whitespace token ≥32 chars containing both alpha+digit without `/` or `:`. Truncates to 2048/4096 bytes with `[truncated]`, strips control chars except `\n`/`\t`.
- **Never stored:** API keys, OAuth tokens, cookies, passwords, Authorization headers, localStorage/sessionStorage, Gmail addresses, private conversation text, complete prompt or response, project brief content. `export_blueprint` and `get_prompt_template` remain plain-string exceptions; all other collection commands return `serde_json::to_string()` JSON strings.

---

## 6. Ring-buffer limits

- **Timeline:** `BROWSER_EVENT_RING_BUFFER_LIMIT = 500` events per agent (`HashMap<String, VecDeque<BrowserEvent>>`). When wrapping, `dropped` is incremented and exposed as `events_dropped_count` per agent and in the snapshot’s `browser_timeline_dropped` map. `TimelineRing::push()` pops front when full.
- **Console:** `MAX_CONSOLE_DIAGNOSTICS_PER_AGENT = 20` (existing). Dedup window 30 s, drops rapid repeats. Bounded.
- **Navigation:** `MAX_NAVIGATION_DIAGNOSTICS_PER_AGENT = 10` (existing). Bounded.
- **Reports:** markdown limits timeline preview to last 200 total events (50 per agent section) to stay lightweight.

All buffers are bounded; no unbounded growth, no continuous full-DOM serialization, no high-frequency polling beyond existing 500 ms readiness checks.

---

## 7. Export format

### Commands

- `get_diagnostic_snapshot() → JSON string` — now includes `browser_timeline: BrowserEvent[]`, `browser_timeline_dropped: {agent: count}`, `browser_timeline_count: number`, plus existing `browser_diagnostics`, `browser_console_*`, `navigation_diagnostics`, `console_diagnostics`.
- `get_browser_timeline() → JSON string` — sorted chronological `BrowserEvent[]`.
- `get_browser_reliability_report() → plain string` — markdown (see below), NOT JSON-parsed on frontend.
- `export_browser_diagnostics() → JSON string` — `{export_dir, report, events, browser_diagnostics, navigation_history, console_errors}` paths under `app_data_dir/diagnostics_export_YYYYMMDD_HHMMSS/`. Contains:
  - `BROWSER_RELIABILITY_REPORT.md`
  - `events.json`
  - `browser-diagnostics.json`
  - `navigation-history.json`
  - `console-errors.json`
- `run_single_model_diagnostic(agent_id: string) → JSON string` — dev-only probe for `chatgpt|claude|gemini|deepseek|qwen|glm|kimi` (including `is_custom` participants). Reuses the shared `arena-nav` window, never creates a third WebView. Probes: `create/navigate`, `page readiness`, `login state`, `composer/input/send/attachment detection`, `injection capability` (method/prefix/suffix), `navigation stability` (timeout 15 s, polls diagnostics). Refused while `session_active`.
- `launch_connected_account(agent_id)` and `get_brain_status()` remain.

### Report (`BROWSER_RELIABILITY_REPORT.md`)

Per model:

- Model, Agent ID, Window, Session, Current phase, Current URL, auth/blocker state, current operation, `events_dropped`
- Timeline (last 50 per agent, chronological)
- Navigation events
- Console errors
- DOM/composer state changes (latest `input_found`, `send_enabled`, `candidate_count`, `page_state_hint`, `last_error`)
- Priming result (`setup_completion_reason`, `prompt_injected_at`, `error`)
- Submission result (`active_turn`, `active_auto_submit_succeeded`, `method`, `error`)
- **Final diagnosis — evidence-based only.** Example:

```
RESULT: FAILED
Observed sequence:
15:28:45.215 Prompt injection started.
15:28:45.300 Prompt visible.
15:28:45.301 Send disabled.
15:28:46.102 Navigation started.
15:28:46.500 Old composer disappeared.
15:28:46.700 New composer detected.
15:28:46.701 Prompt no longer present.
Likely failure subsystem: POST-INJECTION NAVIGATION / COMPOSER LIFECYCLE Confidence: HIGH
```

The generator (`generate_reliability_report_markdown()`) checks `last_navigation` after `prompt_injected_at`: if `cause=page_initiated && !arena_requested` → `POST-INJECTION NAVIGATION`; else if `send_enabled==Some(false)` → waiting for send; else if `prompt_injection_error` → injection. It never invents speculative root causes.

---

## 8. How to interpret reports

1. Open `Settings → Diagnostics → Reliability Report` or run `get_browser_reliability_report`.
2. For a failed model, read its **Timeline** top-to-bottom. Verify `operation_id` groups the sequence.
3. Check **Navigation events** `cause`. `arena_requested=true` = Arena asked for it; `page_initiated` = website reloaded/redirected itself. A `page_initiated` navigation between `prompt_injection_completed` and `send_detected` explains `prompt disappears → composer recreated → pipeline stuck`.
4. Check **DOM snapshots** before vs after injection: `input_identity`/`composer_identity` change proves recreation.
5. Check **Console diagnostics** `classified_origin`. `website` + `automation_related=false` is not an Arena bug (e.g. Segment). `arena` + `automation_error` needs fixing.
6. Use **Export Diagnostics Bundle** to share evidence: another engineer can reconstruct `WHAT/WHEN/WHERE/WHICH MODEL/WINDOW/OPERATION/PHASE/DOM/NAVIGATION/ERROR/WHAT CHANGED/WHAT FAILED` without guessing.
7. Run `get_browser_timeline()` for the full sorted JSON if the markdown truncates.

---

## 9. Known limitations

- Harness is **passive** — it cannot force a website to keep a composer; it only records that it was destroyed.
- `same_document` in navigation forensics is `None` unless the WebView exposes it; we default to `unknown` rather than guess.
- DOM snapshots are metadata, not full dumps — they answer “exists/visible/enabled/candidate_count/identity/length” without leaking prompt/response.
- Console capture cannot distinguish every website-specific “challenge” variant without site knowledge; it classifies by `console_error` text + source + `page_state_hint` but stays `unknown/low` when unsure.
- Timeline is bounded: the oldest events are dropped after 500 per agent (dropped count is visible).
- No live browser test is performed in the current Linux environment — the `ChatGPT priming → send disabled → page refresh → prompt disappears` sequence is **verified by instrumentation** (the harness would capture it as `prompt_injected → prompt_visible → send_disabled → navigation_started (page_initiated) → composer_lost → new composer_detected → prompt no longer present`), not by a live WebView run. See section 11.

---

## 10. What is intentionally NOT fixed in this batch

Per rule, this batch **did not** change:

- ChatGPT injection strategy / selector
- DeepSeek Send detection heuristic
- Gemini login automation
- Login detection heuristics
- Navigation recovery behavior
- Priming state machine semantics
- Send-button selection semantics
- Retry counts or browser timing to hide failures
- Two-WebView architecture (max 2, one leader + one nav)
- Security/blocker bypass logic

If instrumentation exposed a bug, it is recorded (timeline + report) and left for the next implementation batch.

---

## 11. How this will support the next browser-reliability implementation batch

The next batch can answer, from exported evidence alone:

- *ChatGPT:* Did the page navigate after injection? Did the original textarea survive? Did Send exist but remain disabled? Did React/composer state change? Did page reload before Send became enabled?
- *Gemini:* Did the login page receive the click? Did navigation occur after clicking Next?
- *DeepSeek:* Which exact button was selected? (via `send_identity` + `candidate_count`)
- *Any model:* Was this website / Arena / network / login / challenge? Which model/window generated the event? What happened immediately before the failure?

With `BROWSER_RELIABILITY_REPORT.md` + `events.json` + `browser-diagnostics.json` + `navigation-history.json` + `console-errors.json`, an engineer or AI can independently diagnose the failure without guessing, then implement targeted fixes (e.g. “re-injection after `page_initiated` navigation” or “wait for `send_enabled==true` before considering priming complete”) in the next batch, reusing the same harness for regression evidence.

---

## Verification

- `cargo check` — 0 errors (56 warnings, none harness-related).
- `cargo check --tests` — 0 errors.
- `npm run build` — PASS (1709 modules, 345 kB JS).
- `git diff --check` — PASS.

## Files

- `src-tauri/src/browser_harness.rs` — NEW: taxonomy, phases, navigation forensics, DOM snapshots, redaction, ring buffer, report.
- `src-tauri/src/browser_backend.rs` — extended: `BrowserDiagnostics.timeline`, `current_operation`, `setup_generation()`, `emit_harness_event()`, `emit_dom_snapshot()`, navigation forensics, console classification, auth/challenge mapping, SendProbe harness, active-turn harness.
- `src-tauri/src/commands.rs` — extended: `get_browser_timeline`, `get_browser_reliability_report`, `export_browser_diagnostics`, `run_single_model_diagnostic`; `get_diagnostic_snapshot` now includes `browser_timeline*`.
- `src-tauri/src/main.rs` — register new commands.
- `src/panels/SettingsPanel.tsx` — diagnostics export/report/timeline/single-probe UI.
- `BROWSER_RELIABILITY_OBSERVABILITY.md` — this file.
