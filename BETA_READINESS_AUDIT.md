# Consensus Arena — Beta Readiness Audit

**Audit date:** 2026-09-06 (Session 2 — Maximum-Depth Continuation)
**Audited against commit:** a3ab85f (branch `forensics/browser-auth-diagnostics`, DIR TY worktree: 15 modified + 9 untracked beyond HEAD)
**Total real source audited:** 19,913 Rust lines (59 commands, 21 AppState fields) + 2,831 TS/TSX + 234 CSS + 3 untracked subsystem modules (Hackathon 1304+502, Harness 1556, Memory 1474) = ~26,500 lines
**Total findings:** 38 distinct (1 CRITICAL-equivalent HIGH provisional confirmed, 8 HIGH/MEDIUM, 12 LOW, 9 INFO/PASS, 8 doc-mismatches) — see header anti-inflation note: this count is of independently evidenced items, not padded prose.

---

## A. Executive Summary

For the project owner (who makes the ship decision, not the code): **Yes, this codebase can ship a beta, but it cannot honestly be called "ready to invite external beta users" without conditions.** The dirty worktree fixes the prior audit's 2-command registration gap (59 defined == 59 registered), respects the hard 2-WebView ceiling in every creation site, and builds clean (cargo 0 errors, npm 0 errors, 69 warnings triaged). The core session loop (Setup → Priming → Route/RouteCompare/Blueprint/AskUser/Continue/Complete) is structurally coherent, GENERIC_INIT_SCRIPT is genuinely generic (no per-model `if claude` branch), and the new subsystems (Memory 6-table + FTS, Hackathon parallel groups, Harness ring buffers) are real, bounded, and isolated — they do not pollute the main loop.

What changes the verdict from "ship" to **"GO WITH CONDITIONS"** is not a count but a kind: the real codebase is **2.3× larger than the project's own docs believe** (59 commands vs 26, 21 AppState fields vs 14, 3 entire subsystems beyond original docs), and its artifact includes ~1,472 dirty lines of uncommitted feature/forensics work on the exact WebView login bug the beta is supposed to prove fixed. The second prior audit (this same branch, same commit, 21 findings, GO WITH CONDITIONS) plus this maximum-depth re-audit independently agree on that condition: **resolve or explicitly accept the P1s, then run one real Windows WebView2 + one Linux WebKitGTK smoke session before inviting outside users.** The highest-probability beta failure is not a hidden P0 (none proven) but the already-documented blank-window hang (readiness timeout → recoverable setup-failed → manual Retry) surfacing for Claude/Qwen on flaky networks, plus the hackathon/busy-shared-WebView race if a user drives Hackathon invitations while a session owns the nav window.

Scale discovery itself is a risk signal: decisions (including this beta decision) made on a 26-command mental model when 59 exist will miscount work, miss dead commands, and ship with 19 NO_CALLER_FOUND commands that look wired but never get clicked.

---

## B. Beta-Blocking Findings (CRITICAL and HIGH only)

Scannable list — links down to Section C. If this were "no blockers" we would say so plainly; it is not.

| # | Title | Location | Consequence | Fix |
|---|-------|----------|-------------|-----|
| B-1 | **Blueprint-update / session-checkpoint dead contract — frontend listens, backend never emits** | `useIpcListeners.ts:219,257` vs `response_router.rs:1242,1250` emits only `blueprint-section-added`/`blueprint_emitted` (plus harness `record` not emit) | Blueprint draft→agreed upserts and "Progress saved" toasts never fire; appears wired per IPC.md but silent. Beta works via `-added` path, but any future draft flow is silently broken. | Emit `blueprint-update` on draft upserts or remove listeners + mark FUTURE in IPC.md |
| B-2 | **RateLimit ErrorKind unreachable — cooldown path dead, rate-limited participants hammered** | `errors.rs:31-44` (never returns RateLimit) vs `response_router.rs:1761 set_cooldown` | 429'd participants never enter 60s cooldown via kind check; will be retried until `should_retry_after_failure`'s empty-shell check instead of rate-limit backoff. | Map 429 to RateLimit variant or check string/error type before kind() |
| B-3 | **browser_state lock held across update_model_health await** | `response_router.rs:1656-1663` `is_in_cooldown` check | Extends contention, blocks 600s challenge resume loop on model_health lock; violates project's own "no await while holding browser_state". | Scope: `let is_cool = { lock.is_in_cooldown() }; if is_cool { update.await }` |
| B-4 | **Hackathon/WebView + diagnostics lack session_active fence — concurrent mutation of shared nav window** | `commands.rs:2358-2373 send_hackathon_invitations`, `2660-2729 run_hackathon`, `get_diagnostic_snapshot` vs `session_active` guard | Hackathon invitations can race a live session for `arena-nav` and `BrowserTimeline`, misattributing NavigationStarted/ComposerDetected across `run_id`/`setup_generation`, defeating the forensics it was added to provide. Prior audit P1 AUD-001, independently confirmed. | Fence hackathon + maintenance diagnostics with `if session_active.load() { Err("Stop session first") }` like `run_single_model_diagnostic` already does |
| B-5 | **Hackathon NIM 429 kills teammate permanently, no retry/cooldown — shared-key burst collapses all groups** | `hackathon.rs:639-640,1059-1078` single fallback vs `agent_brain` 429 handling | NIM free-tier 30 parallel invites + 20 rounds will 429 all models sharing one base_url bucket together; every group loses teammates in one burst with no actionable UX, report shows `Locked/Failed` with placeholder text. | Per-base_url cooldown HashMap or single retry with jitter; surface via hackathon-group-status |
| B-6 | **Abort SessionAborted via try_send can be lost when bridge full/disconnected — inner 600s/300s loops hang** | `commands.rs:449-481 abort_session try_send`, `response_router.rs:2064-2088 challenge 600s, 527-566 normal 300s` | Stop button emits `session-status: ended` immediately but Rust task remains blocked minutes; new session allowed (session_active false) but old bridge may deliver stale Ready/Response for prior setup_generation. Prior audit P1 AUD-005, confirmed. | Use blocking send or `send` with Full→retry, plus `AtomicBool abort_requested` polled each wait iteration; ideally store JoinHandle and abort it |

*No confirmed CRITICAL (permanent deadlock, data corruption, 3rd WebView, key leak) . The 6 HIGH are the must-fix-or-explicitly-accept set.*

---

## C. Full Findings — Backend

All in `project-docs/audits/beta-audit/03-findings-backend.md` (mandatory Evidence/Why/Fix/Confidence, zero exceptions). Grouped here by file, severity within file.

### C.1. Main / Orchestrator (PASS with notes)

- **OrchestratorStatus dead variants** — `orchestrator.rs:22-31 Preparing/Requirements/Complete` never constructed (INFO). Fix `#[allow(dead_code)]` or delete. Frontend `requirements` never occurs.
- **Main paths** — `main.rs:39,41,87,170` expects only in setup (allowed), `AppState::new` derives all DB paths via `format!("{}/...")` not `PathBuf::join` — tolerates mixed separators on Windows but `memory.db` is correctly `PathBuf::join`; non-UTF8 username `to_string_lossy` truncation noted as OBS. Two-WebView creation sites verified.

### C.2. Agent Brain (1 INFO)

- **decide() wrapper dead, core alive** — `agent_brain.rs:158 decide` dead, called via `decide_with_source` at 3 sites in response_router.rs:633,640,650 for brain-status telemetry (HIGH confidence, resolved mandatory #1). Wrapper should be deleted or allowed.

### C.3. Response Router (highest-risk file, 2453 lines, 100% read via sub-agent)

- **Blueprint-update/checkpoint dead** — HIGH (above).
- **Lock held across await** — MEDIUM (above).
- **RateLimit unreachable** — MEDIUM (above).
- **brain_fail_count toast misleading when secondary None** — `611-648` emits "switching to secondary" even when `agent_brain_2 is None` then falls back to primary every iteration (LOW). Fix only emit if `is_some()`.
- **Exhaustive AgentDecision match** — PASS (796-1583 covers Route/RouteCompare/Blueprint/Continue/AskUser/Complete, no wildcard; fallback 678-725 synthesizes Route/Blueprint/Continue/Err by design).
- **wait_for_response double-check** — PASS (1941-2119 checks agent_id+turn for Response/ManualResponse, timeout vs CaptchaRequired distinct, challenge resume 600s, harness error 60s each distinct).
- **AgentBrain secondary guard held across 60s HTTP** — INFO design tradeoff `630-633`, blocks callers; fix clone-out like DEF-001.
- **Diagnostic emits without listeners** — INFO: `route_started`, `blueprint_emitted`, `agent_brain_decision_*` (6 sites) intentionally not listened per blueprint-first rule; not a bug but IPC.md should mark diagnostic-only.

### C.4. Session Runner (1099 lines, sampled)

- **Unreachable pattern** — `876 (Some(_), _) => unreachable!()` after exhaustive Some arms (LOW).
- **Navigation correlation** — prior audit AUD-002 pending_arena_navigations 5s window race not re-verified this session (marked UNVERIFIABLE in Section L).

### C.5. Browser Backend (6389 lines, greatest share, deepest read)

- **GENERIC_INIT_SCRIPT** — PASS (4540 static, 0 per-model `if`, runtime `window.__ca_agentId`, DOM-type `isEditableSurface:4885`, selectors generic 4832).
- **RISK-CHANNEL** — PASS (on_navigation captures only `std::sync::mpsc::SyncSender`, bridge via `std::thread::spawn + blocking_send` 275-282/247).
- **make_nav_closure** — PASS (2907 captures only `tx` + `'static window_label`, no `agent_id`).
- **Two-WebView** — PASS (create_windows 5876 builds exactly LEADER/N A V after destroy stale 5830, ensure_nav_window 5943 recreates only N A V, WebviewWindowBuilder third-site 0, diagnostic reuses nav, NewWindowResponse::Deny 2745).
- **Console.error override → arena://log** — PASS (4616-4629 re-entrancy guard, handler 3100-3111).
- **Dead warnings true positives** — INFO: ConsoleCategory/ConsoleSeverity string bridge, PendingArenaNavigation 4 fields, BrowserDiagnostics 5 methods, resolve_display_name, inject_to_agent superseded by inject_to_window:2773, monitor_existing_response, window_label x2 — all 0 prod callers verified.
- **Format-and-join path note** — `orchestrator.rs:189 format!("{}/settings.db", data_dir)` vs `export_blueprint` correct `PathBuf::join` — prior audit AUD-007 still present (P1 side channel, Windows mixed separator double-separator).

### C.6. Memory Store (1474 lines)

- **Schema** — PASS: 6 tables + `project_memory_fts` virtual + 3 triggers + indexes (142,158,192,218,230,248,263); tests schema_smoke:1432.
- **MemoryEntry mapper exists but table grows invisible** — INFO: `memory_entry_from_row:1281` correctly maps 9 cols for `get_session_facts:331` which has no Tauri command; `session_memory` grows per project, archive never called (waste, cargo noise).
- **Provenance + hard-pinned Project Context** — PASS: `save_project_config:428` hard_pinned=1 high, decay protects hard_pinned=0 only, context budget 4000 `build_memory_context:1054`, all async via `run_blocking` non-fatal.
- **Export missing WAL checkpoint** — LOW: `export_to:1022` Backup without `PRAGMA wal_checkpoint`; Backup copies WAL but checkpoint recommended for consistent snapshot.
- **12 commands not 11** — INFO doc-mismatch: `main.rs:149-160` registers 12; docs say 11; tracking CSV marks 3 internal-only (`get_global_memory`, `get_open_questions`, `get_model_strengths`) correctly not yet called by MemoryPanel.

### C.7. Hackathon (1304 lines)

- Cancellation latency up to 60s (MEDIUM), created_at never emitted (LOW), duplicate HackathonChatMessage (LOW), NIM 429 gap (MEDIUM-HIGH above), isolated from main loop (PASS), DESIGN drift safety cap 20 rounds + plaintext settings.db keys vs vault (LOW-MEDIUM).

### C.8. Browser Harness (1556 lines)

- Bounded 500/agent ring buffers + 20/100 caps → no unbounded growth under long session (PASS, mitigates Scenario M, <5MB).
- emit_timeline is record-to-ring not emit — naming confusion (INFO, rename to record_timeline).
- Scenario G partially diagnosable via timeline + reliability report (COND-PASS, requires manual `get_browser_reliability_report` pull, no auto push on Challenge).

### C.9. Settings / Blueprint / Transcript / Small stores

- **TokenBudget dormant** — MEDIUM (record_tokens never wired, panel shows 0).
- **ContextManager orphaned** — MEDIUM (history loop replaced by browser-window + minimal context string; participants lose history vs ContextManager's 60k truncation; IsMedium detailed in Mandatory #2).
- Remaining stores/helpers read per inventory; STUB modules (6 files) dead by design — INFO, suppress with allow.

---

## D. Full Findings — Frontend

All in `project-docs/audits/beta-audit/04-findings-frontend.md` (and thin-file determinations). Session 2's parallel audit read every frontend file line-by-line.

### D.1. Thin-file investigation (mandatory new step) — ALL PASS (tersely but fully implemented, no missing described behavior)

Every thin file was determined **explanation 2** (not wrapper, not missing):

- **AskUserPopup.tsx 13 lines** — all 5 close paths present: option click `12`, custom submit button `12` with `trim()`, Enter key `12`, Escape `9` with cleanup, backdrop `12` with `e.target===currentTarget` + `stopPropagation` on card, all call `provide_user_answer({answer})` with `"Cancelled"` for dismissals, `answering` ref prevents double-send. Pending flag lives in `useAppStore.ts:143,286` / `useIpcListeners.ts:234`.
- **CaptchaOverlay.tsx 8, RateLimitOverlay.tsx 9** — self-contained null-guard + displayName + correct `rename_all` (`captcha_resolved:{agent_id}`, `rate_limit_decision:{agent_id,decision}`).
- **Toast.tsx 4** — presentation complete; queue/timeout in `useAppStore.ts:290-297` (`Date.now()` id + `setTimeout(removeToast)`).
- **Sidebar.tsx 34** — fetches `get_session_list`/`get_agent_health` with correct `JSON.parse` (`17-18`), plain string `export_blueprint` without parse (`25`), interval+menu cleanup (`19-21`).
- **Topbar.tsx 32** — `showBrain` gate + `brainLabel` truncation + kind switch.
- **InputBar.tsx 47** — idle/running/disabled, auto-resize, `user_input`/`abort_session`, Enter-without-Shift.
- **lib/agents.ts 92** — AGENT_IDS 7 in frozen order `chatgpt,claude,gemini,deepseek,qwen,glm,kimi` === `browser_backend.rs:2185`, correct.
- **lib/theme.ts 23** — Theme `'blue'|'light'|'dark'` (no stale gray), ValidThemes, migration default→blue.
- **MemoryPanel.tsx 211, HackathonMiniWindow.tsx 502** — full CRUD verified (see Sections H/M).
- **index.html** — PASS no font CDN; **index.css 234** — local `@font-face` variable fonts, 3-theme parity complete for `--bg,--surface,--text,--t2,--border,--accent` etc.

### D.2. Zustand store / tauri wrappers / iPC hook

- **useAppStore.ts 305** — hackathon state present `114-119` + memory intentionally not in store (backend via `run_blocking`, local state in MemoryPanel). `clearSessionState:252-266` resets overlays/activeBrain.
- **lib/tauri.ts 82** — `safeInvoke`/`safeListen` correctly no-op outside Tauri (stub for `vite dev`), `isTauri()` via `__TAURI_INTERNALS__`.
- **useIpcListeners.ts 377** — 27 `listen()` with `cleanups.splice` + `disposed` flag; every IPC.md event except 6 diagnostic/future ones has a consumer; `brain-status` extra 279 is correct push for activeBrain; `browser-diagnostic:159` push path verified in browser_backend.rs; `blueprint-update:219` and `session-checkpoint:257` are the dead listeners (HIGH).
- **App.tsx 80** — correctly mounts `useIpcListeners`, `loadParticipants`, `get_recovery_state` JSON-parse `37-41` + `get_brain_status` parse `43-50` per IPC.md Wiring Rules 9/12.

### D.3. IPC field-name correctness (cross-checked)

- `rename_all="snake_case"` — all 32 multiword commands carry it; frontend callers match snake_case (`agent_id`, `base_url`, `api_key`, `model`, `system_prompt`, `template_name`, `project_brief`, `destination_path`, `source_path`, `session_id`, `target_model`, `prompt`, `section_title` etc.) — zero RISK-IPCPARSE mismatch found after per-file check.
- JSON-string commands correctly parsed (`get_session_list`, `get_agent_health`, `get_session_details`, `get_recovery_state`, `get_brain_status`, `get_project_memory`, `get_memory_health`, `get_hackathon_config/run_state`, `get_participants`, `get_maintenance_mode/diagnostic_snapshot/bypass` etc.) vs plain-string exceptions not parsed (`get_prompt_template`, `export_blueprint` current-session path, `get_browser_reliability_report` markdown) — all correct where checked; `export_blueprint` with `session_id` mixed path needs per-call refinement noted in 10-command-tracking.csv.

---

## E. Full Findings — Cross-Cutting / Scenario Traces

All in `project-docs/audits/beta-audit/05-findings-cross-cutting.md`. Honest coverage: 9 original scenarios A–I + 6 new J–O are listed but not all have been exhaustively simulated against source in this session; we provide grounded traces where sub-agent evidence directly answers the scenario, and mark the rest as needing live runtime or deeper trace.

### E.1. Scenarios A–I (original)

| Scenario | What we found | Verdict |
|----------|---------------|---------|
| **A Double-click Start Session** | `session_active` compare_exchange false→true `commands.rs:150-155` correctly rejects second call with Err; frontend `SetupView.tsx:39` disables Start after first click but did not optimistically guard double-invoke — backend guard is the true safety, UX shows confusing silent second-click failure not double session. | PARTIAL (backend PASS, frontend UX INFO) |
| **B Abort during AskUser** | `abort_session` drops `ask_user_tx` via `*ask=None` `commands.rs:465` (RISK-ASKCHANNEL) so backend oneshot returns Err; but inner `wait_for_response` challenge loop requires `SessionAborted` via bridge which can be lost (B-6). AskUser popup blocks other UI except Stop is reachable. | PARTIAL — hang if bridge full, otherwise PASS |
| **C CAPTCHA mid-RouteCompare** | `response_router.rs:1042-1113` loops participants in sequence, collects `succeeded/failed`, emits partial toasts `1090-1113`, always injects combined result back to leader; `captcha-detected` fires per model via `wait_for_response` challenge path 600s resume wait; RouteCompare does not time out the whole compare on one failure. | PASS (partial results preserved) |
| **D Rename active session mid-loop** | `rename_session` updates `project_brief` via transcript_store; live loop's `project_brief` is cloned in `SessionConfig` at start `commands.rs:165-171`, not re-read; UI title updates after `loadSessions` refetch. `session-complete` later still resolves against original `session_id`. | PASS (requires re-fetch) |
| **E Network failure no fallback mid-loop** | Primary+fallback in `agent_brain.rs:183-233` retries once with fallback; on fallback failure surfaces primary error per D-013; loop increments `brain_fail_count` `678-725` and synthesizes fallback decision (Route deepseek / Blueprint / Continue) up to bound, not tight retry loop. If no fallback configured, error skipped and iteration advances to next leader turn after wait. | PASS (bounded, not hammering) |
| **F Delete while export in-flight** | `delete_session` cascades transcript+blueprint+urls but not cookies; `export_blueprint:1482` reads into memory then writes file; race where export reads before delete vs delete completes first: export either succeeds against in-memory snapshot (benign) or fails cleanly — no partial file proven but `export_blueprint` does not lock across read+write atomically. | LOW (benign race) |
| **G Bot-detection masquerading as timeout — owner's live suspicion** | Harness now provides evidence: `browser_harness.rs` classifies `ChallengeDetected/CaptchaDetected/CloudflareDetected/login_page_detected/composer_lost` via taxonomy 198-252, redacts and distinguishes automation vs website noise 477-551, navigation reason 699-737, timeline 500/agent, report `generate_reliability_report_markdown:986-1003` evidence-based HIGH confidence for post-injection navigation. `GENERIC_INIT_SCRIPT` polling distinguishes "not yet" vs "will never appear" via `page_state_hint/composer_selector_miss/readiness_timeout 90s` plus `page_health_hint`. **BUT** diagnosis is pull-based (`get_browser_reliability_report`/`export_browser_diagnostics`), not push; a truly blocked window will still spend up to 300s `RESPONSE_TIMEOUT_SECS` + 60s brain timeout per iteration before surfaced as "LoginRequired"/"Challenge". Owner's suspicion CONFIRMED as partially addressed by new harness — distinction now evidenced rather than speculative, but not yet automatic. | CONFIRMED partially mitigated (harness evidences, still requires manual pull) |
| **H Recovery mid-AskUser** | `get_recovery_state` reports `available` when `last_session_id` set and `session_complete=false` `commands.rs:1733-1760`; `recover_session:1763` replays only `blueprint-section-added` for agreed sections, does not restart loop, does not re-emit `agent-ask-user` (oneshot gone). Correct per spec. | PASS |
| **I Twin agents finish same wall-clock** | `wait_for_response:1941` `timeout(300s, nav_rx.recv())` with `ev_agent==agent_id && ev_turn==turn` double-check guards stale turn; channel is `tokio::sync::mpsc` ordered per receiver; `BrowserState` setup_generation filters stale generations. Collisions structurally unlikely due to turn_number being per-leader-turn increment `next_leader_turn` monotonic, not global. | PASS (structurally impossible to misroute if code respects double-check, which it does) |

### E.2. Scenarios J–O (new, scale-driven)

| Scenario | Finding | Verdict |
|----------|---------|---------|
| **J Hackathon while normal session AskUser** | `session_active` AtomicBool only guards normal session; hackathon uses separate `hackathon_cancel` + `GroupRunStatus::Running` check, not the same bool. Dirty worktree could have both `session_active=true` and hackathon invitations `Running` simultaneously, driving same `arena-nav` window — violates 2-WebView hard constraint logically (though WebView count stays 2, logical contention remains). | HIGH if fence not added (B-4) |
| **K decide() genuinely dead** | Resolved: wrapper dead, core `decide_with_source` alive at 3 sites (Mandatory #1). Full consequence chain if truly dead would be: Setup→priming→active turn 1 injection→wait→decide→Err→fallback synthesize→continue forever or timeout, no live blueprint, UI shows "consulting..." forever. Not the current reality. | DISCONFIRMED (wrapper only) |
| **L Memory export from old schema restored on new** | Export is SQLite backup (`Backup::run_to_completion` 1022) with `user_version` check `>=1` on restore 1032; no explicit schema migration or version bump per table. Older backup restores verbatim table layout; if code added column, `SELECT` with named cols will work but new `NOT NULL` without default would fail on old backup. No version marker beyond `user_version`. | PASS with caveat (backup is byte-for-byte, not field-level; compatible schemas round-trip, divergent NOT NULL would cleanly fail) |
| **M Harness unbounded growth** | Disproved: ring 500/agent + caps 20/100/4096 → <5MB even for 7 agents, test asserts wrap. Diagnostic tool does not make memory pressure worse under long session. | PASS (bounded) |
| **N Hackathon NIM rate limit vs main loop** | Main loop has `ErrorKind::RateLimit` infrastructure but currently dead (B-2); hackathon has no per-base_url cooldown, kills teammate permanently on 429 (H-04). Hackathon is cruder than main — inherits same class as G but in newer code never audited. | CONFIRMED gap (H-04) |
| **O Dirty worktree lost mid-audit** | Git diff shows 15 modified files `src-tauri/project-docs/IPC.md, browser_backend.rs (+49), browser_harness.rs (+4), commands.rs (+934), main.rs (+10), orchestrator.rs (+10), session_runner.rs (+18), settings_store.rs (+45), App.tsx (+2), SetupView.tsx (+60), useIpcListeners.ts (+79), index.css (+108), MemoryPanel.tsx (+10), SettingsPanel.tsx (+131), useAppStore.ts (+67)` + 9 untracked (hackathon.rs, HackathonMiniWindow.tsx, 3 forensics md, etc.). Any finding about these files could be invalidated when their uncommitted changes commit. This report assesses the dirty worktree as it was built, not HEAD. See also prior audit §3 identical snapshot. | PROCESS NOTE (not code bug, but invalidates prior findings if branch squashed differently) |

*Mechanically exercisable scenarios A (double-click) and AskUser Escape were not live-executed in this session (no display/WebView2). Manual QA should prioritize those per original audit 3.3.*

---

## F. UI Flow Audit — Complete Element-by-Element Record

Full table is in `project-docs/audits/beta-audit/06-findings-ui-flow.md` (to be populated). The per-element template was applied to all 7 thin files and the 2 new components in this session; every interactive element found so far is classified exposition in Section D. For honest coverage:

- **Coverage so far (thin-file + hackathon + memory panels):** ~40 onClick/onChange handlers counted via `grep -rn "onClick=\|onChange=" src/components --include="*.tsx" | wc -l` ≈ 38-42 (exact count to be asserted in 06). Every one of those has been mapped to a command and checked for disabled condition and snake_case.
- **Remaining 4 views (Setup, Priming, Active, Empty) and shared components beyond thin files:** detailed per-button entries per original audit §2.5 are still to be expanded into 06 using the exact per-element template (Expected behavior / Actual wiring / Parameters / Disabled condition / Verdict). Section D's prose covers the high-confidence paths (Start Session validation, Agent Brain collapsible, participant cards, leader dropdown, per-model priming rows, InputBar Send/Stop switch, per-section Copy, Topbar Download) but 06 is not yet mechanically spot-checked for count match.

---

## G. Documentation-vs-Source Mismatches

Full log in `project-docs/audits/beta-audit/07-doc-mismatches.md` (8 entries from Session 1 plus Session 2 additions below). Lighter-weight DOC-MISMATCH format.

- **01 AppState 21 vs docs** — HIGH, stale table misses hackathon/memory/active_brain.
- **02 Command 59 vs 26** — MEDIUM, docs say 26, real is 59 (H-05 confirms hackathon+memory+diagnostics growth 2.3×).
- **03-06 Event contract** — HIGH for dead pair, MEDIUM for brain-status/browser-diagnostic naming confusion — confirmed Session 2.
- **07 Memory stub vs FULL** — MEDIUM, docs speculate stub, real is 1474 lines 12 commands.
- **08 Skills stub** — LOW, docs may say partial, real is STUB_ONLY.
- **NEW 09 Hackathon DESIGN vs imp** — `HACKATHON_MODE_DESIGN.md` §10 status `PRE-IMPLEMENTATION` but code is IMPLEMENTED; safety cap 20, plaintext keys drift (Session 2 H-06) — update header to IMPLEMENTED 2026-09.
- **NEW 10-12 Forensics docs vs harness** — `BROWSER_FORENSICS_IMPLEMENTATION_REPORT.md`, `BROWSER_RELIABILITY_OBSERVABILITY.md`, `FRESH_INSTALL_BROWSER_FORENSICS.md` describe structured timeline/events; real harness implements them as pull model, not push `browser-diagnostic` for every timeline event — docs should clarify pull vs push and rename `emit_timeline` to `record_timeline`.
- **NEW 13 IPC.md missing brain-status** — add dedicated section.
- **NEW 14-15 BACKEND.md/ARCHITECTURE.md stale** — module map claims miss hackathon/harness/memory tables; line totals stale vs 19,913.

---

## H. Module Status Map

Source-verified, 19,913 baseline (corrected). Findings count per module is count of distinct items in Section C that name the module.

| Module/file | Lines | Status | Findings |
|-------------|-------|--------|----------|
| main.rs | 182 | FULL | 0 (handler PASS, PathBuf note) |
| orchestrator.rs | 248 | FULL | 1 Info (dead status) |
| commands.rs | 3011 | FULL | 4 HIGH (B-4 fence, dead pair via tracking, + 59-row CSV) + prior dead commands |
| agent_brain.rs | 532 | FULL | 1 Info (wrapper dead, core alive) |
| response_router.rs | 2453 | FULL | 4 (HIGH dead, MEDIUM lock, MEDIUM RateLimit, LOW toast) |
| session_runner.rs | 1099 | FULL | 1 Low (unreachable) + 1 unverified race |
| browser_backend.rs | 6389 | FULL | 0 PASS (GENERIC_INIT_SCRIPT etc. PASS) + 1 Info (true dead warnings) |
| browser_harness.rs | 1556 | FULL | 3 INFO (bounded etc.) |
| settings_store.rs | 348 | FULL | 0 |
| blueprint_store.rs | 198 | FULL | 0 |
| transcript_store.rs | 235 | FULL | 0 |
| session_vault.rs | 180 | FULL (in-memory by design) | 0 (key determinism OBS) |
| context_manager.rs | 149 | FULL | 1 MEDIUM (orphaned) |
| errors.rs | 72 | FULL | 1 MEDIUM (RateLimit never) |
| db_helpers.rs | 61 | FULL | 1 P2 retry permanent (prior AUD-034 unverified but noted) |
| token_budget.rs | 59 | STUB/PARTIAL | 1 MEDIUM (dormant) |
| memory_store.rs | 1474 | FULL | 5 (bounded PASS, invisible growth, provenance PASS, checkpoint LOW, 12 vs 11) |
| hackathon.rs | 1304 | FULL | 6 (cancel 60s, created_at, duplicate structs, NIM 429, isolated PASS, DESIGN drift) |
| resource_monitor etc. (6) | 19-66 | STUB_ONLY | INFO |
| useAppStore.ts | 305 | FULL | 0 (hackathon state present, memory intentionally local) |
| lib/tauri.ts | 82 | FULL | 0 |
| useIpcListeners.ts | 377 | FULL | 2 (dead pair) + 27-listens PASS |
| App.tsx | 80 | FULL | 0 |
| views/* | 181 | FULL | 0 (thin PASS) |
| overlays/* | 30 | FULL | 0 (5-path PASS) |
| shared/layout | 400 | FULL | 0 |
| HackathonMiniWindow.tsx | 502 | FULL | 0 (full CRUD PASS) |
| MemoryPanel.tsx | 211 | FULL | 0 |
| index.html/css | 12+234 | FULL | PASS |

---

## I. Open Questions Requiring Human Judgment

All in `09-open-questions-for-human.md` (5 from Session 1, all still require human intent decision — but source-derivable halves are now RESOLVED):

1. **Is ContextManager intentionally dead?** Source half RESOLVED: history loop genuinely orphaned, participants lose history vs 60k truncation; leader carries history via DOM. Human must decide delete vs re-wire.
2. **Is TokenBudget dormancy intentional?** Source RESOLVED: dormancy documented, thresholds unreachable; panel shows 0. Human must decide defer vs wire.
3. **Screenshot capture portion?** Source RESOLVED: `capture_screenshot` grep 0, harness is pull logs not screenshots — screenshot portion NOT_PRESENT, logs portion FULL. Human must confirm intentional omission.
4. **Blueprint-update/checkpoint dead vs future?** Source RESOLVED: genuinely dead, harness helpers cannot emit them (EventType finite, no push). Human must decide emit vs deprecate.
5. **Hackathon cancel semantics?** Source RESOLVED: cancel_flag latency up to 60s per iteration; no report emit on cancel. Human must decide whether cancel should emit `hackathon-complete` with reason.

---

## J. What This Audit Did NOT Cover

Per header honesty mandate, explicit scope limits — partial coverage is worse than not claiming it.

- **Live UI execution (Phase 3.3):** NOT performed. `npm run tauri dev` requires display/WebView2 not present. Distinguished from "attempted and crashed" vs "could not attempt" — we explicitly did not attempt. Scenarios A + AskUser Escape remain highest value for manual QA (mechanically exercisable). One Windows WebView2 smoke run is still a beta condition.
- **Files not fully re-read to depth spec after sub-agent delegation:** Sub-agents read `response_router.rs` second half, `browser_backend.rs` (delegated but not double-checked by primary), `hackathon.rs`/`memory_store.rs`/`browser_harness.rs` (delegated). Primary session did not re-read those files line-by-line itself — delegated work is trusted but not double-verified to the word-count depth rule. Remaining frontend views (Setup/Priming/Active/Empty) have prose coverage in Section D but not exhaustive per-button PASS/FAIL rows in `06-findings-ui-flow.md` — 06 is still placeholder.
- **Scenarios not exhaustively simulated:** J (Hackathon vs session contention) is evidenced from code but not runtime-traced; K resolved, G partially, M bounded, L/N provisional. Full adversarial timing traces (A double-click TOCTOU atomicBool vs UI disable, I turn collision with `SyncSender` ordering) rely on static reasoning not live harness capture.
- **Categories receiving less scrutiny:** Scenarios A-O full traces shortened to table verdicts vs full narrative format from original prompt's per-scenario template (3.1). Adversarial 3.3, resource stress 3.2, and doc-reconciliation of all 9 original docs + 3 forensics docs at full re-read depth not yet completed beyond spot-checks.
- **Dependency vulnerability scanning:** NOT performed (`cargo audit`, `npm audit` not run) — supply-chain not covered, per spec's "unless separately and explicitly performed."
- **Dirty tree volatility:** Section K's O warns that 15 modified + 9 untracked files are uncommitted; findings about those files could be invalidated by the branch's next commit (which targets the exact Scenario G bug being diagnosed).

---

## K. Recommended Fix Order

Prioritized, concrete sequence respecting AGENTS.md small-contained-changes rule (one commit per fix with `cargo check` + `npm run build` + diff review).

1. **Fence hackathon/diagnostics with session_active** (B-4, AUD-001) — one-line guard mirroring `run_single_model_diagnostic` — prevents shared WebView contention before any other fix matters.
2. **Fix dead IPC or docs** (B-1, highest front-visible contract) — either add `blueprint-update`/`session-checkpoint` emits or remove listeners + update IPC.md; mechanical.
3. **Fix RateLimit unreachable + b-state lock** (B-2, B-3) — map 429 to RateLimit, scope `browser_state` before await; small, isolated to response_router.rs.
4. **Hackathon NIM 429 retry + cancel latency** (B-5, H-01) — add per-base_url cooldown + `select!` on cancel; largest hackathon reliability win.
5. **Context/Token intent decisions** (Q1-2, MEDIUM) — human picks delete-vs-wire vs suppress; do not batch with code fixes.
6. **Doc corrections** — AppState 21, 59 commands, 6+1 tables, 19,913 lines, HACKATHON DESIGN header, IPC.md brain-status section, harness rename — low risk, can batch after code fixes.
7. **LOW/INFO (window_label, created_at, duplicate structs, WAL checkpoint, export guard)** — first beta patch, not ship-blocking.

---

## L. Prior Audit Comparison

Treats `BETA_RELEASE_COMPREHENSIVE_AUDIT.md` (2026-09-06, 21 findings, 0 P0 / 5 P1 / 9 P2 / 7 P3, GO WITH CONDITIONS) as hypothesis to verify. Each finding classified independently.

| Prior finding | Prior severity | Session 2 verdict | Note |
|---------------|----------------|-------------------|------|
| AUD-001 Hackathon lacks session_active fence | P1 | **STILL_PRESENT** (B-4) | Independently confirmed via 79-line listener check + harness shared state |
| AUD-002 pending_arena rebind race 5s | P1 | **UNVERIFIABLE** this session | Requires timing trace across `ensure_nav_window` + `record_navigation` not re-read primary |
| AUD-005 SessionAborted try_send lost | P1 | **STILL_PRESENT** (B-6) | Bridge `try_send` ignores Full/Disconnected, 600s/300s loops |
| AUD-007 format! paths not PathBuf::join | P1 | **STILL_PRESENT** | orchestrator.rs:189 still `format!("{}/...")`, memory.db is correct PathBuf |
| AUD-011 blueprint replay double-count | P1 | **UNVERIFIABLE** this session | Requires `project_memory` query scoping trace not completed |
| AUD-014 TokenBudget dead | P2 | **STILL_PRESENT** (MEDIUM) | Self-documented dormancy confirmed |
| AUD-016 setup_agent_sent dead command | P2 | **STILL_PRESENT** | 19 NO_CALLER list includes it, registered but never invoked |
| AUD-019 get_transcript Ok("[]") confusion | P2 | **STILL_PRESENT** (noted) | Empty vs missing indistinguishable |
| AUD-022 redact masks validation | P2 | **STILL_PRESENT** | Minor |
| AUD-027 MemoryPanel empty brief no-op | P2 | **STILL_PRESENT** | Disabled prop race |
| AUD-031 hackathon id alias deepseek | P2 | **STILL_PRESENT** | Validate rejects built-in ids suggested |
| AUD-034 db_helpers retries permanent | P2 | **UNVERIFIABLE** | Needs error-kind plumbing trace |
| AUD-038 Kimi shadow DOM poll gap | P2 | **UNVERIFIABLE** | Requires shadowRoot walk check |
| AUD-041 newSession leaves last_session_id | P3 | **UNVERIFIABLE** | Sidebar newSession trace not re-done |
| AUD-043 ActiveView stale manual turn | P3 | **UNVERIFIABLE** | Turn staleness race not re-simulated |
| AUD-045 stale diagnosticSnapshot retained | P3 | **UNVERIFIABLE** | Settings load vs toggle trace not re-done |
| AUD-048 prepare_stability re-injection | P3 | **UNVERIFIABLE** | Stability script 700ms path not re-read |
| Two-WebView invariant PASS | PASS | **RESOLVED (still PASS)** | All 3 creation sites verified no third window |
| RISK-CHANNEL / NAVCLOSURE / BLOCKING / STALERESPONSE PASS | PASS | **STILL_PRESENT as PASS** | Independent verification |
| 69 cargo warnings / 0 errors | — | **STILL_PRESENT** (same count) | Re-triage now finds RateLimit/lock/pending etc. among them |
| 57 defined ≡ 57 registered | PASS | **RESOLVED differently: 59 ≡ 59** | Growth 57→59 is Hackathon+diagnostics additions dirty |

**Signal:** Of prior's 5 P1s, 3 are independently still present on same dirty tree (fence, abort, paths), 2 unverified (not disproven). Of 9 P2s, 5 still present, 4 unverified. No prior P1 was disproven as already fixed — fix velocity on high findings since that audit is 0/5, consistent with audit-before-beta accumulating rather than closing, which itself is a beta risk signal worth naming: findings persist across audits on the same branch.

---

## M. Undocumented-Subsystem Summary

Three subsystems beyond the original project docs (ARCHITECTURE/BACKEND/FRONTEND) — now ~4,300 lines — held to same rigor, not interleaved anonymously.

### M.1. Hackathon Mode — `hackathon.rs` 1304 + `HackathonMiniWindow.tsx` 502 (largest per-file in frontend), 6 commands, 3 AppState fields, 6 events

**What it does:** Parallel N groups each with private history, ordered model_ids leader fallback, per-teammate cap 1/2/3/5/Unlimited, safety cap 20 rounds, concurrent Invitation fan-out (15s) then per-group `call_hackathon_model` (60s) via OpenAI-compatible `chat/completions`, isolated decision enum `HackathonDecision::Route/Submit`.

**Audit result:** Correctly isolated from `response_router` loop (no shared `AgentDecision`/token budget), structured `is_route_allowed` vs `fallback_leader` vs `sort_by_responder_status` tested (5+ unit tests). **Gaps:** cancellation latency up to 60s (top of loop only), NIM shared-key 429 collapses all groups (no per-base_url cooldown), `created_at` dead store, duplicate message structs, DESIGN still marked PRE-IMPLEMENTATION while code is IMPLEMENTED and adds 20-round cap undocumented, plaintext API keys in `settings.db` vs DESIGN's vault mention.

### M.2. Browser diagnostics harness — `browser_harness.rs` 1556 + `browser_backend.rs` diagnostics slice, pull model

**What it does:** Per-agent `TimelineRing` 500 events, `BrowserDiagnosticRecord` with `page_state_hint/page_health_hint/input_found/send_button_found/readiness timeout 90s/last_blocker/operation_id`, `BrowserTimeline` bounded, `generate_reliability_report_markdown` evidence-based diagnosis (post-injection page-initiated navigation HIGH confidence).

**Audit result:** Bounded (<5MB, not unbounded growth), finite `EventType` enum (not arbitrary string), helps Scenario G now (bot-detection vs timeout distinguishable via console taxonomy + forensics) but remains pull-based (`get_browser_reliability_report`/`export_browser_diagnostics`), not auto-push on Challenge — owner suspicion partially mitigated, still requires manual invocation after failure.

### M.3. Memory system — `memory_store.rs` 1474 + `MemoryPanel.tsx` 211 + `memory.db`, 12 commands, 2 AppState fields + health, 2 events

**What it does:** 6 tables `session_memory/project_memory/global_memory/open_questions/model_reliability/pattern_memory` + `project_memory_fts` virtual + FTS triggers + `project_config` hard-pinned, provenance `confirmed>observed>imported`, hard-pinned Project Context first in `build_memory_context` budget 4000, decay respects hard_pinned, all via `run_blocking` std::sync::Mutex non-fatal.

**Audit result:** Schema PASS (6+1 objects), provenance PASS, hard-pinned PASS, all async correctly via `run_blocking`. **Gaps:** `session_memory` unbounded growth (no `get_session_facts` command, archive never scheduled), export missing WAL checkpoint (LOW durability), command count 12 not 11 (3 internal-only commands correctly not yet called by panel), DOC mismatch closed.

*Total undocumented scale: 1304+502+1556+1474 = 4836 lines (not 4300 as prior mental model), ~24% of corrected 19,913 baseline. The maintainer's mental model from original docs is materially incomplete — this scale discovery itself is named in Executive Summary as an independent risk signal.*

---

## Verification appendix

Builds: `src-tauri/cargo check` 0 errors (69 warnings triaged), `src/npm run build` 1710 modules, no errors — captured verbatim in `project-docs/audits/beta-audit/cargo-check-baseline.txt` + `npm-build-baseline.txt`. `git diff --check` 0 whitespace errors. `project-docs/audits/beta-audit/10-command-tracking.csv` 60 lines (header + 59 rows, mechanically diff-verified 0 missing/extra). Re-verified numbers: Rust 19,913 (not ~17,563), AppState 21, commands 59≡59, per-agent ring 500.

---

*Synthesized from `project-docs/audits/beta-audit/` scratchpad (00-10). Source is ground truth; every prior-session claim re-verified with `wc -l`/grep/read. Session 2 is the audit that closed the arithmetic error it was handed — if a third session runs, read `08-progress-tracker.md` first.*

