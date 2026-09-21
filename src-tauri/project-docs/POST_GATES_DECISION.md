# Consensus Arena — Post-Gates Research Decision (Condensed)

**Decision date:** 2026-09-14
**Accepted direction:** Thin Arena product authority + reusable durable workflow mechanics + bounded DSH workers + independent deterministic verification.

## Cross-gate conclusion

The seven substrate gates demonstrated that practical autonomous implementation is available, but product authority should not depend on the worker's internal interaction/session protocol.

A valid product workflow can terminate/restart workers as long as it preserves:

- repository/candidate state;
- task/work-order identity;
- owner decisions;
- acceptance contract;
- attempt/evidence history.

Two principles remain non-negotiable:

1. permission before protected action;
2. verification independent from implementation narration.

## Recommended responsibility split

- **Arena:** product intent, adopted decisions, priorities, milestones/progression policy, acceptance policy, evidence correlation, owner judgment.
- **Dagu candidate:** durable run/wait/retry/history and root human-task mechanics.
- **DSH:** bounded planning/coding/debug/repair worker.
- **Git:** worktree/candidate identity.
- **Project-native runners:** executable verification.
- **Browser/native observation tools:** evidence production, only when appropriate.
- **Deployment/CI:** existing provider/project tooling.

## Work-order contract direction

Conceptual work order:

`task_id, attempt_id, base_revision, workspace, intent/decision_version, acceptance_contract_version, permitted_scope, time/repair_budget`

Conceptual worker result:

`CANDIDATE | BLOCKED | FAILED` plus revision/files/evidence references/structured blocker.

Conceptual verifier result:

`PASS | FAIL | INCONCLUSIVE` plus candidate identity, contract/test identity, required scenario outcomes, evidence locations.

Malformed/missing result is failure/inconclusive, never inferred success from prose.

## Human continuation model

Bounded worker can emit a blocker and terminate. Durable root human task collects the owner answer. A fresh worker receives the recorded answer and existing workspace state.

If a second clarification exceeds the bounded root workflow, create a linked continuation work order rather than building an unbounded nested agent-question engine.

## Acceptance model

PASS requires:

- identified candidate actually ran;
- every required scenario executed;
- required scenario not silently skipped/removed;
- evidence maps to candidate + frozen contract.

Infrastructure/missing evidence => INCONCLUSIVE.
Valid scenario exposes wrong behavior => FAIL.
Owner waiver remains a waiver, not PASS.

## Dagu decision

Dagu is the next workflow candidate because it may remove deceptive durable-mechanics work from Arena while composing directly with DSH.

It is **not implemented** in current Delivery V1. Validate before integration.

## 2–3 day interpretation

The intended standard is that the **core architectural transformation** should be surprisingly small because commodity capability is reused. It is not a promise that every generalized deployment/platform feature ships in 72 hours.

If a narrow slice starts requiring weeks of scheduler/queue/workflow infrastructure, stop and reconsider the boundary.
