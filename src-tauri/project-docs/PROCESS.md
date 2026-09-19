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

For research/reuse milestones, record each decision-critical claim with its
source, version/date/scope, origin, contradictory references, verification
status, revisit trigger, and decision impact. Distinguish documented upstream
capability from source confirmation and runtime proof. A provider-neutral
architecture claim must not be upgraded to cross-provider runtime evidence
without a bounded real task.

The 05A research path is intentionally narrow: direct read-only official
source retrieval may create an Arena evidence proposal, but it cannot mark the
claim independently verified. A separate Arena verifier records the source
identity and verification disposition. GitHub MCP, browser/search MCP, and
upstream skill procedures remain optional integrations until their actual
runtime and permission boundaries are qualified.

Milestone 05B makes that proposal path durable without adding a workflow
engine. Admit research and fact-verification through the existing
`SessionRuntime` identity and persist the current Product OS snapshot and
work-order metadata in `TranscriptStore`. Verify that a proposal is still
current before adopting it; after reopen, reconcile pending work rather than
assuming completion. Treat ambiguity classification as Arena authority and
require the current owner decision for the exact question/revision.

The Linux dogfood uses the official GitHub API directly and records the source
scope without claiming GitHub MCP. M05C proves proposal → independent verifier
→ reopened truth → typed product scope/review/reuse/architecture admission →
owner-adopted direction → current Build Package → applicable
pre-implementation gates. A material scope change must invalidate the old
package rather than carry its gate result forward.

M05D now qualifies a permitted, requalified autonomous web-search runtime for
bounded Linux WebDiscovery. OpenCode 1.18.31 with Muse Spark executed real
websearch through the hosted Exa path; Arena admitted structured results as
Unverified and required a distinct FactVerifier work order for final
disposition. Do not manually relay browser/chat research into Product OS
records. Windows, packaging, broad provider portability, and final founder
dogfood remain separate evidence requirements.

## Verification baseline

Typical checks:

```bash
cd src-tauri && cargo check
npm run build
# plus task-specific tests
# plus git diff --check
```

Use real project paths/source layout rather than assuming the historical command examples are exact.

The 2026-09-16 bounded DSH gate established two matching clean installs, but
Arena's exact headless capability probe timed out in both. Stop DSH setup
recovery at that point. Do not start the Muse tasks or substitute another
worker before the required Astra Trigger A consultation. The current evidence
and question are preserved in
`audits/dsh-runtime-reproducibility/dsh-runtime-reproducibility.md`.

For any future worker qualification, a model response or HTTP success is not
worker proof: each required disposable task must make the repository change,
pass deterministic verification, emit the structured receipt consumed by
Arena, and terminate cleanly. Keep the work-order, acceptance, verifier, and
Apply contracts stable while selecting the implementation worker.

## Product-change discipline

Before building a new subsystem ask:

1. Is this genuinely Arena's unique product responsibility?
2. Does a mature reusable tool already solve it?
3. Is there a higher-level reusable composition that removes more work?
4. Does the dependency eliminate more complexity than it adds?
5. Is the proposed integration boundary actually designed/supported?

Do not protect sunk-cost code. Do not aggregate tools for their own sake.

## Evidence-gate discipline

For Product OS work, represent readiness as one current Arena-owned evidence
package and evaluate the applicable gate deterministically. A package
revision or evidence item that is stale is not eligible for a pass. Owner
decisions remain explicit, research outcomes remain bounded decisions rather
than market-validation claims, and implementation evidence must name the
candidate revision independently verified. Gate predicates do not replace
Delivery admission, SessionRuntime lifecycle authority, or Safe Apply.
The production package is assembled from current Arena-owned records through
the existing Delivery state; worker and renderer summaries are proposals, not
authority inputs. A candidate worktree is non-authoritative and is not an OS
or security sandbox.

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

Dagu's standalone closure is **INCONCLUSIVE — post-V1**. Linux hard
interruption required explicit stale-run reconciliation and demonstrated
at-least-once side effects. DSH composition and Windows execution remain
unproven. Finish only the currently authorized hard-interruption and native
Windows standalone evidence gates, then record the final verdict. Do not
integrate Dagu or broaden its V1 qualification scope. The supported mechanics
and exact failure evidence are recorded in
`audits/dagu-runtime-closure-2026-09-16.md`.

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

The inconclusive Dagu result does not trigger Astra Trigger B. The DSH gate
does independently trigger Astra Trigger A; use only the targeted packet in
the DSH reproducibility audit. Do not spend further sessions attempting to
recover the historic lock or testing Dagu while the worker decision is open.


## Audit storage

Permanent audits belong under:

`src-tauri/project-docs/audits/`

Never delete them merely because a newer audit exists. New docs may supersede their conclusions, but the evidence history remains valuable.

## When to update docs

Update only affected docs after a verified milestone. Do not update status from an implementation claim that has not been checked.

If source contradicts docs, correct the docs in the same milestone when practical.
