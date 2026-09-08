# All-model blank WebView regression — 2026-09-08

Starting HEAD: `4e34d56af642072a1af0c0a6fed90d5809bc046e`.

## Evidence

User runtime differential: before `4e34d56`, five model WebViews visibly
rendered (with Claude and Gemini retaining provider-specific issues). After
`4e34d56`, all model WebViews opened as empty/black windows. The prior working
checkpoint is `3f243a0`; the broken checkpoint is `4e34d56`.

Fresh diagnostics show external navigation and `PageLoadEvent::Finished`, the
static generic init script, Rust navigation delivery, and visible WebView
creation all continue to work. The shared commit difference was removal of the
model-WebView UA override. Linux WebKitGTK then advertised
`AppleWebKit/605.1.15 Version/60.5 Safari/605.1.15`.

Ruled down: a dead navigation ingress (live probes arrive), navigate/show/focus
regression (source flow is unchanged), provider-specific selectors (cannot
explain all seven), and Cloudflare (not reached in the blank-page runtime).
GPU/compositor remains a fallback only because the application checkpoint—not
the machine environment—changed.

Root-cause confidence: **HIGH**, pending post-repair live visual verification.

## Repair

Linux model WebViews now use the engine/platform-consistent compatibility UA:

`Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15`

It is applied only to the persistent leader and shared nav builder paths;
non-Linux WebViews retain native identity. It is not Windows/Chromium
impersonation and makes no navigator, client-hint, webdriver, plugin, or
language spoofing changes.

The generic page classifier now identifies a completed, effectively empty
`/login` document as `empty_shell_or_hydration_stuck` before considering login
evidence. Connected Accounts now distinguishes composer readiness, login,
challenge, and empty-shell outcomes; only a detected composer is announced as
ready. Login/challenge/empty-shell windows remain visible without automatic
reload.

The `4e34d56` channel, stable-WebView reuse, challenge idempotency, exact retry
generation ownership, and narrow OAuth-popup repairs remain intact.

## Verification

Regression coverage validates the Linux UA tokens and exclusions, both model
builder paths, no destructive Connected Accounts repair, behavioral page-state
classification, and Connected Accounts outcome mapping. `cargo fmt --check`,
`cargo check`, `cargo test` (131 passed), and `npm run build` passed; the final
`git diff --check` passed. Live visual provider verification remains to be
recorded separately when an interactive desktop session is available.
