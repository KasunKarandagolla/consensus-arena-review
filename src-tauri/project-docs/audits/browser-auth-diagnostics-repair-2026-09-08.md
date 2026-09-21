# Browser/Auth Diagnostics Repair — 2026-09-08

Source-proven repairs in this session:

- Connected Accounts and the single-model diagnostic now treat Tauri's named
  window registry as authoritative. A missing `arena-nav` clears the cached
  nav handle and Connected Accounts lease before `ensure_nav_window` creates
  the one valid replacement.
- Connected Accounts challenge handling is non-terminal. Challenge and resume
  signals remain on the operation channel until genuine same-agent readiness,
  an error/unshowable/cancellation/channel-close outcome, or the deadline.
- The normal Settings diagnostic UI uses the maintenance-gated plain-Markdown
  `get_diagnostic_brief`; raw forensic export remains available and now starts
  with `DIAGNOSTIC_BRIEF.md`.
- Historical note, superseded by runtime evidence: the Safari-17 compatibility
  UA and init-script switches were diagnostic experiments, not proof that
  native WebKitGTK Version/60.5 caused the blank regression. The proven
  boundary was Arena's document-start automation: Claude succeeded in the
  same persistent native-WebKit environment when it was absent. Production now
  uses native UA and post-load automation activation. No live seven-model
  visual matrix was performed in this historical repair.

Google authentication remains provider-controlled and can be unavailable in
embedded model WebViews, including Gemini. Popup allowlisting is not evidence
of reliable embedded Google sign-in.
