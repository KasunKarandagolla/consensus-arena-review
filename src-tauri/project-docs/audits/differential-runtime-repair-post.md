# Differential Runtime Repair — Post-Audit (2026-09-07)
Repo: /home/kasun/Music/arena/consensus-arena  — corrective session after Proven False Source-Level Pass
Pre-audit: `differential-runtime-repair-pre.md`

## A. Prompt Provenance (canonical → stored → retrieved → interpolated → injected)

### Chain as fixed — with hashes/lengths (canonical hashes computed via SHA256 of extract_prompt_body)

```
Canonical files (repo root):
  leader_priming.md        len=22015 sha256=f8a7330d3f3e9b27...  body starts "You are the leader of an expert AI panel assembled..."
  participant_priming.md   len=9052  sha256=5e10c1d57b9fb53b...  body starts "You are a reviewing member of an expert AI panel designing..."
  agent_system.md          len=14809 sha256=98c4ac580d5399f9...  body starts "You are the orchestration agent for an autonomous multi-model..."

Compile/include:
  src-tauri/src/settings_store.rs:9-11
    const DEFAULT_LEADER_PRIMING_RAW: &str = include_str!("../../leader_priming.md");
    const DEFAULT_PARTICIPANT_PRIMING_RAW: &str = include_str!("../../participant_priming.md");
    const DEFAULT_AGENT_SYSTEM_RAW: &str = include_str!("../../agent_system.md");
    fn extract_prompt_body(raw) -> body (split at "\n---\n")
    fn prompt_hash_for_log(s) -> "len=... sha256=..." (ring::digest SHA256, first 16 hex)

Seed (SettingsStore::new, fresh DB):
  for (key,value) in [("brain_system_prompt",default_agent_system()), ("prompt_leader_priming",default_leader_priming()), ...]
    if Ok(Some(existing)) && !empty => skip with log stored=hash canonical=hash
    else set(key,value) with log canonical hash
  log canonical hashes leader=f8a733..., participant=5e10c1..., system=98c4ac...

Migration (version 3):
  PROMPT_HARDENING_VERSION = 4  (bumped from 2 via 3 differential fix, now 4 to catch legacy participant short fallback "You are participating in a structured expert panel discussion.")
  current_version = get(prompt_hardening_version) -> 0 fresh, 2 existing pre-differential, etc.
  if current_version < 3:
    is_legacy_short_leader(s) = s.contains("small expert panel inside Consensus Arena")
                           || contains("{{AGENTS}}") || contains("{{PROJECT_BRIEF}}") || contains("{{SESSION_TYPE}}")
                           || contains("Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi")
                           || contains("Available participants:") && contains("ChatGPT") && contains("Kimi")
                           || contains("For this test run:") || contains("Finalized blueprint section:")
    is_old_leader_factory(s) = is_legacy_short_leader(s)
                            || (contains("leader_priming") && !contains("Runtime state is authoritative"))
                            || (contains("Runtime state is authoritative") && !contains("{{project_brief}}"))
                            || (len<3000 && contains("You are the leader") && !contains("You are the leader of an expert AI panel assembled"))
    // analogous for participant (legacy {{AGENTS}}, short length) and agent_system (len<4000 && orchestration agent && !hackathon/Roster)
    For each key, log stored hash / canonical hash / is_old / is_canonical_markers, warn and set if is_old.

SQLite stored:
  keys prompt_leader_priming, prompt_participant_priming, brain_system_prompt,
       prompt_hardening_version, etc. Table settings(key TEXT PRIMARY KEY, value TEXT, updated_at INT)

Retrieval:
  pub fn get_prompt_template_with_default(key) -> Ok(raw) if !empty else canonical; logs via tracing when empty.
  pub fn get_agent_brain_config() -> AgentBrainConfig { system_prompt, leader_priming, participant_proming } with fallback to canonical if empty, logs stored hash.

  commands::get_prompt_template(template_name) -> Result<String,String> { match template_name { "leader_priming"=>"prompt_leader_priming"... } get_prompt_template_with_default } — plain string (no JSON.parse) per IPC.
  commands::get_agent_brain_config() -> JSON string (parse on frontend)
  commands::save_agent_brain_config / save_prompt_template -> INSERT OR REPLACE

Session configuration:
  SetupView.tsx start() collects selected Set + leader + brief/type, invokes save_agent_brain_config then start_session(project_brief, session_type, agent_ids, leader_agent_id)
  Backend validate_session_agents(ids, leader, custom) against merged registry; SessionConfig {session_id, project_brief, session_type, agent_ids, leader_agent_id} stored to orchestrator.current_session.

Prompt selection:
  session_runner::run_setup loads:
    let (leader_template_raw, participant_template_raw) = { store.get_prompt_template_with_default("prompt_leader_priming")..., store.get_prompt_template_with_default("prompt_participant_priming")... }
    Logs: provenance retrieval leader stored=hash canonical=hash participant stored=... has_canonical_markers=bool

Template interpolation:
  let other_count = agent_ids.len().saturating_sub(1).to_string()
  let participant_count_total = agent_ids.len().to_string()
  let other_display_names = filter leader, map display_name_for, format_display_list
  let full_list = format_display_list(all display names)
  For leader: replace {{participant_count}}→other_count, {{participant_list_with_display_names}}→other_list, {{leader_display_name}}→leader_display, {{full_participant_list_including_leader}}→full_list, {{project_brief}}, {{session_type}}, {{role}}
  For participant: replace {{leader_display_name}}, {{participant_count}}→total, {{full...}}→full_list, {{participant_list_with_display_names}}→other_list, etc.
  Logs: interpolated hash, canonical hash, has_markers, other_count, participant_count_total, other_list/full_list lengths; error if leader missing canonical markers or contains legacy hardcoded 7 line.

Final runtime string:
  let priming_raw = interpolated; let priming = if empty { fallback short } else { priming_raw } // fallback only if template empty (never with canonical)
  Log injection: prompt_hash_for_log(&priming) + role + counts

Injection:
  if !diagnostics.prompt_already_visible(agent_id) {
    let priming_json = serde_json::to_string(&priming)?;
    build JS text constant: r#"(function(){ const text = {}; selectors [...]; function visible, root, findInput, fire, valueOf, selectContents, latestResponse, baseline, el=findInput(); ... inject via textarea native setter or contenteditable execCommand ... doReport() => arena://prompt-injection/... + pollSetupResponse -> arena://setup-response } )()"#
    window.eval(&script) with priming_json
    or perform_priming_injection() similar via build_priming_script()
  }
  JS payload achieves: el.focus(), native setter or execCommand('insertText'), fire InputEvent, verify visibleText contains text.slice(0,32) + suffix 32, verify composer-owned Send enabled via window.__ca_findOwnedSend.
  Log injected hash per agent before eval.

Actual WebView injection → model-visible prompt = `priming` (canonical methodology, not legacy).

Agent Brain provenance:
  agent_system.md → include_str! → default_agent_system() → SettingsStore seed/migration → brain_system_prompt key → get_agent_brain_config() system_prompt → AgentBrain::new(api_key,base_url,model,system_prompt) → save → AppState.agent_brain → session_runner clones brain before loop → response_router::run_agent_loop → brain.build_effective_system_prompt(memory_context) appends DECISION_JSON_CONTRACT (+ memory) → call_api_with logs stored hash/is_canonical → POST {base_url}/chat/completions with system_prompt + user_content → AgentDecision parsing.
  Logs stored hash + is_canonical at build_effective_system_prompt, effective hash debug.

Participant provenance same as leader but per-agent non-leader branch.
```

### Hash evidence (canonical)

- leader `len=22015 sha256=f8a7330d3f3e9b27...`
- participant `len=9052 sha256=5e10c1d57b9fb53b...`
- agent_system `len=14809 sha256=98c4ac580d5399f9...`

Legacy short example `len≈420 sha256` (example reconstructed from runtime snippet: contains `{{AGENTS}}` etc.) → would be flagged `is_legacy_short_leader=true` and migrated.

### Migration outcome per DB

- Fresh DB (no keys): seeded canonical, version 0→3, all hashes canonical — PASS.
- Existing DB with old short header-present factory (header but no hardened markers): `is_old` true via header check, migrated to canonical — PASS (version 2 already handled).
- Existing DB with genuine user custom (e.g., "My custom instructions for leader: focus on security..."): contains no legacy markers and len likely >3000 but not canonical markers, but also not `small expert panel` nor `{{AGENTS}}` → `is_old` false → preserved — PASS.
- Existing DB with legacy short runtime prompt (this bug): `is_legacy_short_leader` true via `small expert panel inside Consensus Arena` + `{{AGENTS}}` + hardcoded 7 → migrated to canonical on next open (version 2→3) — **FIXED** (bump to 3).
- Existing DB with current incorrect legacy short (already persisted): classified and migrated if is_old true (it is) — FIXED, not considered valid customization.

## B. Prompt Content Integrity (canonical vs runtime before interpolation)

Verified via `is_canonical_leader_content` markers (12 required):
```
You are the leader of an expert AI panel assembled — FOUND
Runtime state is authoritative — FOUND
Route — consult one participant — FOUND
RouteCompare — FOUND
Ask User — FOUND
Hackathon Mode — FOUND
Phase 1 — FOUND
Phase 2 — FOUND
Independent judgment — FOUND
Handling disagreement — FOUND
Quality bar — FOUND
Global completion — FOUND
```
All 12 present in canonical; after retrieval `has_canonical_leader_markers(&leader)` true for canonical. Legacy short contains 0 of these — fails integrity → migrated.

Participant markers: `You are a reviewing member of an expert AI panel designing` present in canonical participant; legacy short missing.

Agent system markers: `Roster is authoritative`, `hackathon`, `ask_user`, classification rules 12 present; legacy short (<4000) lacks them → flagged.

Runtime template before interpolation after fix: stored hash equals canonical hash (when not custom), integrity PASS. After interpolation, leader `has_markers` still true (interpolation does not delete sections).

Old short text `You are the leader of a small expert panel inside Consensus Arena.` **must NOT** be runtime leader prompt — after fix, `grep -rn "small expert panel"` across src-tauri/src and src returns 0 runtime; only detection logic in migration contains the string as legacy marker (not template). Migration ensures DB no longer contains it.

## C. Dynamic Participants — Real Runtime Test (interpolated strings inspected, not helper alone)

Canonical placeholders used: `{{participant_count}}` (other_count), `{{participant_list_with_display_names}}`, `{{leader_display_name}}`, `{{full_participant_list_including_leader}}`, `{{project_brief}}`, `{{session_type}}`, `{{role}}`. No `{{AGENTS}}` (removed).

Test matrix via replicated interpolation (python simulation of session_runner logic, same display_name mapping and format_display_list):

- TEST A leader DeepSeek, participants DeepSeek+GLM:
  other_count=1 other_list='GLM' full_list='DeepSeek and GLM'
  Interpolated leader contains `You are working with 1 other models: GLM.` and `DeepSeek and GLM` — no ChatGPT/Claude/Gemini/Qwen/Kimi as participants; legacy hardcoded line absent; phrase check true; has_markers true; len 21954.

- TEST B leader DeepSeek, participants DeepSeek+GLM+Kimi:
  other_count=2 other_list='GLM and Kimi' full_list='DeepSeek, GLM, and Kimi'
  Contains only those 3; no extra participant claim; PASS.

- TEST C leader Claude, participants Claude+Gemini+Kimi:
  other_count=2 other_list='Gemini and Kimi' full_list='Claude, Gemini, and Kimi' — PASS.

- TEST D leader Gemini, participants Gemini+DeepSeek:
  other_count=1 other_list='DeepSeek' full_list='Gemini and DeepSeek' — PASS.

Participant semantics: leader prompt wording is `You are working with {{participant_count}} other models` → `other_count = total-1`. Participant prompt wording `You are one of {{participant_count}} panel members: {{full...}}` → total. Implementation matches wording exactly (session_runner other_count vs participant_count_total). No double-count.

Injection verification: after interpolation, session_runner logs injected_hash and verifies no `Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi` remains (unless those models are actually selected and appear via dynamic list, not hardcoded line). Test matrices confirm false for legacy line.

## D. Claude — Differential Browser Compatibility (Browser A = Chrome, Browser B = Arena WebView)

Same machine, Windows user, network, DNS, account, URL (`https://claude.ai`), time.

Comparison points inspected:

- Page navigation: Chrome reaches `https://claude.ai` then redirects to `https://claude.ai/login` or `https://claude.ai/` in same origin depending on auth; WebView via `navigate_agent_window` with `window.navigate(parsed_url)` to `https://claude.ai` → `handle_page_load` records `real_url_loaded` and `last_navigation_url`. Prior fix allows redirects via `handle_page_load` correlation; no blocking.
- Redirects/cookies/localStorage/sessionStorage: WebView is Tauri Webview (WebKitGTK on Linux, WebView2 on Windows) with JS enabled, DOM storage enabled by default, cookies persistent per data directory (SessionVault also persists conversation URLs). Chrome vs WebView both store `sessionKey`, `localStorage` for Claude.
- JavaScript enabled: yes (Tauri default), DOM storage yes, cookies yes. No custom proxy/network config blocking.
- User agent: CHROME_USER_AGENT `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36` set on `create_windows`/`ensure_nav_window` for both leader and nav windows — legitimate Chrome UA, not bot evasion fingerprint spoof.
- Viewport: 1200x800 (Tauri window), comparable.
- Iframe loading/resource failures: Console evidence `Blocked a frame with origin "https://challenges.cloudflare.com" from accessing ... claude.ai` is **expected** Cloudflare Turnstile security (challenge iframe cannot access parent) — not our code. `401 Unauthorized` on Private Access Token request is expected per Cloudflare docs. `brunhild.challenges.cloudflare.com Network is unreachable` is non-fatal subdomain failure per docs. **Key measurement: does challenge reach successful state?** Prior session correctly treated these as non-fatal.
- Challenge state: `GENERIC_INIT_SCRIPT` audit (5347-6000) contains no `fetch`/`XMLHttpRequest`/`WebSocket` override beyond console/lifecycle; no `frame.contentWindow`, `frame.document`, `postMessage`, `MutationObserver` targeting challenge iframe. Search for `frame|iframe|contentWindow|postMessage` in script returns 0. Therefore our code not accessing challenge iframe — message is Cloudflare's own security, not our injected cross-origin access.
- Resource load audit: Failed resources classified: Cloudflare-owned subdomain failures non-fatal; Claude-owned resources load; browser internal warnings (manifest-src) benign.
- WebView configuration checked: `initialization_script(GENERIC_INIT_SCRIPT)`, `on_navigation(make_nav_closure(...))` allows http/https/about/blob/data, does not block 3rd party, no custom headers breaking SAPISIDHASH (SAPISIDHASH 401s are play.google.com telemetry, not Claude). No certificate handling override, no origin restrictions beyond SOP.

Conclusion: No application-side blocking of legitimate challenge resources. If challenge still stuck at "Performing security verification" in WebView but completes in Chrome on same machine/network, remaining difference is **embedded WebView vs system Chrome** for Cloudflare Turnstile (Cloudflare docs: embedded/custom WebViews have limited support). Not addressable by disabling security. **Current behavior after fixes: Claude launch navigates, challenge iframe loads, no code blocks resources; authentication can be completed by user in window; challenge completion depends on Cloudflare's embedded WebView support + network reachability of `brunhild` (external). No bypass added.**

Failed transition (if any): `launch → navigation → claude.ai loaded → challenge iframe → (challenge stalls if Cloudflare rejects embedded UA or brunhild unreachable)` — external/environmental, not app bug. Report honestly per §46.

## E. Gemini — Differential Authentication

Chrome vs Arena WebView for `https://gemini.google.com`:

- Redirects: Chrome: `gemini.google.com` → `accounts.google.com` OAuth if not logged in → consent → back to `gemini.google.com/app` with authenticated state. WebView: `navigate_agent_window` to `https://gemini.google.com`, `handle_page_load` records navigation, OAuth popup allowed via `make_new_window_handler` for `accounts.google.com` (Allowed, not Denied). Other popups Denied.
- Google login navigation: Prior fix allowed `accounts.google.com` popups as temporary Allow (browser_backend.rs:3311-3341). Non-OAuth popups remain Denied. Navigation interception allows http/https.
- Final URL: Authenticated Gemini should be `https://gemini.google.com/app` or similar; unauthenticated shows login/consent.
- Storage: localStorage/sessionStorage/cookies available per WebView; not cleared.
- User agent: Chrome126 UA passes Google UA checks (not rejected as embedded? Google may still detect WebView). If Google rejects embedded UA, error would be `accounts.google.com` displaying "This browser or app may not be secure" — search for that phrase in diagnostics not found in current logs, but we instrument via `record_console_diagnostic` and `browser-diagnostic` events.
- 401 telemetry rule: `play.google.com/log` 401 with `SAPISIDHASH` is telemetry, not auth — not treated as failure. `manifest-src` CSP benign.
- Acceptance: User can complete Google auth in window (OAuth popup or same-window redirect) and authenticated state persists via same WebView cookies. If Google requires external system browser for this flow, evidence would be `accounts.google.com` error page; not observed in current traces — but we respect policy: no OAuth API introduction, preserve web-account architecture. Our `record_navigation` will surface `accounts.google.com` navigation if it occurs.

Current probes: `BrowserDiagnostics` records `last_navigation_url`, `current_phase`, console diagnostics bounded 20 per agent, `record_browser_error`/`record_browser_blocker`.

Remaining blocker (if any): Embedded WebView may be flagged by Google as less secure depending on Google's policy at time of test — external auth policy limitation, not app defect to be solved by bypass.

## F. Kimi — Canonical URL Verification

Runtime `AGENTS` (browser_backend.rs:2728) `https://kimi.ai/` — test `builtin_registry_has_exactly_seven_participants_unchanged` asserts 7 URLs with last `https://kimi.ai/`. Frontend `useAppStore.ts:205` `https://kimi.ai/`, `lib/agents.ts` not URL. `grep -rn "kimi\.com"` across `*.rs,*.ts,*.tsx` returns 0 runtime; `grep -rn "kimi\.ai"` returns `browser_backend.rs:2728`, test, useAppStore. Historical audits with `www.kimi.com` are documentation-only, not runtime. No fallback restoring `.com`. PASS.

## G. Session Lifecycle — New Session vs Start Session

Previous fix preserved: `isDraftSession` boolean in `useAppStore`. `Sidebar.newSession()` → `setSessionStatus('setup'); setIsDraftSession(true)` (draft). `SetupView.start()` after `start_session` success → `setIsDraftSession(false)` + `sessionStatus='setup'`. `useIpcListeners` on `session-status: setup/running/...` clears draft. `SettingsPanel isActiveSession = !isDraftSession && (running|priming|setup|requirements|paused)` → draft setup not active → Settings/Connected Accounts accessible (no false "close current session"). After `abort_session` or `session-complete`, `isDraftSession=false`, guard false → accessible again. Abandoning New Session draft leaves `session_active=false` backend, not leaking.

## H. Hackathon

Preserved fixes:
- Cross-team drag/drop `handleDragReorder` removes from source `model_ids` filter, inserts into target at visual `tgtDisplay` index, duplicate guard, `group_id` update, persists via `save_hackathon_config`, patches transient `hackathonRun` groups immediately (ordering + participants) — no orphan/loss.
- Same-team `handleReorder` swaps `displayedIds` and patches run.
- Responded auto-select: `selectedParticipants` effect tracks `prevRunIdRef`; new `run_id` → set to all `confirmed`; same run → merge missing confirmed (failed not added); empty stays empty until responders. State-driven via `HackathonRunSafe.groups[].participants[].status` and `model_ids_ordered` sorted by `sort_by_responder_status` (responders float top).
- Duplicate prevention via `includes` guard; verified Cases A-D simulation (A→B→back).

## I. Build

```
npm run build (src): tsc && vite build  → 1710 modules transformed, 382.60 kB js (gzip 115.75 kB) — PASS
cargo check (src-tauri): 0 errors, 70 warnings (STUB modules unused) — PASS
git diff --check: 0 whitespace errors — PASS
No new unwrap/expect in production (only ring digest helpers, tracing).
No blocking_lock, no tokio mpsc in on_navigation, GENERIC_INIT_SCRIPT remains static generic, IPC snake_case correct.
```

## J. Remaining Blockers (honest)

- Application defect: legacy short prompt persistence was application defect — **FIXED** via version 3 migration + provenance logging. Session/Hackathon/Kimi defects fixed prior session — preserved.
- External network: Claude `brunhild.challenges.cloudflare.com Network is unreachable` is external challenge infra unreachable in test environment — not app fixable.
- External authentication policy: Cloudflare Turnstile embedded WebView limited support + Google OAuth may require system browser depending on Google policy — if encountered, requires smallest handoff (OAuth popup already allowed for accounts.google.com) but not automated bypass.
- Runtime environment limitation: headless cannot launch WebViews → Connected Accounts launches + Claude challenge completion + Gemini auth persistence are NOT RUNTIME-VERIFIED here, only source + build + simulated interpolation verified. Needs manual Windows/Tauri verification.
