# Consensus Arena — Post-Gates Substrate Research

**Purpose:** Prevent repeated research and preserve why certain attractive architectures were rejected.

## Core diagnosis after seven gates

The repeated mistake was requiring a generic execution substrate to also provide Arena's exact product-control boundary.

The accepted separation is:

1. **Product authority** — Arena.
2. **Durable run/wait/retry mechanics** — reusable workflow engine where useful.
3. **Bounded implementation/debug worker** — reusable coding harness.
4. **Behavioral evidence** — independent deterministic project tools.

A worker may terminate at a blocker. Product continuity lives in repository/task/decision/acceptance state, not necessarily a live model turn.

## DeepSeek Harness (DSH)

### Proven useful

Gate testing proved real bounded autonomous implementation/repair with NVIDIA NIM on the current modest Linux Lite laptop. Resource measurement in the tested configuration was roughly 185 MB RSS during active DSH work.

### Rejected role

DSH is **not** selected as Arena's full durable product substrate. Multiple host-seam experiments failed to cleanly provide the external structured owner-interaction boundary Arena required.

### Current role

**Use narrowly** as the intended bounded coding/debugging/repair worker
contract, but keep hosted model qualification explicit. One earlier
`meta/muse-glimmer-30b` run completed a real DSH coding task and emitted an
Arena-shaped schema-1 receipt; treat that as historical single-run evidence,
not a repeatable baseline. In the 2026-09-16 closure pass the documented DSH
lockfile/runtime could not be reconstructed, so neither required independent
Muse run was attempted and current runtime repeatability remains unproven.
`deepseek-ai/deepseek-v4-flash-0731` previously timed out during bounded
inference and did not complete the worker contract. Neither route currently
qualifies the frozen worker runtime.

See `audits/delivery-dsh-v4-worker-and-e2e.md` for the route, receipt, and
resource evidence.

Do not upgrade the tested DSH version merely to chase features without qualification; upstream moves quickly.

## Morning Star

Initially investigated as DSH workflow layer. Gate work did not establish a sufficiently strong reason to stack its workflow ownership beneath the current recommended architecture.

**Current decision:** Watch / reuse ideas or skills selectively. Do not make it a required core workflow dependency without a specific proven benefit.

## Fusion

Strong higher-level mission/interview architecture, but tested release `0.77.0` constructed the active mission execution loop without the behavioral `verificationCapability` needed for authoritative execution-based acceptance.

**Current decision:** Watch/reference. Not current substrate.

## OpenHands

Gate testing found Arena-critical defects for the required path:

- `ClientToolSpec` could emit structured actions but lacked the supported external result-submission path needed for the host contract;
- NVIDIA NIM worked directly, but OpenHands/LiteLLM emitted unsupported `prompt_cache_key` parameters in the tested configuration;
- repair/reverify/restart/Windows requirements remained unproven.

**Current decision:** Reject for now.

## OpenCode

Credible noninteractive worker and portability fallback. It is not the selected current worker because DSH has already been proven locally for bounded execution.

**Current decision:** Reserve worker fallback, not another architecture to integrate in parallel.

## Pi

Evaluated during substrate exploration but not selected as the current primary architecture. Do not reopen solely because it is lightweight; compare only if the proven DSH bounded-worker contract fails materially.

## Dagu — deferred workflow candidate; standalone gate inconclusive

Why it is interesting:

- local workflow runner;
- dependencies/retries/history;
- no external DB/broker requirement in normal local architecture;
- CLI/REST surfaces;
- native Windows releases;
- human tasks with durable answer/resume semantics;
- existing `harness.run` provider for DeepSeek Harness.

Important discovered semantics:

- human tasks are processless and release the executing attempt;
- root-workflow restriction: do not assume arbitrary nested/repeated human task use;
- repeated ambiguity can become a linked new root work order;
- separate approval feature may run an attached command before waiting — **do not use that as pre-action permission**;
- use a preceding human task for permission-gated external actions;
- duplicate identical human completion is documented as idempotent; conflicting answer should be rejected;
- if completion persists but enqueue/resume fails, explicit resume semantics exist and must be tested;
- GPL/distribution terms require legal/product decision before bundling/embedding; initial validation should use external CLI/REST boundary.

**Standalone gate status (2026-09-16): INCONCLUSIVE.** A Dagu 2.16.6 local
Linux run supported dependency scheduling, retries/history, durable root human
tasks, attempt release while waiting, pending-task discovery after controller
restart, identical-answer idempotency, conflicting-answer rejection, resume,
linked follow-up work orders, and REST/CLI control. It did not prove automatic
reconciliation after a hard interruption during an active run, and two
disposable Dagu→DSH composition attempts stopped before a worker result because
the exact DSH runtime/credential boundary was unavailable. No Arena integration
was attempted. The Windows archive was checksum/PE-inspected but not executed
on Windows.

Measured carrying costs in that local experiment were approximately 80–109 MiB
server RSS, 1.1 MiB fixture state, and a 161 MiB Linux binary. Upstream
documents GPL-3.0-or-later and distinguishes running CLI/server as a separate
program from embedding/linking its Go API; distribution obligations require
legal review before packaging. See
`audits/dagu-standalone-qualification.md`, [official human-task semantics](https://docs.dagu.sh/writing-workflows/human-tasks),
[official installation options](https://docs.dagu.sh/getting-started/installation/),
and [upstream licensing](https://github.com/dagucloud/dagu/blob/main/LICENSING.md).

**Current status:** not implemented and not yet qualified as an Arena
workflow-engine candidate. Do not prepare an integration or broad Astra
consultation from the partial mechanics alone. Revisit only after exact DSH
runtime repeatability and the remaining Dagu composition/recovery tests.

## LangGraph / XState / DBOS

Fallback/reference choices, not parallel dependencies.

- **LangGraph** — strong durable graph/interrupt model; more application-owned graph/checkpoint integration.
- **XState** — small explicit local state-machine fallback if Arena must own a deliberately small controller.
- **DBOS** — attractive durable-workflow model, but TypeScript/PostgreSQL carrying cost is currently unattractive for this low-spec local architecture.

## Factory Missions

Serious higher-level alternative because it provides mission planning/workers/validation/headless execution and separate model lanes. However vendor runtime/access/model compatibility remain less certain for Arena's single-NIM minimum mode.

**Status:** strong alternative reference; do not qualify in parallel with Dagu.

## Verification/tool decisions

- **Web projects:** Playwright Test.
- **Tauri/native desktop:** WebdriverIO Tauri service.
- **Deep browser diagnosis:** Chrome DevTools MCP on demand.
- **Git isolation:** Git worktrees.
- **Sandboxing:** reuse an existing implementation; Anthropic Sandbox Runtime is a candidate but native Windows remains alpha and must be qualified.
- **Procedural knowledge:** portable Agent Skills / worker-native context.
- **Deployment:** existing project/provider tooling, not an Arena deployment platform.

## Research rule

Do not restart broad “best agent framework” research unless the current bounded-worker/workflow/verifier contracts themselves are disproven.

Next research/qualification should be narrow and falsifiable.
