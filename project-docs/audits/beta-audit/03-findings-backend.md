# 03 — Backend Findings (Phase 1 + Session 2 Maximum-Depth Update)

Resolves the 5 mandatory items, the 59-command tracking table, and all 1.11-1.13 undocumented subsystems. Every finding uses the mandatory format. Session 2 re-verified every number.

---

## Arithmetic Correction (prerequisite)

**Session 1's ~17,563 total was arithmetically inconsistent with its own per-file listing (which summed to 19,913); re-verified against real wc -l output in session 2: 19,913 (19,916 with build.rs 3 lines).** All depth-budget decisions below use 19,913. See `00-file-inventory.md` correction.

AppState 21 fields and 59/59 command registration gap both re-verified by direct grep in session 2 (orchestrator.rs:121-173, commands.rs 59 `#[tauri::command]`, main.rs 59 `commands::`) — zero mismatch, D-056 fix holds.

---

## Mandatory Resolution #1 — agent_brain::decide() dead-code contradiction

### [INFO] [DEAD-CODE] agent_brain::decide() wrapper dead, core alive via decide_with_source

**Location:** src-tauri/src/agent_brain.rs:158-167 (decide), 169-235 (decide_with_source), src-tauri/src/response_router.rs:633,640,650
**Category:** DEAD-CODE
**Severity:** INFO

**Evidence:**
```rust
// agent_brain.rs:158
pub async fn decide(&self, leader_response: &str, context: &str, memory_context: Option<&str>) -> Result<AgentDecision, AgentError> {
    self.decide_with_source(leader_response, context, memory_context).await.map(|(decision, _source)| decision)
}
// agent_brain.rs:169
pub async fn decide_with_source(&self, ...) -> Result<(AgentDecision, BrainSource), AgentError> { ... primary fallback logic ... }
```
Cargo says `method decide is never used` at 158:18. Grep:
```
grep -rn "\.decide\(" src-tauri/src --include="*.rs"
→ only 158 definition + internal self.decide_with_source call
grep -rn "decide_with_source" src-tauri/src
→ response_router.rs:633,640,650 are the THREE real call sites
```

**Why this is a problem:**
If only the wrapper name is grepped, it looks catastrophic ("brain never decides"). In reality the loop calls `decide_with_source` directly because it needs the `BrainSource` for `brain-status` telemetry (primary vs fallback vs secondary). The wrapper exists for callers that don't need the source discriminator; no current caller uses it. Cargo is correctly flagging the wrapper as dead, not the brain's core logic. No functional bug.

**Suggested fix:**
Keep `decide_with_source` as the canonical entry. Either delete the `decide` wrapper or mark `#[allow(dead_code)] // wrapper for external crates/tests, core path uses decide_with_source` so baseline is quiet. Do not let this single warning obscure the prior "catastrophic" misread.

**Confidence:** HIGH

---

## Mandatory Resolution #2 — context_manager real prompt path

### [MEDIUM] [LOGIC] context_manager history loop orphaned — replaced by browser-window + minimal context string, not by equivalent

**Location:** src-tauri/src/context_manager.rs:50-148 (dead), src-tauri/src/commands.rs:223 (only reset), src-tauri/src/response_router.rs:598-605,798-964 (real path)
**Category:** LOGIC
**Severity:** MEDIUM

**Evidence:**
Greps:
```
grep -rn "ContextManager|build_prompt_for_agent|add_turn" src-tauri/src/response_router.rs → 0
grep -rn "ContextManager|build_prompt_for_agent|add_turn" src-tauri/src/session_runner.rs → 0
grep -rn "context_manager" src-tauri/src --include="*.rs" → only orchestrator.rs field, commands.rs:223 `*ctx = ContextManager::new(project_brief, stype)`, settings_store import
```
Real path used instead:
```rust
// response_router.rs:598
let context = format!("Session iteration: {}\nSelected participant IDs: {}\nLeader ID: {}\nDeepSeek selected: {}...", iteration, config.agent_ids.join(", "), leader_id, deepseek_selected, ...);
let _ = app.emit("agent_brain_decision_started", ...);
// then brain.decide_with_source(&leader_response, &context, mem_ctx)
// response_router.rs:825-912 participant prompt is exactly `&prompt` from AgentDecision::Route{prompt}
// response_router.rs:916-964 return_prompt = format!("[Response from {}]:\n{}\n\nIncorporate...", target_model, participant_response) → leader window
```
`ContextManager::build_prompt_for_agent` would have produced a 60k-truncated, charter+history+question prompt. `add_turn`/`is_consensus_reached`/`detect_consensus_signal` are never called.

**Why this is a problem:**
Two possibilities: (a) "orphaned but replaced by equivalent" vs (b) "orphaned and nothing replaced it, so capability silently lost." This is (b) for history fidelity: participant models receive ONLY the single routing prompt (e.g., "Review for risks...") with zero conversation history, while the leader's browser window implicitly carries history via the chat DOM. Participants therefore operate with materially less context than ContextManager intended. `is_consensus_reached` (Agrees across all agent_ids on last iteration) and `is_improvement_velocity_low` are also unreachable, so consensus tracking is purely brain-driven now. Not immediately ship-blocking (the product shipped this way), but it changes the intended consultation quality.

**Suggested fix:**
For beta, document ContextManager as legacy/deferred and suppress dead_code warnings. Post-beta, either re-wire: after each `agent-message` capture call `context_manager.lock().await.add_turn(record)` and pass `build_prompt_for_agent` output as part of `prompt` for participants, or explicitly delete ContextManager and rely on browser-window history, making the narrower context intentional and documented.

**Confidence:** HIGH

---

## Mandatory Resolution #3 — blueprint-update / session-checkpoint dead events

### [HIGH] [CONTRACT] blueprint-update and session-checkpoint are genuinely dead — frontend listens forever, backend never emits

**Location:** src/hooks/useIpcListeners.ts:219 (blueprint-update), 257 (session-checkpoint) vs src-tauri/src/*.rs emits; src-tauri/src/response_router.rs:1242,1250 (only blueprint-section-added/blueprint_emitted); src-tauri/src/browser_backend.rs:362-451 (emit_timeline → timeline.record only, not Tauri emit); grep `emit("blueprint-update"|"session-checkpoint")` → 0
**Category:** CONTRACT
**Severity:** HIGH

**Evidence:**
```
grep -rn "blueprint-update|session-checkpoint" src-tauri/src --include="*.rs" → 0
grep -rn "emit.*blueprint|emit.*checkpoint" src-tauri/src --include="*.rs" -n
→ only emit("blueprint-section-added") 1242, emit("blueprint_emitted") 1250, emit("route_started") 807
```
Harness helpers `emit_timeline`/`emit_harness_event` (browser_backend.rs:362,441, browser_harness.rs record) take `EventType` enum (finite, mapped to strings like "window_created", "navigation_started", "automation_error") — they cannot emit arbitrary IPC event strings; checked enum variants 170-252 contain no "blueprint-update" or "session-checkpoint". Sub-agent confirmed second half emits list: `memory-updated, blueprint-section-added, blueprint_emitted, boss-message, agent-ask-user, session-complete, rate-limit-reached, active-turn-state, agent-message` — no dead pair.

**Why this is a problem:**
Frontend upsert for draft→agreed (`blueprint-update` payload section_id,title,content,status) will never fire; only final Agreed sections via `blueprint-section-added` are visible. `session-checkpoint` toast "Progress saved" will never show. IPC.md Wiring Rule 7 (main content via blueprint-update) is stale. Beta still works via the -added path, but any future draft-flow will silently appear broken.

**Suggested fix:**
Either emit `blueprint-update` on draft upserts (blueprint_store.upsert_section in Blueprint arm already writes status=Agreed only — if draft status is intentional future, add emit there) and emit `session-checkpoint` after blueprint/memory writes, or remove the two `listen()` calls and update IPC.md to mark them as `FUTURE`/`DEPRECATED` so a grep audit doesn't flag them as dead.

**Confidence:** HIGH

---

## Mandatory Resolution #4 — brain-status and browser-diagnostic line-number spot-check

### [INFO] [DOC-MISMATCH] brain-status documented gap + browser-diagnostic correctly push (not harness) — session 1 claims verified

**Location:** src-tauri/src/response_router.rs:460,675,690 (brain-status 3 emits), src-tauri/src/browser_backend.rs:1121-1139 (browser-diagnostic push)
**Category:** DOC-MISMATCH/CONTRACT
**Severity:** INFO

**Evidence:**
```rust
// response_router.rs:460
let _ = app.emit("brain-status", json!({ "active": "unknown", "model": "" }));
// 675
let _ = app.emit("brain-status", json!({ "active": kind_str, "model": model, "iteration": iteration }));
// 690
let _ = app.emit("brain-status", json!({ "active": "unavailable", ... }));
```
Frontend `useIpcListeners.ts:279 listen('brain-status', e => store.setActiveBrain({kind: active, model}))` matches. IPC.md lacks a dedicated brain-status section.

```rust
// browser_backend.rs:1121
fn emit_browser_diagnostic(app: &AppHandle, record: &BrowserDiagnosticRecord, message: &str) {
    if let Err(error) = app.emit("browser-diagnostic", BrowserDiagnosticPayload { agent_id, window_label, phase, url, message, error }) { ... }
}
```
Called via `record_browser_error` (1141) etc., not via harness `emit_timeline`. Harness `emit_timeline`/`emit_harness_event` record to ring buffer (browser_harness.rs) and do NOT emit Tauri events (`timeline.record(event)` only). Sub-agent verified `emit("browser-diagnostic")` count = 1 site (1127), frontend listen 159 exists, correctly push-based. Naming confusion noted.

**Why this is a problem:**
Doc-only: IPC.md should add brain-status section; harness helper should be renamed `record_timeline` to avoid grep false negatives.

**Suggested fix:**
Add IPC.md section for brain-status `{ active, model, iteration }`; add doc comment on browser_backend helpers clarifying push vs record paths.

**Confidence:** HIGH

---

## Findings carried from Session 1 (still valid, re-confirmed)

### [INFO] [DEAD-CODE] OrchestratorStatus dead variants

**Location:** src-tauri/src/orchestrator.rs:22-31
**Category:** DEAD-CODE
**Severity:** INFO
**Evidence:** Cargo warns Preparing/Requirements/Complete never constructed; grep `OrchestratorStatus::Complete` → 0. Frontend sessionStatus includes `requirements` but never occurs.
**Fix:** `#[allow(dead_code)]` + `// reserved for future phases` or delete. **Confidence:** HIGH

### [MEDIUM] [DEAD-CODE] TokenBudget recording never wired

**Location:** src-tauri/src/token_budget.rs:14, src-tauri/src/response_router.rs:351-403 (loop never calls it), src-tauri/src/commands.rs:202 reset_all
**Category:** DEAD-CODE/RESOURCE
**Severity:** MEDIUM
**Evidence:** File comment 46-51 self-documents dormancy; cargo 6 methods dead; 70%/90% thresholds unreachable.
**Fix:** Wire `record_tokens(text.len()/4)` after response capture or suppress + defer. **Confidence:** HIGH

### [LOW] [LOGIC] session_runner unreachable pattern

**Location:** src-tauri/src/session_runner.rs:876 `(Some(_), _) => unreachable!()` after 3 exhaustive Some arms
**Category:** LOGIC
**Severity:** LOW
**Evidence:** Cargo warns; if SetupCompletionProof adds variant, panic.
**Fix:** Generic handler not panic. **Confidence:** HIGH

### [LOW] [LOGIC] browser_backend ignored window_label

**Location:** src-tauri/src/browser_backend.rs:1465,2409,2414
**Category:** LOGIC
**Severity:** LOW
**Evidence:** Cargo warns unused variable/field; forensics weakens attribution.
**Fix:** Include in diagnostic or annotate `_`. **Confidence:** MEDIUM

### [INFO] [DEAD-CODE] STUB modules entirely unused (6 files)

**Location:** agentic_manager.rs, capability_registry.rs, persona_manager.rs, proxy_manager.rs, resource_monitor.rs, signals.rs, turn_manager.rs (40-66 lines zero callers, cargo 20+ warnings)
**Category:** DEAD-CODE
**Severity:** INFO
**Evidence:** grep for each module name across src → 0 callers.
**Fix:** `#[allow(dead_code)]` + STUB header. **Confidence:** HIGH

---

## Session 2 Deep Findings — response_router.rs second half (1153-2453)

### [HIGH] [CONTRACT] Dead IPC: blueprint-update/session-checkpoint confirmed (see Mandatory #3 above)

Already recorded. Second half confirms no emit; additional harmless diagnostic emits without listeners: `route_started`, `blueprint_emitted`, `agent_brain_decision_started/failed/fallback` (6 sites, diagnostic-only, by design). Frontend intentionally ignores them for blueprint-first UI.

### [MEDIUM] [CONCURRENCY] browser_state lock held across update_model_health await

**Location:** src-tauri/src/response_router.rs:1656-1663 (inject_and_wait_with_retry cooldown check)
**Category:** CONCURRENCY
**Severity:** MEDIUM
**Evidence:**
```rust
let browser = state.browser_state.lock().await; // 1656
if browser.is_in_cooldown(target_model) {
    let e = AgentError::NetworkError(...);
    update_model_health(state, target_model, false, Some(e.to_string())).await; // await while holding browser
    return Err(e);
}
```
`update_model_health` at 1909 awaits `model_health.lock().await`; holding browser_state across it inverts lock order. All other 20 locks correctly scoped as `{ let browser = ...; clone } // drop before await` (1257,1311,1456).
**Why:** Extends contention; 600s challenge path blocked on model_health lock.
**Fix:** Scope: `{ let is_cool = state.browser_state.lock().await.is_in_cooldown(...); if is_cool { update_...await } }`
**Confidence:** HIGH (sub-agent, verified)

### [LOW] [LOGIC] brain_fail_count toast misleading when secondary is None

**Location:** src-tauri/src/response_router.rs:611-648, src-tauri/src/orchestrator.rs:151
**Category:** LOGIC
**Severity:** LOW
**Evidence:** `if !use_secondary && load>=3 { use_secondary=true; emit "switching to secondary" }` then `if use_secondary { if Some(b2) { use b2 } else { drop(guard); // fallback to primary } }` — emits secondary switch even when agent_brain_2 is None, then falls back to primary every iteration with wasted lock.
**Fix:** Only emit/set if `is_some()`, else "secondary not configured".
**Confidence:** HIGH

### [MEDIUM] [LOGIC] RateLimit ErrorKind unreachable — cooldown dead

**Location:** src-tauri/src/errors.rs:31-44, src-tauri/src/response_router.rs:62-91,1761,1876
**Category:** LOGIC
**Severity:** MEDIUM
**Evidence:** `errors.rs::kind()` returns Transient for Timeout/Injection/Navigation/Network, Permanent for Captcha/Database/etc., **never RateLimit**. `if kind==RateLimit { browser.set_cooldown(60) }` at 1761 unreachable. Harness categorizes 429 but errors.rs doesn't.
**Why:** Rate-limited participants never enter cooldown via this path; hammering continues despite `rate-limit-reached` emit 1615 via separate cooldown-check path only.
**Fix:** Map 429 to RateLimit variant or check string directly before kind().
**Confidence:** HIGH

### [INFO] [CONCURRENCY] agent_brain_2 guard held across 60s HTTP await by design (noted)

**Location:** src-tauri/src/response_router.rs:630-633 `let guard = state.agent_brain_2.lock().await; if let Some(b2) = guard.as_ref() { b2.decide_with_source(...).await }`
**Category:** CONCURRENCY
**Severity:** INFO (design tradeoff)
**Evidence:** Comment says acceptable for tokio MutexGuard (Send) but blocks callers for full BRAIN_HTTP_TIMEOUT_SECS=60.
**Fix:** Clone secondary brain out before await (like DEF-001 for primary): `let b2 = { guard.as_ref().cloned() }; drop(guard);` then call.
**Confidence:** MEDIUM

---

## Browser Backend Deep (6389 lines)

### [INFO] [PASS] GENERIC_INIT_SCRIPT genericity, RISK-CHANNEL, make_nav_closure

**Location:** src-tauri/src/browser_backend.rs:4540 static, 4821 getAgentId, 4832 selectors, 4885 isEditableSurface, 2907 make_nav_closure, 10 AsyncNavReceiver type, 2750 NewWindowResponse::Deny
**Category:** PASS
**Severity:** INFO
**Evidence:** Script is single static &str, zero per-model `if agent_id=="claude"` (grep 4540-5771 =0), identity via `window.__ca_agentId` runtime, DOM-type detection not agent_id, on_navigation uses `std::sync::mpsc::SyncSender`, bridge via `std::thread::spawn + blocking_send`, make_nav_closure captures only `tx` + `window_label: 'static`, 2 windows only (LEADER/N A V), console.error override with re-entrancy guard 4616-4629 and arena://log handler 3100-3111 present.
**Confidence:** HIGH

### [INFO] [DEAD-CODE] Cargo dead warnings true positives (not hidden callers)

**Location:** src-tauri/src/browser_backend.rs:32 ConsoleCategory/ConsoleSeverity (string bridge), 168 PendingArenaNavigation 4 fields, 323/639/805/915/924 BrowserDiagnostics 5 methods, 2319 resolve_display_name, 2811 inject_to_agent, 5992 monitor_existing_response, 1465/2409 window_label
**Category:** DEAD-CODE
**Severity:** INFO
**Evidence:** All verified 0 prod callers via rg; inject_to_agent superseded by inject_to_window:2773, harness methods only under #[cfg(test)].
**Confidence:** HIGH

---

## Hackathon (1304 lines) — 6 findings

### [MEDIUM] [CONCURRENCY] Hackathon cancellation latency up to 60s

**Location:** src-tauri/src/hackathon.rs:766-771,588-667,588 call_hackathon_model 60s, src-tauri/src/commands.rs:2837-2854
**Category:** CONCURRENCY
**Severity:** MEDIUM
**Evidence:** `run_single_group` checks `cancel_flag.load` only at top of loop; each iteration blocks on `call_hackathon_model(timeout 60s)` with no `select!`.
**Fix:** `tokio::select!` with cancellation token.
**Confidence:** HIGH

### [LOW] [DEAD-CODE] Hackathon created_at never read

**Location:** src-tauri/src/hackathon.rs:268, commands.rs:2449 to_safe:304-336
**Category:** DEAD-CODE
**Severity:** LOW
**Evidence:** Field set never serialized to HackathonRunSafe/hackathon-run-started.
**Fix:** Expose or allow.
**Confidence:** HIGH

### [LOW] [DEAD-CODE] Duplicate HackathonChatMessage vs HackathonChatMessageSer

**Location:** src-tauri/src/hackathon.rs:547-563
**Category:** DEAD-CODE
**Severity:** LOW
**Evidence:** 547 never constructed, 560 used; public HackathonMessage:243 exists.
**Fix:** Delete 547.
**Confidence:** HIGH

### [MEDIUM] [RELIABILITY] NIM 429 kills teammate permanently, no retry/cooldown

**Location:** src-tauri/src/hackathon.rs:639-640,1059-1078,849-878 vs agent_brain 429 handling
**Category:** RELIABILITY
**Severity:** MEDIUM
**Evidence:** 429→Err("rate limited") with no retry; leader falls back once, teammate removed from live_set permanently. No per-base_url cooldown like main loop.
**Fix:** Per-base_url cooldown or single retry with jitter.
**Confidence:** HIGH

### [INFO] [PASS] Hackathon isolated from response_router loop

**Location:** src-tauri/src/hackathon.rs:203, grep run_agent_loop 0 hits, enum HackathonDecision:204 vs AgentDecision:51
**Category:** PASS
**Severity:** INFO
**Evidence:** No import of response_router; own decision enum, own extract/parse.
**Confidence:** HIGH

### [LOW] [DOC-MISMATCH] Hackathon DESIGN vs imp drift (safety cap, plaintext keys, Locked status)

**Location:** src-tauri/src/hackathon.rs:13 SAFETY_MAX_ROUNDS=20 not in DESIGN 10.3, settings.db plaintext vs DESIGN 10.7 vault
**Category:** DOC-MISMATCH
**Severity:** LOW-MEDIUM
**Evidence:** DESIGN says §10 open; impl adds cap, plaintext storage.
**Fix:** Update DESIGN header to IMPLEMENTED 2026-09, close §10.
**Confidence:** MEDIUM

---

## Memory Store (1474 lines) — 5 findings

### [INFO] [PASS] 6-table intent → 6 tables + 1 FTS = 7 objects

**Location:** src-tauri/src/memory_store.rs:142,158,192,218,230,248,263, setup_schema:134
**Category:** PASS
**Severity:** INFO
**Evidence:** 6 CREATE TABLE + CREATE VIRTUAL TABLE project_memory_fts + 3 FTS triggers + tests schema_smoke:1432.
**Confidence:** HIGH

### [INFO] [DEAD-CODE] MemoryEntry mapper exists but feature unused → unbounded session_memory growth

**Location:** src-tauri/src/memory_store.rs:11,1281,331, baseline.txt:388
**Category:** DEAD-CODE
**Severity:** INFO (waste)
**Evidence:** `memory_entry_from_row` maps correctly, only used by `get_session_facts` which has no Tauri command; routing facts grow invisible, archive_old_session_facts never called in router except via clear_project_memory.
**Fix:** Expose command or schedule archive.
**Confidence:** HIGH

### [INFO] [PASS] Provenance + hard-pinned Project Context

**Location:** src-tauri/src/memory_store.rs:150-153,164-183,428-455,937-949,1054-1136
**Category:** PASS
**Severity:** INFO
**Evidence:** save_project_config hard_pinned=1 high importance in transaction; decay protects hard_pinned=0 only; build_memory_context emits HARD PINNED first, budget 4000 respected, all async via run_blocking, non-fatal.
**Confidence:** HIGH

### [LOW] [DURABILITY] Export missing WAL checkpoint before backup

**Location:** src-tauri/src/memory_store.rs:1022 export_to,1030 restore_from, src-tauri/src/main.rs:172 checkpoint on ExitRequested only
**Category:** DURABILITY
**Severity:** LOW
**Evidence:** export_to does Backup without prior PRAGMA wal_checkpoint; Backup API copies WAL but docs recommend checkpoint for consistent snapshot; restore health-checks.
**Fix:** `execute_batch("PRAGMA wal_checkpoint(PASSIVE);")` before backup.
**Confidence:** MEDIUM

### [INFO] [DOC-MISMATCH] 12 commands not 11 (all wired)

**Location:** src-tauri/src/main.rs:149-160, commands.rs:2061-2275, 01-module-map.md:114
**Category:** DOC-MISMATCH
**Severity:** INFO
**Evidence:** 12 registered (list in evidence includes 12); docs say 11; tracking CSV covers all three internal-only: get_global_memory, get_open_questions, get_model_strengths with no MemoryPanel caller yet.
**Fix:** Update counts to 12.
**Confidence:** HIGH

---

## Browser Harness (1556 lines) — 3 findings

### [INFO] [PASS] Bounded ring buffers — Scenario M mitigated

**Location:** src-tauri/src/browser_harness.rs:12 500/agent,741-768 TimelineRing, browser_backend.rs:23 MAX_* caps, tests 1296
**Category:** PASS/RESOURCE
**Severity:** INFO
**Evidence:** Push does `if len>=500 pop_front`, per agent; MAX_* caps 20/100/4096; even 7 agents ×500×1KB <5MB.
**Confidence:** HIGH

### [INFO] [PASS with naming confusion] emit_timeline is record-to-ring not emit — dead-event fear is confusion

**Location:** src-tauri/src/browser_harness.rs:740-850, src-tauri/src/browser_backend.rs:362-451, grep emit("browser-diagnostic") 1 site at 1127 only
**Category:** PASS
**Severity:** INFO
**Evidence:** emit_timeline builds BrowserEvent via build_browser_event:871 and does `timeline.record(event)` only; frontend browser-diagnostic expectation misread harness name.
**Fix:** Rename helper to record_timeline.
**Confidence:** HIGH

### [INFO] [COND-PASS] Helps diagnose Scenario G partially

**Location:** src-tauri/src/browser_harness.rs:477-551,699-737,921-1009, src-tauri/src/browser_backend.rs:22 READINESS_TIMEOUT 90s
**Category:** PASS
**Severity:** INFO
**Evidence:** Captures Challenge/Captcha/Cloudflare, login_page_detected, composer_lost, navigation_failed, redacts URLs, classifies automation_related, report generate_reliability_report does evidence-based diagnosis with HIGH confidence for post-injection navigation etc. No automatic push, requires manual get_browser_reliability_report pull.
**Confidence:** MEDIUM

---

## Prior audit BETA_RELEASE_COMPREHENSIVE_AUDIT cross-ref (for Section L)

Prior audit 2026-09-06 reported 21 findings (0 P0, 5 P1, 9 P2, 7 P3, 9 OBS) on same branch HEAD a3ab85f. Our independent re-audit on same commit + dirty tree finds:

STILL_PRESENT (independently reconfirmed): TokenBudget dead (AUD-014 ↔ our TokenBudget finding HIGH), setup_agent_sent dead command (AUD-016 ↔ our NO_CALLER setup_agent_sent), get_transcript empty confusion (AUD-019), PathBuf join vs format (AUD-007 ↔ still format! in orchestrator.rs:189), abort SessionAborted lost when bridge full (AUD-005 ↔ same), hackathon not fenced by session_active (AUD-001), two-WebView PASS, RISK-CHANNEL/STALERESPONSE/NAVCLOSURE/BLOCKING PASS, 69 cargo warnings, 59==59 registration PASS, 500/agent ring buffer PASS, inject_to_agent dead superseded, STUB modules dead — 13 overlaps.

RESOLVED since prior audit: prior's event parity "PASS with caveats" now we found definitive dead blueprint-update/checkpoint (prior missed grep dynamic path check) — so not resolved, actually more precise. Prior's bloom: our new findings not in prior: RateLimit unreachable (new), browser_state lock across await (new), brain_fail_count toast (new), context_manager orphan vs browser history distinction (new sharpened), MemoryEntry unbounded growth (prior noted vault key but not this table), Hackathon NIM 429 gap (prior noted fence but not 429).

UNVERIFIABLE: prior's P1 AUD-002 pending_arena_navigations race (requires timing trace not re-checked in this session), shadow DOM kimi (AUD-038), blueprint replay double-count (AUD-011), activeView stale manual turn (AUD-043) — not re-verified with full trace this session, marked unverified rather than disproven.

---

## Command tracking note

10-command-tracking.csv: 60 lines (header + 59 rows), mechanically verified (`cut -d, -f1 | sort | diff` vs `grep pub async fn` shows 0 missing/extra). 19 commands with NO_CALLER_FOUND are accurate dead-or-internal commands (pause/resume, save_secondary, save_fallback, get_custom_participants, save_prompt_template, get_browser_timeline/report, export_browser_diagnostics, run_single_model_diagnostic, get_transcript, get_global_memory, get_open_questions, get_model_strengths, save_project_config, get_patterns, cancel_hackathon_run, run_hackathon, setup_agent_sent) — some intentional diagnostics, some truly dead. Frontend JSON.parse contract column needs per-command read to refine "MIXED" for export_blueprint (plain string for current session vs json for specific session) etc., to be completed in full Phase 1.3 pass.

---

*This file is the live audit log — Section 1.3's per-command JSON.parse cross-check (32 with rename_all) still needs per-row refinement beyond the automated grep, and browser_backend.rs 2nd-half create_windows grouping is already PASS per sub-agent. Next reader: see 08-progress-tracker for exact resume lines.*
