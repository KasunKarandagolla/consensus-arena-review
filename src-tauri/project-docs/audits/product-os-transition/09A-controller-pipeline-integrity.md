# M09A — Controller + Pipeline Integrity Audit

Date: 2026-09-20
Branch: `codex/arena-dev-temp`
Baseline resolved: `660380eff8dbc33a51e3109bc14af0bbdd091edf`

This audit covers the controller layer only. Browser transport, packaging,
native GUI proof, and other capability work remain outside M09A and are
explicitly not claimed here.

## Status vocabulary

- **DOCUMENTED** — the contract is recorded in the current project docs.
- **SOURCE-CONFIRMED** — the current Rust/TypeScript source implements the
  boundary described here.
- **INTEGRATED** — the controller uses the boundary in the production path.
- **RUNTIME-PROVEN** — a deterministic test or bounded runtime exercise
  executed it.
- **ENVIRONMENT-BLOCKED** — proof requires an external capability that was not
  part of this session.
- **REJECTED/NOT REQUIRED** — intentionally outside this session or rejected
  by the authority model.

## Route and stage model — INTEGRATED / SOURCE-CONFIRMED

`src-tauri/src/pipeline_contract.rs` defines the small Arena-owned contract.
Bounded intent markers select `NewProduct`, `ExistingFeature`, or `Incident`.
Arena persists the selected route, current stage, and an explicit reason for
each omitted stage in `ProductCoordinatorRun`.

- New product: `Discover → Decide → Deliver → Release`.
- Existing feature: `Decide → Deliver → Release`; broad discovery is omitted
  with a persisted reason.
- Incident: `ReproduceDiagnose → Decide → Deliver → Release`; discovery is
  omitted with a persisted diagnosis-first reason.
- `Terminal` is used for Stop/Pivot and other bounded terminal outcomes.

The incident diagnosis stage is a controller entry point only in M09A. The
browser/reproduction capability is M09B scope.

## Resource and execution model — INTEGRATED / RUNTIME-PROVEN

`SessionRuntime` remains the top-level live ownership authority. M09A adds the
small `ResourceScheduler` in `pipeline_contract.rs` as a subordinate serial
claim layer. Claims contain a work-order ID, execution epoch, and resource
class. `run_scheduled_role` claims the exclusive semantic-role slot before
dispatch and releases the exact claim after completion. A stale or non-owner
release fails closed.

Architect A/B and reviewer work orders remain separate IDs and are admitted in
deterministic serial order. This preserves independent proposals without
creating an A/B `SessionRuntime` collision or weakening low-memory limits.
Execution epochs and persisted remediation counters prevent stale work from
being silently admitted or retried forever.

The controller/resource proof fixture coupled `ResourceScheduler` with the
real `SessionRuntime`: a second claim and second session admission were both
rejected until the first permit/claim were released. No heavy LSP or browser
concurrency was introduced.

## Immutable inputs and architecture synthesis — INTEGRATED / RUNTIME-PROVEN

Every semantic work order receives a non-empty `ResolvedInputManifest` before
execution; preflight validates delivered content rather than accepting an
unresolved reference. Architecture review uses an
`ArchitectureReviewPacket` containing both proposal IDs and full proposal
content. The packet hash and manifest bind the reviewer/chief inputs to the
project revision.

The Chief Engineer is a real Arena role with typed `ArchitectureSynthesis`.
The output must select A, B, or Hybrid, disposition reviewer findings, list
risky assumptions and experiment need, state an owner-level tradeoff, and
provide project-specific `REUSE`, `WRAP`, `ADAPT`, `COMPOSE`, or `BUILD`
proof. `BUILD` requires alternatives and evidence. Arena validates packet
identity and adopts the result; there is no vote and no worker authority over
ProductAuthorityRecords, acceptance, verification, or Apply.

## Research verification policy — INTEGRATED / RUNTIME-PROVEN

For ProblemResearch and Positioning, Arena-selected `decision_impact` claims
are the required set when any are present. Every such claim must be current,
independently verified, source-bound, verifier-bound, and contradiction-free.
Incidental verified claims cannot satisfy an unverified decision-critical
claim. If no critical claim is selected, the existing bounded verified-claim
path remains in force; incidental facts are not promoted to criticality by a
worker.

## Owner decisions — INTEGRATED / SOURCE-CONFIRMED

Backend `OwnerDecisionKind` now distinguishes:

- `AuthorizeValidationExperiment`
- `AuthorizeNarrowBuild`
- `AuthorizeBuild`
- `StopRun`
- `PivotRun`
- `ApproveApply`
- `ApproveRelease`

Validation experiments return to `Decide`; they do not enter Delivery.
Stop/Pivot are distinct typed terminal outcomes. The existing frontend owner
question maps validation, narrow-build, Stop, and Pivot to the canonical
backend option strings, with no visual redesign.

## Gate remediation — INTEGRATED / RUNTIME-PROVEN

`route_gate_remediation` maps a non-PASS gate to a typed bounded outcome such
as `NeedsResearch`, `NeedsArchitectureRevision`, `NeedsRepair`,
`NeedsExperiment`, `NeedsOwnerDecision`, `RecommendStop`, or
`RecommendPivot`. The coordinator persists the outcome, reason, attempt, and
per-gate counter, with a maximum of two remediation attempts. Ordinary missing
evidence is therefore not silently converted into a terminal failure, while
repeated failure cannot loop indefinitely.

## Pipeline integrity fixtures — RUNTIME-PROVEN

The focused controller fixtures cover:

- route selection and explicit omitted-stage reasons;
- weak-product Stop/Pivot before Delivery;
- ExistingFeature skipping broad discovery;
- Incident entering ReproduceDiagnose;
- validation-experiment not mapping to NarrowBuild;
- A/B exclusive-resource collision and exact release;
- reviewer packet rejection when proposal B/content is missing;
- packet-bound synthesis and project-specific reuse validation;
- generic hard-coded reuse rejection;
- decision-critical research verification;
- bounded remediation for every non-PASS gate;
- pure snapshots and explicit restart reconciliation;
- stale manifest/packet rejection and execution-epoch persistence.

The source-level negative fixtures protect the following adversarial cases:
worker-supplied stronger profiles, missing semantic input content, generic
reuse masquerading as project evidence, skill/tool output redefining
acceptance, stale execution state, and read paths mutating persisted state.

## Verification results — RUNTIME-PROVEN

Successful low-memory test command:

```bash
cd /home/kasun/Music/arena/consensus-arena/src-tauri
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 \
RUSTFLAGS='-C debuginfo=0 -C linker=gcc -C link-arg=-Wl,--reduce-memory-overheads' \
cargo test --locked --bin consensus-arena <focused-filter>
```

The pipeline-contract focused run passed 9 tests with maximum RSS
`1,526,488 KiB` (about `1.46 GiB`). This is the Rust test-link process, not
normal Arena runtime. Product OS gate/authority tests passed 15/15;
evidence-gate tests passed 13/13; Product OS runtime tests passed 13/13 with
2 pre-existing live-model tests intentionally ignored; coordinator tests
passed 3/3 with one pre-existing live-model test ignored.

The historical unbounded/failed focused harness reached approximately
`2.06 GiB`; it is retained as a test-link resource observation, not a Product
OS runtime-performance claim. The supported developer/CI policy is one Cargo
job, stripped test debuginfo, GCC, and the linker memory-reduction option.

## Reused or skipped evidence

The following prior proofs were not rerun because M09A did not change their
paths, as required by the verification economy:

- Context7 and LSP qualification;
- Playwright Test/MCP and native browser transport;
- Safe Apply full matrix;
- package/Windows capability tests;
- repository-intelligence cached/HEAD-invalidation proof;
- model-backed OpenCode/Superpowers runtime tasks.

They remain prior evidence or their recorded environment status. M09A uses
fake-role controller traces and deterministic local checks; it does not claim
new live-model effectiveness.

The repository did not contain a separate local file named Pipeline Contract
v1 or the referenced Astra pipeline consultation during reconnaissance. The
current implementation therefore follows the source-confirmed contracts in
`product_os.rs`, `product_os_runtime.rs`, `product_os_coordinator.rs`,
`session_runtime.rs`, and `evidence_gates.rs`.

## Authority disposition

**INTEGRATED:** routes, persisted stage/status, manifests, architecture
packet/synthesis, serial resource admission, typed owner decisions, research
criticality policy, bounded gate remediation, pure snapshots, and controller
fixtures.

**RUNTIME-PROVEN:** focused controller/authority/gate/runtime tests and the
low-memory serial scheduling proof above.

**ENVIRONMENT-BLOCKED:** no new browser, package, native GUI, or live-model
proof was required or attempted in M09A.

**REJECTED/NOT REQUIRED:** workflow-engine replacement, full AgentSys or
global skill orchestration, worker-selected profiles, a second task authority,
browser/tool checks, and any change to Product OS, verifier, or Apply authority.
