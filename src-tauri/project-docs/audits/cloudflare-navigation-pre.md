# Cloudflare / Unsupported Navigation — Pre-Implementation Audit

Date: 2026-07-07

Result: **FAIL — verification blockers and unsupported navigation are not
diagnosed or resumable in the setup path.**

## Scope and evidence

The root `AGENTS.md`, `DECISIONS.md`, `PROCESS.md`, `BACKEND.md`,
`ARCHITECTURE.md`, `FRONTEND.md`, `IPC.md`, the four named latest audits, and
all requested backend/frontend/config files were read completely before edits.
Searches covered `captcha-detected`, `captcha_resolved`, `rate-limit-reached`,
`browser-diagnostic`, Cloudflare/challenge strings, unsupported/unshowable URL
strings, `on_navigation`, external URL schemes, `target=_blank`, `window.open`,
CSP, user-agent/devtools settings, builders, navigation, initialization script,
and browser diagnostic emitters. The installed Tauri 2.11.2 source was also
checked for `WebviewWindowBuilder::on_new_window`.

The worktree is dirty before this task only because the tracked generated
binary exists as modified:
`/home/kasun/Music/arena/consensus-arena/src-tauri/target/debug/consensus-arena`.
That change predates this pass and must be preserved.

## Current behavior answers

### What does `on_navigation` currently allow or deny?

`make_nav_closure` in
`/home/kasun/Music/arena/consensus-arena/src-tauri/src/browser_backend.rs`
allows every non-`arena` URL by returning `true` immediately. It only denies
handled fake `arena://ready`, `arena://response`, `arena://done`,
`arena://sent`, and `arena://log` navigations by returning `false` after
enqueuing/logging the event. Unknown `arena://` shapes return `true`.

There is no scheme allowlist, no redacted URL recording for normal navigation,
and no diagnostic for an unsupported external scheme.

### Does the app currently observe unsupported navigation URLs/schemes?

No. `navigate_agent_window` validates only the app-requested target URL before
calling `window.navigate`. Redirects, auth handoffs, custom schemes, and
browser-generated unsupported targets are not classified. `handle_page_load`
records only loaded URLs after the WebView accepts them. If WebKitGTK replaces
the page with an internal "The URL can't be shown" error page, the current
backend has no specific unshowable-url phase.

### Does `browser-diagnostic` include failed URL/scheme/phase?

Partially. The existing event payload includes `agent_id`, `window_label`,
`phase`, `url`, `message`, and `error`. Current phases are the previous
browser-loading phases (`creating`, `loading`, `ready`, `waiting_user_send`,
`consulting`, `error`, `unknown`). There is no `navigation_error`,
`unsupported_url`, `unshowable_url`, or `captcha_or_challenge` phase, and the
record has no blocker fields.

Existing URL sanitization strips all query and fragment values, which is safe,
but it does not classify sensitive query parameters or preserve non-sensitive
query keys for diagnostics.

### How is `captcha-detected` currently emitted?

Repository search found no backend emitter for `captcha-detected`. The frontend
listener exists in
`/home/kasun/Music/arena/consensus-arena/src/hooks/useIpcListeners.ts`, and
`CaptchaOverlay.tsx` can invoke `captcha_resolved`, but no browser code
currently detects a challenge and emits the event.

### Does ChatGPT-specific Cloudflare detection exist?

No. There is no ChatGPT-specific or generic Cloudflare/security-challenge
detection. There are no source matches for Cloudflare strings, Turnstile,
`cf-challenge`, "checking your browser", "verify you are human", or
"security check" outside this audit work.

### Can the user resume after solving a challenge?

Not in the setup flow. The overlay's Resume button calls `captcha_resolved`.
The Rust command only inserts the agent id into `BrowserState.captcha_resolved`
and returns. No setup wait loop, runtime wait loop, or navigation/readiness
path consumes that set. The UI can clear its overlay and show "Session
resumed", but no backend waiter is woken.

### Does `captcha_resolved` actually resume a blocked setup path?

No. `run_setup` waits only for `NavEvent::Ready`, `NavEvent::Error`, and
`NavEvent::SendDetected`. It has no event or condition for user resume. A
challenge page that never emits ready will time out and fail setup; clicking
Resume does not re-check readiness.

### Are `target=_blank` / `window.open` navigations handled?

No project source uses `WebviewWindowBuilder::on_new_window`. Tauri supports an
`on_new_window` handler, but the two AI model WebViews do not install one. If a
login or challenge flow requests a new window, the current app neither forces a
safe same-window path nor emits a diagnostic. Adding a third persistent WebView
would violate the project architecture.

### Is there a likely source-level reason for "The URL can't be shown"?

Yes, source inspection shows two plausible causes:

1. The `on_navigation` callback allows every non-`arena` scheme. If a
   Cloudflare/auth challenge redirects to a custom, `intent:`, `blob:`,
   `data:`, `file:`, OAuth helper, or otherwise unsupported target, WebKitGTK
   may display its own unshowable-url page. The app currently records neither
   the scheme nor a blocker.
2. `window.open` / `target=_blank` is not handled. A security or login flow
   that expects a browser popup may fail inside the embedded WebView. The app
   currently provides no diagnostic for that limitation.

This audit does **not** prove that Cloudflare itself failed. It proves the app
lacks safe detection/resume/diagnostic handling for exactly the class of
security-check redirects the user observed.

### What minimal changes are needed?

1. Add secret-redacted unsupported/unshowable navigation diagnostics without
   bypassing challenges or solving CAPTCHAs.
2. Add generic challenge detection in the static `GENERIC_INIT_SCRIPT` using
   title/body/location indicators only. It must emit `captcha-detected` and
   `browser-diagnostic` with phase `captcha_or_challenge`; it must not click,
   solve, or fake success.
3. Add a resume signal path: `captcha_resolved(agent_id)` should wake setup and
   readiness waiters so they re-check the model window and continue only after
   a real `arena://ready` and later a real `arena://sent`.
4. Add blocker fields to `BrowserDiagnosticRecord` and the existing
   `get_diagnostic_snapshot`: `last_blocker`, `last_blocker_url_redacted`,
   `last_challenge_detected_at`, `resume_attempt_count`, and `last_resume_at`.
5. Add an `on_new_window` handler that diagnoses popup/new-window requests and
   denies them without creating a third WebView.
6. Update IPC documentation and the existing frontend listener/toasts so the
   Captcha overlay appears with the model name and unshowable URL errors are
   understandable from the main UI and Settings diagnostics.
