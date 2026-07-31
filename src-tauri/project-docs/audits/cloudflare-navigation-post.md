# Cloudflare / Unsupported Navigation — Post-Implementation Audit

Date: 2026-07-07

Result: **PASS — no FAIL items. Interactive Cloudflare/account verification
remains a manual runtime step.**

## Required checks

- **PASS — Unsupported/unshowable navigation is diagnosed.** Non-HTTP(S)
  top-level navigation is now blocked and converted into a redacted
  `navigation_error` browser diagnostic. The WebView error page text
  "The URL can't be shown" / curly-apostrophe variant emits
  `unshowable_url`. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:122`–`:167`,
  `:296`–`:320`, `:785`–`:807`, `:855`–`:860`, and `:1016`–`:1024`.
- **PASS — Cloudflare/CAPTCHA/security challenge is detected but not
  bypassed.** The static generic init script scans only title/body/location
  text for safe indicators: Cloudflare, checking-browser, verify-human,
  just-a-moment, cf-challenge, challenge-platform, Turnstile, CAPTCHA, and
  security-check. It emits `arena://challenge`; it does not click, solve,
  token-inject, fake success, or use a solver. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:971`–`:1028`
  and `:1040`–`:1048`.
- **PASS — Existing `captcha-detected` flow is used.** The backend emits the
  existing event on challenge detection, and the existing frontend listener and
  overlay are still used. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:274`–`:293`,
  `/home/kasun/Music/arena/consensus-arena/src/hooks/useIpcListeners.ts:76`–`:80`,
  `:135`–`:140`, and
  `/home/kasun/Music/arena/consensus-arena/src/components/overlays/CaptchaOverlay.tsx:1`–`:8`.
- **PASS — Resume works for setup challenge flow or clearly reports still
  blocked.** `captcha_resolved(agent_id)` now sends a `ResumeRequested`
  nav event. Setup readiness waits pause on a challenge, wait for resume or
  a later real ready signal, then re-enter the ready wait; repeated challenge
  detection re-emits the overlay/diagnostic. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs:467`–`:478`
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:20`–`:116`.
- **PASS — `setup-complete` cannot fire while a model is blocked by challenge
  or unshowable URL.** `run_setup` returns on readiness timeout/error and
  handles challenge/unshowable blockers before prompt injection. During send
  detection, challenge/unshowable events also return errors; `setup-complete`
  remains after every selected agent has completed real send detection.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/session_runner.rs:198`–`:230`,
  `:288`–`:345`, and `:382`–`:409`.
- **PASS — `window.open` / target-blank behavior is diagnosed without adding a
  third WebView.** Both AI windows install `on_new_window`; requests are denied
  and reported as `navigation_error` with the reason that the two-WebView
  architecture is being preserved. No popup WebView is created. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:637`–`:652`,
  `:1211`–`:1214`, and `:1232`–`:1235`.
- **PASS — Diagnostics snapshot includes blocker fields.** Browser records now
  include `last_blocker`, `last_blocker_url_redacted`,
  `last_challenge_detected_at`, `resume_attempt_count`, and `last_resume_at`;
  Settings' diagnostic type and existing collapsed JSON display expose them.
  Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:12`–`:31`,
  `:67`–`:85`,
  `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx:46`–`:64`,
  and
  `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md:124`–`:141`.
- **PASS — No cookies/tokens/API keys/prompts/model responses are logged or
  exposed by diagnostics.** Browser diagnostics store model/window identity,
  phases, timestamps, redacted URLs, and blocker labels only. Sensitive query
  keys including token/code/state/auth/key/session/cf_clearance are redacted;
  the init script sends only origin/path for the unshowable page URL. Existing
  prompts/model responses remain outside browser diagnostics. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:122`–`:167`,
  `:225`–`:250`, and `:983`–`:989`.

## Named risks

- **PASS — RISK-BLOCKING clear.** Required grep for `blocking_lock()` returned
  no matches. New diagnostic mutex use is synchronous and short; no guard is
  held across an await.
- **PASS — RISK-CHANNEL clear.** Required grep in `browser_backend.rs` found
  only `use tokio::sync::mpsc::Receiver;`, used by async consumers. The
  `on_navigation` path still uses `std::sync::mpsc::SyncSender` and `try_send`;
  no Tokio mpsc is inside `on_navigation`.
- **PASS — RISK-INITSCRIPT clear.** `GENERIC_INIT_SCRIPT` remains a static
  generic string. The added detection is model-agnostic and reads identity from
  `window.__ca_agentId` / the existing generic `window.name` restoration. No
  CAPTCHA solving or model-specific branch was added. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:901`–`:1139`.
- **PASS — RISK-NAVCLOSURE clear.** `on_navigation` captures a sync sender and
  a stable generic window label only; it does not capture an agent id, prompt,
  URL, or model-specific closure value. Agent identity still comes from
  `arena://` segments or the active diagnostics map. Evidence:
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs:785`–`:860`.
- **PASS — RISK-UNWRAP clear for touched live paths.** Grep over touched live
  files found no raw `.unwrap()` or `.expect()` calls.
- **PASS — No architecture/dependency expansion.** No dependency, Agent Brain,
  Memory, IPC argument-case, or Tauri capability change was made. The two AI
  WebViews remain `arena-leader` and `arena-nav`.

## Verification

- **PASS — Baseline `cargo check`: exit 0.** It finished the dev profile in
  37.07s with the existing non-fatal warnings.
- **PASS — Baseline `npm run build`: exit 0.** `tsc && vite build`; 1,709
  modules transformed; built in 2m25s.
- **PASS — Final `cargo check`: exit 0.** It initially waited on the existing
  build-directory lock, then finished the dev profile in 12m32s with 40
  non-fatal warnings.
- **PASS — Final `npm run build`: exit 0.** `tsc && vite build`; 1,709 modules
  transformed; built in 1m20s.
- **PASS — `git diff --check`: exit 0 with no output.**
- **PASS — Required greps:** no `blocking_lock()`; one Tokio mpsc import only;
  no `memory_store.lock().await`; secret grep matches are classified as the new
  `cf_clearance` redaction key plus existing configuration/API-key fields and
  the existing `agent_brain.rs` Authorization header. No browser diagnostics
  expose `cf_clearance`, API keys, prompts, cookies, or model responses.

## Runtime limitation retained intentionally

If ChatGPT/Cloudflare requires a true third-party browser popup that cannot be
completed inside the existing model WebView, this pass intentionally does not
add a third WebView. It reports that limitation through `browser-diagnostic`
instead, preserving the project architecture.
