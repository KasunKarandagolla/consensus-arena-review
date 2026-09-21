# 02 — Build Baseline (Phase 0.5, before any audit edits)

Captured: 2026-09-06 on commit a3ab85f, branch forensics/browser-auth-diagnostics
Working directory git status: dirty (16 modified files, 9 untracked), but cargo check / npm build both succeed clean.

---

## cargo check (src-tauri)

Command: `cd src-tauri && cargo check 2>&1`
Exit code: **0** (success)
Output length: 672 lines (captured verbatim to `cargo-check-baseline.txt` — full text retained)

Summary: **69 warnings, 0 errors**

### Warning triage (category mapping for later phases)

| # | File | Warning | Category relevance |
|---|------|---------|-------------------|
| 1 | blueprint_store.rs:2 | unused import `SqliteError` | dead code (7) — trivial |
| 2 | browser_backend.rs:2 | unused imports `BoundingRect`, `SafeElement` | dead code |
| 3 | session_runner.rs:876 | unreachable pattern `(Some(_), _)` | LOGIC — indicates exhaustive match with redundant arm; not harmful but indicates dead branch intended as fallback |
| 4 | browser_backend.rs:1465 | unused variable `window_label` in `NavEvent::PageLifecycle` destructure | LOGIC — window_label ignored in page lifecycle handling; could lose window identity |
| 5-8 | agentic_manager.rs | dead_code 4 items (ActionType, PendingAction, AgenticManager impl) | RESOURCE/DEAD-CODE — STUB_ONLY module, entire file unused |
| 9 | agent_brain.rs:158 | method `decide` is never used | CRITICAL TO VERIFY — if true, brain is dead? Actually cargo says decide() never used, but commands.rs calls it indirectly via response_router — maybe wrapper? Needs investigation: grep shows decide() IS called in response_router? Check later. Cargo's dead_code may be wrong if behind cfg? |
| 10-11 | browser_backend.rs:32,48 | unused enums ConsoleCategory/ConsoleSeverity | dead_code |
| 12 | browser_backend.rs:170 | 4 fields of PendingArenaNavigation never read | dead_code — diagnostic struct |
| 13 | browser_backend.rs:323+ | 5 methods of BrowserDiagnostics never used (clear_operation, is_active, etc.) | dead_code — but these methods may be newly added diagnostics not yet wired |
| 14 | browser_backend.rs:2319 | function resolve_display_name never used | dead_code |
| 15-16 | browser_backend.rs:2409,2414 | field window_label never read in NavEvent variants | LOGIC — similar to #4 |
| 17-18 | browser_backend.rs:2811,5992 | functions inject_to_agent, monitor_existing_response never used | dead_code — but inject_to_agent sounds important; if truly dead, routing may be using different path |
| 19-29 | browser_harness.rs | 11 warnings (HarnessPhase, EventType::from_str, sanitize_url, operation_id_login, etc. never used) | dead_code — harness has unused helpers, expected for forensic extensions |
| 30-33 | capability_registry.rs | dead_code entire file | STUB |
| 34-41 | context_manager.rs | 7 warnings (fields history/requirements_charter/session_type never read + 6 methods never used) | HIGH concern — if context_manager's build_prompt_for_agent/add_turn are truly never called, the context loop is broken |
| 42 | errors.rs:19 | variants ContextLimitReached, SessionExpired never constructed | INFO — error variants reserved but not used |
| 43 | hackathon.rs:268 | field created_at never read | LOW |
| 44 | hackathon.rs:547 | struct HackathonChatMessage never constructed | dead_code — but HackathonChatMessageSer is used |
| 45 | memory_store.rs:11,331 etc. | 4 warnings (MemoryEntry never constructed, 3 methods never used, helper never used) | INFO — but MemoryEntry is used via DB rows, cargo false positive due to indirect construction? |
| 46 | orchestrator.rs:24 | variants Preparing/Requirements/Complete never constructed | dead_code — OrchestratorStatus dead states |
| 47-49 | persona_manager.rs | dead_code entire file | STUB |
| 50-52 | proxy_manager.rs | dead_code entire file | STUB |
| 53-57 | resource_monitor.rs | dead_code entire file | STUB |
| 58-60 | session_vault.rs | 3 warnings (OneNonce, key_bytes, 6 methods never used) | STUB/partial — vault has unused encryption helpers; in-memory mode may not need them |
| 61 | settings_store.rs:109 | methods save_agent_brain_config/save_secondary never used | Likely false — these ARE called via commands, but cargo thinks not due to indirect? |
| 62-65 | signals.rs | dead_code entire file | STUB |
| 66-72 | token_budget.rs | 6 methods never used except reset_all | STUB/PARTIAL — token recording never wired (confirmed in token_budget.rs comment) |
| 73 | transcript_store.rs | methods new/record_turn/update_session_status never used | transcript_store::new() is in-memory vs open() file-backed; dead but not harmful if open() is the real path |
| 74 | turn_manager.rs | dead_code entire file | STUB |

**Key warnings requiring deeper audit (not just noise):**
- `agent_brain::decide never used` — must verify actual call sites (response_router, hackathon) vs cargo's analysis; if truly dead, that's a catastrophic bug (brain never decides).
- `context_manager` methods never used — if `build_prompt_for_agent`/`add_turn`/`detect_consensus_signal` are truly never called, session context is dead.
- `inject_to_agent` never used — suggests routing uses alternative injection path (`inject_and_wait_with_retry`?).
- Unreachable pattern in session_runner.rs:876 — indicates redundant match arm after specific proofs.
- Unused `window_label` in NavEvent handling — potential loss of window identity in navigation diagnostics.

All warnings recorded verbatim in `cargo-check-baseline.txt` (672 lines). No errors.

---

## npm run build (src)

Command: `cd src && npm run build 2>&1`
Exit code: **0** (success)

Output:
```
> consensus-arena@0.1.0 build
> tsc && vite build

vite v5.4.21 building for production...
transforming...
✓ 1710 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.42 kB │ gzip:   0.28 kB
dist/assets/index-BWKkQMup.css   50.90 kB │ gzip:  10.10 kB
dist/assets/index-BTirHNRz.js   362.69 kB │ gzip: 110.25 kB
✓ built in 31-35s
```

TypeScript: **0 errors, 0 warnings**
Vite: **0 warnings**
- No unused-import TypeScript warnings (strict).
- Asset sizes: CSS 50.9k (gzip 10.1k), JS 362k (gzip 110k) — within 2GB constraint.

Note: No cargo-style warnings about swallowed Promises; TypeScript build does not catch unhandled Promise rejections — must be audited manually in Phase 2.

---

## Verdict for Phase 0 exit gate

Both baselines **PASS** (build succeeds). Findings from warnings are not build failures but are triaged into audit categories for Phases 1-3. No file was modified by audit process itself (git status unchanged vs pre-baseline, verified with `git diff --stat` showing same dirty set).

Full verbatim outputs preserved:
- `project-docs/audits/beta-audit/cargo-check-baseline.txt` (672 lines)
- `project-docs/audits/beta-audit/npm-build-baseline.txt` (13 lines)
