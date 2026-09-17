# Consensus Arena — Memory and Product Truth

## Status

Phase 1 memory is implemented in the current codebase. The older `PHASE1_MEMORY_v10_FINAL.md` was an implementation specification and should no longer be treated as the current status document.

Current project context/source exposes persistent concepts including:

- session memory;
- project memory;
- global memory;
- open questions;
- model reliability/strength history;
- pattern memory;
- project configuration/context;
- FTS/search health and repair;
- export/restore with health checks.

The production `MemoryStore` backend is runtime-qualified by the isolated
`memory_store::tests::phase1_runtime_round_trip_and_repair` exercise. That
qualification covers `run_blocking` access, session/project/global writes,
open questions, model reliability aggregation, pattern confidence, context
assembly, FTS search and repair, store reopen persistence, and export/restore
health checks. Native UI wiring, provider-driven writes through a live model
run, and whole-app restart continuity remain unproven.

The 2026-09-17 source follow-up now resolves Consult AskUser memory questions
using the same normalized full question key that creation stores, and filters
credential-like question/answer text before adding it to product memory.
Delivery answers are adopted as owner decisions after the answer is sent,
through `db_helpers::run_blocking`; a source-level test checks adoption and
credential skipping. Memory-write failure remains non-fatal. These paths have
targeted source tests, but neither was run through a live provider/WebView.
Whole-app AppState recreation, process-restart continuity, and re-presentation
of a persisted owner question remain unproven.

Read current `memory_store.rs` before relying on exact table/field schema.

## Product boundary

Arena memory should primarily preserve **product truth and useful continuity**, not become a generic autonomous-agent memory platform.

Arena should own durable information such as:

- product requirements and scope;
- adopted owner decisions;
- open product questions;
- accepted versions/outcomes;
- product-relevant patterns;
- model/provider reliability evidence relevant to routing.

## Delivery decisions

For Build/Delivery:

- the pending delivery question belongs to the delivery/workflow mechanism until answered;
- once resolved, Arena adopts the decision into product truth with stable identity/context;
- do not create two independent pending-question queues that can conflict.

If Dagu is later integrated, Dagu may own the pending human-task mechanics while Arena owns the adopted product decision.

## Worker memory

External coding workers may use their native context/session/skills for bounded execution. Arena should not duplicate that internal conversational state unless it materially supports product continuity.

The post-gates architecture explicitly does **not** require a worker process/session to remain alive across every owner decision.

## Procedural skills

Prefer portable Agent Skills / worker-native procedural knowledge rather than inventing a new Arena skill runtime.

Only move a learned procedure into Arena-owned durable product knowledge if it is actually part of product truth/policy rather than worker technique.

## No generic vector database requirement

There is currently no justified requirement to add a vector database or broad autonomous memory stack. Reuse the implemented SQLite/FTS architecture unless measured needs prove otherwise.
