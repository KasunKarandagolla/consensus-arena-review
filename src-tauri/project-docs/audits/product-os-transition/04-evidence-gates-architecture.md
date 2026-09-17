# Milestone 04 — Evidence Gates, Architecture Boundary, and Founder Journey

**Date:** 2026-09-17
**Scope:** source-level evidence gates and founder-facing journey summary after
the Milestone 03 consultation/reuse work.
**Status:** bounded implementation complete; full architecture-competition
closure remains limited by the independent-review wave recorded below.

## Preflight closure

The mandatory preflight was closed before the M04 implementation:

- `ConsultationRequest::budget_tokens` now reaches a consultation-specific
  `max_tokens` request field. Existing Hackathon callers retain the historical
  1024-token default. A focused request-builder regression test proves the
  requested bound is used.
- The consultation seam remains internal to Arena. `origin` and declared
  disclosure are descriptive request metadata, not authorization. Arena-owned
  work-order/session admission, cancellation, supersession, and durable result
  reconciliation are still required before this seam can become a Product OS
  evidence source.
- No provider portability experiment was repeated. OpenCode Zen/Muse Spark is
  the existing bounded Linux execution evidence; external-provider portability
  remains unproven and is not upgraded here.
- GitHub MCP, pinned ECC/gstack procedures, and external consultation
  execution remain unqualified research capabilities. No simulated research
  record is treated as runtime proof.

## Implemented boundary

`src-tauri/src/evidence_gates.rs` defines one serializable contract and one
pure evaluator. The nine gate identifiers are grouped under the existing
Discover / Decide / Deliver / Release phases:

| Phase | Gate | Required evidence/authority |
| --- | --- | --- |
| Discover | Vision | fresh restatement, no invented behavior |
| Discover | Problem research | current evidence and explicit stop/pivot/experiment/narrow-build outcome |
| Discover | Positioning | current evidence and explicit bounded decision |
| Discover | Ambiguity | next irreversible commitment; no unresolved critical/high blocker; medium mitigation and revisit trigger |
| Discover | Reuse | completed reuse scan; BUILD classification has alternative evidence |
| Decide | Architecture | two proposals, constraints/reuse/risk checks, red team, dissent, no unresolved high blocker |
| Decide | Build readiness | accepted product/architecture package is actionable |
| Deliver | Implementation | independent verifier PASS for the exact candidate revision |
| Release | Release | candidate, package, install, security, and QA evidence |

The evaluator fails closed for stale package revisions, stale evidence,
missing required evidence, unrecorded owner decisions, unresolved ambiguity,
incomplete architecture challenge, and verifier evidence tied to another
candidate revision. It does not persist a journey, authorize a worker, replace
`SessionRuntime`, or create a second acceptance/verifier/Safe Apply system.

The implementation is therefore a REUSE/ADAPT boundary, not a new workflow
engine:

- **REUSE:** existing Delivery, SessionRuntime, worktree, protected hashes,
  verifier, Safe Apply, credential, and containment authorities.
- **WRAP/ADAPT:** represent their evidence as one Arena-owned package and
  expose a calm founder summary.
- **DEFER:** durable consultation lifecycle, GitHub MCP execution, ECC/gstack
  qualification, Dagu, and generic team orchestration.
- **BUILD:** only the narrow Arena-specific gate predicate contract, because
  no existing commodity component owns Arena's product/acceptance semantics.

## Architecture competition record

The neutral brief produced two materially different bounded options:

1. **Minimum-complexity/reuse-first (provisional leader).** Keep the existing
   Tauri/Rust authority kernel, add pure evidence predicates over a versioned
   Arena package, and let Delivery/SessionRuntime continue to own lifecycle,
   verification, and Apply. Add no scheduler or durable external team runtime
   until a concrete need is proven.
2. **Future-proof bounded ledger.** Keep the same authority kernel but add an
   append-only project journey/evidence ledger with explicit event identities,
   projection/rebuild, and a narrow adapter port for future team runtimes.
   This improves restart/audit reconstruction but adds schema migration,
   projection, event ordering, and recovery cost before the first dogfood.

The first option is selected provisionally for week-one work because the
current evidence already proves Delivery authority and the gate predicates,
while the second option's durable recovery benefits are not yet required by a
runtime-tested milestone. The strongest argument against the selected option
is that a purely in-memory/package-level contract can be lost or duplicated
across restart until Milestone 05 binds it to Arena-owned lifecycle state.

The independent architect wave was requested in parallel. A reuse-first
review and a future-proof bounded proposal both returned. The independent
red-team review identified one additional invariant: a gate result must carry
and compare an Arena-owned authority/evidence fingerprint, not only a numeric
package revision. `evidence_gates.rs` now fails closed when that fingerprint is
missing or changed. The red team also identified durable lifecycle cases —
claim-by-claim Vision coverage, immutable evidence references, owner-owned
ambiguity severity, compare-and-swap persistence, and stale UI projections —
which remain next-milestone work rather than being hidden in this predicate.

The architecture choice is still provisional: the available evidence supports
the minimum-complexity option, but no claim is made that it is objectively
best. Durable team integration should preserve the future-proof ledger
alternative, red-team dissent, and the revisit trigger before adding
persistence or external consultation execution.

## Founder journey

`src/components/shared/FounderJourney.tsx` adds only a four-step summary to
the existing Delivery view:

```text
Intent → Build → Verify → Apply
```

It uses the existing Delivery phase, marks failure/cancellation as terminal,
and leaves technical identifiers and evidence in the existing details/status
surface. It does not create frontend authority or a second workflow state
machine.

## Verification evidence

Focused Rust tests cover stale package/evidence/fingerprint invalidation, missing/fake
evidence, owner-decision authority, ambiguity, reuse BUILD evidence,
architecture challenge requirements, current-candidate verifier correlation,
and release evidence categories. The consultation budget regression and
existing consultation tests remain green.

The frontend build and full Rust check are required at checkpoint time. This
audit must be amended only with verified command results; it must not imply
Windows, packaging, or runtime qualification that was not run.

## Proven and unproven

**Proven by this milestone's source/tests:**

- bounded consultation output limit is applied without changing unrelated
  Hackathon defaults;
- pure Arena-owned gate contract rejects stale, missing, invented, and
  mismatched evidence;
- owner decisions and independent current-candidate verification remain
  explicit;
- minimal founder journey presentation compiles at the frontend boundary;
- existing OpenCode authority and Delivery ownership remain the documented
  authority model.

**Unproven or intentionally deferred:**

- durable evidence-package persistence and restart reconciliation;
- full Product OS work-order/session binding for consultation;
- GitHub MCP, ECC/gstack, and external consultation execution;
- an independent second architecture proposal and completed architecture red
  team in this session;
- native Windows behavior, packaged build, production-scale performance, and
  GUI-to-Apply dogfood.

The M04 result is therefore a bounded foundation for the next milestone, not
a claim that all team-runtime capabilities are qualified.
