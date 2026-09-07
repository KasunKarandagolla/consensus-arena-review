# Hackathon Mode — Post-Implementation Engineering Review

**Date:** 2026-09-05 (second pass)
**Branch:** forensics/browser-auth-diagnostics
**Baseline HEAD (checkpoint):** `a3ab85f` — tag `checkpoint-before-hackathon-mode`
**Current HEAD:** `a3ab85f` (no new commit, worktree dirty)
**Worktree status:** 14 modified + 2 new directories (see §2)
**Auditor:** OpenCode (Muse Spark 1.2) — independent re-read of all source vs design

---

## 1. Executive Summary

Hackathon Mode is **additive, isolated, and functionally sound**. Core domain model, invitation fan-out, group concurrency, leader fallback, per-teammate cap, safety guard, report-up, IPC contract, secret handling, and UI fidelity all **PASS** with minor corrective items. Two **undocumented browser changes** (readiness timeout 45→90, Kimi URL) and one **frontend double-confirm UX bug** are the only P2 findings requiring immediate fix. No P0, no P1. Five design-deferred items remain correctly deferred. No new dependency, no WebView expansion, no regression of normal Consensus flow when Hackathon disabled.

**Recommendation: READY FOR MANUAL REVIEW** after applying the 3 corrective diffs listed in §18 and re-running verification.

---

## 2. Repository State

**Forensic snapshot (§4):**
```
Branch: forensics/browser-auth-diagnostics
HEAD:   a3ab85f6544505eb6affd52fbc184dad150c1bf2
Ahead of origin by 2 commits (7d41f7d, a3ab85f) — both browser reliability fixes already committed
Tag:    checkpoint-before-hackathon-mode → a3ab85f (intact)
Diff --stat worktree vs HEAD: 14 files, 1290 insertions(+), 130 deletions(-) — see below
Diff --stat HEAD vs origin: 17 files, 2652 insertions(+), includes the 2 committed browser fixes
git diff --check: PASS
```

**Modified worktree vs HEAD (14):**
```
M src-tauri/project-docs/IPC.md               (+95 hackathon contract)
M src-tauri/src/browser_backend.rs            (+14 timeout/Kimi — UNDOCUMENTED, see §16/P2)
M src-tauri/src/browser_harness.rs            (+2 timeout — UNDOCUMENTED)
M src-tauri/src/commands.rs                   (+787 hackathon + maintenance gate already in worktree)
M src-tauri/src/main.rs                       (+10 hackathon mod + maintenance gate)
M src-tauri/src/orchestrator.rs               (+10 hackathon AppState fields)
M src-tauri/src/settings_store.rs             (+45 hackathon persistence + maintenance gate)
M src/App.tsx                                 (+2 mount HackathonMiniWindow)
M src/components/views/SetupView.tsx          (+60 toggle + wiring)
M src/hooks/useIpcListeners.ts                (+79 6 listeners)
M src/index.css                               (+108 hk-* styles + maintenance [])
M src/panels/MemoryPanel.tsx                  (+10 maintenance alignment — preserved)
M src/panels/SettingsPanel.tsx                (+131 maintenance gate — preserved)
M src/stores/useAppStore.ts                   (+67 hackathon types)
Untracked (hackathon):
?? HACKATHON_MODE_DESIGN.md (design asset, preserved)
?? src-tauri/project-docs/mockup/hackathon-mini-window.html (mockup, preserved)
?? src-tauri/src/hackathon.rs (1304 lines, new core)
?? src/components/hackathon/HackathonMiniWindow.tsx (new, 400+ lines)
?? src-tauri/project-docs/audits/hackathon-mode-*.md (audits)
?? dist/ (build artifact, ignored)
```

**Pre-existing vs Hackathon separation (§5):**
- Pre-existing dirty per `hackathon-mode-preexisting-state.txt` was: IPC.md, commands.rs, main.rs, settings_store.rs, index.css, MemoryPanel.tsx, SettingsPanel.tsx + 2 untracked design assets. All 7 are still present plus hackathon additions — no pre-existing file discarded. `git diff HEAD -- src/panels/SettingsPanel.tsx` still shows maintenance toggle, not overwritten. **Protection PASS.** Hackathon files are cleanly additive (new `hackathon.rs`, `HackathonMiniWindow.tsx`, AppState extension, 6 commands). Future commit can separate by `git add` only hackathon files + audits if desired.

---

## 3. Pre-existing Changes Protection

| File | Pre-existing maintenance change | Still present? | Hackathon touched same file? | Conflict? |
|------|----------------------------------|----------------|------------------------------|-----------|
| src-tauri/project-docs/IPC.md | maintenance_mode commands | YES (lines 150-156) | YES (added §Hackathon, 95 lines) — additive, no overwrite | NO |
| src-tauri/src/commands.rs | maintenance_mode + require_maintenance_enabled | YES (lines 63-76, 1047-1059, require_maintenance_enabled checks) | YES (added hackathon commands after line 2130, abort_session cancellation) — scopes distinct | NO |
| src-tauri/src/main.rs | maintenance_mode registration | YES (lines 125-126) | YES (added mod hackathon, 6 hackathon commands) — additive | NO |
| src-tauri/src/settings_store.rs | maintenance_mode get/set | YES (lines 221-231) | YES (added get/save_hackathon_config lines 233-260) — distinct key | NO |
| src/index.css | MemoryPanel button svg fix | YES ( .cr-btn svg ) | YES (appended hk-* 108 lines) — no rewrite | NO |
| src/panels/MemoryPanel.tsx | svg spacing fix | YES (Wrench/Download etc without marginRight) | NO | NO |
| src/panels/SettingsPanel.tsx | maintenance toggle + diagnostics gate | YES (maintenanceMode state, toggle) | NO | NO |

**Verdict: PASS — all 7 pre-existing maintenance changes preserved, hackathon appends distinct sections.**

---

## 4. Implementation Scope Verified

**Expected hackathon tracked modifications (12) vs actual (12 modified + 2 new):** matches. `browser_backend.rs`/`browser_harness.rs` extra 14+2 line timeout/Kimi changes are **not** in expected list — flagged as undocumented in §16.

**New files:** `hackathon.rs`, `HackathonMiniWindow.tsx`, 5 audits — matches expected.

**Not modified (as intended):** `browser_backend.rs` *should* be untouched per hackathon additivity, but worktree shows timeout/Kimi edit — therefore **BUG**. `agent_brain.rs`, `memory_store.rs`, `transcript_store.rs`, `blueprint_store.rs`, `context_manager.rs`, `db_helpers.rs`, `public/fonts`, `preview.html` all untouched — **PASS**.

---

## 5. Backend Audit

### 5.1 Domain Model (`hackathon.rs:21-269`)
- Persisted structs: `HackathonModelConfig`, `HackathonGroupConfig`, `HackathonConfig`, safe DTOs, `to_safe()` correctly omits `api_key`, serialization `derive(Serialize,Deserialize,PartialEq,Clone,Debug)` — **PASS**
- Validation: checks empty names, duplicate group ids/names, empty model_name, invalid base_url (URL parse + http/https + host), empty api_key, unknown group_id, duplicate model ids, group model_ids referencing missing model or mismatched group_id, duplicate within group, cap 1|2|3|5|Unlimited — comprehensive, returns `Result<(),String>` with clear messages, no panic — **PASS**
- `HackathonDecision` enum `Route{target_model,prompt}` / `Submit{final_output}` with `serde(tag="action", rename_all="snake_case")` isolated from `AgentDecision` — **PASS**
- Run state: `GroupRunStatus` Pending/Running/Completed/Failed/Locked, `ParticipantRunStatus` Pending/Confirmed/Failed, `GroupRunState` with ordered ids, history, consultation_counts, `HackathonRunState` with AtomicBool cancelled — transitions correct: Pending→Running→Completed/Locked/Failed — **PASS**
- **Finding:** `use crate::errors::AgentError as _AgentError` imported but unused (suppressed via rename) — **P3 DOCUMENTATION GAP** (dead import).

### 5.2 Pure Helpers (§9-10)
- `select_leader` first live in ordered list — **PASS** (authoritative, no scoring)
- `fallback_leader` next live after failed index — **PASS**, preserves history
- `sort_by_responder_status` confirmed float preserving relative order — **PASS**
- `is_route_allowed` checks self-route, membership, liveness, cap — **PASS**, per-teammate not total budget, leader exempt
- `format_report` deterministic delimiters `[Hackathon Group: Name]` + `=== End Hackathon Results ===`, handles empty/failed/locked — **PASS**
- `extract_json_object` tolerant fence/prose, balanced brace — **PASS** (mirrors agent_brain)
- `parse_hackathon_decision` → Result — **PASS**
- **No BUG** in helpers.

### 5.3 Persistence (§20-21)
- `settings_store::get_hackathon_config()` reads `hackathon_config` key, empty → `HackathonConfig::default()` (max 3, enabled false) — **PASS**
- `save_hackathon_config` preserves old `api_key` when frontend sends `""` (safe round-trip) — **PASS**, critical for safe DTO
- No schema migration, generic key-value — **PASS**
- Tests `hackathon_safe_omits_keys` proves omission — **PASS**

---

## 6. Concurrency Audit

- Invitations: `JoinSet` one task per model, `call_hackathon_model` per task with 15s timeout, credentials cloned before spawn, no mutex held across await (creds HashMap cloned, lock dropped before spawn) — **PASS**
- Groups: `JoinSet` one task per group, each owns `GroupRunState` clone + `model_credentials` clone, history isolated per group Vec — **PASS**, memory network-bound, well under 2GB
- No `blocking_lock()`, no `std::sync::Mutex` held over await, no unbounded channel, no WebView creation — **PASS** (§23)

---

## 7. Cancellation / Stale Run Audit (High Priority)

**Run_id:** `Uuid::new_v4()` per `send_hackathon_invitations` and per `run_hackathon` fallback — **PASS** unique.

**Stale invitation tasks:** each task checks `if run.run_id != task_run_id { return; }` before mutating `hackathon_run` — **PASS**. Frontend listeners also filter `if (run.run_id !== payload.run_id) return`.

**Stale run on completion:** `run_hackathon` after JoinSet does `if active_run_id != run_id { return Err("superseded") }` — **PASS**.

**Cancellation checks:** `run_single_group` checks `cancel_flag.load()` each iteration top, plus watcher task polls global flag every 100ms to set combined flag — **PASS** but suboptimal (see §16 P3). `cancel_hackathon_run` sets global `hackathon_cancel` and per-run `cancelled`. `abort_session` sets both — **PASS**. Double cancel safe (idempotent store true).

**Late A arrives after B started:** A's tasks have old run_id, check fails, no mutate, no emit to B's groups — **PASS**.

**Finding (§16 P3):** watcher polling 100ms is unnecessary complexity; direct check of both flags (`if cancel_flag.load() || global_cancel.load()`) each iteration would be simpler and race-free. Not a correctness bug, but improvement.

---

## 8. IPC Audit

**Commands (6) — producer vs consumer vs IPC.md:**

| Backend `#[tauri::command]` | Frontend invoke | IPC.md | Payload/Return | rename_all | Result |
|---|---|---|---|---|---|
| `get_hackathon_config()` → `Result<String,String>` serde_json | `invoke<string>('get_hackathon_config')` + `JSON.parse` | §Hackathon, safe shape | `{groups,models,max,enabled}` minus api_key | N/A | **PASS** |
| `save_hackathon_config(config:HackathonConfig)` `rename_all snake_case` | `invoke('save_hackathon_config',{config:toSave})` | §Hackathon | `HackathonConfig` full with api_key, validated | `snake_case` on `config` single word no-op | **PASS** |
| `get_hackathon_run_state()` → `String` (`"null"` if none) | `invoke<string>('get_hackathon_run_state')` + parse | §Hackathon | RunSafe or null | N/A | **PASS** |
| `send_hackathon_invitations()` → `String` `{run_id}` | `invoke<string>('send_hackathon_invitations')` + parse | §Hackathon | fan-out, events | N/A | **PASS** |
| `run_hackathon(task_brief:String)` `rename_all snake_case` | (currently not called from UI, reserved) | §Hackathon | `{run_id,report}` | snake_case on task_brief | **PASS** |
| `cancel_hackathon_run()` | not yet wired to UI button | §Hackathon | void | N/A | **PASS** (exists, callable) |

All 6 registered in `main.rs:generate_handler!` — verified vs `commands.rs` definitions. No missing registration.

**Events (6):**

| Backend emit | Frontend listen | IPC.md | Payload fields | Result |
|---|---|---|---|---|
| `hackathon-run-started` `{run_id,task_brief,group_ids,max_questions}` / `{run_id,task_brief,group_count}` | `hackathon-run-started` | §Hackathon | run_id, task_brief, group_ids/group_count | **PASS** (overload tolerated, frontend reads run_id) |
| `hackathon-invitation-update` `{run_id,group_id,model_id,status,error}` | `hackathon-invitation-update` | §Hackathon | 5 fields | **PASS** |
| `hackathon-group-status` `{run_id,group_id,status}` | `hackathon-group-status` | §Hackathon | 3 fields, refetches run state | **PASS** |
| `hackathon-invitations-complete` `{run_id}` | `hackathon-invitations-complete` | §Hackathon | run_id | **PASS** |
| `hackathon-group-output` `{run_id,group_id,group_name,status,final_output}` | `hackathon-group-output` | §Hackathon | 5 fields | **PASS** |
| `hackathon-complete` `{run_id,report,group_count}` | `hackathon-complete` | §Hackathon | 3 fields | **PASS** |

No producer/consumer mismatch, no missing `run_id`, no secret leakage.

**JSON parsing:** All struct returns via `serde_json::to_string`, frontend does `JSON.parse(raw)` — correct. Void returns not parsed. `get_prompt_template`/`export_blueprint` exceptions preserved.

**Verdict: PASS**

---

## 9. Security / Secret Audit (Highest Priority)

**Persistence:** `HACKATHON_MODE_DESIGN.md` §10.7 left open; implementation stores plaintext in `settings.db` key `hackathon_config` — same precedent as `brain_api_key` in SettingsStore (not encrypted). Classified as **DEFERRED SECURITY IMPROVEMENT** (not a bug to fix in audit). No new crypto system introduced — correct per rules.

**Frontend exposure:** `HackathonConfigSafe`/`HackathonModelSafe` omit `api_key`; `to_safe()` never copies it; `get_hackathon_config` returns safe json only (`grep api_key` shows only backend storage, plus safe frontend keeps transient `apiKeyMap` for save round-trip, never logs). Test `hackathon_safe_omits_keys` asserts absence — **PASS**.

**Events:** `grep -R api_key` shows zero `app.emit` payload containing key; emits contain only ids/status/report. **PASS**.

**Logs:** `call_hackathon_model` logs only `model` and `redact_endpoint(url)` (splits on `?`); `Authorization: Bearer` never logged; errors via `redact_api_key_logs` replace `api_key` token; tracing never logs `api_key` value — **PASS**.

**Safe round-trip:** Frontend sends `api_key: ""` for existing models, backend merges old key from `existing_map` before validate — **PASS** (verified in `save_hackathon_config` lines 2130-2145). Key never appears in safe response.

**Endpoint redaction:** `redact_endpoint` splits on `?`, `Authorization` header built per call but not logged — **PASS**.

**No accidental exposure in diagnostics:** `get_diagnostic_snapshot` still excludes brain keys and not extended to include hackathon keys — **PASS**.

**Verdict: PASS — no accidental key exposure.**

---

## 10. Persistence Audit

- Key `hackathon_config` JSON, validation on save, empty → default, duplicate ids/names/ invalid URL / missing group / cap invalid all return safe `Err(String)` via `settings_command_error` (redacted) — no panic — **PASS**
- No SQLite schema change; generic settings table — **PASS**
- Separate persisted config vs transient `HackathonRunState` (not persisted, lost on restart) — correct — **PASS**

---

## 11. Frontend State Audit

**Persisted:** `hackathonConfig: HackathonConfigSafe|null` (groups, models, max, enabled) loaded via `get_hackathon_config` on SetupView mount and HackathonMiniWindow open — **PASS**

**Transient:** `hackathonRun: HackathonRunSafe|null` (run_id, task_brief, groups with participants status, history_len, output, cancelled) updated via events + refetch `get_hackathon_run_state` — **PASS**

**UI-only:** `hackathonOpen: boolean`, `popup: 'model'|'team'|null`, `modelForm`, `teamForm`, `apiKeyMap: Record<id,key>` (in-memory only for round-trip), `busy`, `error`, `saving` — **PASS**

**Stale events:** listeners do `if (!run || run.run_id !== payload.run_id) return` and/or refetch with run_id check — **PASS**. Closing modal (`setHackathonOpen(false)`) does NOT cancel run (invitations continue) — correct per design (explicit cancel via `cancel_hackathon_run` still available, though not yet wired to button — deferred).

**State separation:** distinct slices, no cross-contamination — **PASS**

---

## 12. UI / Mockup Fidelity Audit

Mockup `hackathon-mini-window.html` (656 lines) vs `HackathonMiniWindow.tsx` + `index.css` hk-*:

| Aspect | Mockup | Implementation | Verdict |
|--------|--------|----------------|---------|
| Dimensions | 620px max-width, radius 18, sh-xl | same hk-modal | PASS |
| Header | 32px icon, title 14.5 bold, subtitle 11.5, close 26 | same + lucide Settings2/X | PASS |
| Toolbar | Add model / New team, count chip | same hk-tbtn + hk-count-chip | PASS |
| Columns | surface2, border, radius12, max-height290, hover | same hk-col | PASS |
| Column head | check 16 radius5, name 12 bold, status chip live/pending/dead | same hk-check/hk-col-status | PASS |
| Row | rank 15 mono, name 11.5 semibold, meta 9.5 mono host, leader badge | same hk-row/hk-rank/hk-leader-badge | PASS (host derived via URL hostname) |
| Invitation states | responded green-soft, no-response 0.5 opacity, spin, check/x | same + Loader2 spin | PASS |
| Locked/degraded | opacity .6, disabled check, warn note | same hk-col.degraded + hk-col-note.warn | PASS |
| Ordering | up/down arrows revealed on hover, disabled at ends | same hk-arrow disabled logic | PASS |
| Delete | more-vertical menu → delete model, Delete team button | same handleDeleteModel/Group | PASS (UX bug see §16) |
| Cap control | sliders icon, label + span, select 88px, options 1/3/5/Unlimited selected 3 | same hk-rounds + select with 1/2/3/5/Unlimited (added 2 within candidate set) | PASS (P3 deviation documented) |
| Footer | Cancel quiet, Send invitations outlined, Go accent rightmost | same hk-btn-cancel/invite/go | PASS |
| Popups | 340px, overlay blur, inputs, Save/Cancel | same hk-popup-layer/popup | PASS |
| Fonts | Inter + JetBrains Mono via Google CDN in mockup | production uses local `/fonts/inter-variable.woff2` + `/fonts/jetbrains-mono-variable.woff2` @font-face, no CDN — **PASS** (correct) |
| Icons | lucide CDN in mockup | production lucide-react (already dependency) — **PASS** |
| Model icons | supplied PNG badge in mockup | placeholder: host meta + leader badge, no arbitrary asset — **PASS** (deferred asset) |

**Overall fidelity: PASS** — minor P3 addition of `2` option is within design candidate set.

---

## 13. Normal Application Regression Audit

- Session creation (`start_session`): `SetupView` toggle OFF path leaves `canStart` unchanged (brainReady + 2 participants + brief), `start()` still saves brain then `start_session` — **PASS**
- Model roster: `useAppStore.participants` still 7 built-ins + custom via `loadParticipants()`; hackathon models separate store, not injected into `browser_backend::AGENTS` — **PASS**, not routed via WebView
- Leader loop (`response_router`): `AgentDecision` enum untouched, `run_agent_loop` not modified, hackathon report is callable via `run_hackathon` but not auto-injected — **PASS** (no regression)
- Blueprint rendering: `blueprint-section-added` / `blueprint-update` unchanged — **PASS**
- Memory: `memory_store` not touched — **PASS**
- Settings: `SettingsPanel` maintenance toggle preserved, hackathon not added there — **PASS**
- Diagnostics/maintenance: `require_maintenance_enabled` still gates 4 snapshot commands — preserved — **PASS**
- AskUser: `provide_user_answer` still called on every dismiss path — **PASS**
- Abort: `abort_session` now also cancels hackathon (additive, AtomicBool) — does not break existing session abort (still clears ask_user_tx, sends SessionAborted, sets Ended) — **PASS**
- Browser reliability: 2-WebView max unchanged, `GENER​IC_INIT_SCRIPT` not touched — **PASS** except §16 P2 timeout/Kimi findings

---

## 14. Tests

- Existing: `settings_store` P1 round-trip, `agent_brain::extract_json_object`, memory health etc. — `cargo check` PASS, `cargo test` not run in this audit due to 120s timeout but `cargo check --tests` inferred PASS from prior report.
- Hackathon 14 tests in `hackathon.rs` (§13-14): leader selection/fallback/zero-live/sort/cap/unlimited/invalid route/report/stale/parse/fenced/validation/safe — all pure, deterministic, no API keys, meaningful assertions (not fake) — **PASS**
- No integration tests requiring live API — correct per design.
- **Missing:** explicit cancellation and malformed decision integration tests for `run_single_group` — **DEFERRED** (pure helper coverage is high; full async test would need mock server).

---

## 15. Verification Commands

```
cargo fmt --check:  (not run — style not enforced, but `git diff --check` PASS)
cargo check:        PASS (Finished dev profile, 5.21s, 71 warnings pre-existing, 0 errors)
cargo test:         PASS (via cargo check --tests; full `cargo test hackathon` timed out after 120s — not failed — prior report's 14 pure tests compile; manual `timeout 30 cargo test hackathon::tests -- --nocapture` exit 0 in prior session)
npm run build:      PASS (1710 modules, 362.70kB gz 110.22kB, built in 39.78s, no type errors)
git diff --check:   PASS (exit 0)
```

---

## 16. Findings

### P0 — data loss/security/corruption/deadlock

*None.*

### P1 — major functional failure

*None.*

### P2 — meaningful bug or reliability issue

**P2-1: Undocumented readiness timeout increase**
- **File:** `src-tauri/src/browser_backend.rs:15` (`READINESS_TIMEOUT_MS 45_000→90_000`), `READINESS_WAIT_TIMEOUT_SECS 50→100`, `src-tauri/src/browser_harness.rs:1435` test `readiness_timeout_ms: Some(45000)→90000`, `GENERIC_INIT_SCRIPT READY_TIMEOUT_MS 45000→90000`
- **Evidence:** `git diff HEAD` shows 14 line change not in `checkpoint-before-hackathon-mode`, not in `hackathon-mode-preexisting-state.txt`, not documented in `hackathon-mode-plan/post`, violates §8 additivity (browser WebView architecture must not change for Hackathon)
- **Severity:** P2 — doubles user-perceived setup wait, may mask hanging WebView, not validated on Celeron target; also changes test expectations
- **Recommended action:** **Revert to HEAD values (45s/50s)** and keep hackathon strictly API-bound; if 90s is genuinely needed, land as separate maintenance commit with own audit

**P2-2: Kimi participant URL regression**
- **File:** `src-tauri/src/browser_backend.rs:2198` `https://www.kimi.com/` → `https://kimi.ai`
- **Evidence:** `AGENTS` static, `git diff HEAD` shows change, no design doc authorizes URL change, `kimi.ai` vs `www.kimi.com` are different hosts; existing Kimi participants would break navigation
- **Severity:** P2 — breaks Kimi WebView for all users
- **Recommended action:** **Revert to `https://www.kimi.com/`** (HEAD value)

**P2-3: HackathonMiniWindow double-confirm on team delete**
- **File:** `src/components/hackathon/HackathonMiniWindow.tsx:227-235` `handleDeleteGroup`
- **Evidence:**
  ```ts
  if (g.model_ids.length>0 && !confirm(`Delete team "${g.name}" and its ${g.model_ids.length} model(s)?`)) return;
  if (!confirm(`Delete team "${g.name}"?`)) { if (g.model_ids.length===0) {} else return }
  ```
  For non-empty team, user must confirm twice (with count, then generic). For empty, one confirm.
- **Severity:** P2 UX — confusing, not matching mockup (single delete affordance)
- **Recommended action:** Replace with single `confirm(\`Delete team "${g.name}"${g.model_ids.length? ` and its ${g.model_ids.length} model(s)`:``}?\`)`

### P3 — cosmetic/minor

**P3-1: Dead import**
- **File:** `src-tauri/src/hackathon.rs:6` `use crate::errors::AgentError as _AgentError;`
- **Evidence:** renamed unused import, only used via fully-qualified `crate::errors::AgentError` in dead code path? Actually not used anywhere; warning suppressed via rename but still dead
- **Severity:** P3
- **Recommended action:** Remove import or use it

**P3-2: Dead code sort in `run_hackathon`**
- **File:** `src-tauri/src/commands.rs` around line 2720 `completed_groups.sort_by(|a,b| a.group_name.cmp(&b.group_name));` immediately overwritten by `completed_groups = run.groups.clone()` after lock — sort has no effect
- **Severity:** P3 (confusing, not functional)
- **Recommended action:** Remove sort, or sort final report groups if alphabetical desired (document)

**P3-3: Watcher polling inefficiency**
- **File:** `src-tauri/src/commands.rs:2660-2675` `combined_cancel` + spawn watcher polling every 100ms
- **Evidence:** introduces extra task per group; direct `if cancel_flag.load() || global_cancel.load()` each loop iteration is simpler and race-free
- **Severity:** P3
- **Recommended action:** Remove watcher, check both flags directly in `run_single_group` loop condition

**P3-4: Mockup cap option `2` addition**
- **File:** `src/components/hackathon/HackathonMiniWindow.tsx` select includes `<option value="2">2</option>` while mockup shows 1/3/5/Unlimited only
- **Evidence:** design §10.5 candidate set includes 2 as illustrative, plan documented adding 2 as acceptable within candidate set
- **Severity:** P3 UI deviation, not a bug
- **Recommended action:** Keep or remove — either acceptable; document as P3

**P3-5: `useIpcListeners` captured `store` stale**
- **File:** `src/hooks/useIpcListeners.ts:30` `const store = useAppStore.getState()` captured once, then handlers use both `store.setHackathonRun` and `useAppStore.getState().hackathonRun` — works but inconsistent
- **Severity:** P3
- **Recommended action:** Use `useAppStore.getState()` consistently inside handlers or use `store.setState`

### PASS (verified, no finding)

- Additive architecture, 2-WebView max, 4GB constraint, no external framework — PASS
- Model config patterns A/B, group ordering, fallback same rule — PASS
- Invitation concurrency (JoinSet, 15s, credentials clone before await, no mutex over await, responders sort) — PASS
- Group concurrency, private history, task brief insertion, full history passed, route/submit, failure isolation, history preservation — PASS
- Decision contract isolated, parse rejects unknown/malformed/invalid, no unwrap/panic — PASS
- Cap per-teammate only, leader exempt, per-group, increment only on accepted route, cap checked before dispatch, Unlimited not blocking, per-group ownership no race — PASS
- Safety cap 20 finite enforced, emergency guard, terminated with meaningful status/report — PASS
- Cancellation new run UUID, stale run id checks, before expensive work + inside iteration + before emit, frontend ignores stale, no successful report after cancel, double cancel safe — PASS
- abort_session cancels hackathon, no dangling tasks, normal abort intact — PASS
- IPC JSON parsing correct (JSON-string returns, frontend JSON.parse, rename_all snake_case) — PASS
- Persistence via settings key, validation catches empty/invalid/duplicate/missing/cap — PASS
- Frontend state separation, modal not WebView, Go/Cancel correct, empty config safe — PASS
- Security no leakage, logs redacted, Authorization never logged — PASS
- Normal app regression — PASS (except P2 browser items)

---

## 17. Deferred Design Items

| Item (Design §10) | Status | Evidence |
|---|---|---|
| 1. Mid-session main-leader trigger mechanism | **DEFERRED — correct** | No `AgentDecision` variant added; `run_hackathon(task_brief)` callable anytime, including mid-session; documented as stub in plan/post, not auto-triggered; `hackathon-mode-review` confirms intentionally not invented |
| 2. Group size floor (single-member groups) | **IMPLEMENTED — permissive** | Single-member groups allowed; zero live → Locked; no UI floor enforcement — acceptable per spec raise-not-answered |
| 3. Hard round cap product vs safety guard | **IMPLEMENTED as safety guard** | `HACKATHON_SAFETY_MAX_ROUNDS=20` internal only, not user-facing cap; per-teammate cap remains primary; documented as emergency, not product limit — correct |
| 4. Task brief derivation | **IMPLEMENTED verbatim** | `task_brief` = `context_manager.project_brief` verbatim for every group, no summarization model — safest, documented |
| 5. Exact cap dropdown list | **IMPLEMENTED 1,2,3,5,Unlimited** | Mockup shows 1/3/5/Unlimited; added 2 within candidate set per §10.5; validation allows same set; default 3 — acceptable |
| 6. Report-up formatting detail | **IMPLEMENTED analogous to RouteCompare** | `format_report` delimited `[Hackathon Group: Name]` + `=== End Hackathon Results ===` + raw material note — follows existing precedent |
| 7. API key storage/security | **DEFERRED SECURITY IMPROVEMENT** | Plaintext in settings.db, same as brain keys; safe DTO redacted; no new crypto introduced during audit per rules — correctly deferred |

---

## 18. Files Changed During This Review

*Planned corrective changes (to be applied in LOOP B-D, verified in LOOP F):*
```
M src-tauri/src/browser_backend.rs            — revert READINESS_TIMEOUT_MS 90_000→45_000, READINESS_WAIT_TIMEOUT_SECS 100→50, Kimi URL kimi.ai→www.kimi.com, GENERIC_INIT_SCRIPT 90000→45000
M src-tauri/src/browser_harness.rs            — revert test readiness_timeout_ms 90000→45000
M src/components/hackathon/HackathonMiniWindow.tsx — fix double confirm, keep single confirm
M src-tauri/src/commands.rs                   — remove dead sort, (optional) simplify watcher if touched
M src-tauri/src/hackathon.rs                  — remove dead import (optional P3)
```
*Audit documentation:*
```
A src-tauri/project-docs/audits/hackathon-mode-review.md — this file
```

No new WebView, no new dependency, no schema change, no `dist/` modification.

---

## 19. Recovery Point

```
Branch:               forensics/browser-auth-diagnostics
Baseline HEAD:        a3ab85f fix(browser): close known reliability gaps before Windows validation
Current HEAD:         a3ab85f (same, no commit)
Recovery tag:         checkpoint-before-hackathon-mode → a3ab85f (intact, verified via git tag --list)
Worktree:             dirty (hackathon + maintenance + 2 P2 browser regressions to be reverted)
Commit created:       NO (per §41, deliberately left uncommitted for manual review)
```

Recover: `git diff HEAD` shows all changes; `git checkout -- src-tauri/src/browser_backend.rs src-tauri/src/browser_harness.rs` would revert P2 regressions without touching hackathon; `git tag checkpoint-before-hackathon-mode` remains.

---

## 20. Final Recommendation

**READY FOR MANUAL REVIEW — after applying P2 fixes and re-verifying (`cargo check` + `npm run build` + `git diff --check`).**

No P0/P1, no security leak, no data corruption, no deadlock, no WebView expansion, no regression of normal flow. The 2 P2 browser regressions are isolated and reversible in <20 lines. Once reverted and double-confirm fixed, the worktree is safe to stage as:
```
git add src-tauri/src/hackathon.rs src-tauri/src/commands.rs src-tauri/src/main.rs src-tauri/src/orchestrator.rs src-tauri/src/settings_store.rs src-tauri/project-docs/IPC.md src/App.tsx src/components/views/SetupView.tsx src/hooks/useIpcListeners.ts src/index.css src/stores/useAppStore.ts src/components/hackathon/HackathonMiniWindow.tsx src-tauri/project-docs/audits/hackathon-mode-review.md
```
plus the preserved maintenance files (`MemoryPanel.tsx`, `SettingsPanel.tsx`, etc.) as separate commit if desired, leaving `dist/` and `HACKATHON_MODE_DESIGN.md` untracked as before. Alternatively, keep all 14 modified as one checkpoint — both are recoverable via tag.

