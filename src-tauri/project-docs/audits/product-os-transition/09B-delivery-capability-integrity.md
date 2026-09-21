# M09B Delivery and Capability Integrity

Status: source-correction checkpoint complete; bounded M09B capability proof complete

Baseline: `3e88fe123c90fbf00daa0f82a7317bd1d8e27b79`

This audit preserves the M09A audit as historical evidence. It records the
independent post-push source review and the M09B corrections without changing
the accepted controller authority model.

## Post-M09A independent source review

### Historical M09A claim

M09A documented packet-bound architecture review, bounded remediation,
decision-critical research verification, explicit experiment return to Decide,
and route-aware controller semantics.

### Source-confirmed gaps found

- Dissent was still dispatched from the generic brief and was absent from Chief
  Engineer synthesis input.
- Packet hashing retained proposal prose while typed assumptions, reuse choices,
  interfaces, and risks were only summary text.
- Architecture admission still ran a generic `git diff --check` feasibility
  spike and required risk evidence even when synthesis said no experiment.
- Discovery verification still used `completed.evidence_ids.take(1)`.
- Research, repair, experiment, recommendation, and lifecycle transitions did
  not all perform or persist a concrete next action.
- Route classification used broad `fix`/`change` markers, and incident and
  existing-feature paths shared NewProduct-like behavior.

### M09B correction status

The current source now binds Dissent and Chief Engineer to the complete packet,
adds typed `ExperimentContract` PASS/FAIL/INCONCLUSIVE conditions, removes the
unconditional architecture hygiene spike, selects bounded decision-critical
claims for verification, records decision questions on evidence, distinguishes
terminal/suspended lifecycle statuses, and fences the legacy DSH path from new
Build admissions. Focused tests cover route predicates, packet detail hashing,
contract bounds, profile restrictions, tool receipts, and advisory authority.

The follow-up source correction also makes the conditional established-pattern
architecture path real: it creates one typed proposal packet, omits the second
architect, and admits `ArchitectureCompetitionMode::EstablishedPattern` only
when the deterministic predicate matches. Competing proposals retain the full
A/B packet and packet-bound Dissent path. Owner-authorized validation now
persists and executes its `ExperimentContract` before returning to Decide.

## Evidence classification

### INTEGRATED / SOURCE-CONFIRMED

- Arena-owned role-to-profile selection remains authoritative; workers cannot
  select a stronger profile.
- Complete typed architecture packets and packet-bound Dissent are integrated.
- All Chief Engineer reuse decisions are admitted; duplicate capabilities
  supersede the prior current decision.
- `ExperimentContract` is required only when synthesis requests an experiment;
  no-experiment synthesis records a bounded reason.
- Existing-feature and incident route predicates require high-confidence
  context; ambiguous greenfield ideas default to NewProduct.
- New Build starts reject the legacy DSH fallback and require the qualified
  OpenCode runtime. Existing persisted DSH records remain deserializable for
  safe historical recovery.
- BrowserQa has an Arena-selected profile with pinned Playwright MCP config;
  exploratory `ToolUseReceipt` records are advisory only.
- Semantic candidate review remains exact-SHA/acceptance-bound and cannot mark
  Verified or Apply.

### RUNTIME-PROVEN

- Low-memory test command: `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'
  cargo test --locked --bin consensus-arena pipeline_contract::tests --no-run`.
  The successful test-link peak was approximately 1.61 GiB RSS (1,688,200 KiB
  observed) on this run; the strategy completed in 6m45s. This is test-link
  memory, not normal Arena runtime memory.
- Contract tests: 13 passed, 0 failed.
- Controller/profile/authority tests: 52 passed, 0 failed, 2 ignored because
  they require the authorized external OpenCode Zen path.
- Delivery regression tests: 14 passed, 0 failed, 1 ignored because it
  requires an approved external DSH credential/runtime. The DSH path is not
  admitted for new V1 launches.
- `cargo check --locked`: passed.
- `npm run build` in `/home/kasun/Music/arena/consensus-arena/src`: passed.
- Playwright MCP `@playwright/mcp@0.0.82`: `--help` completed and a bounded
  headless isolated server start remained alive until the 15-second probe
  timeout; the exact process tree was empty afterward. This proves server
  startup/cleanup, not a model-mediated BrowserQa action.
- ToolUseReceipt bounded/advisory behavior and candidate-review exact-SHA
  resilience are covered by the focused tests above.

### ENVIRONMENT-BLOCKED

- Live model-backed architecture/Delivery dogfood and ExistingFeature model
  execution remain blocked by the current OpenCode free-tier/authorized
  provider runtime state; the focused live tests are explicitly ignored by
  their existing credential-gated annotations. No PASS or Verified claim is
  made from those paths.
- Full MCP tool interaction through an Arena/OpenCode BrowserQa model was not
  claimed from the standalone server-start probe. The server qualification is
  proven; model-mediated interaction requires the same blocked authorized
  model path.

### REJECTED/NOT REQUIRED

- Full AgentSys orchestration, global Superpowers bootstrap, Sentry MCP, DSH as
  active V1 fallback, and Dagu are not required by M09B and do not own Arena
  coordination or authority.

## Verification economy

Unchanged Q1/08A Context7, LSP, Superpowers, Playwright standalone,
package/install, Windows, and Safe Apply matrices are reused rather than
rerun. M09B reruns only the focused controller corrections, one bounded
Delivery/capability proof where wiring changed, and the Tier-3 exit commands.

No browser visual redesign, package build, or unrelated platform matrix was
run in this controller/capability checkpoint.
