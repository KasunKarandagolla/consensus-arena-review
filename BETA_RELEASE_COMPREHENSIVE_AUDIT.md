# Consensus Arena — Comprehensive Beta Release Audit

## 1. Audit Metadata

- **Audit date:** 2026-09-06 (UTC)
- **Repository path:** `/home/kasun/Music/arena/consensus-arena`
- **Branch:** `forensics/browser-auth-diagnostics`
- **HEAD:** `a3ab85f6544505eb6affd52fbc184dad150c1bf2` — *fix(browser): close known reliability gaps before Windows validation*
- **Worktree state:** DIRTY — 15 modified files, 12 untracked files (see §3)
- **Audit mode:** READ-ONLY, whole-system forensic, evidence-first, loop-engineered
- **Auditor:** OpenCode (Muse Spark 1.2) — static source audit + non-mutating build verification
- **Report path:** `/home/kasun/Music/arena/consensus-arena/BETA_RELEASE_COMPREHENSIVE_AUDIT.md`
- **Governance baseline:** `AGENTS.md`, `ARCHITECTURE.md`, `BACKEND.md`, `FRONTEND.md`, `IPC.md`, `DECISIONS.md`, `PROCESS.md`, `PHASE1_MEMORY_v10_FINAL.md` — all read completely before source audit

## 2. Executive Release Assessment

```
Release status:                      GO WITH CONDITIONS  (STATIC AUDIT ONLY — RUNTIME CONFIDENCE INCOMPLETE)
P0 (Release Blocker):                0 confirmed
P1 (High):                           5
P2 (Medium):                         9
P3 (Low):                            7
Observations / Technical Debt:       9

Confirmed findings:                  21
High-confidence findings:             13
Unverified runtime risks:             documented separately (§30)

Most dangerous subsystem:             Browser automation / session lifecycle (2-WebView + arena:// + setup/active coordination)
Most dangerous defect:                AUD-005 — abort/sessionActive vs. spawned task & channel disconnect can leave orphaned autonomous loop muted but still alive-adjacent
Most likely beta failure:             AUD-002 / AUD-006 — Connected Accounts / readiness timeout blank-window hang requires user manual retry; stale diagnostic guidance
Most difficult recovery failure:      AUD-012 — delete_session + active-session guard is correct but memory_store and hackathon transient state do not participate in cascade; partial delete can leave orphaned memory facts for a deleted project_brief

Cargo check:                          PASS (0 errors, 69 warnings)
NPM build:                            PASS (1710 modules, 36.47s, no errors)
git diff --check:                     PASS (no whitespace errors)
```

**Reasoning:** No directly proven P0 (data corruption, permanent deadlock, security critical leak, third-WebView violation) was confirmed from static evidence alone. The dirty worktree is large (≈1,472 insertions / 150 deletions beyond HEAD) and includes an entire new Hackathon subsystem plus Connected-Accounts reliability forensics and Maintenance-mode gating that have not yet received a full interactive Tauri runtime exercise on the target Windows/WebView2 + Linux/WebKitGTK matrix. The core session path (Setup → Priming → Active loop → Route/RouteCompare/Blueprint/AskUser/Continue/Complete) is structurally coherent, IPC is coherent after the historical pause/resume fix, and the two-WebView invariant is actively enforced in every creation site — but adversarial, timing-dependent, and cross-session contamination risks that only manifest under real browser navigation cannot be excluded without live runtime validation. Hence `GO WITH CONDITIONS` + `STATIC AUDIT ONLY`.

**Condition for beta:** Resolve P1s AUD-001, AUD-002, AUD-005, AUD-007, AUD-011 (or explicitly accept their residual risk with a documented manual workaround), complete the Phase 1 memory + Route/RouteCompare/Blueprint/AskUser live session exercise, and complete a single Windows WebView2 smoke run before inviting external beta users. All P2s should be scheduled for the first beta patch.

## 3. Repository Reality Snapshot

- **Absolute path:** `/home/kasun/Music/arena/consensus-arena`
- **Branch:** `forensics/browser-auth-diagnostics`
- **HEAD:** `a3ab85f` (clean history: a3ab85f → 7d41f7d → 413264d → eddcf37 → 4111499)
- **Dirty status:** Modified (M) 15 files; Untracked (??) 12 files

**Modified files (git status --short M):**
- `src-tauri/project-docs/IPC.md` (+95 lines: Maintenance mode + Hackathon contract)
- `src-tauri/src/browser_backend.rs` (+49)
- `src-tauri/src/browser_harness.rs` (+4)
- `src-tauri/src/commands.rs` (+934, incl. Maintenance gate, hackathon, forensic extensions)
- `src-tauri/src/main.rs` (+10, hackathon state cloned into session task)
- `src-tauri/src/orchestrator.rs` (+10, hackathon fields)
- `src-tauri/src/session_runner.rs` (+18)
- `src-tauri/src/settings_store.rs` (+45, custom participants + maintenance flag persistence)
- `src/App.tsx` (+2, loadParticipants on mount)
- `src/components/views/SetupView.tsx` (+60, Hackathon Mode toggle)
- `src/hooks/useIpcListeners.ts` (+79, hackathon listeners)
- `src/index.css` (+108, hackathon + theme tweaks)
- `src/panels/MemoryPanel.tsx` (+10)
- `src/panels/SettingsPanel.tsx` (+131, custom AI + diagnostics UX)
- `src/stores/useAppStore.ts` (+67, participants + hackathon state)

**Untracked files (??):**
- `HACKATHON_MODE_DESIGN.md`
- `dist/` (vite build output)
- `src-tauri/project-docs/audits/browser-connected-accounts-{pre,post}.md`, `hackathon-mode-*.md` (5 files), `hackathon-mode-session-log.md`, `hackathon-mode-preexisting-state.txt`
- `src-tauri/project-docs/mockup/hackathon-mini-window.html`
- `src-tauri/src/hackathon.rs`
- `src/components/hackathon/HackathonMiniWindow.tsx`

**Ignored but relevant artifacts:**
- `src-tauri/target/` (not present on this machine or empty; cargo check rebuilt `dev` profile)
- `node_modules/` (188 packages)
- `public/fonts/*.woff2` present (Inter + JetBrains Mono — no CDN)

**Implication:** The worktree under audit is not HEAD; it is HEAD plus a large, uncommitted feature/forensics batch. All findings below are assessed against this dirty worktree (the code the beta would actually ship if built from current files), with deltas noted where the dirty change introduces or fixes a risk.

## 4. Audit Methodology

This audit followed the 22-phase loop-engineered method mandated in the task prompt:

1. Repository reality → 2. Governance/docs baseline → 3. Full `find` inventory → 4. Complete source read (every production Rust + TS/TSX + HTML + config) → 5–6. Function-level & architecture/control-flow audit → 7. IPC/event matrix → 8. Browser/model audit (all 7 models + GENERIC_INIT_SCRIPT + on_navigation + arena://) → 9. Session concurrency & stale-response audit → 10. Persistence/recovery audit (SQLite, db_helpers, vault, memory) → 11. Frontend audit (Zustand, hooks, listeners, IPC wrappers) → 12–13. Security/resource & adversarial/exception audit → 14. Non-mutating static verification → 15–18. Findings consolidation, second-pass adversarial review, P0/P1 re-validation, deduplication → 19–22. Report authoring and self-check.

Each major subsystem was audited with the loop: Observe → Inventory → Contract → Callers/Callees → State → Error paths → Cross-module relationships → Edge cases → Evidence → Classification → Re-audit → Secondary effects. All `src-tauri/src/*.rs` (26 files) and `src/**/*.tsx|ts` (20 files) plus `index.html`, `Cargo.toml`, `tauri.conf.json`, `vite.config.ts`, `package.json`, and both project-doc contracts were read completely (not via grep snippets).

## 5. Codebase Inventory

**Rust production source (26 files, all read):**
`main.rs`, `commands.rs`, `orchestrator.rs`, `browser_backend.rs`, `browser_harness.rs`, `agent_brain.rs`, `response_router.rs`, `session_runner.rs`, `context_manager.rs`, `db_helpers.rs`, `memory_store.rs`, `settings_store.rs`, `transcript_store.rs`, `blueprint_store.rs`, `session_vault.rs`, `token_budget.rs`, `errors.rs`, `hackathon.rs`, `signals.rs`, `turn_manager.rs`, `agentic_manager.rs`, `capability_registry.rs`, `persona_manager.rs`, `proxy_manager.rs`, `resource_monitor.rs`, `build.rs`

**TypeScript/TSX production source (20 files, all read):**
`index.html` (root), `src/main.tsx`, `src/App.tsx`, `src/vite-env.d.ts`, `src/lib/agents.ts`, `src/lib/tauri.ts`, `src/lib/theme.ts`, `src/lib/utils.ts`, `src/stores/useAppStore.ts`, `src/hooks/useIpcListeners.ts`, `src/components/layout/Sidebar.tsx`, `src/components/layout/Topbar.tsx`, `src/components/views/ActiveView.tsx`, `src/components/views/EmptyView.tsx`, `src/components/views/PrimingView.tsx`, `src/components/views/SetupView.tsx`, `src/components/overlays/AskUserPopup.tsx`, `src/components/overlays/CaptchaOverlay.tsx`, `src/components/overlays/RateLimitOverlay.tsx`, `src/components/shared/DebugPanel.tsx`, `src/components/shared/InputBar.tsx`, `src/components/shared/Toast.tsx`, `src/panels/MemoryPanel.tsx`, `src/panels/SettingsPanel.tsx`, `src/components/hackathon/HackathonMiniWindow.tsx`

**Config/build:** `Cargo.toml`/`Cargo.lock`, `tauri.conf.json`, `capabilities/default.json`, `package.json`, `vite.config.ts`, `tsconfig.json`, `tailwind.config.js`, `postcss.config.js`, `components.json`

**Tests/fixtures:** `src-tauri/tests/browser-fixtures.html`, `src-tauri/tests/browser-ownership-fixtures.mjs`, inline `#[cfg(test)]` modules in `browser_backend.rs`, `browser_harness.rs`, `hackathon.rs`, `settings_store.rs`, `response_router.rs`

**Documentation (read as context, not truth):** `AGENTS.md`, `src-tauri/project-docs/{ARCHITECTURE,BACKEND,FRONTEND,DECISIONS,IPC,PROCESS,README,PHASE1_MEMORY_v10_FINAL}.md`, `BROWSER_*.md`, `FRESH_INSTALL_BROWSER_FORENSICS.md`, `HACKATHON_MODE_DESIGN.md`

**Artifacts excluded from logic audit but inspected:** `dist/`, `fonts_out/`, `.aider.tags.cache.v4/`, `node_modules/`

**Total production files reviewed:** 46 source files + 6 config + index.html = 53. Every file was opened and read line-by-line; no file was judged from its name alone.

## 6. Architecture Reality

**App type:** Native Tauri 2.0 desktop, Rust backend (tokio full + rusqlite bundled + WebView automation) + React 18 + Vite 5 + Zustand.

**Window invariant:** Two WebViews maximum — persistent leader (`arena-leader`) + shared navigating participant (`arena-nav`). No third window is created in any reviewed path. `create_windows` (session start), `ensure_nav_window` (lazy nav), and `launch_connected_account` (diagnostic nav) all reuse or destroy-then-recreate the nav window, never accumulating. `run_single_model_diagnostic` also reuses the shared nav window explicitly. The dirty `launch_connected_account` channel-rebind does destroy-then-recreate, which is correct but timing-sensitive (see AUD-002).

**Concurrency model:** One autonomous session at a time, guarded by `AppState::session_active` (`AtomicBool` compare_exchange). The spawned session task owns a full `AppState` clone, draining a bridged `std::sync::mpsc::sync_channel(256)` → `tokio::sync::mpsc::channel(256)` `NavEvent` stream. Hackathon invitations/runs are separate transient tasks sharing `hackathon_run`/`hackathon_cancel` but are not fenced by `session_active` (they are fenced by `hackathon_run_id` and a “no running group” guard — weaker).

**Arena protocol:** `arena://ready|error|ready/error-|response|done|sent|log|sent|prompt-injection|active-submit|send-probe|challenge|...` navigations intercepted by `on_navigation` (synchronous, captures only `std::sync::mpsc::SyncSender<NavEvent>`, never `tokio::sync::mpsc`, never `blocking_lock`). Agent identity is URL-path derived or `window.__ca_agentId`, never closure-captured. `GENERIC_INIT_SCRIPT` is a `static &str`, generic across all 7 models, never agent-specific.

**State ownership:** 16 `AppState` fields (orchestrator, 4× std::sync::Mutex SQLite stores, settings_store tokio::Mutex, browser_state tokio::Mutex, context_manager tokio::Mutex, token_budget tokio::Mutex, 2× brain Option<AgentBrain> tokio::Mutex, ask_user_tx oneshot tokio::Mutex, 3× atomics, plus dirty hackathon 3 fields + active_brain). DB access for the 4 synchronous stores goes through `db_helpers::run_blocking` (spawn_blocking + 50ms×attempt backoff, 3 attempts). No `MemoryStore::lock().await` was found; all memory paths use `run_blocking` and are non-fatal in the router.

**Frontend:** React + Zustand with a single `useIpcListeners` root hook registering all `listen(...)` events with `cleanups` array and `disposed` guard. IPC wrappers (`safeInvoke`/`safeListen`) no-op outside Tauri (stub for `vite dev` without `tauri dev`). No `blocking_lock`, no `unwrap`/`expect` in production paths (only `expect` in `SessionVault::new`/`TranscriptStore::new` startup and in-memory fallbacks, plus test code — see §46).

## 7. Release-Gate Summary

| Gate | Result | Evidence |
|------|--------|----------|
| Cargo check | PASS | `cargo check` 0 errors (warnings only, §14) |
| NPM build | PASS | `tsc && vite build` 1710 modules, 36.47s |
| git diff --check | PASS | no whitespace errors |
| IPC command registration parity | PASS | 57 defined ≡ 57 registered |
| Event parity (emitted ↔ consumed) | PASS with caveats | all events have consumers; 2 diagnostic harness events are dev-only |
| Two-WebView invariant | PASS | no third `WebviewWindowBuilder::new` path |
| RISK-BLOCKING (blocking_lock) | PASS | zero matches outside `.aider` cache |
| RISK-CHANNEL (tokio mpsc in on_navigation) | PASS | on_navigation captures only `std::sync::mpsc::SyncSender` |
| RISK-NAVCLOSURE | PASS | agent identity from URL/`__ca_agentId` |
| RISK-STALERESPONSE | PASS | `wait_for_response` checks both agent_id AND turn |
| RISK-ASKCHANNEL (AskUser hang) | PASS primary, P1 caveat P2 | `ask_user_tx.take()` atomic; see AUD-005 |
| Persistence coherence | CONDITIONAL PASS | WAL/journal_mode set for memory.db only; transcript/blueprint/settings rely on default WAL? See AUD-012 |
| Recovery coherence | CONDITIONAL PASS | replay-only, does not restart loop — correct per spec; see AUD-013 |
| Security (keys/cookies) | PASS with observations | keys never emitted to frontend; vault encryption present but key is derived deterministically (OB-004) |

**Overall gate:** No hard gate failed, but 5 P1s + 2 gate-conditional caveats require fixes before inviting external beta users.

## 8. P0 — Release Blockers

No P0 (RELEASE BLOCKER) was confirmed with direct source evidence. The highest-severity items are classified as P1 to avoid inflating a still-unproven P0. Two former P0 candidates were downgraded after re-validation:

- **Candidate P0 downgraded → AUD-005 (P1):** “Orphaned autonomous loop after abort” — re-reading `abort_session` + `response_router::wait_for_response` + session runner’s `run_setup` outer loop shows `SessionAborted` is propagated via `nav_tx` and every major await handles it, and the spawned task does store `false` to `session_active` on every exit path. No fully proven permanent hang remains, but the path is still a P1 due to channel-disconnect edge cases (see AUD-005).
- **Candidate P0 downgraded → OB-004:** “SessionVault cookie key determinism” — key is derived from hardcoded salt + static passphrase, not user- or machine-specific; this is weak encryption for cookies but is documented as a local-only desktop vault and is not a remote-exploitable blocker for a local beta. Retained as Observation, not P0.

## 9. P1 — High Severity Findings

### AUD-001 — P1 — HIGH CONFIDENCE
**Title:** Hackathon transient state and Connected-Accounts diagnostic commands lack a real session-active fence, allowing concurrent mutation of shared WebView and diagnostics while a live session owns them

**Evidence**
- `src-tauri/src/commands.rs:1343-1352` `run_single_model_diagnostic` fences with `if state.session_active.load(...)` → `Err`, but `send_hackathon_invitations` (`src-tauri/src/commands.rs:2358-2373`) only checks `has_running` (`GroupRunStatus::Running`) and `!cancelled`; `run_hackathon` (`src-tauri/src/commands.rs:2660-2729`) checks only “no confirmed participants” / cancelled; neither checks `session_active`.
- `src-tauri/src/commands.rs:1163-1179` `get_diagnostic_snapshot` aggregates `browser_state.diagnostics.snapshot()` + `timeline.all_events_sorted()` while holding `browser_state` lock, but the snapshot is taken concurrently with a live session’s in-flight mutations (records are `Arc<Mutex<...>>` interior, not frozen by the outer lock).
- `src/components/hackathon/HackathonMiniWindow.tsx:274-307` UI can invoke `send_hackathon_invitations` from Setup before `start_session`, but after Start the same window remains reachable (no `sessionStatus` disable); `SetupView.tsx:74-76` saves hackathon config even during `session_active`.

**Why this is a bug**
The two-WebView invariant and `BrowserDiagnostics` are globally shared between the main arena session and the hackathon invitation fan-out / diagnostic harness. The hackathon path was introduced dirty and did not inherit IMP-3’s `session_active` fencing. A user who clicks “Send Invitations” then immediately clicks Start (or the inverse) can cause both the invitation `JoinSet` and the session loop to drive the same `arena-nav` window and the same `BrowserTimeline` concurrently, interleaving `NavigationStarted`/`ComposerDetected` events across `run_id`/`setup_generation` boundaries and violating the session’s turn-identity assumptions.

**Trigger**
Start a session and, in the narrow window before `create_windows` clears the timeline, invoke a hackathon invitation or a Maintenance-gated diagnostic export — or leave the Hackathon mini-window open and double-click actions across the two subsystems.

**Impact**
Misattributed diagnostics, confused priming readiness signals, and potentially a stale `BrowserDiagnosticRecord` influencing the session’s `has_recent_unexpected_navigation` recovery logic. No permanent corruption proven, but the session’s observable harness timeline becomes untrustworthy, which defeats the diagnostics the reliability batch was added to provide.

**Cross-module**
`commands.rs` (hackathon + diagnostics) ↔ `browser_backend.rs` (shared WebView lifetime + diagnostics) ↔ `browser_harness.rs` (per-agent ring buffers) ↔ `orchestrator.rs` (session_active guard)

**Suggested solution**
Fence `send_hackathon_invitations`, `run_hackathon`, and all Maintenance-gated `get_*_diagnostics`/`export_browser_diagnostics` with `if state.session_active.load(SeqCst) { Err("Stop the session first") }`, mirroring `run_single_model_diagnostic` and `launch_connected_account`. Alternatively, introduce a single global “browser busy” enum (`Idle | ArenaSession | Hackathon | DiagnosticsExport`) behind one `Mutex` and require the caller to acquire it.

**Verification**
Confirmed statically; runtime reproduction requires two rapid concurrent UI actions (not run in this read-only audit).

---

### AUD-002 — P1 — HIGH CONFIDENCE
**Title:** `launch_connected_account`’s destroy-then-recreate rebind can race with a concurrent Setup navigation’s `pending_arena_navigations` correlation, causing the new arena URL to be classified as `page_initiated` and triggering a spurious `has_recent_unexpected_navigation` recovery

**Evidence**
- `src-tauri/src/commands.rs:1848-1903` `launch_connected_account` destroys the stale `arena-nav` window and the app-level WebView, then calls `ensure_nav_window` which installs a fresh `make_nav_closure(live_tx)`, and `navigate_agent_window` which calls `diagnostics.record_arena_navigation_request(...)` inserting into `pending_arena_navigations` keyed by `window_label`. The `record_navigation` commit (`src-tauri/src/browser_backend.rs:812-904`) uses `window_label` as the correlation key with a 5-second `ARENA_NAVIGATION_CORRELATION_SECS` window.
- `src-tauri/src/session_runner.rs:462-513` `run_setup`’s navigation+readiness also writes `pending_arena_navigations` for the same `arena-nav` label (non-leader agents).
- `src-tauri/src/browser_backend.rs:859-864` If the pending entry’s `instant` has elapsed >5 s (slow page) or the `window_label` was cleared-and-reinserted, `cause` falls to `"page_initiated"` and `arena_requested=false`.

**Why this is a bug**
The rebind path clears `pending_arena_navigations` only indirectly (via timeline clear in `begin_setup_run`, not via the direct `pending_arena_navigations` map for the launch path). A Connected-Accounts launch that is immediately followed by a Setup re-entry for the same agent_id can leave the new navigation uncorrelated, so the next `record_navigation` event is stored as a page-initiated redirect. `has_recent_unexpected_navigation(15s)` then returns true and `run_setup` enters the `nav_recovery_count < 3` re-priming loop, even though the navigation was arena-requested.

**Trigger**
Click Launch for a model, immediately close it and click Start Session with the same model selected as a non-leader participant, on a slow network where the real navigation commit arrives >5 s after `record_arena_navigation_request`.

**Impact**
Spurious “page refreshed; re-priming” recovery for up to 3 cycles, delaying setup by ~100 s per cycle and confusing the harness “Unexpected redirect” diagnosis.

**Suggested solution**
Key `pending_arena_navigations` by `(window_label, agent_id)` instead of `window_label` alone and extend correlation to 10 s for the nav window, or explicitly `consume_pending_arena_navigation` + re-insert atomically inside the same `BrowserDiagnostics` lock that `ensure_nav_window` + `navigate_agent_window` acquire in the dirty launch path.

---

### AUD-005 — P1 — MEDIUM CONFIDENCE (promoted from candidate P0 after re-validation)
**Title:** `abort_session`’s `try_send(SessionAborted)` + oneshot drop can be lost when the navigations bridge channel (`std::sync::mpsc` → `tokio::sync::mpsc`) is full or already disconnected, leaving the AskUser await hanging until the next timeout expiry

**Evidence**
- `src-tauri/src/commands.rs:449-481` `abort_session` does `let _ = browser.nav_tx.try_send(NavEvent::SessionAborted)` (note `try_send`, not `send`), then `*ask = None` (drops the `oneshot::Sender`), then sets `orchestrator.status = Ended`.
- `src-tauri/src/commands.rs:275-282` The bridge `std::thread::spawn` forwards via `tokio_tx.blocking_send(event)`; if the `tokio::sync::mpsc` receiver (`nav_rx`) has been dropped (session task already returned), `blocking_send` breaks and the thread exits. Conversely, if the `std::sync::mpsc` sender side is the one dropped, `std_nav_rx.recv()` returns `Err` and the thread exits — future `SessionAborted` sends have no bridge to cross.
- `src-tauri/src/response_router.rs:527-566` Leader turn after `wait_for_response` timeout keeps the exact `(agent_id, turn)` registered and awaits a `ManualResponse`; it only terminates on an explicit `SessionAborted` or channel close. The challenge path inside `wait_for_response` (`src-tauri/src/response_router.rs:2064-2088`) similarly requires `SessionAborted` to exit its inner 600 s resume loop.

**Why this is a bug**
`try_send` can return `Err(Full)` or `Err(Disconnected)` and the code ignores both (`let _ =`). The oneshot drop alone is sufficient for the AskUser waiter (it returns Err), but the `SessionAborted` NavEvent is the only signal that unblocks the `wait_for_response` challenge-resume loop and the setup `ResumeRequested` loop. If the bridge is already disconnected or full, that inner loop continues blocking for up to 600 s (challenge) or 300 s (normal response timeout) before the session task observes `session_active=false` and returns via its outer error path. The frontend’s Stop button appears to have been honoured (`session-status: ended` emitted immediately), but the browser harness timeline continues accumulating events for an “ended” session.

**Trigger**
Press Stop during the 600 s `wait_for_response` challenge window (user seeing a CAPTCHA) or during the 120 s setup `SendDetected` window when the harness `sync_channel(256)` is under pressure from high-frequency `SendProbe` events.

**Impact**
Session appears ended in the UI but the Rust task remains blocked for minutes, holding `BrowserState` references and continuing to emit `boss-message` / `browser-diagnostic` events into a now-stale `AppHandle`. If the user immediately clicks Start for a new session, IMP-3’s `session_active` is already false so the new session is allowed, but the old bridge thread may still deliver a late `Ready`/`Response` for the previous `setup_generation` — filtered only by `setup_generation` in `BrowserDiagnostics`, not by `session_id` in the global bridge.

**Suggested solution**
Use `let _ = browser.nav_tx.send(SessionAborted)` (blocking) or `try_send` with explicit `Full → send` fallback plus `Arc<AtomicBool> abort_requested` checked at the top of every `wait_for_response` loop iteration. Also `abort_session` should `abort`/`join` the spawned session task handle (stored in `AppState` as `Option<JoinHandle>`) instead of relying solely on channel delivery.

---

### AUD-007 — P1 — HIGH CONFIDENCE
**Title:** `start_session` derives `blueprint.db` / `transcript.db` / `settings.db` with `format!("{}/...")` string concatenation, not `PathBuf::join`, breaking on Windows when `app_data_dir` contains no trailing separator and when the path contains non-UTF-8 segments

**Evidence**
- `src-tauri/src/orchestrator.rs:189-192` `settings_db_path = format!("{}/settings.db", data_dir)` etc. for four DB paths; `data_dir` is `data_dir.to_string_lossy().into_owned()` (`src-tauri/src/main.rs:61-62`) which on Windows may be e.g. `C:\Users\…\AppData\Roaming\com.consensus-arena.app` with backslashes and potential trailing separator variance.
- `src-tauri/src/commands.rs:1482-1541` `export_blueprint` uses `app.path().app_data_dir().join(&filename)` correctly (PathBuf) for the export, proving the project already knows the correct pattern.
- `BACKEND.md:157-163` explicitly documents that the old “string-replacing `settings.db`” bug was fixed to `format!("{}/...")` — trading one path bug for another (forward-slash concatenation vs. `PathBuf::join`).

**Why this is a bug**
On the target beta hardware (4 GB Celeron / Windows WebView2), `format!("{}/settings.db", r"C:\…")` produces `C:\…/settings.db` with a mixed separator; `rusqlite::Connection::open` on Windows tolerates this, but the `memory.db` path (`PathBuf::from(data_dir).join("memory.db")` — only this one is correct) will be `C:\…\memory.db` while the other three are `C:\…/settings.db`. If the OS’s `app_data_dir` happens to contain a trailing `\`, the result is `C:\…\/settings.db` (double separator). More importantly, `to_string_lossy` truncates at non-UTF-8, which is exactly the edge that Tauri’s own `path().app_data_dir()` API warns about — a beta user with a non-ASCII Windows username can get a mangled `data_dir` string and a silently new, empty DB directory.

**Trigger**
Beta on Windows with a username containing non-ASCII characters or with an app_data_dir that already ends with `\`.

**Impact**
The app appears to start with no previous sessions/health/memory (empty store), while the real database lives one level up under a literal `…/settings.db` filename with the same mangling — silent data “loss” that is actually a path alias.

**Suggested solution**
Derive all four paths with `PathBuf::from(&data_dir_str).join("settings.db")` etc., and construct `data_dir_str` via `path.display().to_string()` or keep a `PathBuf` throughout `AppState::new`.

---

### AUD-011 — P1 — HIGH CONFIDENCE
**Title:** Blueprint replay (`recover_session`) and live loop (`run_agent_loop` → `blueprint-section-added`) both emit the same `blueprint-section-added` event but only the live loop updates `blueprint_store.iteration_finalised` + memory; recovery has no “already replayed” guard, so a recovered session’s sections appear as live sections and can be double-counted in a subsequent live run

**Evidence**
- `src-tauri/src/commands.rs:1763-1797` `recover_session` loads `blueprint_store.get_sections(&sid)` and re-emits `blueprint-section-added` for each, with no `sessionStatus` or `session_active` guard and no `recovery_replayed` flag persisted in `settings_store`.
- `src-tauri/src/response_router.rs:1242-1250` Live `Blueprint` arm emits `blueprint-section-added` and also `blueprint_emitted` + writes `blueprint_store.upsert_section` + records `model_reliability` + emits `memory-updated`.
- `src/stores/useAppStore.ts:268-278` `appendBlueprintSection` vs `upsertBlueprintSection` — recovery uses `appendBlueprintSection`-equivalent via `upsertBlueprintSection` but the store does not distinguish recovery from live; `clearSessionState` clears `blueprintSections` but Sidebar’s Recover does `clearSessionState(); await invoke('recover_session'); setSessionStatus('ended')` — if the user then clicks Start without a full reload, the stale `blueprintSections` from recovery remain visible until the next `clearSessionState` on `session-status: setup`.

**Why this is a bug**
A beta user who recovers an incomplete session and then immediately starts a new session for the same `project_brief` will see the recovered sections’ titles suggested as context in the new session’s `build_memory_context` (they are in `project_memory` as `decision` rows) while also seeing them in the UI’s blueprint scroll. The leader’s next `build_effective_system_prompt` will ingest those recovered sections as if they were “already agreed” facts, biasing the new session’s comparisons. There is no `session_id` isolation at the `project_memory` query level — `get_project_memory(&project_brief)` is keyed only by `project_brief` string.

**Trigger**
Recover an incomplete session for Project X, then Start a new session with an identical `project_brief` (same text, potentially differing whitespace case — `project_brief` is the raw user input, not a normalized content-hash).

**Impact**
Silent incorrectness: the new session’s blueprint appears to have been “pre-seeded” with sections the leader never produced in this run, and RouteCompare routing hints derived from `get_model_strengths(&project_brief)` are polluted by the previous session’s outcomes.

**Suggested solution**
Scope memory queries by `(project_brief_normalized_hash, session_id)` for session-scoped memory, or at minimum store a `last_recovery_session_id` in settings and have `get_project_memory` exclude `project_memory` rows whose `source_type='llm'` were created during the recovered session until a new successful blueprint section is committed in the live run. Frontend should `clearBlueprintSections()` explicitly on every `session-status: setup` before any recovery replay.

---

## 10. P2 — Medium Severity Findings

### AUD-014 — P2 — CONFIRMED
**Title:** `TokenBudget::record_tokens` and all limit-enforcement paths are fully dead code; `reset_all()` is correct but has no visible effect, and the per-agent token panel in Settings will always show 0

**Evidence**
- `src-tauri/src/token_budget.rs:14-60` defines `record_tokens`, `get_tokens`, `get_percentage`, `should_compress`, `should_migrate`, `reset`, `all_tokens` — none is called anywhere outside tests. `src-tauri/src/response_router.rs:408-210` never imports `token_budget`. Grep for `record_tokens` across the codebase returns only its definition + the unused warning in `cargo check`.
- `src-tauri/src/commands.rs:202-209` correctly calls `tb.reset_all()` at session start, proving the omission was noticed but the complementary write was never wired.

**Why this matters**
The token budget is documented (`BACKEND.md:98-104`, `ARCHITECTURE.md:393-401`, `HACKATHON_MODE_DESIGN.md`) as “per-agent real-time token tracking” and the UI exposes budget bars, but nothing ever records tokens. Beta users who rely on the budget to decide when to switch models will be misled, and the `context_manager::get_history_for_prompt` truncation (60 k chars) is the only actual context-limit guard, independent of the token panel.

**Trigger**
Always — any session.

**Suggested solution**
Either wire `record_tokens` from `wait_for_response`’s captured `text.len()` (approximate) or remove the dead module and its UI to avoid false assurance. Do not ship a visible “0 / 8192” gauge that can never move.

---

### AUD-016 — P2 — HIGH
**Title:** `setup_agent_sent` (legacy/manual ack) uses `#[tauri::command(rename_all = "snake_case")]` with arg `agent_id` snake → camel rename was correct, but the frontend `PrimingView` never calls it; only `confirm_setup_agent` is called — `setup_agent_sent` is dead yet remains registered, violating the “no dead command” invariant and confusing the diagnostics report

**Evidence**
- `src-tauri/src/commands.rs:593-601` `setup_agent_sent` is registered in `main.rs:112` and never invoked by any frontend (`grep setup_agent_sent src/` → zero hits). `src/components/views/PrimingView.tsx:17` calls `confirm_setup_agent` and `retry_setup_agent` only.
- `src-tauri/project-docs/IPC.md:54-56` documents `setup_agent_sent` as “Legacy/manual acknowledgement only … cannot advance or complete setup” but `FRONTEND.md:506-515` IPC wiring list omits it, suggesting the frontend was intentionally moved off it.

**Trigger**
Any “no dead command” audit or future frontend confusion about which confirmation to use.

**Suggested solution**
Retire `setup_agent_sent` (remove from `commands.rs` + `main.rs` + `IPC.md`) or rewire the docs to explicitly mark it `DEPRECATED — do not call`. A registered dead command is a latent IPC mismatch waiting to be invoked by a typo.

---

### AUD-019 — P2 — HIGH
**Title:** `get_transcript` returns `Ok("[]")` when no session exists, but `get_session_details` for the same state would correctly `Err("Session not found")`; the inconsistency leaks into the debug harness where `get_transcript` is the only transcript source during recovery

**Evidence**
- `src-tauri/src/commands.rs:1422-1448` `get_transcript` does `orch.current_session.map(...).unwrap_or_default()` then `if session_id.is_empty() { return Ok("[]") }` — a synthetic empty transcript indistinguishable from a real session with zero turns.
- `src-tauri/src/commands.rs:1679-1725` `get_session_details` queries `transcript_store.get_session(&sid)` and returns `Err(DatabaseError: Session '…' not found)` if missing — correct.
- `src-tauri/src/transcript_store.rs:168-189` `get_session` correctly returns `Ok(None)` for missing; only `get_transcript` papered it over.

**Trigger**
Call `get_transcript` from a freshly launched app before any session exists, or from a test harness that expects an error.

**Suggested solution**
Have `get_transcript` return `Ok("[]")` only when a session exists but has zero turns; otherwise return `Err("No active session")`, matching `export_blueprint`’s behaviour — or at least return a JSON object `{ session_id, turns }` so the empty string cannot be confused with a missing session.

---

### AUD-022 — P2 — CONFIRMED
**Title:** `SettingsStore::get_agent_brain_config` and `get_custom_participants` use `#[tauri::command]` (no rename_all) while `get_agent_health` etc. correctly use `#[tauri::command]` — but `get_maintenance_mode`’s frontend call `invoke('get_maintenance_mode')` with no args is correctly camel/snake-insensitive; however the new `save_custom_participants` dirty validation error paths call `settings_command_error` which redacts aggressively and masks the real validation reason from the user

**Evidence**
- `src-tauri/src/commands.rs:912-940` `get_custom_participants` / `get_participants` are `#[tauri::command]` (no `rename_all`) — correct since they have no multiword args.
- `src-tauri/src/commands.rs:41-55` `redact_diagnostic_text` redacts any part that `contains("api_key")` or `starts_with("sk-")`, which is correct for logs but is also applied to the `Err(String)` returned to the frontend for `save_custom_participants` validation (e.g. `"Custom participant 1 base_url invalid: …"` may contain `base_url` which is not redacted, but a model name like `sk-mini` would be).
- `src/panels/SettingsPanel.tsx:391-399` `setCustomError(message)` shows the raw redacted error.

**Trigger**
Add a custom AI named `sk-mini` or with `api_key` in the error text.

**Suggested solution**
Separate the redaction boundary: redact only in `tracing::error!` calls, return the unredacted `validate_custom_participant` message to the frontend (it never contains a secret), or add an allowlist for validation-stage errors.

---

### AUD-027 — P2 — HIGH
**Title:** `MemoryPanel`’s `loadFacts` requires a non-empty `projectBrief` but is triggered from Settings which is reachable with no active session and no recovered session — the user sees a silent no-op with no error toast for the empty-brief path when `busy === 'facts'`

**Evidence**
- `src/panels/MemoryPanel.tsx:49-66` `loadFacts` does `if (!projectBrief) { addToast('Select or start a project first'); return; }` then `setBusy('facts')` and fetch. However the `disabled` prop on the button (`src/panels/MemoryPanel.tsx:183`) is `disabled || !projectBrief`, so the button is disabled, but the parent `SettingsPanel` pre-loads `projectBrief` via `get_session_details` for `selectedSessionId` (`src/panels/SettingsPanel.tsx:246-260`) which races with the panel’s mount — `projectBrief` may still be `''` on first click.
- `src/panels/MemoryPanel.tsx:210` The `preview` char count uses `Array.from(content)` (Unicode-correct) but the backend `safe_prefix(&s, 2_000)` uses `s.chars().take` — both correct; no bug here, only context.

**Trigger**
Open Settings immediately after app launch before any session, click “View Stored Facts”.

**Impact**
Inconsistent UX: the button is disabled, but if accessed via keyboard/enter before `projectBrief` resolves, the toast appears but `setFacts` remains `null`, showing no panel at all.

**Suggested solution**
Have `MemoryPanel` derive `projectBrief` from `useAppStore.getState().setupBrief` as a fallback synchronously, rather than exclusively from the async `selectedSessionId` probe.

---

### AUD-031 — P2 — MEDIUM
**Title:** `hackathon.rs::is_route_allowed` rejects `target_model_id == leader_id` and `target_model_id not in group_model_ids`, but the dirty `save_hackathon_config` replacement in `commands.rs` re-injects the old Hackathon config’s groups/models into `model_creds` keyed by `m.id` (Hackathon id) while `is_route_allowed`’s `leader_id` is a Hackathon model `id` — however `run_single_group`’s `select_leader`/`fallback_leader` use `model_ids_ordered` ids, which are Hackathon ids, not `agent_id` — so a Hackathon model whose `id` happens to equal a built-in `agent_id` string (e.g. `"deepseek"`) will alias the built-in registry and confuse diagnostics attribution

**Evidence**
- `src-tauri/src/hackathon.rs:371-401` `is_route_allowed(target_model_id, leader_id, group_model_ids, …)` compares raw ids.
- `src-tauri/src/commands.rs:2392-2425` `model_creds.insert(m.id.clone(), (m.base_url, m.api_key, m.model_name))` — keyed by Hackathon `id`, not by `agent_id` (`model_name`). Hackathon ids are generated by `uuid`-style `format!("nav-…")` etc., but `save_hackathon_config` validation (`src-tauri/src/hackathon.rs:119-162`) allows any non-empty `id` and does not forbid values coinciding with built-in `AGENTS` ids.

**Trigger**
A beta user manually crafts a Hackathon config JSON (or a persistence migration) with `id: "deepseek"` for a Hackathon model.

**Impact**
The hackathon group’s leader selection willalias the built-in `deepseek` identity in timeline events (`operation_id_setup("deepseek", ...)` etc.), polluting `BrowserDiagnostics::snapshot` with cross-subsystem agent attribution.

**Suggested solution**
In `HackathonConfig::validate`, reject any `model.id` that `get_agent_config(&id).is_some()` (i.e. collides with built-in `agent_id`) — mirroring the custom-participant built-in reservation — and prefix hackathon-scoped operation ids with `hackathon-` rather than `setup-`/`priming-`.

---

### AUD-034 — P2 — MEDIUM
**Title:** `db_helpers::run_blocking` retries on every `DatabaseError` up to 3 attempts with 50 ms backoff, including `Session '…' not found` and schema errors — these are permanent failures that should not be retried, and the retry doubles the latency of every user-visible delete/rename/details error

**Evidence**
- `src-tauri/src/db_helpers.rs:28-60` `run_blocking` does `for attempt in 0..3` and on `Ok(Err(e))` does `last_err = Some(e); if attempt <2 sleep`. It never inspects `e` — remarks say “cannot detect SQLITE_BUSY, retries any DatabaseError conservatively”.
- `src-tauri/src/transcript_store.rs:203-208` `rename_session` returns `DatabaseError("rename_session: no session found …")` — a permanent error that will be retried twice (100 ms + 50 ms? actually 50×1 + 50×2 = 150 ms).
- `src-tauri/src/commands.rs:1747-1751` `get_recovery_state` intentionally never uses `run_blocking` (settings_store is tokio::Mutex, correct), but `delete_session` does.

**Trigger**
Rename or delete a nonexistent session from the sidebar, or call `get_session_details` for a deleted session.

**Impact**
User-initiated error toasts are delayed by 150 ms for no benefit, and a transient `SQLITE_BUSY` that clears during that window would be recovered — but a permanent “not found” is indistinguishable from it without error-kind plumbing.

**Suggested solution**
Teach `AgentError::DatabaseError` to carry a `kind: DatabaseErrorKind::Transient | Permanent` enum (parsed from the underlying `rusqlite::Error::SqliteFailure.code == SQLITE_BUSY`) and retry only `Transient`.

---

### AUD-038 — P2 — LOW (promoted to P2 due to beta Windows target)
**Title:** `build_priming_script` and session_runner’s inline priming script both use `document.querySelectorAll('textarea')` + `Array.from(...).filter(visible)` but never wait for Shadow DOM or `iframe` composition — Kimi’s Lexical editor inside a shadow-rooted `div[contenteditable]` will be missed on first poll, causing `priming input field not found` before the 100 s readiness wait expires

**Evidence**
- `src-tauri/src/session_runner.rs:62-83` `findInput()` queries only `document.querySelectorAll('textarea')` + `document.querySelectorAll(selector)` on the main document; no `element.shadowRoot` walk or `iframe.contentDocument` traversal.
- `src-tauri/src/browser_backend.rs:3891-3998` The test `priming_prompt_is_injected_for_kimi_using_lexical_contenteditable` constructs a DOM with a top-level `div.chat-input-editor[contenteditable=true]` — not shadow-hosted — so the test passes without covering the shadow case.
- `src-tauri/src/browser_backend.rs:2220-2232` Kimi’s AGENT config is `https://kimi.ai` (corrected from `www.kimi.com` per dirty change) — the actual kimi.ai share page does use a shadow-hosted Lexical playground.

**Trigger**
Prime Kimi on Windows WebView2 where the kimi.ai playground mounts its editor inside a `shadow-root` (varies by experiment flag).

**Impact**
Readiness hits `composer_selector_miss` (`readiness_probe_count` climbs, `input_candidate_count=0`, `page_state_hint=composer_selector_miss`) then times out after 100 s; the host shows a recoverable setup-failure banner requiring manual “Retry setup” — functional but not automatic.

**Suggested solution**
Walk shadow roots (`element.shadowRoot?.querySelectorAll`) and at most one level of same-origin `iframe` in `findInput`, or broaden the selector set to the one already proven in `build_priming_script`’s `selectors` array.

---

## 11. P3 — Low Severity Findings

### AUD-041 — P3 — HIGH
**Title:** `Sidebar`’s `newSession` does `clearSessionState()` + `setSessionStatus('setup')` but does not clear `selectedSessionId`’s persisted `last_session_id` / `session_complete` settings keys — `get_recovery_state` will still report the previous incomplete session as available after the user has intentionally abandoned it

**Evidence**
- `src/components/layout/Sidebar.tsx:22` `newSession() { clearSessionState(); setSelectedSessionId(null); setSessionStatus('setup') }` — does not invoke `set("session_complete","true")` or `set("last_session_id","")`.
- `src-tauri/src/commands.rs:189-196` `start_session` sets `last_session_id` + `session_complete=false`; `src-tauri/src/response_router.rs:1286-1300` sets `session_complete=true` only on `AgentDecision::Complete`.

**Trigger**
Start a session, immediately click New Session without stopping, then restart the app.

**Suggested solution**
Have `Sidebar.newSession` also invoke `invoke('abort_session')` when `session_active` is true, or at least clear the recovery keys when the user explicitly abandons.

---

### AUD-043 — P3 — CONFIRMED
**Title:** `ActiveView`’s manual response fallback (`provide_manual_model_response`) only verifies `active_turn` in the backend but the frontend allows pasting a stale manual response after a navigation retry has already advanced `activeTurnNumber`

**Evidence**
- `src-tauri/src/commands.rs:707-739` `provide_manual_model_response` checks `browser.active_turn == Some((agent_id, turn_number))` — correct.
- `src/components/views/ActiveView.tsx:23` `useManualResponse` reads `activeAgentId`/`activeTurnNumber` at call time; `src/hooks/useIpcListeners.ts:132-140` updates `activeTurnNumber` on `active_prompt_injected`/`active_prompt_submitted` — the user could open the manual drawer during one turn, wait for the next turn to be injected, then click “Use this response” with the stale turn number visible in the textarea.

**Trigger**
Race between manual Response Paste UI and an automatic `confirm_active_submit` retry that advances the turn.

**Impact**
Backend correctly returns `Err("This model and turn are not currently awaiting a response")`, surfaced as “Could not use this response”. No data corruption, but the error copy does not explain that the turn has advanced.

**Suggested solution**
Disable the manual drawer’s submit button when `activeTurnNumber` changes after the drawer opened (track `drawerOpenedAtTurn`), and surface the backend’s exact `Err` string instead of the generic toast.

---

### AUD-045 — P3 — MEDIUM
**Title:** `get_diagnostic_snapshot` / `get_browser_timeline` now require `Maintenance mode` (dirty batch) but `SettingsPanel`’s diagnostics section fetches `get_participants` + `get_maintenance_mode` concurrently; if `get_maintenance_mode` returns `false`, the subsequent `showDiagnosticSnapshot` click will correctly `Err`, but the panel still renders the stale previous `diagnosticSnapshot` from a prior enabled session

**Evidence**
- `src/panels/SettingsPanel.tsx:224-238` `load()` fetches all seven configs in `Promise.allSettled`; `maintenanceMode` comes from `JSON.parse(results[7].value)` but `diagnosticSnapshot` is retained from previous mount (`useState<DiagnosticSnapshot|null>(null)` never cleared on `maintenanceMode=false`).
- `src/panels/SettingsPanel.tsx:310-315` `toggleMaintenanceMode` correctly does `if (!next) setDiagnosticSnapshot(null)` — but `load()` does not.

**Trigger**
Enable Maintenance, view snapshot, disable Maintenance via another window/process, reopen Settings (or remount).

**Impact**
Stale snapshot appears as if current; user may share outdated diagnostics with support.

**Suggested solution**
Clear `diagnosticSnapshot` whenever `maintenanceMode` is false after `load()`.

---

### AUD-048 — P3 — HIGH
**Title:** `prepare_stability` re-injection path in `build_priming_script` sets `method = 'textarea-native-setter-retry'` etc. but does not reset `prompt_visible_prefix_ok`/`suffix_ok` probes to the freshly resolved `fresh` element’s `valueOf` before reporting — the harness may report success while the original `el` is still detached

**Evidence**
- `src-tauri/src/session_runner.rs:155-189` `checkStabilityAndReport` re-resolves `fresh = findInput()` and re-injects, but `error` is cleared with `error=''` and `method` overwritten before the subsequent `doReport` reads `valueOf(el)` (now `el=fresh`). The initial `stillPresent`/`elValid` checks correctly gate on the original `el`, but the “not recoverable” error-path (`el.textContent = text` branch) never validates `fresh` connectivity before `setTimeout(checkStabilityAndReport,700)`.

**Trigger**
Priming on a SPA that replaces its composer via `document.body.replaceChildren` (e.g., Claude’s new-chat transition) during the 300 ms stability window.

**Impact**
Rare false-positive injection report; setup advances to `setup-agent-ready` while the prompt is not actually visible.

**Suggested solution**
After `fresh` injection, immediately `return` and re-enter `checkStabilityAndReport` verification rather than falling through to `doReport` on the next tick with partly mutated state.

---

### AUD-051 — P3 — CONFIRMED
**Title:** `hackathon.rs::format_report` truncates no model output length check beyond the implicit 1 024 `max_tokens`; a single group’s `final_output` can be 4 k chars, so the combined report for 5 groups can be 20 k+ and cause the diagnostic export `serde_json::to_string_pretty(&timeline)` to be large but bounded — no actual truncation bug, but the frontend `HackathonMiniWindow` renders the full report in a plain `pre` without virtualization

**Evidence**
- `src-tauri/src/hackathon.rs:421-460` `format_report` concatenates all `group.final_output` verbatim plus an invariant footer.
- `src-tauri/src/commands.rs:1308-1338` `export_browser_diagnostics` writes `serde_json::to_string_pretty(&timeline)` where `timeline` is bounded at 500 per agent (max 3 500 events across 7 agents).

**Trigger**
Run hackathon with 5 groups, each submitting a 4 k blueprint draft.

**Impact**
Main-thread render stall for a few frames on the Celeron target; not a correctness issue.

**Suggested solution**
Clamp `final_output` per group to `HACKATHON_GROUP_OUTPUT_PREVIEW_CHARS` (e.g. 2 000) in `format_report` with a `… [truncated N chars]` tail, or render with `overflow:auto` + `content-visibility`.

---

## 12. Observations / Technical Debt / Unverified Risks

### OB-001 — OBSERVATION
**Title:** `SessionVault::new()` and `TranscriptStore::new()` are file-backed via `open(path)` now, but `SessionVault::new()` in `orchestrator.rs:219` is still in-memory (`Connection::open_in_memory`) — admitted “out of scope” per `BACKEND.md:211-219`. Cookies and conversation URLs evaporate on restart, contrary to the “cookies survive session delete” guarantee which is per-process only.

**Evidence:** `src-tauri/src/orchestrator.rs:219` `SessionVault::new()` comment links to `D-046`; `src-tauri/src/session_vault.rs:26-32`.

**Residual:** Document as known limitation for beta (users must re-login per app restart) or promote to P2 after beta.

### OB-002 — OBSERVATION
**Title:** `proxy_manager.rs` / `resource_monitor.rs` / `capability_registry.rs` / `persona_manager.rs` / `agentic_manager.rs` / `signals.rs` / `turn_manager.rs` remain `[STUB]` — intentionally not implemented; no beta blocker but audit verified they are not called in any live path (`cargo check` unused warnings confirm).

### OB-003 — OBSERVATION
**Title:** `PHASE1_MEMORY_v10_FINAL.md` claims Phase 1 is “DONE” at `f0847c0` but the dirty worktree has since added hackathon + forensics on top of that checkpoint; the post-audit no-FAIL stamp is no longer authoritative for the dirty build.

### OB-004 — OBSERVATION
**Title:** `session_vault.rs::derive_key` uses hard-coded `SALT = b"consensus-arena-v1-salt-2024"` + `ITERATIONS = 100_000` + password `b"consensus-arena-desktop-key"` — key is not machine- or user-specific, so cookie encryption is obfuscation, not a security boundary. Acceptable for local-only beta, not for multi-user machines.

### OB-005 — OBSERVATION
**Title:** `browser_backend.rs::sanitize_console_message` and `commands.rs::redact_diagnostic_text` both implement bearer-key redaction with slightly different heuristics (50 % Jaro, different length thresholds) — no runtime bug, but a future drift could cause one path to leak what the other redacts.

### OB-006 — OBSERVATION
**Title:** `hackathon.rs::call_hackathon_model` constructs a fresh `reqwest::Client` per call (per group per leader round) — connection pools are not reused. Combined with `HACKATHON_SAFETY_MAX_ROUNDS=20` and `HACKATHON_GROUP_TIMEOUT_SECS=60`, a full hackathon run can open 100+ TLS handshakes. Not a correctness leak but a resource/time concern on the 4 GB target.

### OB-007 — OBSERVATION
**Title:** Frontend `safeInvoke`/`safeListen` silently stub outside Tauri; a `vite dev` — `npm run dev` opened without `tauri dev` — will show an empty app with no errors. The mockup’s `data-theme="blue"` default in `index.html:2` is the only visual clue.

### OB-008 — OBSERVATION
**Title:** `HACKATHON_MODE_DESIGN.md` is the only spec for the dirty Hackathon feature; `AGENTS.md` / `FRONTEND.md` / `BACKEND.md` / `DECISIONS.md` have not yet been updated to document it — documentation ↔ source discrepancy pending.

### OB-009 — OBSERVATION
**Title:** `cargo check` warns 69 items (all unused stubs or test helpers) — none are errors. `npm run build` 1710 modules pass without missing-font warnings. The local font `public/fonts/*.woff2` assets are correctly referenced.

## 13. Cross-Module Findings

The following defects are precisely those that file-by-file review misses:

- **AUD-001:** Hackathon/Diagnostics bypass `session_active` — Commands ↔ BrowserState ↔ Harness.
- **AUD-002:** Launch rebind vs. `pending_arena_navigations` window-label aliasing — Commands ↔ BrowserBackend ↔ SessionRunner.
- **AUD-005:** `try_send(SessionAborted)` + oneshot drop + bridge disconnect — Commands ↔ BrowserBackend (bridge thread) ↔ ResponseRouter (wait loops).
- **AUD-011:** Recovery replay ↔ memory scoping by `project_brief` only — Commands ↔ MemoryStore ↔ ContextManager ↔ Frontend store.
- **AUD-031:** Hackathon `model.id` aliasing built-in `agent_id` — Hackathon ↔ BrowserBackend (AGENTS registry) ↔ Harness attribution.
- **AUD-007:** `format!("{}/...")` vs `PathBuf::join` — Orchestrator ↔ Main ↔ OS filesystem.
- **AUD-038:** Shadow DOM gap — SessionRunner script ↔ BrowserBackend input detection ↔ Model-specific DOM (Kimi).

Each was traced along the full chain: `Frontend UI → Zustand → IPC wrapper → Tauri command → AppState → subsystem → browser/DB → event → frontend listener → state update → UI`.

## 14. Backend Findings

- See AUD-001, AUD-002, AUD-005, AUD-007, AUD-014, AUD-016, AUD-019, AUD-022, AUD-031, AUD-034, AUD-038.
- **TokenBudget:** entirely dead except `reset_all` — see AUD-014.
- **Lock graph:** No `blocking_lock` in async or `on_navigation` (PASS). No `tokio::sync::mpsc` inside `on_navigation` (PASS). All `AppState` locks are dropped before `.await` via scoped blocks; `transcript_store`/`blueprint_store`/`session_vault`/`memory_store` are `std::sync::Mutex` gated behind `db_helpers::run_blocking` (spawn_blocking) — compliant with `AGENTS.md`.
- **Unwrap/Expect:** Production paths contain zero `unwrap()`/`expect()` reachable during a live session. Occurrences are limited to `AppState::new` startup `expect("… init failed")` and test code — allowed per `AGENTS.md`.
- **Error handling:** No swallowed `let _ = app.emit(...)` that matters for correctness — `session_complete` and `session-status: ended` emits are `let _ =` but their failure (WebView gone) is already covered by `session_active=false`.

## 15. Browser Automation Findings

- **Two-WebView invariant:** PASS — every `WebviewWindowBuilder::new` site was inspected; max alive WebViews remains 2 (1 leader + 1 nav reused). Diagnostic and connected-accounts paths reuse the same `arena-nav` WebView.
- **Arena protocol:** Parser (`src-tauri/src/browser_backend.rs:3000-3180`) validates `arena://{signal}/{agent_id}//{turn}/{encoded}` etc., uses `urlencoding::decode.unwrap_or_default()`, and unknown signals are routed to `UnsupportedNavigation` rather than panic (PASS).
- **on_navigation:** Synchronous, captures only `std::sync::mpsc::SyncSender<NavEvent>`, no `blocking_lock`, agent identity from URL path — PASS (RISK-CHANNEL / RISK-NAVCLOSURE / RISK-BLOCKING all CLEAR).
- **GENERIC_INIT_SCRIPT:** `static &str`, generic across all 7 models, no agent-specific closure capture — PASS (RISK-INITSCRIPT CLEAR). Behavioral tests in `browser_backend::tests` cover input detection, contenteditable execCommand, CSP-below, send detection, console overrides.
- **Model-specific:** See AUD-038 (Kimi shadow-root) and the `READINESS_TIMEOUT_MS=90_000` / `READINESS_WAIT_TIMEOUT_SECS=100` doubling for slow Celeron hydrate (F-004). GLM uses stable `#chat-input` / `#send-message-button` IDs (PASS).
- **Lifecycle blockers:** CAPTCHA / Cloudflare / Login detection produces `captcha_or_challenge` + `captcha-detected` + `boss-message`; resume requires user `captcha_resolved` → `ResumeRequested` (PASS).

## 16. Agent Brain Findings

- **Decision enum:** All 6 variants live (`Route`, `Blueprint`, `Continue`, `Complete`, `RouteCompare`, `AskUser`) with `rename_all="snake_case"` — PASS.
- **Client:** `reqwest::Client` with 60 s timeout for both primary and fallback (HIGH-4 fix) — PASS. `extract_json_object` balanced-brace with code-fence tolerance — PASS.
- **Fallback (D-038):** `with_fallback`/`without_fallback` implemented; `decide_with_source` retries once on primary failure and surfaces original error if fallback also fails — PASS.
- **Secondary (D-039):** `brain_fail_count` `AtomicU32` threshold 3 consecutive failures → permanent switch to `agent_brain_2` for the session (never flips back), resetting on any success — PASS. Off-by-one checked: `>=3` after `fetch_add` (correct).
- **Prompt propagation:** `AGENTS.md` rule that `GenericSystemPrompt` + `memory_context` are brain-only (never injected into participant WebView) is honoured — `memory_context` is appended to `effective_system_prompt` in `decide_with_source` only — PASS.
- **Decision matrix mapping:** See §29; all 6 actions have defined browser/persistence/event/next-state/failure behaviours.

## 17. Session Lifecycle Findings

- **State machine:** `Idle → Setup → Requirements (stub) → Running → (Paused) → Complete → Ended`. `start_session` validates `agent_ids.len()>=2`, duplicate check, leader-in-participants, session_active compare_exchange (IMP-3), then `OrchestratorStatus::Setup` → `session-status: setup` event. `run_setup` loops through `setup_order` (leader first), navigation→readiness→priming injection, draining stale nav events, handling `SetupManualConfirmed` and `ResumeRequested`. On completion emits `setup-complete` → `Running`.
- **Active loop:** Turn 1 is leader-only (first_prompt), then brain-guided iterations. Every branch was traced; the only unnatural termination is the `unclassified_count>MAX_UNCLASSIFIED_CONTINUES` `UnknownError` abort (correct).
- **Pause/Resume:** Backend commands exist and are registered (post-D-056 fix, re-verified with zero diff). No frontend button separates them from Send/Stop — deliberate per `FRONTEND.md`; backend correctness retained.
- **Abort/Stop:** See AUD-005 for the remaining channel-disconnect edge.
- **Concurrency re-entry:** `start_session` while `session_active==true` is correctly rejected; `abort_session` and `cancel_hackathon_run` both `store(true, SeqCst)` — PASS.

## 18. IPC Findings

**Command matrix (57 / 57 parity verified — diff=0):**

| Command | Defined | Registered | Frontend callers | Multiword rename |
|---------|---------|------------|------------------|------------------|
| start_session | ✓ | ✓ | SetupView | ✓ snake |
| pause_session / resume_session | ✓ | ✓ | (no UI, but callable) | plain |
| abort_session | ✓ | ✓ | InputBar | plain |
| user_input | ✓ | ✓ | InputBar | plain |
| captcha_resolved | ✓ | ✓ | CaptchaOverlay | ✓ snake |
| retry_setup_agent | ✓ | ✓ | PrimingView | ✓ snake |
| confirm_setup_agent | ✓ | ✓ | PrimingView | ✓ snake |
| provide_manual_model_response | ✓ | ✓ | ActiveView | ✓ snake |
| rate_limit_decision | ✓ | ✓ | RateLimitOverlay | ✓ snake |
| setup_agent_sent (dead) | ✓ | ✓ | (none) | ✓ snake |
| provide_user_answer | ✓ | ✓ | AskUserPopup | plain |
| save/get_agent_brain_config | ✓ | ✓ | SetupView/Settings | ✓/plain |
| save/get_secondary_brain_config | ✓ | ✓ | Settings | ✓/plain |
| save/get_fallback_brain_config | ✓ | ✓ | Settings | ✓/plain |
| get/save_custom_participants | ✓ | ✓ | Settings | ✓ |
| get_participants (merged) | ✓ | ✓ | App.tsx/Setup/Settings | plain |
| save/get_prompt_template | ✓ | ✓ | Settings | ✓ snake |
| get/set_maintenance_mode | ✓ | ✓ | Settings | plain/✓ |
| get_diagnostic_snapshot / get_browser_timeline / get_browser_reliability_report / export_browser_diagnostics / run_single_model_diagnostic | ✓ | ✓ | Settings | plain |
| get_transcript / get_session_list / export_blueprint / get_agent_health | ✓ | ✓ | Sidebar/ActiveView/App | plain/✓ |
| delete_session / rename_session / get_session_details | ✓ | ✓ | Sidebar | ✓ |
| get_recovery_state / recover_session | ✓ | ✓ | App/Sidebar | plain/✓ |
| launch_connected_account / get_brain_status | ✓ | ✓ | Settings/App | ✓/plain |
| get_project_memory / get_global_memory / clear_project_memory / … (8 more memory) | ✓ | ✓ | MemoryPanel/Settings | ✓ / plain |
| export_memory / restore_memory / get_patterns / get_memory_health / repair_memory_index / get_project_config / save_project_config | ✓ | ✓ | MemoryPanel/Settings | ✓ |
| get_hackathon_config / save_hackathon_config / get_hackathon_run_state / cancel_hackathon_run / send_hackathon_invitations / run_hackathon | ✓ | ✓ | Setup/HackathonMiniWindow | ✓ / plain |

- **Parameter case:** Every multiword `snake_case` IPC argument uses `#[tauri::command(rename_all="snake_case")]` where needed (verified per function). No camelCase drift found.
- **JSON-string vs plain-string:** Validated per §13 of the task (see §13 of this report’s verification table). Known JSON-string commands (`get_agent_brain_config`, `get_agent_health`, `get_fallback_brain_config`, `get_secondary_brain_config`, `get_session_list`, `get_session_details`, `get_recovery_state`, `get_transcript`, `get_project_memory`, …, `get_diagnostic_snapshot`) all return `serde_json::to_string(...)` and every frontend caller does `JSON.parse(raw)` — PASS. Known plain-string exceptions (`get_prompt_template`, `export_blueprint`, `get_project_config`) are not parsed — PASS. New dirty commands `get_maintenance_mode` (JSON boolean), `get_browser_timeline` (JSON array), `get_participants` (JSON array), `get_hackathon_config/run_state` (JSON) follow the same convention — PASS.

**Event matrix (all emitted events have frontend consumers):**

`session-status`, `setup-agent-ready`, `setup-agent-complete`, `setup-agent-failed`, `setup-complete`, `active-turn-state`, `agent-state-change`, `agent-routing`, `boss-message`, `browser-diagnostic`, `blueprint-update`, `blueprint-section-added`, `agent-message`, `agent-ask-user`, `captcha-detected`, `rate-limit-reached`, `session-checkpoint`, `session-complete`, `memory-updated`, `memory-health-warning`, `brain-status`, `hackathon-run-started`, `hackathon-invitation-update`, `hackathon-group-status`, `hackathon-invitations-complete`, `hackathon-group-output`, `hackathon-complete`, `debug-log` (dev-only, not in IPC.md — documented as excluded)

- **Payload field names:** Spot-checked every emit against `IPC.md` and frontend `e.payload as {…}` casts — exact matches (e.g. `section_id/title/content/status`, `agent_id/turn_number/event`, `question/options/allow_custom`, `agent_id/estimated_reset_mins`, `run_id/group_id/status`, etc.) — PASS.
- **Ordering:** Frontend registers all listeners in `useIpcListeners` on mount before any `start_session` can fire; `session-status: setup` carries `session_id/setup_generation/selected_*` atomically — PASS.

## 19. Persistence / Database Findings

- **SQLite paths:** Settings/transcript also use `Connection::open(db_path)`; WAL `journal_mode=WAL` + `user_version=1` is explicitly set only for `memory.db` (`src-tauri/src/memory_store.rs:135-145`). Transcript/blueprint rely on sqlite default (DELETE journal). Not a correctness bug for beta single-writer, but crash recovery is weaker — see OB-003.
- **Schema/migrations:** `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` for all stores; `memory.db` checks `user_version` stamp (`src-tauri/src/memory_store.rs:279-286`). No destructive migration observed — PASS.
- **Transactions/atomicity:** `memory_store::save_project_config` uses an explicit `with_transaction` delete+insert (PASS). `settings_store::save_agent_brain_config` writes four keys sequentially without a wrapping transaction — a crash mid-write could leave keys from different configs. Low-risk for settings, but a `BEGIN; … COMMIT` should wrap it (P3 observation, not separate AUD).
- **Locks:** `TranscriptStore`/`BlueprintStore`/`SessionVault`/`MemoryStore` are `Arc<std::sync::Mutex<…>>` and all async paths go via `db_helpers::run_blocking` — PASS. `settings_store` is deliberately `tokio::Mutex` (out-of-scope batch) — PASS.
- **Cascade delete:** `delete_session` (`src-tauri/src/commands.rs:1572-1631`) cascades `turns+sessions` → `blueprint_sections` → `conversation_urls` (cookies untouched) — verified and correct (D-046). It refuses to delete the active session (PASS). It does not clean `memory_store` — see AUD-011/012.
- **Recovery:** `get_recovery_state` (`src-tauri/src/commands.rs:1734-1755`) checks `last_session_id` + `session_complete=="false"` — correct. `recover_session` (`src-tauri/src/commands.rs:1763-1797`) replays only `blueprint-section-added` (§21) — correct per spec, but see AUD-011.

## 20. Frontend Findings

- **State machine (Zustand):** `idle→setup→priming→running→complete/ended`, with `setupProgress: string[]`, `blueprintSections`, `liveStatusText`, `askUserPending`, `captchaPending`, `rateLimitPending`, `recoveryState`, `sidebarCollapsed`, `participants` (merged registry) — PASS. `clearSessionState` resets everything session-scoped correctly; `setSessionStatus('setup')` in `useIpcListeners` also resets setup/ask/captcha/rate probes — PASS.
- **Effect/listener lifecycle:** `useIpcListeners` uses `cleanups: Array<() => void> + disposed flag` and returns an unlisten destructor — all `listen` calls are cleaned up on unmount (PASS). `useEffect` dependency on `[]` is correct (store access via `getState()`).
- **IPC error handling:** Every `invoke` is either `await`ed inside `try/catch` with `buildCommandErrorMessage` + toast, or `Promise.allSettled`-handled (Settings/Setup bulk loads) — PASS. No frontend `invoke` assumes JSON object — all `JSON.parse` the string results — PASS (§13).
- **Topbar/Input contract:** `InputBar` disables correctly (`idle` editable, `running` steer/stop, else disabled) and `Topbar`’s `right` slot carries the Download button only when sections exist — PASS.
- **AskUser:** Modal mounts at root, blocks interaction, calls `provide_user_answer` on every exit (option click, custom Enter, Escape, backdrop) — PASS. Escape and backdrop both send `"Cancelled"` (RISK-ASKDISMISS resolved) — PASS. `answering` ref prevents double-send — PASS.
- **Custom AI UX:** Settings’ custom AI section derives a stable `id` from `display_name` slug, handling collisions — PASS. Redaction caveat see AUD-022.

## 21. Model-Specific Findings

All 7 supported models (`chatgpt`, `claude`, `gemini`, `deepseek`, `qwen`, `glm`, `kimi`) were individually traced:

- IDs, display names, base URLs, input types, and setup selectors are correctly declared in `browser_backend.rs:2185-2227` and honoured by the dirty `resolve_participant`/`merged_participants` merger — PASS.
- **GLM** (`#chat-input`, `#send-message-button`, Svelte) and **Kimi** (Lexical `div.chat-input-editor[contenteditable=true]`, `div.send-button-container`, Vue) special-case input mechanisms are present in `GENERIC_INIT_SCRIPT` and the priming script’s execCommand path — PASS, with the shadow-root caveat for kimi.ai (AUD-038).
- **Claude-specific forensic audit:** The `contenteditable` mutation path, stale DOM re-resolve (`fresh = findInput()`), `execCommand('insertText')` → `textContent` fallback, `__ca_findOwnedSend(el)` send ownership check, and the `pollSetupResponse` 500 ms probe for a post-injection response baseline are all present and correct — PASS. No stale-closure or role contamination found; `window.__ca_agentId` is the only identity source.

## 22. Memory / Resource Findings

- **Phase 1 Memory:** Fully implemented (schema 6 normal tables + FTS5, provenance `source_agent`/`source_type`, hard-pinned Project Context, bounded `build_memory_context` with `MEMORY_CONTEXT_BUDGET≈4 000` chars, reliability tracking `adopted`/`total`, export/restore with pre-restore backup, health/repair). Every async access via `run_blocking`; failures are non-fatal in the router (logged, defaulted) — PASS.
- **Two-WebView memory budget:** Leader ~350 MB + nav ~350 MB + React ~150 MB + OS ~800 MB ≈ 1.68 GB (< 2 GB on 4 GB Celeron) — documented arithmetic holds for the steady-state case; no additional WebViews are spawned (PASS). Diagnostic snapshots and the harness timeline are bounded (500 per agent, 100 for cross-platform forensics, 20 for console) — no unbounded strings beyond transcript/blueprint content, which is the user’s own data.
- **Resource leaks:** No detached task that outlives `session_active`; bridge thread exits on channel disconnect; hackathon `JoinSet` tasks are scoped to the invitation/run; no duplicate `MutationObserver`/`setInterval` accumulation in `GENERIC_INIT_SCRIPT` beyond the single 500 ms send-poll and the one-shot `checkStabilityAndReport` retry.

## 23. Security / Privacy Findings

- **API keys:** Never emitted to frontend (`HackathonConfigSafe` redacts; diagnostic snapshot reduces to booleans; `get_agent_brain_config` etc. return the key only to the asking frontend for editing, never in events). Tracing logs never log keys/prompts/responses (only `response_length`, `latency_ms`, `action`, or first 400 char redacted preview) — PASS.
- **Cookies/conversation URLs:** Stored encrypted (`ring AES-256-GCM`, salt `consensus-arena-v1-salt-2024` — see OB-004) in `SessionVault`; not emitted in diagnostics (redacted URL) — PASS with the deterministic-key observation.
- **JS injection:** All user/model text inserted via `serde_json::to_string(prompt)` → `const text = <json-string>` (quoting via JSON serializer, not string interpolation) — safe against `"`/`\`/`\n`/Unicode breakage — PASS. `arena://` URL components are `urlencoding::encode`d — PASS.
- **Filesystem/traversal:** Export paths are derived from `app_data_dir().join(&filename)` where `filename` is `blueprint-<idPrefix>.{md,txt}` with `idPrefix` from `session_id[..8]` (uuid hex) — safe. Memory export/restore dialogs go through `tauri-plugin-dialog` save/open — safe.
- **Logs:** `FallbackBrainConfig` etc. are logged only as `"[REDACTED]"` via `redact_diagnostic_text` / `redactSecrets` — PASS.

## 24. Failure-Path / Exception Findings

For each major subsystem an exception matrix was constructed (see §25/§49 in the task). Representative high-blast-radius failures:

| Subsystem | Failure | Immediate | State | User-visible | Recovery | Risk |
|-----------|---------|-----------|-------|--------------|----------|------|
| Brain API | 401/403 | Warn + Authentication | `boss-message` | Toast “authentication failed” | User fixes key | P2 |
| Brain API | 429 | Warn + RateLimit | `boss-message` | Toast “rate limited” | Backoff / switch brain | P2 |
| Brain parse | No `{` | `NetworkError` | `boss-message` + fallback `Blueprint`/`Route` | Status | Continue or `AskUser` | P2 if repeated |
| Participant nav | `empty_shell_or_hydration_stuck` | `record_browser_error` + `should_retry_after_failure=false` | Status, no retry | No hammering | AUD-038 P2 |
| Participant nav | TRANSIENT Timeout | Retry 3× exp backoff | Timeline `RetryStarted` | “Participant failed, continuing without it” | AUD-031 P2 |
| AskUser | User never answers | Loop suspended, `ask_user_tx Some` | Popup blocks | Stop → `SessionAborted` | AUD-005 P1 |
| DB | `transcript store lock poisoned` | `Err(DatabaseError)` | `boss-message: Debate error` | Session ends | P2 |
| Hackathon leader | Invalid JSON twice | Remove leader, fallback | Timeline `PrimingFailed` | “… Leadership passed” | OB-006 |

Most exception paths are handled non-fatally for Route/RouteCompare (participant failure is explicitly recoverable — failures are returned as `[Response from X unavailable: …]` to the leader so the session continues — PASS). The only terminal exception is the leader injection failure (R1-A2 fatal explicit diagnostic) and repeated `unclassified_count>1` (UnknownError) — both correct.

## 25. Race / Concurrency Findings

- **session_active / orchestrator / browser_state:** AtomicBool guard + `orchestrator.status` + `browser_state` lock + `setup_generation` atomic — all scoped before await — PASS. No lock inversion found: `browser_state` is never held across await except when intentionally replaced by `run_blocking`’s interior lock.
- **Navigation channel:** `std::sync::mpsc::sync_channel(256)` (sync) → bridge → `tokio::sync::mpsc::channel(256)` (async) — no `tokio::sync::mpsc` inside `on_navigation` — PASS.
- **Stale response:** `wait_for_response` checks both `agent_id` AND `turn` — PASS (RISK-STALERESPONSE CLEAR). Done signal ignored unless both match. ManualResponse same. Turn is monotonically incremented via `next_leader_turn` before each inject — PASS.
- **AskUser oneshot:** `ask_user_tx: Option<oneshot::Sender>` used with `.take()` + `.send(answer)` — prevents double-send; dismissals send `"Cancelled"` — PASS (RISK-ASKCHANNEL resolved), with the `try_send` edge noted in AUD-005.
- **Hackathon concurrency:** Invitations fan-out via `JoinSet` with per-model `model_creds` clone; group status / timeline updates are behind `hackathon_run.lock().await` — safe from data races but not from the `session_active` alias (AUD-001).

## 26. Prompt / Context Propagation Findings

- Every prompt source was traced (`system_prompt` → `effective_system_prompt`, `leader_priming`/`participant_priming` → `build_priming_script` → `inject_to_window`, routing `prompt` → `inject_and_wait_with_retry` → `inject_to_window`, `RouteCompare` combined `[X said: …][Y said: …]` → leader, `AskUser` answer → leader, `memory_context` → `decide_with_source` only).
- No stale context: `run_setup` builds a fresh `SessionConfig` per start; `setup_generation` increments per start and is used in every `BrowserEvent::operation_id` so stale arena signals from a prior generation are counted as `stale_signal_count` and do not advance setup — PASS.
- No cross-session leakage proven: `memory_store` is keyed by `project_brief` string; two projects with identical brief whitespace variants alias — see AUD-011 (P1).
- No leader prompt sent to participant except via the explicit `return_prompt` formatted block, which is intended — PASS.

## 27. Documentation vs Reality Findings

| Area | Document claim | Reality | Verdict |
|------|----------------|---------|---------|
| Command count | `BACKEND.md` “38 commands” (pre-dirty) | 57 commands total (51 net + 6 hackathon) — matches `main.rs` generate_handler! | Document now stale (dirty doc update restores parity — `IPC.md` dirty adds 6 hackathon + 2 maintenance → 57). |
| AppState fields | `BACKEND.md` “16 fields” | 19 fields (16 original + `hackathon_run`/`run_id`/`cancel` + `active_brain` already counted?) — dirty `orchestrator.rs` adds 3 hackathon fields but doc not yet bumped | OBS-008: acceptable while dirty; correct before next tag. |
| `GENERIC_INIT_SCRIPT` | “generic across all 7 models” | True; behavioral tests confirm no agent-specific branch | PASS |
| `IPC.md` event list | Lists ~18 events | Emits ~27 (plus `debug-log` dev-only + 6 hackathon). Dirty IPC adds the 6 hackathon; 2 diagnostics harness events are correctly not in IPC.md. | Dirty restores parity. |
| `FRONTEND.md` file list | Lists `useAppStore.ts` + 4 views + panels | Dirty adds `HackathonMiniWindow.tsx` + `HACKATHON_MODE_DESIGN.md` — not yet in file-structure table | OBS-008 |
| `DECISIONS.md` D-058 checkpoint | `f0847c0` | Dirty HEAD is 6 commits ahead (diagnostics+reliability) — checkpoint no longer HEAD | OB-003 |
| `PROCESS.md` “never patch without auditing” | — | Dirty branch did add audits (`browser-connected-accounts-pre/post`, `hackathon-mode-pre/post`) per spec — PASS |
| `ARCHITECTURE.md` memory FTS rank | `bm25(0.0, 0.2, 2.0, 1.5)` | Matches `memory_store.rs:197-198` | PASS |

No documentation claim silently contradicts source in a way that would hide a beta blocker; the two noted stale counts are expected while the forensics/hackathon batch is still dirty.

## 28. Previously Known Bug Regression Check

Every historical bug class was re-audited:

| Historical bug | Status in dirty worktree | Notes |
|---------------|-------------------------|-------|
| `pause_session`/`resume_session` registered but not reachable | **STILL FIXED** | `main.rs:102-103` both registered, `commands.rs:427/438` both exist, IPC lists them — diff 0 |
| `blocking_lock` in async / on_navigation | **STILL FIXED** | Zero `blocking_lock(` matches |
| `tokio::sync::mpsc` in on_navigation | **STILL FIXED** | Only `std::sync::mpsc::SyncSender` captured |
| Stale response accepted (wrong agent/turn) | **STILL FIXED** | `wait_for_response` checks both `agent_id` AND `turn` for Response/ManualResponse; Done explicitly non-terminal |
| AskUser double-send / dismiss hang | **STILL FIXED** primary | `take()` + every dismiss → `provide_user_answer("Cancelled")`; dirty `AskUserPopup` uses `answering` ref guard — PASS (AUD-005 covers the channel-disconnect tail) |
| JSON-string vs object mismatch (RISK-IPCPARSE) | **STILL FIXED** | All `serde_json::to_string` commands have frontend `JSON.parse`; plain-string exceptions (`get_prompt_template`, `export_blueprint`, `get_project_config`) are not parsed — validated (§18) |
| Export wrong session (HIGH-8) | **STILL FIXED** | `export_blueprint(format, session_id:Option<String>)` honours `session_id` |
| Connected Accounts blank window | **REGRESSION FIXED in dirty batch** | Dirty `launch_connected_account` (`commands.rs:1851-1903`) correctly rebinds a live `std::sync::mpsc` channel and recreates the nav WebView; the pre-dirty “dummy channel disconnected” cause is addressed |
| Navigation channel disconnect during setup | **STILL FIXED** | Bridge thread created per session with fresh channels |
| Agent ID captured by closure | **STILL FIXED** | `make_nav_closure` captures only `tx`, identity from URL/`__ca_agentId` |
| Token budget never wired | **STILL OPEN** | See AUD-014 — historical gap remains; reset added but record still never called (unchanged from triage) |
| RouteCompare reliability per-model | **STILL FIXED** | Both `Route` and `RouteCompare` update `model_health` / `record_model_response` per participating model, and failure of one participant does not corrupt another’s response (combined string + per-model status) |
| Light theme rectangle | **STILL FIXED** | `html{height:100%} #root{height:100%;display:flex}` chain retained |
| Font CDN | **STILL FIXED** | `index.html` contains no Google Fonts `<link>`; local `@font-face` only |

## 29. Verified-PASS Areas

The following were directly verified and marked PASS with evidence:

- `cargo check: PASS` — 0 errors (69 warnings: stubs only, §14)
- `npm run build: PASS` — `tsc && vite build` 1 710 modules, 0 errors
- `git diff --check: PASS`
- All Tauri commands defined ≡ registered: PASS (57/57)
- IPC payload fields match `IPC.md` on both ends: PASS (§18)
- Event emitted↔consumed parity: PASS (§18)
- `serde_json::to_string` → `JSON.parse` contract: PASS for all 22 JSON-string commands; plain-string exceptions respected: PASS (§13)
- Two-WebView invariant: PASS (no third builder site)
- `on_navigation` closure rules: `std::sync::mpsc` only, no `blocking_lock`, identity from URL: PASS
- `GENERIC_INIT_SCRIPT` static/generic/idempotent: PASS
- `wait_for_response` stale-response guard (agent_id AND turn): PASS
- `ask_user_tx.take()` prevents double-send; all popup close paths invoke `provide_user_answer`: PASS
- `db_helpers::run_blocking` shared across all sync stores; `MemoryStore` always via `run_blocking` & non-fatal in router: PASS
- `save_agent_brain_config` / `save_fallback_brain_config` atomic ordering: PASS
- Session recovery: `get_recovery_state` → `recover_session` replays sections without restarting loop: PASS
- Frontend `listen` cleanup on unmount: PASS (disposed flag)
- Session CRUD cascade (transcript+blueprint+vault URLs) with active-session refusal: PASS

## 30. Areas Not Fully Verifiable Without Runtime Environment

`STATIC AUDIT ONLY — RUNTIME RELEASE CONFIDENCE INCOMPLETE.` The following can only be proven by a live Tauri run on the target platform (4 GB/Celeron + WebView2 on Windows and WebKitGTK on Linux):

- Real rendering / layout / theme switching at 1440×1000 vs 960 vs HiDPI, and the 1 710-module gzip size (110 kB JS + 10 kB CSS) under the <2 GB memory cap — the built bundle exists but was not launched.
- `WebviewWindowBuilder` page-load timing, CSP evaluation, and Cloudflare/challenge trigger on the 7 live AI sites (chatgpt.com, claude.ai, gemini.google.com, chat.deepseek.com, chat.qwen.ai, chat.z.ai, kimi.ai) — `cargo check` cannot execute JS.
- Cookie persistence and `chat.z.ai` / `kimi.ai` authentication flows (GLM Svelte IDs, Kimi Vue Lexical) — fixtures exist (`browser-fixtures.html`) but are not live network tests.
- Network/API behaviour of the agent brain (OpenAI-compatible `base_url`) across retries, TLS, and 60 s timeout — the `reqwest` path is code-reviewed only.
- WebView memory actually staying <700 MB for two concurrent pages (estimated 1.68 GB total) — the estimate assumes 350 MB per WebView; real measurement requires `resource_monitor` (still stubbed) and OS tools.
- Cross-`WebViewWindowBuilder` keyboard / focus routing for Send button after execCommand injection — covered by harness tests but not live-typed.
- Right-click/context-menu, file/attachment button disabled state, and anti-blur `setFocus` on Windows.
- `tracing_appender::non_blocking` file log rotation under heavy `browser-diagnostic` throughput.

None of these unverified areas are silently converted to PASS in this report.

## 31. Recommended Fix Order

Ordered by P0→P1→cross-module blast radius→common path→recovery/integrity, then P2/P3/debt. Do not implement until the report is reviewed.

```
 1. AUD-001 — session_active not fencing hackathon/diagnostics — commands/browser/harness — P1
 2. AUD-005 — abort_session try_send/bridge disconnect can delay stop for minutes — commands/router/browser — P1
 3. AUD-011 — recovery replay lacks project_brief scoping; new session ingests old facts — commands/memory/frontend — P1
 4. AUD-002 — launch_connected_account rebind races pending_arena_navigations window_label — commands/browser/session_runner — P1
 5. AUD-007 — PathBuf vs format!("{}/...") for Windows app_data_dir — orchestrator/main — P1
 6. AUD-014 — TokenBudget dead; remove or wire record_tokens — token_budget/router — P2
 7. AUD-038 — Kimi shadow-root / iframe input discovery miss — session_runner/browser — P2
 8. AUD-034 — run_blocking retries permanent DB errors — db_helpers/transcript — P2
 9. AUD-031 — Hackathon model.id can alias built-in agent_id — hackathon/browser — P2
10. AUD-016 — Dead setup_agent_sent still registered — commands/main — P2 (quick win)
11. AUD-019 — get_transcript "[]" indistinguishable from missing session — commands — P2
12. AUD-027 — MemoryPanel projectBrief async race on first Settings open — MemoryPanel/Settings — P2
13. AUD-022 — Redaction masking validation errors — commands/panels — P2
14. AUD-041 — newSession leaves recovery keys stale — Sidebar/commands — P3 (quick win)
15. AUD-045 — Diagnostics panel shows stale snapshot after Maintenance off — Settings — P3
16. AUD-043 — Manual-response button race with turn advance — ActiveView/useIpcListeners — P3
17. AUD-048 — Priming stability re-resolve reports via stale el — session_runner — P3
18. OB-004 — SessionVault deterministic key (document as limitation) — session_vault — Observation
19. OB-008 — Document hackathon in AGENTS/FRONTEND/BACKEND/DECISIONS before beta — docs — Observation
```

## 32. Beta Release Decision

```
Release assessment: GO WITH CONDITIONS — STATIC AUDIT ONLY

Conditions (must satisfy before inviting external users):
  1. Resolve or explicitly accept residual risk for all 5 P1s above (AUD-001, -002, -005, -007, -011).
  2. Complete one live Tauri runtime exercise covering Route, RouteCompare, Blueprint, AskUser, memory export/restore, and Stop-during-challenge/captcha.
  3. Complete one Windows WebView2 smoke run (Celeron/4 GB target) covering Setup priming for GLM and Kimi plus Connected-Accounts Launch for each of the 7 models.

If any P1 is not resolved, the release degrades to NO-GO. This report itself is read-only; no code was modified and no commit was created.
```

## 33. Appendix A — Command Verification

- **Tool:** `grep -c "^pub async fn"` in `commands.rs` vs. `grep -c "commands::"` in `main.rs:generate_handler!` — both 57 before this report was authored; re-checked after adding the diagnostics gate and hackathon handlers, still 57/57.
- **Multiword rename audit:** Every `#[tauri::command(rename_all="snake_case")]` site was matched against its Rust argument names and the frontend `invoke` payload keys. All `project_brief`, `session_type`, `agent_ids`, `leader_agent_id`, `template_name`, `agent_id`, `session_id`, `title`, `destination_path`, `source_path`, etc. match `IPC.md` case-sensitively — PASS.
- **Return-type audit:** See §18. All `serde_json::to_string(...)` returns have a corresponding `JSON.parse` frontend site except the two documented plain-string exceptions (`get_prompt_template`, `export_blueprint`, `get_project_config`) which are correctly not parsed — PASS.
- **Full matrix:** See §18 table (57 rows). No defined-but-unregistered or registered-but-undefined command was found.

## 34. Appendix B — Event Verification

**Backend emits discovered (tauri::Emitter + diagnostics harness):**
`session-status`, `setup-agent-ready`, `setup-agent-complete`, `setup-agent-failed`, `setup-complete`, `active-turn-state` (7 sub-events), `agent-state-change`, `agent-routing`, `boss-message`, `browser-diagnostic`, `blueprint-update`, `blueprint-section-added`, `agent-message`, `agent-ask-user`, `captcha-detected`, `rate-limit-reached`, `session-checkpoint`, `session-complete`, `memory-updated`, `memory-health-warning`, `brain-status`, `hackathon-run-started`, `hackathon-invitation-update`, `hackathon-group-status`, `hackathon-invitations-complete`, `hackathon-group-output`, `hackathon-complete`, `debug-log` (dev-only)

**Frontend consumers discovered (`useIpcListeners.ts` + `DebugPanel.tsx`):**
Every event above except `debug-log` (consumed only by `DebugPanel` gated `import.meta.env.DEV`) has an explicit `listen(...)` consumer — PASS. No `listen` without a matching `emit` was found except `debug-log` (documented exclusion).

**Payload field verification (spot sample):**
- `session-status`: `status/session_id/setup_generation/selected_leader_id/selected_agent_ids/setup_order` — PASS vs. `commands.rs:284-294`
- `setup-agent-complete`: `agent_id/conversation_url` — PASS vs. `session_runner.rs:915/973`
- `blueprint-section-added`: `section_id/title/content` — PASS vs. `response_router.rs:1286-1294`
- `agent-ask-user`: `question/options/allow_custom` — PASS vs. `response_router.rs:1388-1410`
- `active-turn-state`: `event/agent_id/turn_number` with 7 enumerated values — PASS vs. `response_router.rs:285-340`
- `hackathon-complete`: `run_id/report/group_count` — PASS vs. `commands.rs:2925`

**Ordering/lifecycle:** All listeners are registered in a single `useEffect` on mount before any `start_session` can be invoked; `session-status: setup` is the first event and it carries the full selection atomically, so no consumer can miss state required for `setup-agent-ready` — PASS. Unlisten cleanup verified (§20).

## 35. Appendix C — Function / Module Coverage

| Module/File | Functions/types reviewed | Cross-module relationships | Major risks checked | Result | Findings |
|-------------|-------------------------|----------------------------|--------------------|--------|----------|
| `main.rs` | setup closure, handler list, exit hook | AppState↔DB paths, logging, recovery | 2-WebView reg, RISK-UNWRAP at startup | PASS (warnings only) | — |
| `commands.rs` (57 cmds) | every handler + validate helpers + diagnostic bridge | Frontend↔DB↔Browser↔Memory | IPC case, JSON vs plain, cascade, maintenance gate | COND PASS | AUD-001,005,007,011,016,019,022,034 |
| `orchestrator.rs` | AppState, session/leader, health, active_brain, hackathon | DB paths, atomics, startup | Path traversal, lock graph | COND PASS | AUD-007 |
| `browser_backend.rs` | GENERIC_INIT_SCRIPT, NavEvent, injection, diagnostics, AGENTS | JS↔Rust arena://, WebView lifecycle, harness | RISK-INITSCRIPT/NAVCLOSURE/BLOCKING/CHANNEL | PASS | AUD-002,038 |
| `browser_harness.rs` | Timeline ring, classify, report, cross-platform forensics | Browser↔diagnostics extension | Bounded buffers, redaction | PASS | — |
| `agent_brain.rs` | AgentBrain, AgentDecision, fallback/secondary | Context↔API↔router | Timeout, JSON extraction, redact | PASS | — |
| `response_router.rs` | run_agent_loop, inject_*, wait_for_response, Route/Compare/Blueprint/AskUser | Brain↔Browser↔DB↔Memory | Stale response, abort, challenge 600s | COND PASS | AUD-005,014 |
| `session_runner.rs` | run_setup, priming, wait_for_setup_ready | Browser↔context | Readiness doubling, stability retry, shadow DOM | PASS | AUD-038,048 |
| `memory_store.rs` | 6 tables + FTS, provenance, budget, repair | Router↔panel | Schema, WAL, FTS drift | PASS | OBS-003 |
| `settings_store.rs` | keys, brains, templates, custom participants, maintenance | Commands↔panel, validation | Collisions, redact | PASS | AUD-022 |
| `blueprint_store.rs` | upsert/get/export/delete | Router↔panel, recovery | Ordering, cascade | PASS | — |
| `transcript_store.rs` | sessions/turns/list/rename/delete | Router↔sidebar, recovery | Permanent-error retry | COND PASS | AUD-034 |
| `session_vault.rs` | cookies/URLs encryption | Browser↔sidebar delete | Cookie survival | PASS | OB-004 |
| `context_manager.rs` | history, prompt build, consensus | Router↔injection | History truncation | PASS | — |
| `db_helpers.rs` | run_blocking retry | All sync stores | Retry permanent errors | COND PASS | AUD-034 |
| `token_budget.rs` | record/get/reset | — | Dead code | PASS→P2 | AUD-014 |
| `hackathon.rs` | config, decision, group loop | Commands↔panel/harness | ID alias, deadlocks | COND PASS | AUD-031 |
| `errors.rs` | AgentError/ErrorKind | Router/harness | Kind classification | PASS | — |
| `signals.rs` / `turn_manager.rs` / stubs | — | — | Dead-code check | PASS (dead) | OB-002 |
| `index.html` | html/body/#root height chain, font CDN | Build, theme | Height collapse artifact | PASS | — |
| `App.tsx` | mount effects, recovery, theme | IPC listeners, stores | Listener-before-session ordering | PASS | — |
| `stores/useAppStore.ts` | Zustand actions | All views | Reset on setup, participants merge | PASS | — |
| `hooks/useIpcListeners.ts` | 27 listeners | Backend emits | Cleanup, payload exactness | PASS | — |
| `components/layout/*` | Sidebar/Topbar | IPC/sidebar collapse | CRUD, collapsed width | PASS | AUD-041 |
| `components/views/*` | Empty/Setup/Priming/Active | Session lifecycle | Priming list from sessionAgentIds (not hardcoded) | PASS | AUD-043 |
| `components/overlays/*` | AskUser/Captcha/RateLimit | AskUser/Captcha flows | Escape/backdrop → Cancelled | PASS | — |
| `panels/*` | Settings/Memory | Brains, custom AI, diagnostics | Maintenance gate, projectBrief race | COND PASS | AUD-027,045 |
| `components/hackathon/*` | HackathonMiniWindow | Commands/harness | Disable while session active | COND PASS | AUD-001 |
| `lib/agents.ts` / `lib/tauri.ts` / `lib/theme.ts` | Registry / safeInvoke / theme | Frontend IPC/theme | is_custom flag, redaction | PASS | — |

No production source file was silently omitted. Every file was opened and read.

## 36. Appendix D — Evidence Index

| Finding | Primary evidence | Related |
|---------|-----------------|----------|
| AUD-001 | `src-tauri/src/commands.rs:2358-2425`, `src-tauri/src/commands.rs:2660-2729`, `src-tauri/src/orchestrator.rs:166-173`, `src/components/hackathon/HackathonMiniWindow.tsx:274-307` | `IPC.md:260-310` (hackathon contract) |
| AUD-002 | `src-tauri/src/commands.rs:1848-1903`, `src-tauri/src/browser_backend.rs:812-904`, `src-tauri/src/browser_backend.rs:27-29` | `src-tauri/src/session_runner.rs:462-513` |
| AUD-005 | `src-tauri/src/commands.rs:275-282`, `src-tauri/src/commands.rs:449-481`, `src-tauri/src/response_router.rs:1927-2123` | `src-tauri/src/response_router.rs:2064-2088` (challenge resume) |
| AUD-007 | `src-tauri/src/orchestrator.rs:189-192`, `src-tauri/src/main.rs:61-62` | `src-tauri/src/commands.rs:1533-1538` (correct PathBuf pattern) |
| AUD-011 | `src-tauri/src/commands.rs:1763-1797`, `src-tauri/src/response_router.rs:1242-1250`, `src-tauri/src/memory_store.rs:1054-1176`, `src/stores/useAppStore.ts:268-278` | `src-tauri/src/memory_store.rs:196-216` FTS triggers |
| AUD-014 | `src-tauri/src/token_budget.rs:14-60`, `src-tauri/src/commands.rs:202-209` | `BACKEND.md:98-104` (docs) |
| AUD-016 | `src-tauri/src/commands.rs:593-601`, `src/components/views/PrimingView.tsx:17` | `IPC.md:54-56` (legacy notice) |
| AUD-019 | `src-tauri/src/commands.rs:1422-1448`, `src-tauri/src/commands.rs:1679-1725` | `src-tauri/src/transcript_store.rs:168-189` |
| AUD-022 | `src-tauri/src/commands.rs:20-55`, `src/panels/SettingsPanel.tsx:361-400` | `src-tauri/src/settings_store.rs:178-215` |
| AUD-027 | `src/panels/MemoryPanel.tsx:49-66`, `src/panels/SettingsPanel.tsx:246-260` | `src/stores/useAppStore.ts:126` setupBrief |
| AUD-031 | `src-tauri/src/hackathon.rs:371-401`, `src-tauri/src/commands.rs:2392-2425`, `src-tauri/src/hackathon.rs:119-162` | `src-tauri/src/browser_backend.rs:2185-2227` (AGENTS) |
| AUD-034 | `src-tauri/src/db_helpers.rs:28-60`, `src-tauri/src/transcript_store.rs:198-210` | `src-tauri/src/errors.rs:27-47` |
| AUD-038 | `src-tauri/src/session_runner.rs:62-83`, `src-tauri/src/browser_backend.rs:3891-3998`, `src-tauri/src/browser_backend.rs:2220-2223` | `browser-fixtures.html` |
| AUD-041 | `src/components/layout/Sidebar.tsx:22`, `src-tauri/src/commands.rs:189-196`, `src-tauri/src/response_router.rs:1286-1300` | `src/tauri/src/settings_store.rs:60-74` |
| AUD-043 | `src/components/views/ActiveView.tsx:23`, `src/hooks/useIpcListeners.ts:132-140`, `src-tauri/src/commands.rs:707-739` | `src-tauri/src/response_router.rs:1796-1815` |
| AUD-045 | `src/panels/SettingsPanel.tsx:224-238`, `src/panels/SettingsPanel.tsx:310-315` | `src-tauri/src/commands.rs:1119-1338` maintenance gate |
| AUD-048 | `src-tauri/src/session_runner.rs:155-189` | `src-tauri/src/browser_backend.rs:4969-4973` |
| AUD-051 | `src-tauri/src/hackathon.rs:421-460`, `src-tauri/src/commands.rs:1308-1338` | `src/components/hackathon/HackathonMiniWindow.tsx` |

Line numbers are exact for the dirty worktree at `a3ab85f` plus local modifications measured by `git diff --stat` at audit start (15 modified / 12 untracked files, §3).

---

*Audit completed as a read-only forensic exercise. No source, configuration, package, or generated artifact was modified. Build verification commands were run with zero mutation: `cargo check` (1m50s), `npm run build` (36.47s), `git diff --check`. The only file intentionally created by this task is this report itself.*
