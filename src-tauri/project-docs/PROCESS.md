# Consensus Arena — Development Process

## Core rule

**Read reality before changing it.** Source/runtime evidence outranks docs. Latest permanent audits outrank older status narratives.

## Context-efficient document loading

1. Read `README.md`.
2. Read `DECISIONS.md`.
3. Use the task map in README to open only relevant module docs.
4. Read the actual source files affected by the task completely before editing.
5. Read latest relevant audit.

Do not load the entire project-doc set by default.

## Current execution workflow

Codex CLI remains the primary coding/verification executor.

For a normal task:

1. **Scope** — state the exact behavior/problem.
2. **Pre-audit** — read affected source + relevant docs/audits.
3. **Plan** — smallest coherent change; identify invariants.
4. **Implement** — avoid unrelated refactor.
5. **Verify** — real checks/build/tests/runtime evidence.
6. **Post-audit** — for reliability/architecture-sensitive changes, write a concise permanent audit.
7. **Review** — show diff and result.
8. **Checkpoint** — Git commit only after verification and user approval when approval is reserved.

## Verification baseline

Typical checks:

```bash
cd src-tauri && cargo check
npm run build
# plus task-specific tests
# plus git diff --check
```

Use real project paths/source layout rather than assuming the historical command examples are exact.

For hosted-worker qualification, first establish the installed provider
adapter contract, then run a bounded direct provider sanity check and one
deterministic standalone repository task. A model response or HTTP success is
not worker proof: the task must make the repository change and emit the
structured receipt consumed by Arena. Use one control model only when a
primary route failure leaves DSH-versus-provider ownership ambiguous. Record
route failures and runtime blockers in a new permanent audit; do not change
Arena's default model or start Dagu from incomplete worker evidence.

## Product-change discipline

Before building a new subsystem ask:

1. Is this genuinely Arena's unique product responsibility?
2. Does a mature reusable tool already solve it?
3. Is there a higher-level reusable composition that removes more work?
4. Does the dependency eliminate more complexity than it adds?
5. Is the proposed integration boundary actually designed/supported?

Do not protect sunk-cost code. Do not aggregate tools for their own sake.

## Delivery-specific change process

Any Delivery change must explicitly audit the invariants in `DELIVERY.md` / `RELIABILITY.md`:

- clean base;
- worktree isolation;
- acceptance freeze;
- protected acceptance;
- independent verification;
- same-check re-run;
- bounded attempts;
- durable owner question;
- exact abort;
- no secrets;
- safe Apply;
- cross-platform process/path semantics.

Do not route new Delivery logic through legacy browser `response_router` merely because it already contains orchestration code.

## Consult-specific change process

Read `CONSULTATION.md`, `IPC.md`, `RELIABILITY.md`, and affected browser/session source. Check all named browser risks.

## Dagu validation process

Dagu is pending qualification. Do not call it current architecture until a real integrated slice passes.

The narrow validation should prove:

- durable root human task;
- Arena close/restart while waiting;
- duplicate identical answer is harmless;
- conflicting answer rejected;
- fresh bounded worker continuation after answer;
- linked continuation root for a second blocker;
- frozen scenario FAIL→repair→same scenario PASS;
- interrupted candidate reconciliation without duplicate work;
- skipped/missing required scenario cannot pass;
- malformed worker result cannot pass;
- process cleanup/resource use on Linux Lite;
- later native Windows parity.

If Dagu fails materially, preserve the work-order/worker/verifier contracts and test a small Arena controller instead. Do not restart the entire substrate search.

## Audit storage

Permanent audits belong under:

`src-tauri/project-docs/audits/`

Never delete them merely because a newer audit exists. New docs may supersede their conclusions, but the evidence history remains valuable.

## When to update docs

Update only affected docs after a verified milestone. Do not update status from an implementation claim that has not been checked.

If source contradicts docs, correct the docs in the same milestone when practical.
