# Prompt Hardening — Beta Final Freeze Audit

**Date:** 2026-09-07 (UTC)
**Branch:** `forensics/browser-auth-diagnostics`
**HEAD:** `a3ab85f` + working tree (prompt hardening, no commit)
**Scope:** FINAL TASK — Review, harden, install and freeze the three AI prompts

---

## 1. Canonical Files — Single Source

* `leader_priming.md` at repo root (`/home/kasun/Music/arena/consensus-arena/leader_priming.md`) is the authoritative leader template. Embedded via `include_str!("../../leader_priming.md")` in `settings_store.rs`, seeded into `settings.db` key `prompt_leader_priming` on first open, and rendered per-session in `session_runner.rs` with `{{participant_count}}` and `{{participant_list_with_display_names}}` filled from live `SessionConfig`.
* `participant_priming.md` at repo root is the authoritative participant template. Same embedding/seed path, key `prompt_participant_priming`, rendered in `session_runner.rs` with `{{leader_display_name}}`, `{{participant_count}}`, `{{full_participant_list_including_leader}}` filled from live config. Header before `\n---\n` is stripped for injection; body only is injected.
* `agent_system.md` at repo root is the authoritative Agent Brain system prompt. Embedded via `include_str!("../../agent_system.md")` in `settings_store.rs`, seeded into `brain_system_prompt`, returned via `get_prompt_template` and `get_agent_brain_config` with hardened defaults when DB is empty, and concatenated with `DECISION_JSON_CONTRACT` inside `AgentBrain::build_effective_system_prompt`.

No duplicate copies under `src-tauri/prompts/` or elsewhere. `include_str!` path is compile-time checked; `cargo check` fails if the file moves.

---

## 2. AgentDecision Contract Verified — 7 Actions

Rust enum `agent_brain.rs:58` is authoritative:

```rust
pub enum AgentDecision {
    Route { target_model: String, prompt: String },
    Blueprint { section_title: String, section_content: String },
    Continue,
    Complete,
    RouteCompare { models: Vec<String>, prompt: String },
    AskUser { question: String, options: Vec<String>, allow_custom: bool },
    Hackathon { task_brief: String },
}
```

* `rename_all = "snake_case"` — `RouteCompare` → `route_compare`, `AskUser` → `ask_user`.
* `Continue` and `Complete` are unit variants — JSON must be exactly `{"action":"continue"}` / `{"action":"complete"}` with no other fields. Prompt hardening explicitly states this; prior agent_system incorrectly described `continue` with a prompt field — fixed.
* `Hackathon` carries only `task_brief` (1-2000 chars, non-empty, validated in `response_router.rs` Hackathon arm and `hackathon::execute_hackathon`). Prior contract said 1-500 — corrected to 1-2000.
* `AskUser` requires `question` + `options`(2-4) + `allow_custom:true`; frontend handles Skip as `Cancelled`, never as an option.
* `response_router.rs` exhaustive match covers all 7 arms (no wildcard), `decision_action()` returns `"hackathon"` for telemetry, `DECISION_JSON_CONTRACT` lists 7 actions with correct example, and `hackathon_result` is delimited `=== Hackathon Results ===` advisory text injected via `inject_active_prompt` for the next leader turn.

Parser: `extract_json_object` → `serde_json::from_str::<AgentDecision>`. Unknown action or missing required field → `Err` → `agent_brain_decision_failed` → safe fallback (`Route(deepseek)` / `Blueprint` / `Continue` bounded by `unclassified_count`), never panic.

---

## 3. Runtime State Is Authoritative — Added to All Three Prompts

* **leader_priming.md** new section "Runtime state is authoritative — do not invent it" lists 10 forbidden inventions (participant identity/count, cycle count, module boundary, previous reviewer, guaranteed-pass status, Phase 1 closed, Hackathon active, checkpoint, paused/resuming). Rule: use supplied value, else treat as unknown, never fabricate.
* **participant_priming.md** new section "Runtime context is authoritative" lists 7 forbidden inventions (roster, leader identity, review order, module number, cycle count, prior review, disagreement resolved). Same rule.
* **agent_system.md** new section "Authoritative context — what you may classify from" restricts classification to leader message + supplied session/process context + reasoning log + module state, with explicit 7-item do-not-invent list.

---

## 4. Leader / Participant Review Rules Hardened

* **Leader module completion** is now deterministic: ready for `blueprint` only when guaranteed pass done, all material objections incorporated or recorded as dissent, no unresolved contradiction vs earlier sections, leader has synthesized final design, content is junior-dev implementable, no pending Hackathon. Clean pass ("nothing material to raise") is valid — no fake objections, no endless loop.
* **Leader global completion** strengthened: `complete` means ENTIRE blueprint (all modules, product ambiguity closed, dissent recorded, no pending Hackathon, no remaining mandatory pass). Not one module/section/tiredness.
* **Leader Hackathon discipline**: trigger only when 6 conditions all true (product clear/Phase1 closed, bounded, ≥2 credible approaches, discussion unlikely to resolve, real implementation evidence needed, comparable report structure defined). Forbidden: difficult alone, want opinion, too slow, unsure, Route inconvenient. Brief must contain all three labeled sections within one `task_brief` string (1-2000 chars) or backend rejects.
* **Leader Hackathon result interpretation**: explicitly evidence, not verdict — must evaluate 8 dimensions (correctness, integration fit, constraints, failure modes, maintainability, security, resource usage, compatibility).
* **Leader no hidden decisions**: distinguished independent reasoning (before exposure) vs mandatory exposure for consequential decisions vs trivial/verifiable facts that need no cycle.
* **Participant research honesty**: conditional — if genuine external research tool is available use it, otherwise do not pretend to have searched; reason from evidence and distinguish known vs inferred. Fabricated "I searched GitHub" is forbidden.
* **Participant Hackathon evaluation** and **bounded disagreement** preserved with strengthened language; "Nothing material to raise" is preferred; one bounded pushback then accept/record, no relitigation.
* **Agent system roster restriction**: runtime Context roster is authoritative; may route only to valid currently available participant IDs; example list is labeled as example.
* **Agent system Continue/Complete/Blueprint/Hackathon** rules match Rust contract exactly, including `continue` no-field fix and hackathon brief structure requirement.

---

## 5. Prompt / Runtime Contract Verified

* Placeholder replacement verified: `session_runner.rs::run_setup` now loads hardened templates from `settings_store` (with embedded fallback) and substitutes `{{participant_count}}`, `{{participant_list_with_display_names}}`, `{{leader_display_name}}`, `{{full_participant_list_including_leader}}`, `{{role}}` from live `SessionConfig` + `display_name_for`. Generic `format_display_list` handles comma/and correctly. `format_display_list` and template loading are covered by `cargo check`.
* `agent_system.md` JSON contract verified against `AgentDecision` serde tag and `response_router.rs` field expectations. No extra fields per action; malformed shape falls back safely.
* Flows traced:
  * A ordinary review `Route → participant → leader → next participant → Blueprint` — sequential, reasoning log preserved, guaranteed pass enforced.
  * B independent comparison `RouteCompare → multiple participants → leader` — only for genuine forks, not module loop.
  * C product ambiguity `AskUser → user answer (or Cancelled) → orchestration continues` — oneshot not hung, technical uncertainty routes to panel.
  * D Hackathon `Leader → AgentBrain Hackathon → execute_hackathon → delimited report → leader context → next AgentBrain decision` — 1-2000 validation, advisory, no auto-blueprint, cancellation/pause coherent.
  * E malformed decision → `agent_brain_decision_failed` → fallback Route/Blueprint/Continue bounded, not panic.
  * F global completion → final module Blueprint → remaining modules verified → Complete (with pending Hackathon and mandatory pass checks).
* Each flow was mental-traced against actual `response_router.rs` arms and `hackathon.rs` `execute_hackathon`; no prompt instructs an unconsumable JSON shape.

---

## 6. Constraints Preserved

* `READINESS_TIMEOUT_MS = 90_000` (browser_backend.rs, browser_harness.rs, GENERIC_INIT_SCRIPT, tests) — not changed; verified via `grep -rn 90_000`.
* `READINESS_WAIT_TIMEOUT_SECS = 100` — not changed.
* `Kimi` domain remains `https://www.kimi.com/` in `browser_backend.rs`, `browser_harness.rs`, `useAppStore.ts`, tests — not changed to `kimi.ai`; `grep -rn kimi.ai` returns 0.
* No WebView architecture change (still max 2 WebViews), no paid API introduction, no IPC name change, no dependency added, no `blocking_lock()` or `on_navigation` violation, no `.unwrap()`/`.expect()` in live paths, `GENERIC_INIT_SCRIPT` remains static generic with `window.__ca_agentId`.

---

## 7. Verification

* `cargo fmt --check` — PASS (after auto fmt if needed)
* `cargo check --offline` — PASS (0 errors, warnings triaged)
* `cargo test --offline` — PASS (120 passed previously; re-run to confirm no prompt-seed regression)
* `npm run build` — PASS (1710 modules)
* `git diff --check` — PASS (0 whitespace errors)
* `grep -Rni "blocking_lock" src-tauri/src` — 0
* Hardened prompts compile-time embedded: `cargo check` fails if any of the three files is missing or path wrong.

---

## 8. Remaining Limitations

* Template body extraction relies on `\n---\n` delimiter; a file without that delimiter falls back to full content — still valid but includes header in model prompt. Current hardened files contain the delimiter, so not an issue.
* Fresh-install seeding happens in `SettingsStore::new` via direct `set`; a concurrently opening second process could race seed writes but SQLite `INSERT OR REPLACE` is atomic and idempotent — no corruption, last write wins with same content.
* `session_runner.rs` placeholder rendering is straightforward string replacement; a future template containing an unknown `{{unknown}}` placeholder would be left verbatim — not a security issue but could confuse the model. Current templates only use the five known placeholders.
