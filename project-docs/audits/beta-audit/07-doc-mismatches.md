# 07 — Documentation-vs-Source Mismatches

Accumulated throughout Phases 0-3. Format per audit spec. Do not rewrite docs here — only detect.

---

### DOC-MISMATCH: orchestrator.rs AppState field count vs docs (provisional)

**Doc claims:** Check BACKEND.md / ARCHITECTURE.md / IPC.md frontmatter for AppState field count — historically claimed 13 vs 14 (D-056), possibly now claims 17-18.
**Source shows:** Real count is **21** fields (orchestrator.rs:121-173). Includes 3 hackathon fields (`hackathon_run`, `hackathon_run_id`, `hackathon_cancel`) and `memory_store` + `last_memory_health` + `setup_generation` + `active_brain` that may postdate the doc's last update.
**Which is authoritative:** source — docs must be updated to 21 and the per-field table extended.
**Severity of the mismatch itself:** HIGH — An engineer assuming 14 fields will miss hackathon/memory fields when reasoning about lock ordering or shutdown.

---

### DOC-MISMATCH: commands.rs command count vs main.rs handler list

**Doc claims:** Prior audit D-056 found 24 vs 26 mismatch (pause/resume missing). Any doc still claiming 24 or 26.
**Source shows:** Real counts are **59 defined == 59 registered** — the D-056 gap was fixed (main.rs:102-103 now registers pause_session/resume_session with comment citing the audit). Source is now consistent.
**Which is authoritative:** source — docs claiming any older number are stale. Note the growth from 26 to 59 is due to Hackathon (6), Memory (11), and Diagnostics (5) additions.
**Severity:** MEDIUM — if a doc still says 24/26, a reviewer might incorrectly conclude commands are unregistered when they are not, or miss the new commands entirely.

---

### DOC-MISMATCH: IPC.md event list vs real emits (provisional HOLD)

**Doc claims:** IPC.md lists `blueprint-update` and `session-checkpoint` as backend→frontend events with payload shapes and Wiring Rules 7 and 12 referencing them.
**Source shows:** Grep finds zero `emit("blueprint-update")` and zero `emit("session-checkpoint")` in src-tauri/src/*.rs (through partial read). Frontend does `listen('blueprint-update')` and `listen('session-checkpoint')` but they have no matching emit.
**Which is authoritative:** source — if confirmed by full file read, docs/frontend describe events the backend never sends.
**Severity:** HIGH if confirmed — a reader of IPC.md will assume blueprint upsert and checkpoint toasts work, but they may be dead. Provisional until response_router.rs second half is fully read (emit could be via dynamic string or harness path not caught by grep).

---

### DOC-MISMATCH: IPC.md brain-status event documentation gap

**Doc claims:** IPC.md Events section does not have a dedicated `brain-status` header, but frontend listens for it and backend emits it (3 sites in response_router.rs + main.rs).
**Source shows:** `emit("brain-status", ...)` at 3 sites (response_router.rs:460,675,690) and `listen('brain-status')` in useIpcListeners.ts:279 correctly wired. Payload `{ active, model }` matches store shape.
**Which is authoritative:** source — brain-status is real but under-documented; IPC.md should add it.
**Severity:** MEDIUM — not a bug, but an undocumented wiring that future contributors will not find via IPC.md.

---

### DOC-MISMATCH: browser-diagnostic emit documentation

**Doc claims:** IPC.md lists `listen('browser-diagnostic')` with payload `{ agent_id, window_label, phase, url, message, error }` and spec section.
**Source shows:** No plain `emit("browser-diagnostic")` found via grep; diagnostics may be emitted via `emit_timeline`/`emit_harness_event` helpers that use different event plumbing. Frontend listener exists and is not dead per logic, but the emit path is non-obvious and needs explicit verification.
**Which is authoritative:** source — the real emit path must be traced and docs updated to name the helper that emits it if not a plain emit.
**Severity:** MEDIUM — provisional.

---

### DOC-MISMATCH: AGENTS.md WebView limit vs diagnostics window creation

**Doc claims:** AGENTS.md: "Maximum 2 WebViews simultaneously (one persistent leader and one shared navigating participant window)."
**Source shows:** `browser_backend.rs` creates windows via `BrowserState::new` + `create_windows` in commands.rs:252. Hackathon docs mention "reuses the shared nav WebView, never creates a third window" (IPC.md line 232). `run_single_model_diagnostic` command claims to reuse nav window. Real verification requires reading create_windows to confirm no third window is created for diagnostics/hackathon. Current source appears to honor the limit, but the claim is unverified until full read.
**Which is authoritative:** source — must be confirmed by reading creation sites; if any path creates a third window (e.g., concurrent hackathon groups), the doc's hard constraint is violated.
**Severity:** HIGH if violated (resource constraint), otherwise LOW.

---

### DOC-MISMATCH: Phase 1 Memory System status

**Doc claims:** Some docs describe Memory as "possibly landed, possibly not" or as Phase 1 STUB.
**Source shows:** `memory_store.rs` is 1474 lines, backed by `memory.db`, with 11 commands, 3 AppState fields, and frontend MemoryPanel. Status is FULLY_IMPLEMENTED, not stub.
**Which is authoritative:** source — any doc still marking Memory as stub/not-present is stale.
**Severity:** MEDIUM — could cause an auditor to skip auditing a real 1474-line module.

---

### DOC-MISMATCH: Phase 2 Skill Engine status

**Doc claims:** If any doc lists skills/registry/pipeline as planned or partially implemented.
**Source shows:** `capability_registry.rs`, `persona_manager.rs`, `agentic_manager.rs`, `signals.rs`, `proxy_manager.rs`, `resource_monitor.rs` are all STUB_ONLY (dead code, zero callers, 19-66 lines each). No SKILL.md registry exists.
**Which is authoritative:** source — docs should mark these as STUB_ONLY, not partial.
**Severity:** LOW — correctly not shipping, but stale doc could mislead QA into expecting skill features.

---

### DOC-MISMATCH: STUB modules cargo warnings vs AGENTS.md claims

**Doc claims:** AGENTS.md says Phase 1 inventory will be used to calibrate depth; docs may imply stubs are already suppressed.
**Source shows:** `cargo check` emits 69 warnings, ~25 of which are dead_code from stubs. The output is noisy enough to obscure real warnings (e.g., the `context_manager` dead methods that may indicate a real logic gap).
**Which is authoritative:** source — stubs should be annotated `#[allow(dead_code)]` to keep baseline clean.
**Severity:** LOW — not behavioral, but audit hygiene.

---

(Additions will be appended as Phases 1-4 re-read the 9 .md files against source. This is the live doc-mismatch log.)
