# Milestone 05 — Delivery/team authority integration

Date: 2026-09-18

## Scope

This checkpoint binds the Product OS evidence-gate contract to the existing
persisted `DeliveryState` and the existing OpenCode/SessionRuntime/Delivery
path. It does not add a database, workflow engine, verifier, worktree
manager, or provider framework.

## Implemented boundary

`product_os::assemble_build_package` is the Arena-owned assembler. It accepts
current Product OS records, validates current evidence references, adopted
owner decisions, owner-required ambiguities, reuse evidence, and the complete
architecture reference set, then derives a secret-free `BuildPackage` and
fingerprint. The pure `evidence_gates::evaluate` function remains the
predicate; workers, renderers, and consultation cannot supply an authoritative
passing summary.

`delivery::bind_build_package` is the handoff seam. It requires current
Architecture and Build Readiness decisions before storing the package and its
authority records in the existing persisted `DeliveryState`. OpenCode
admission rechecks the package fingerprint and those gates and binds package
identity to `OpenCodeWorkOrder`. The owner UI exposes only a calm package-bound
status; low-level identifiers remain technical state.

The candidate worktree remains a non-authoritative candidate working
directory. It is not an OS or security sandbox. Arena still owns protected
acceptance, candidate commits, independent verification, and Safe Apply.

## Proven by source/tests

- Build readiness is derived from required records and a `NarrowBuild`
  decision, not a free-form `build_ready` claim.
- Owner-required ambiguities require a current adopted owner-authority decision
  ID; stale, cancelled, superseded, or technical records do not satisfy it.
- Architecture readiness references two proposals, reuse and constraint
  reviews, risk experiments, red-team evidence, dissent, and blocker records.
- A changed project/evidence fingerprint makes an old package stale.
- Runtime completion without an independent receipt cannot pass the
  Implementation gate.
- DSH protected/canonical acceptance inspection now rejects and discards the
  candidate before an Arena-created candidate commit.
- OpenCode protected-state rejection and late/cancelled-result behavior remain
  covered by the M02 qualification tests; receipt ingestion additionally
  rejects empty verification IDs and changed candidate trees.
- The OpenCode Safe Apply path requires a present candidate worktree, unchanged
  candidate HEAD, and a current matching independent PASS receipt, including
  acceptance identity.
- The real OpenCode M05 exercise was executed on Linux with
  `opencode/muse-spark-1.2-contributor-free`: two bounded advisory roles ran
  in parallel on disposable candidates, returned distinct correlated session
  identities/evidence, and a separate integrator candidate reached the
  independent verifier. The test remains ignored by default because it
  requires the installed OpenCode Zen account.

The adversarial role attempted to write `acceptance.txt`; Arena left the
canonical acceptance bytes unchanged and left the attack candidate at its
base commit. The integrator task changed only `greet.py` from `before` to
`after`; the frozen Python check passed and the work order reached Verified.
The parallel roles wrote only disposable `plan-a.md` and `review-b.md`
artifacts, so they could not self-approve or alter the integrator candidate.
The timed Linux run took 190.20 seconds wall-clock and reached 526,352 KiB
maximum resident set size for the test process. This is an observation, not a
new acceptance threshold; the earlier M01/M02 observations remain historical
comparators.

## Required adversarial matrix

The package tests cover forged architecture summaries without references,
stale owner decisions, changed package identity, and runtime-only completion.
Existing Delivery/OpenCode tests cover protected acceptance mutation,
cancellation/late result rejection, verifier failure/repair behavior, and
post-verification candidate changes. The remaining full-process races and
cross-platform containment limits are not claimed as closed by this
checkpoint.

## Not proven

- Windows execution, Windows OpenCode launch, and packaged builds.
- Production-scale performance or multi-project concurrency.
- Hostile same-user filesystem isolation: Git worktrees and process cleanup
  do not provide that security property.
- A durable external child-task scheduler or native OpenCode child
  orchestration integrated into SessionRuntime. The M05 exercise uses
  bounded parallel disposable roles and one Arena-owned integrator without
  adding a scheduler.
- GitHub MCP, pinned ECC/gstack procedures, external consultation execution,
  broad provider portability, and real M06 research dogfood.
- Full compare-and-swap persistence and crash-window reconciliation for every
  Delivery command; those remain follow-up reliability work.

## Decision

The authority composition is suitable for the bounded Linux candidate path
behind Arena-owned Build Package admission and existing Delivery gates. This
is not a blanket OpenCode, Windows, packaging, hostile-worker, or provider
portability qualification.
