# Browser human-verification forensics — 2026-09-07

> Investigation executed on 2026-09-08 (Asia/Colombo). The filename follows the
> requested 2026-09-07 audit series. This document intentionally preserves the
> pre-repair evidence and appends, rather than replaces it, after repair.

## Scope and evidence rules

Principal question: can an Arena change cause, amplify, restart, invalidate, or
incorrectly classify Claude/Cloudflare human verification, especially after a
human completes verification?

Classification vocabulary in this audit:

- **PROVEN**: the source and control flow demonstrate the defect.
- **STRONG CANDIDATE**: the source demonstrates a technically credible local
  amplification mechanism, but Cloudflare's server-side decision is external.
- **PLAUSIBLE**: capable of contributing, without enough evidence to claim the
  observed loop was caused by it.
- **DISPROVEN**: searched or traced and not active in the runtime path.

Source is authoritative for current behavior. Git history is authoritative for
introduction claims. Runtime behavior is authoritative for Cloudflare outcomes.
No Cloudflare/Turnstile bypass, token manipulation, iframe manipulation, or
automated solving is in scope.

## Baseline Git state

- Repository: `/home/kasun/Music/arena/consensus-arena`
- Branch: `forensics/browser-auth-diagnostics`
- Starting HEAD: `3f243a028ee3f9ab27bed1103780433fd294f4a5`
- Expected parent confirmed: `a3ab85f`
- Remote: `origin = https://github.com/KasunKarandagolla/consensus-arena-review.git`
- Remote branch SHA after `git fetch --prune origin`: same as starting HEAD
- Worktree: clean
- Histories: local and remote branch aligned; no fast-forward or integration
  was necessary.

## Baseline build state

- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check`:
  **PASS**, 72 existing warnings.
- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo test`:
  **PASS**, 120 passed, 0 failed; 62 existing warnings.
- `cd /home/kasun/Music/arena/consensus-arena && npm run build`:
  **PASS**, 1,710 modules transformed in 43.31 s.
- `cd /home/kasun/Music/arena/consensus-arena && git diff --check`:
  **PASS**.
- `package.json` scripts inspected: `dev`, `build`, `preview`, and `tauri`; no
  repository frontend lint or test script exists.

These results establish that later failures are not baseline compile/test
failures. Warnings are pre-existing unless the post-repair diff proves
otherwise.

## Commit timeline and differential evidence

- `4111499`: initial source snapshot already destroyed both named Arena WebViews
  when `create_windows` started a session.
- `7d78d74`: introduced browser verification blocker diagnostics.
- `7d41f7d`: hardened readiness/retry behavior and contains the active
  `wait_for_ready` challenge-wait branch later modified at HEAD.
- `a3ab85f`: checkpoint immediately before the investigated large change.
- `3f243a0`: introduced the fixed Windows Chrome 126 user agent; the Connected
  Accounts fresh-channel plus destructive-recreation workaround; expanded
  browser diagnostics; setup navigation recovery; and broad response/retry
  guards.

The claims above were independently checked using `git diff a3ab85f..3f243a0`,
`git log --follow`, `git blame`, `git log -S`, and `git log -G` over every named
browser/auth symbol. `CHROME_USER_AGENT`, its builder calls, and the Connected
Accounts `old.destroy()` repair all first appear in `3f243a0`. Session-start
destruction predates it, so that part is a longstanding defect rather than a
new regression.

## Complete `a3ab85f..3f243a0` file classification

Every changed path is classified below. A = direct browser/auth/navigation
behavior or its controlling UI/IPC; B = indirect session/setup/diagnostic
dependency; C = unrelated to the investigated defect. All A source files were
read completely. Relevant control-flow portions and dependencies of all B files
were inspected. C files were inventoried; no dependency search led from them
back into the browser/auth path.

| Class | Path | Basis |
|---|---|---|
| C | `.gitignore` | repository ignore rules only |
| B | `BETA_READINESS_AUDIT.md` | release-level evidence and browser references |
| B | `BETA_RELEASE_COMPREHENSIVE_AUDIT.md` | release-level evidence and browser references |
| C | `HACKATHON_MODE_DESIGN.md` | hackathon feature design |
| B | `agent_system.md` | participant orchestration description |
| B | `leader_priming.md` | setup prompt content; no browser mechanism |
| B | `participant_priming.md` | setup prompt content; no browser mechanism |
| B | `project-docs/audits/beta-audit/00-file-inventory.md` | inventory evidence |
| B | `project-docs/audits/beta-audit/01-module-map.md` | module dependency evidence |
| B | `project-docs/audits/beta-audit/02-build-baseline.md` | historical build evidence |
| B | `project-docs/audits/beta-audit/03-findings-backend.md` | backend findings |
| C | `project-docs/audits/beta-audit/04-findings-frontend.md` | empty/non-browser audit stub |
| C | `project-docs/audits/beta-audit/05-findings-cross-cutting.md` | empty audit stub |
| C | `project-docs/audits/beta-audit/06-findings-ui-flow.md` | empty audit stub |
| B | `project-docs/audits/beta-audit/07-doc-mismatches.md` | stale-doc evidence |
| B | `project-docs/audits/beta-audit/08-progress-tracker.md` | repair provenance |
| B | `project-docs/audits/beta-audit/09-open-questions-for-human.md` | runtime validation context |
| B | `project-docs/audits/beta-audit/10-command-tracking.csv` | historical verification evidence |
| B | `project-docs/audits/beta-audit/cargo-check-baseline.txt` | historical Rust baseline |
| B | `project-docs/audits/beta-audit/npm-build-baseline.txt` | historical frontend baseline |
| C | `src-tauri/gen/schemas/acl-manifests.json` | generated schema removal |
| C | `src-tauri/gen/schemas/capabilities.json` | generated schema removal |
| C | `src-tauri/gen/schemas/desktop-schema.json` | generated schema removal |
| C | `src-tauri/gen/schemas/linux-schema.json` | generated schema removal |
| A | `src-tauri/project-docs/ARCHITECTURE.md` | WebView ownership/lifecycle contract |
| A | `src-tauri/project-docs/BACKEND.md` | backend browser/session contract |
| B | `src-tauri/project-docs/CLAUDE-PROJECT-INSTRUCTIONS.md` | project constraints and claims |
| A | `src-tauri/project-docs/DECISIONS.md` | binding WebView/channel decisions |
| A | `src-tauri/project-docs/IPC.md` | CAPTCHA/setup/browser IPC contract |
| A | `src-tauri/project-docs/audits/auth-handoff-pre.md` | auth handoff history |
| A | `src-tauri/project-docs/audits/beta-stabilization-2026-09-07.md` | browser stabilization history |
| A | `src-tauri/project-docs/audits/browser-connected-accounts-post.md` | Connected Accounts history |
| A | `src-tauri/project-docs/audits/browser-connected-accounts-pre.md` | Connected Accounts history |
| B | `src-tauri/project-docs/audits/connected-session-hackathon-prompts-post.md` | setup/session integration evidence |
| B | `src-tauri/project-docs/audits/connected-session-hackathon-prompts-pre.md` | setup/session integration evidence |
| A | `src-tauri/project-docs/audits/differential-runtime-repair-post.md` | browser differential claims |
| A | `src-tauri/project-docs/audits/differential-runtime-repair-pre.md` | browser differential claims |
| B | `src-tauri/project-docs/audits/final-beta-finalization-audit.md` | release/session evidence |
| C | `src-tauri/project-docs/audits/hackathon-mode-implementation-plan.md` | hackathon-only plan |
| C | `src-tauri/project-docs/audits/hackathon-mode-post.md` | hackathon-only audit |
| C | `src-tauri/project-docs/audits/hackathon-mode-pre.md` | hackathon-only audit |
| B | `src-tauri/project-docs/audits/pre-existing-condition-registry.md` | provenance of pre-existing defects |
| A | `src-tauri/project-docs/audits/runtime-failure-forensic-review.md` | browser/runtime failure history |
| A | `src-tauri/project-docs/audits/runtime-reliability-repair-plan.md` | retry/browser repair design |
| A | `src-tauri/project-docs/audits/runtime-reliability-repair-post.md` | retry/browser repair evidence |
| C | `src-tauri/project-docs/mockup/hackathon-mini-window.html` | static hackathon mockup |
| B | `src-tauri/project-docs/mockup/windows/active.html` | active-turn UI reference |
| B | `src-tauri/project-docs/mockup/windows/ask-user.html` | AskUser UI reference |
| A | `src-tauri/project-docs/mockup/windows/captcha.html` | CAPTCHA UI reference |
| B | `src-tauri/project-docs/mockup/windows/debug-panel.html` | browser diagnostics UI reference |
| C | `src-tauri/project-docs/mockup/windows/empty.html` | static empty-state mockup |
| C | `src-tauri/project-docs/mockup/windows/hackathon-config.html` | hackathon UI mockup |
| A | `src-tauri/project-docs/mockup/windows/priming.html` | setup/challenge UI reference |
| B | `src-tauri/project-docs/mockup/windows/rate-limit.html` | distinct blocker UI reference |
| A | `src-tauri/project-docs/mockup/windows/settings.html` | Connected Accounts UI reference |
| A | `src-tauri/project-docs/mockup/windows/setup.html` | setup lifecycle UI reference |
| B | `src-tauri/src/agent_brain.rs` | consumes participant responses |
| C | `src-tauri/src/agentic_manager.rs` | no browser/auth call path |
| B | `src-tauri/src/blueprint_store.rs` | session output persistence only |
| A | `src-tauri/src/browser_backend.rs` | WebView builders, callbacks, JS, state machines |
| A | `src-tauri/src/browser_harness.rs` | navigation/action classification and diagnostics |
| B | `src-tauri/src/checkpoint.rs` | session resume metadata |
| A | `src-tauri/src/commands.rs` | session, resume, CAPTCHA, Connected Accounts commands |
| B | `src-tauri/src/context_manager.rs` | session turn/context state |
| A | `src-tauri/src/errors.rs` | retry/permanent error classification |
| C | `src-tauri/src/hackathon.rs` | hackathon-only runtime |
| A | `src-tauri/src/main.rs` | process-lifetime AppState construction |
| A | `src-tauri/src/orchestrator.rs` | AppState/BrowserState lifetime ownership |
| C | `src-tauri/src/proxy_manager.rs` | no WebView proxy/profile wiring |
| A | `src-tauri/src/response_router.rs` | active navigation/injection/retry/challenge flow |
| A | `src-tauri/src/session_runner.rs` | setup navigation/injection/challenge flow |
| A | `src-tauri/src/session_vault.rs` | cookie/session persistence boundary |
| B | `src-tauri/src/settings_store.rs` | participant URL/config persistence |
| B | `src-tauri/src/transcript_store.rs` | transcript/session persistence only |
| B | `src/App.tsx` | overlay/listener composition |
| B | `src/components/layout/Sidebar.tsx` | session navigation controls |
| B | `src/components/overlays/RateLimitOverlay.tsx` | distinct blocker state |
| B | `src/components/shared/InputBar.tsx` | active/manual prompt controls |
| A | `src/components/views/SetupView.tsx` | setup retry/manual recovery controls |
| A | `src/hooks/useIpcListeners.ts` | CAPTCHA/setup/browser event handling |
| C | `src/index.css` | styling only |
| C | `src/panels/MemoryPanel.tsx` | memory UI only |
| A | `src/panels/SettingsPanel.tsx` | Connected Accounts launch UI |
| A | `src/stores/useAppStore.ts` | singular CAPTCHA/setup browser state |
| C | `src/windows/HackathonMiniWindow.tsx` | hackathon-only UI |

## Pre-repair findings

### F1 — Contradictory fixed browser identity

- Severity: **High**
- Confidence: **High** for Arena's defect; **Medium** that it influences a
  particular Cloudflare decision
- Status: **PROVEN CODE DEFECT / STRONG CANDIDATE amplification**
- Introduced by: `3f243a0`
- Source: `src-tauri/src/browser_backend.rs`, `CHROME_USER_AGENT` and the
  builders in `create_windows` and `ensure_nav_window` (pre-repair lines 21,
  6704, 6730, and 6772).
- Git evidence: `git log -S CHROME_USER_AGENT` and blame identify only
  `3f243a0`.

Mechanism:

```text
3f243a0 fixed Windows Chrome 126 UA
  -> Linux WebKitGTK / Windows WebView2 sends a claimed identity selected by Arena
  -> claimed UA may contradict the real engine/platform and client hints
  -> a challenge system may treat the inconsistent browser identity as risk
  -> Arena can plausibly increase challenge frequency
```

The final Cloudflare inference is necessarily external. The Arena defect is not
speculative: the app deliberately misstates browser/platform identity. Tauri's
builder documents `user_agent` as a custom override; the safe default is the
native WebView identity. No Arena feature reads or requires this constant.

Smallest safe remediation: remove the constant and every builder override. Do
not replace it with another spoofed identity. Keep passive UA diagnostics.

### F2 — Dropped process-lifetime receiver and destructive channel repair

- Severity: **Critical**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**
- Introduced by: disconnected initial channel predates/exists at `3f243a0`;
  destructive Connected Accounts workaround introduced by `3f243a0`.
- Source: `AppState::new` in `src-tauri/src/orchestrator.rs` (pre-repair lines
  226-230) creates `(nav_tx, _nav_rx)` and drops the receiver. `make_nav_closure`
  permanently captures a sender. `launch_connected_account` in
  `src-tauri/src/commands.rs` (pre-repair lines 2651-2687) replaces `nav_tx`,
  destroys the shared nav window, and rebuilds it to capture the new sender.

Why the old channel disconnects: the only `std::sync::mpsc::Receiver` created by
`AppState::new` is dropped on return. Every send through the original sender is
therefore disconnected.

Why recreation appears to repair it: replacing `BrowserState.nav_tx` cannot
change a closure already installed in a WebView. Destruction forces a new
builder call, whose new callback captures the replacement sender. A short-lived
bridge thread then owns the replacement receiver only for that command/session.

Mechanism:

```text
AppState drops nav_rx
  -> initial callback sender is permanently disconnected
  -> Connected Accounts replaces BrowserState.nav_tx
  -> existing callback still owns old sender
  -> command destroys/rebuilds arena-nav
  -> new callback captures new sender and signals work
  -> authenticated/challenge document and fragile in-page state are discarded
```

This can directly restart a page-level verification flow. Native persistent
cookies may survive, but document state, sessionStorage, transient OAuth/challenge
state, JS state, and the exact verified page do not have to survive destruction.

Smallest safe remediation: create one process-lifetime std navigation ingress
and bridge/dispatcher, keep every WebView callback bound to it, and attach the
single current async consumer without replacing the ingress. Reuse healthy
named windows; rebuild only if a named window does not exist.

### F3 — Session creation also destroys both healthy named WebViews

- Severity: **High**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**, longstanding rather than introduced by 3f243a0
- Source: `create_windows` in `browser_backend.rs` pre-repair lines 6667-6675.
- Git evidence: blame traces the destructive loop to the initial `4111499`
  snapshot.

Mechanism:

```text
human verifies or logs in through Connected Accounts
  -> start_session calls create_windows
  -> both named WebViews are destroyed
  -> fresh documents start at about:blank and navigate again
  -> page-local verification/OAuth/session state is lost
  -> external challenge can legitimately run again
```

Native profile cookies are not proof that a specific verified document or
sessionStorage survives. Rebuild only when a named window is absent; reuse the
two stable WebViews across Connected Accounts, session start, and recovery.

### F4 — Repeated challenge terminates active readiness wait

- Severity: **High**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**
- Source: `wait_for_ready` in `browser_backend.rs` pre-repair lines 3457-3504.

The first `ChallengeDetected` enters a challenge loop. A second signal for the
same still-present challenge immediately returns `CaptchaRequired`. This is not
idempotent. It differs from setup readiness, which records and remains waiting.
Although `CaptchaRequired` is classified permanent and therefore does not itself
consume the ordinary navigation retry budget, it prematurely fails the active
participant and can cause higher-level recovery/participant switching.

Smallest safe remediation: repeated same-agent challenge stays in the same
wait; Resume is only a request to check again; only same-agent `Ready` proves
resolution. Preserve abort/channel/unshowable terminal handling.

### F5 — Post-priming challenge escapes to setup restart and reinjection

- Severity: **Critical**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT / STRONG CANDIDATE loop amplifier**
- Source: `run_setup` post-injection proof loop in `session_runner.rs`
  pre-repair lines 987-1110, plus the outer setup recovery loop in
  `commands.rs` pre-repair lines 334-416.

Mechanism:

```text
priming prompt injected
  -> ChallengeDetected during submission-proof wait
  -> run_setup returns CaptchaRequired
  -> outer command waits for ResumeRequested
  -> Resume reruns run_setup from the beginning
  -> navigate_agent_window performs another full navigation
  -> priming is injected again
  -> external verification may be challenged again
```

This exactly supplies an Arena-controlled transition capable of turning one
challenge into a navigation/reinjection loop. The human Resume click is not the
proof, but it triggers the retry path that navigates.

Smallest safe remediation: keep the post-priming proof wait in place during a
challenge. Repeated challenge and Resume remain non-terminal. Only genuine
same-agent `Ready` exits challenge-wait into bounded on-page recovery, without
rerunning the outer setup navigation.

### F6 — Active retry suppression uses setup-era, agent-only evidence

- Severity: **High**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**
- Source: `BrowserDiagnostics::has_response_observed_after_injection` in
  `browser_backend.rs` pre-repair lines 1128-1135 and
  `should_retry_after_failure` in `response_router.rs` pre-repair lines 64-99.

`response_observed_after_injection` is an agent-level setup field. It can be set
by setup response evidence and persists as later active turns use the record.
The retry guard does not correlate response agent + active turn + setup/window
generation. It can suppress a legitimate later retry and cannot prove that a
response belongs to the failing active injection.

Smallest safe remediation: record exact active response turn and generation
only from exact `(agent_id, turn)` Response/Done/ManualResponse events; reset it
when a new logical turn/generation begins; require those keys in retry checks.

### F7 — Retry navigation-skip guard lacks cause/generation/window ownership

- Severity: **Medium**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**
- Source: `BrowserDiagnostics::can_skip_navigation_on_retry` in
  `browser_backend.rs` pre-repair lines 1138-1150 and its active route call in
  `response_router.rs` pre-repair line 2202.

The guard checks only agent record URL equality and `composer_detected`. It does
not require the current setup generation, current assigned window/agent, or an
Arena-requested navigation cause. A stale record can therefore authorize reuse
of a page whose live ownership has moved.

Smallest safe remediation: correlate generation, active window assignment, the
last navigation entry, explicit Arena cause, blocker state, and composer state.

### F8 — Repeated active challenge events reset the nominal 600-second bound

- Severity: **Medium**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**
- Source: `wait_for_response` in `response_router.rs` pre-repair lines
  2508-2608.

The inner loop calls `timeout(Duration::from_secs(600), recv())` on every
iteration. Each repeated challenge or unrelated event starts a new 600-second
timer, so the documented bound is not an absolute bound.

Smallest safe remediation: compute one deadline and use `timeout_at` for all
receives inside that challenge episode. This changes no verification behavior;
it only makes the bound truthful.

### F9 — Allowed OAuth popup is mislabeled as a navigation blocker

- Severity: **Medium**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT**, not evidence that OAuth itself is broken
- Source: `make_new_window_handler` in `browser_backend.rs` pre-repair lines
  3276-3319 sends an `UnsupportedNavigation` event for an allowed popup;
  `record_nav_event` pre-repair lines 1868-1888 records it as
  `navigation_error`.

Claude/Google popup allow/deny policy is otherwise correctly narrow and must be
preserved. The defect is classification: a deliberately allowed OAuth popup is
reported as a blocking unsupported navigation, contaminating diagnostics and
potentially surfacing a false user error.

Smallest safe remediation: treat the existing allowed reason as a passive
diagnostic, not a blocker. Keep the whitelist and `NewWindowResponse::Allow`
unchanged; keep all other popups denied.

### F10 — Connected Accounts always navigates even when already healthy

- Severity: **Medium**
- Confidence: **High**
- Status: **PROVEN CODE DEFECT / PLAUSIBLE challenge amplifier**
- Source: `launch_connected_account` always calls `navigate_agent_window`
  after creating/recovering the shared nav window (pre-repair lines 2704-2720).

Reopening the same connected account can reload a healthy, authenticated
composer and create another externally visible browsing transition. The safest
reuse condition is strict: same active agent, same-origin current/target URL,
current-generation composer evidence, and no blocker.

Smallest safe remediation: focus an already healthy same-agent/same-origin page
without navigation. A different model, unhealthy page, missing evidence, or
closed window still performs normal navigation.

## Persistence/auth boundary

Searches covered cookie APIs, cookies, storage, IndexedDB, profiles,
`data_directory`, incognito/private mode, cache/clear-data, window destruction,
and app/session restarts.

- `SessionVault::save_cookies` and `load_cookies` exist but have no runtime call
  sites. Arena does not extract or reinject authentication cookies.
- No Arena WebView builder sets `data_directory`, incognito/private mode, or a
  custom profile.
- Authentication therefore relies on the native Tauri/Wry WebView profile and
  its ordinary storage behavior.
- Arena frontend `localStorage` is used for theme state, not model auth.
- Destroying a WebView is still harmful even if native cookies persist: the
  document, sessionStorage, popup relationship, and transient verification
  state can be lost.

Conclusion: do not add custom cookie copying. Stable native browser/profile use
is the correct first-line persistence design.

## Fingerprint and bypass search

The complete production tree was searched for changes to `navigator`,
`webdriver`, plugins, languages, platform, hardware concurrency, screen,
timezone, UA/client hints, Cloudflare, Turnstile, CAPTCHA, and challenge frames.

- `GENERIC_INIT_SCRIPT` is static and generic. It does not override navigator
  properties or manipulate a challenge iframe/token. It passively reports the
  native `navigator.userAgent` for diagnostics.
- `persona_manager.rs` contains dormant synthetic fingerprint/UA generation,
  but there is no runtime caller and it is not applied to either WebView.
- No Cloudflare/Turnstile bypass or solver exists.
- No app code calls `window.location.reload`; retry transitions use explicit
  navigation or reinjection paths identified above.

The dormant persona generator is **DISPROVEN as a current cause**, but should
not be wired into browser construction.

## State-transition traces (pre-repair)

### Connected Accounts

```text
SettingsPanel click
 -> invoke launch_connected_account(agent_id)
 -> AppState.browser_state (process owner; initially has disconnected nav_tx)
 -> 10s/120s shared-window busy lease
 -> replace nav_tx + destroy arena-nav + rebuild via ensure_nav_window
 -> navigate_agent_window (cause=arena_requested; identity set before navigate)
 -> generic on_navigation callback -> per-command std receiver/bridge
 -> Ready: command finishes and focuses window
    Challenge/Login: window remains open for human; command treats it as usable
    Timeout: warning, window remains open
 -> close/reopen: next launch destroys/rebuilds and navigates again
```

Owner: `BrowserState` for window handles/diagnostics, command-local thread and
Tokio receiver for events. Expected agent is the command argument; expected
window is `arena-nav`; timeout is 100 s readiness and 600 s only in session
challenge waits. Destruction is unconditional on each launch at HEAD.

### Session setup

```text
start_session
 -> replace BrowserState and construct per-session channels
 -> create_windows destroys/rebuilds arena-leader + arena-nav
 -> run_setup in setup_order
 -> navigate_agent_window for each assigned agent
 -> wait_for_setup_ready (100s normal; 600s human verification)
 -> genuine Ready
 -> inject priming prompt
 -> capability proof OR wait for user Send/assistant response
 -> challenge after injection escapes run_setup
 -> outer recovery waits Resume
 -> Resume replays run_setup navigation + injection
 -> setup complete -> save in-memory conversation URL
```

Expected agent comes from setup order; expected window is leader for selected
leader and shared nav for every participant. Stale setup events are counted,
but post-challenge replay can duplicate navigation/injection.

### Autonomous participant turn

```text
route participant(agent, turn)
 -> ensure arena-nav
 -> saved conversation URL or validated base URL
 -> navigate (retry may skip using broad record heuristic)
 -> wait_for_ready
 -> inject active prompt + exact agent/turn auto-submit ack
 -> wait_for_response exact agent/turn
 -> later leave page for another participant
 -> revisit saved URL on a later route
```

Ordinary attempts: response timeout 300 s, maximum three retries. Human
challenge episode nominally 600 s but event-by-event timer reset makes it
effectively unbounded. Repeated challenge during readiness terminates early.

### Claude/Google OAuth

```text
Claude login page requests popup
 -> make_new_window_handler
 -> allow accounts.google.com / narrowly matched Google or Claude OAuth URL
 -> native temporary popup performs Google flow
 -> provider callback/parent session update is site/native-WebView behavior
 -> popup closes by provider/site behavior
```

Arena does not automate credentials, tokens, callback, or closure. Other popup
destinations are denied. The allowed event is incorrectly recorded as an Arena
navigation blocker, but the allow response itself remains intact.

## Disproven or external hypotheses

- **External / not caused by Arena:** Cloudflare can challenge a legitimate
  embedded browser based on its own policy, network reputation, account risk,
  service changes, or unsupported embedded-engine behavior. Source inspection
  cannot prove those server-side causes.
- **DISPROVEN:** Arena contains no challenge solver, Turnstile-token mutation,
  iframe manipulation, or CAPTCHA bypass.
- **DISPROVEN:** active browser fingerprint JavaScript spoofing. The only live
  identity override is the builder-level fixed UA in F1.
- **DISPROVEN:** SessionVault cookie copying causes auth corruption; those APIs
  are unused.
- **DISPROVEN:** frontend creates duplicate CAPTCHA overlays. The store holds
  one `captchaAgentId`; repeated events update the same overlay state.
- **DISPROVEN:** Resume itself is translated into `Ready`. Backend event types
  remain distinct. Defects arise where Resume triggers replay or where inner
  code breaks back to an outer ready loop, not from event aliasing.
- **DISPROVEN:** ordinary app code explicitly reloads with
  `window.location.reload`; no live hit exists.

## Repair gate

The pre-repair pass supports local, coherent remediation of F1-F10 without
adding a dependency, paid service, persistent third WebView, cookie copying, or
challenge bypass. The required implementation invariants are:

1. Native WebView UA; passive UA diagnostics retained.
2. One process-lifetime std navigation ingress and dispatcher; never Tokio mpsc
   in `on_navigation`; no dropped receiver.
3. Stable reuse of healthy named Arena WebViews; reconstruct only an absent
   window.
4. Repeated challenge and Resume are non-terminal until genuine Ready/page
   evidence.
5. Post-verification recovery stays on the current page instead of replaying
   setup navigation.
6. Active retry response evidence is exact agent + turn + generation.
7. Navigation skip is exact current generation/window ownership/cause/page.
8. Challenge timeout uses one absolute deadline.
9. OAuth allow policy remains unchanged and is classified non-blocking.
10. `GENERIC_INIT_SCRIPT`, AskUser oneshot/dismissal, and IPC event contracts
    remain unchanged.

## POST-REPAIR

### Implementation ledger (2026-09-08)

The repair deliberately changes Arena lifecycle and state handling, not any
provider challenge. No Cloudflare, Turnstile, iframe, token, cookie, or browser
fingerprint bypass was added.

#### F1 — forced fixed browser identity

**Finding:** PROVEN, high severity, high confidence.
**Original evidence:** `3f243a0` introduced a Windows/Chrome 126 constant and
applied it to the persistent and navigating WebView builders.
**Root cause:** the embedded WebKitGTK/WebView2 engine could be made to report a
browser/platform identity inconsistent with the actual renderer.
**Fix:** removed the constant and every builder `.user_agent(...)` override;
native platform WebView identity is now used.
**Files/functions changed:** `src/browser_backend.rs`, notably
`create_windows` at line 6990, `ensure_leader_window` at line 7055, and
`ensure_nav_window` at line 7100.
**Targeted test:** `browser_backend::tests::arena_builders_use_native_user_agent_and_no_destructive_reuse`.
**Build verification:** `cargo check` PASS; full Rust suite PASS; frontend build
PASS.
**Runtime verification:** native Tauri/WebKitGTK process launched; provider
challenge flow NOT EXECUTED.
**Residual risk:** a provider can still reject an embedded browser based on its
own policy.
**Status:** PASS for Arena's removed mismatch; provider behavior NOT EXECUTED.

#### F2 — dropped navigation receiver and destructive connected-account repair

**Finding:** PROVEN, high severity, high confidence.
**Original evidence:** AppState created a `std::sync::mpsc` channel then dropped
the receiver; the WebView callback therefore held a sender whose receiver was
gone. `launch_connected_account` compensated by replacing the sender and
destroying/rebuilding the navigation WebView.
**Root cause:** callback ingress lifetime was coupled to a replaceable session
receiver. Recreating a WebView captured a new sender but discarded fragile page,
auth, and challenge state.
**Fix:** `BrowserState::new_live` (line 3137) now owns one process-lifetime
standard-library ingress. Its dispatcher forwards to the current bounded async
consumer; `attach_nav_receiver` (3156) replaces only that consumer.
`reset_for_session` (3167), `create_windows` (6990), and
`launch_connected_account` (2560) reuse healthy named windows rather than
destroying them.
**Targeted tests:** `navigation_sink_can_change_without_replacing_ingress` and
`connected_page_reuse_requires_same_agent_origin_and_healthy_composer`; source invariant confirms no production
`destroy()` call.
**Build verification:** full Rust suite 129 passed, 0 failed.
**Runtime verification:** process launch PASS; Connected Accounts, login, and
reopen flows NOT EXECUTED because no user-authenticated interactive account was
available.
**Residual risk:** if Tauri no longer has a named window in its registry,
`ensure_*_window` reconstructs only that absent window; this is intentional
recovery, not normal reuse.
**Status:** PASS for the ownership/lifecycle repair; live auth persistence NOT
EXECUTED.

#### F3/F4 — challenge, Resume, and setup replay

**Finding:** PROVEN, high severity, high confidence.
**Original evidence:** a repeated `ChallengeDetected` ended `wait_for_ready`,
and a post-priming challenge could return from setup to a caller that replayed
navigation/injection. A genuine `Ready` after that inner wait could also fall
through to a second readiness wait.
**Root cause:** challenge was modeled as an exception/restart signal rather than
an idempotent waiting state; Resume was permitted to drive replay indirectly.
**Fix:** `wait_for_ready` (3652) keeps waiting through repeated challenges and
Resume; only a same-agent `Ready` completes it. `wait_for_setup_ready` (341) and
`run_setup` (503) keep the proof episode on the current page, use a single
absolute verification deadline, and resume only after genuine page readiness.
The frontend wording in `src/components/overlays/CaptchaOverlay.tsx` now makes
the same contract explicit.
**Targeted tests:** `repeated_challenge_and_resume_wait_for_genuine_ready` and
`ready_for_wrong_agent_is_ignored`.
**Build verification:** full Rust suite 129 passed, 0 failed; `npm run build`
PASS.
**Runtime verification:** human verification NOT EXECUTED.
**Residual risk:** a provider can emit no usable readiness evidence after a
successful challenge; that remains a visible timeout/manual diagnosis case, not
a bypass or automatic restart.
**Status:** PASS for deterministic state semantics; live provider result NOT
EXECUTED.

#### F5/F6 — stale retry/reinjection and unbounded challenge episode

**Finding:** PROVEN, high severity, high confidence.
**Original evidence:** retry suppression was based on broad setup-era response
state, skip-navigation lacked generation/window/cause checks, and a response
could arrive during retry backoff before navigation was suppressed. Challenge
time was reset per event.
**Root cause:** response evidence and retry validity were not tied to the
logical active turn and the current navigation generation.
**Fix:** `begin_active_turn` (3203), `clear_active_turn` (3267), and
`record_nav_event` track exact agent/turn/generation response evidence.
`can_skip_navigation_on_retry` (1137) requires current generation, window,
agent, target, Arena cause, composer, and no blocker. `response_router` uses
those exact checks at lines 64, 151, and 2201–2487 before retry navigation or
injection; a skip does not wait for a nonexistent fresh Ready. A challenge
episode has one absolute deadline.
**Targeted tests:** `queued_response_recovery_requires_exact_agent_and_turn`,
`retry_suppression_requires_exact_active_turn_and_generation`, and
`retry_navigation_reuse_requires_current_owned_arena_navigation`.
**Build verification:** full Rust suite 129 passed, 0 failed.
**Runtime verification:** session setup and participant revisit NOT EXECUTED.
**Residual risk:** a same-agent event emitted concurrently by an old document
cannot carry a native Tauri document generation. Arena drains stale events
before navigation and guards destructive retry by current diagnostics, but the
provider/runtime interaction remains a live-test item.
**Status:** PASS for the local retry/reinjection guards; live browser sequence
NOT EXECUTED.

#### F7 — OAuth allow event classified as a blocker

**Finding:** PROVEN, medium severity, high confidence.
**Original evidence:** the recent narrowly-scoped OAuth/new-window allow path
also recorded an `UnsupportedNavigation` blocker.
**Root cause:** an allowed site popup was treated as an Arena navigation error.
**Fix:** `is_allowed_oauth_popup` (3568) retains the existing narrow Google and
Claude OAuth policy. `make_new_window_handler` (around 3544) records allowed
popups as passive diagnostics rather than a blocker; all other destinations
remain denied.
**Targeted test:** `oauth_popup_allow_policy_is_preserved`.
**Build verification:** full Rust suite 129 passed, 0 failed.
**Runtime verification:** Claude/Google OAuth NOT EXECUTED.
**Residual risk:** provider popup/callback behavior is site-owned and can
change independently of Arena.
**Status:** PASS for Arena classification; live OAuth NOT EXECUTED.

#### F8/F9/F10 — persistence, duplicate frontend overlay, and external causes

**Finding:** DISPROVEN as local code defects, high confidence.
**Original evidence:** `SessionVault::save_cookies` and `load_cookies` have no
callers; no profile override, incognito setting, cookie extraction, local
fingerprint override, challenge solver, or `window.location.reload` path was
found. The overlay owns one pending agent.
**Root cause:** not applicable. Authentication persistence is the native WebView
profile, so window destruction was the relevant Arena risk.
**Fix:** no custom cookie reinjection was introduced. The lifecycle repair
preserves native state; `CaptchaOverlay` text was aligned with the verified
Ready-only semantic.
**Targeted verification:** whole-tree source searches and frontend build.
**Build verification:** `cargo fmt --check`, `cargo check`, full Rust suite,
`npm run build`, and `git diff --check` PASS.
**Runtime verification:** provider-side Cloudflare/account/network behavior NOT
EXECUTED and cannot be inferred from compilation.
**Residual risk:** external service policy, account reputation, network
reputation, and embedded-engine support remain outside Arena's control.
**Status:** PASS for source evidence; external behavior remains unproven.

### Post-repair named-risk review

- **RISK-BLOCKING:** PASS — no `blocking_lock()` was introduced.
- **RISK-CHANNEL:** PASS — `on_navigation` ingress is `std::sync::mpsc`; Tokio
  is used only after dispatch.
- **RISK-EVENTMATCH / RISK-IPCPARSE:** PASS — no command/event payload changed;
  `IPC.md` remains accurate and no command parsing changed.
- **RISK-UNWRAP:** PASS — no new production `unwrap()` or `expect()` added.
- **RISK-STALERESPONSE / RISK-RETRY-CAUSE:** PASS — retry and response evidence
  require agent, turn, generation and current navigation ownership.
- **RISK-INITSCRIPT / RISK-NAVCLOSURE:** PASS — generic static init script
  retained; callback does not capture agent identity.
- **RISK-ASKCHANNEL / RISK-ASKDISMISS:** PASS — no AskUser path changed.
- **RISK-UA-MISMATCH:** PASS — no WebView UA override remains.
- **RISK-AUTH-DESTRUCTION:** PASS — no normal healthy-window destruction remains.
- **RISK-CHALLENGE-LOOP:** PASS — repeated challenge/Resume waits for Ready and
  cannot itself navigate, recreate, or inject.
- **RISK-OAUTH:** PASS in source/tests — narrow popup allow policy retained;
  live provider OAuth remains NOT EXECUTED.

### Final verification record

- `cargo fmt --check`: PASS.
- `cargo check`: PASS (72 pre-existing/non-fatal warnings).
- `cargo test`: PASS, 129 passed / 0 failed.
- `npm run build`: PASS.
- `git diff --check`: PASS.
- `npm run tauri dev`: native application launch PASS (`Running
  target/debug/consensus-arena`); GTK/EGL environment warnings only. Connected
  Accounts, human verification, authenticated composer, reopen, Claude setup,
  and participant revisit: **NOT EXECUTED** because this environment has no
  user-operated authenticated Claude account. No claim about provider challenge
  success is made.
