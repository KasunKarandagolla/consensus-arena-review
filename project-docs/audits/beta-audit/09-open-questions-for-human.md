# 09 — Open Questions Requiring Human Judgment

Questions that cannot be resolved from source alone and require the project owner's intent/decision.

---

### Q1: Is ContextManager intentionally dead?

**Context:** `context_manager.rs` is 149 lines but cargo says its fields `history`/`requirements_charter`/`session_type` and methods `build_prompt_for_agent`/`add_turn`/`detect_consensus_signal`/`is_consensus_reached` are never used. The real loop in `response_router.rs` builds prompts from `leader_response + context` (iteration count + brief) and browser window history, not via ContextManager.

**Question for human:** Is ContextManager legacy code awaiting removal, or was its wiring accidentally dropped and should be restored to include conversation history in participant prompts? Should it be deleted, or re-wired?

---

### Q2: Is token budgeting intentionally dormant post-beta?

**Context:** `token_budget.rs` has `record_tokens` never called; the file's own comment admits this. `reset_all` is correctly called on session start, but without recording the 70%/90% thresholds never trigger.

**Question for human:** Is token-budget wiring intentionally deferred past beta (and the dormant code acceptable for now), or should it be wired before beta to protect the <2GB constraint on long sessions? If deferred, should the dead-code warnings be suppressed?

---

### Q3: Screenshot capture portion of flight-recorder — intentional omission?

**Context:** The diagnostic harness (browser_harness.rs 1556 lines + diagnostics snapshot) is FULLY_IMPLEMENTED for structured logs/timeline. The spec's "screenshot capture" portion has no `capture_screenshot` implementation found.

**Question for human:** Was screenshot capture intentionally deprioritized (logs sufficient for beta), or is it a gap that should be flagged as missing? Should the Phase 0 classification remain PARTIALLY_IMPLEMENTED for the recorder as a whole?

---

### Q4: Do blueprint-update and session-checkpoint represent intentional future events or dead code?

**Context:** IPC.md documents both; frontend listens for both; backend emits neither (per provisional grep). Either they are dead listeners/docs drift, or the emits are hidden in a harness helper not caught by grep.

**Question for human:** Were these events intentionally added to IPC.md ahead of implementation (and should be marked as such), or did their emits regress?

---

### Q5: Hackathon run cancellation semantics

**Context:** Hackathon loops check `cancel_flag` each iteration and `hackathon_cancel` AtomicBool is set in `abort_session`. The spec says cancel "does not delete persisted HackathonConfig."

**Question for human:** Should cancelling a hackathon run also emit `hackathon-complete` with a cancellation reason, or is the current silent `Failed` status sufficient for the UI to distinguish cancel vs failure?

---

(Add more as Phases 1-4 progress. These are not findings — they are intent questions that cannot be answered from code alone.)
