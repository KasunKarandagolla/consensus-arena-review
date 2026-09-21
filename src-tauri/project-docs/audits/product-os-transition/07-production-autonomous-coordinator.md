# Milestone 07 — Production Autonomous Coordinator

**Date:** 2026-09-19
**Branch:** `codex/arena-dev-temp`
**Predecessor:** `dcbdd9de57c0acb85f8c7cde948a185c4d082a77`

## Scope and verdict

M06 proved the founder-to-candidate component chain in an ignored integration
harness. M07 moves sequencing into the Arena production boundary. The
coordinator is a small deterministic progression controller, not a workflow
engine and not a replacement for `SessionRuntime`, `ProductAuthorityRecords`,
or `DeliveryState`.

**M07 verdict: PRODUCTION AUTONOMY PROVEN for the bounded Linux path exercised
below.** This is not a Windows, packaging, native-GUI, Safe Apply, or hostile
same-user filesystem qualification.

## M06 harness versus M07 production run

| Boundary | M06 | M07 |
|---|---|---|
| Entry | Ignored test harness | `start_product_project` production API |
| Sequencing | Test manually invoked internal stage functions | `product_os_coordinator::run_to_terminal` advances durable phases |
| Research | Harness-created/admitted sequence | Arena-created WebDiscovery and distinct FactVerifier work orders |
| Product review | Test fixture semantics | Real Product Director OpenCode role with strict JSON parsing |
| Architecture | Harness-provided bounded content | Separate Architect A and Architect B OpenCode sessions |
| Package/Delivery | Harness-owned ordering | Current gates → `admit_build_package` → existing Delivery |
| Owner | Harness helper | Persisted WaitingForOwner plus `answer_product_question` |

## Runtime-proven production dogfood

The fresh internal validation idea was to safely change a tiny `greet_tool.py`
so `greet()` returns `ready`, keeping the slice local, reversible,
independently testable, and explicitly not market validation or a broader
product commitment.

The production test invoked only the coordinator start/status boundary and the
owner direction answer. It did not copy research, select libraries, order
roles, create a package, create DeliveryState, or relay worker output.

Observed result from the successful run:

- OpenCode 1.18.31 on Linux;
- model `opencode/muse-spark-1.2-contributor-free`;
- hosted Exa through OpenCode `websearch`; production provenance remained
  search-level and did not claim `webfetch`;
- elapsed runtime after the test binary was ready: 963.57 seconds;
- coordinator run: `arena-coordinator:fee33d36-fe0b-4b57-a901-ee2bf547587b`;
- Product OS project: `arena-project:60856b5f-5ccb-4691-9c17-56b649f51f2b`;
- research work orders: 3; independently verified representative evidence: 1;
- distinct architecture evidence records: 2;
- current Build Package: `arena-project:60856b5f-5ccb-4691-9c17-56b649f51f2b:build-package:35`;
- Delivery session: `arena-coordinator:fee33d36-fe0b-4b57-a901-ee2bf547587b:delivery`;
- candidate commit: `c6538d612b4fa110b96ca162b14ef961b127fcac`;
- one owner intervention: the bounded `narrow_build` direction answer;
- canonical checkout HEAD and complete Git status remained unchanged;
- independent Delivery verifier passed and the candidate became `Verified`.

The run used real external semantic roles for the three research categories,
fact verification, Product Director, Architect A, Architect B,
reuse/constraints/red-team/dissent review, feasibility, and implementation.
The existing SessionRuntime/resource boundary serialized these roles; this
audit makes no claim that they overlapped in time.

## Authority and failure review

The production path retains the existing fail-closed boundaries:

- researcher output is admitted as Unverified; a researcher cannot self-verify;
- FactVerifier identity, project, role, currentness, and cancellation are
  checked by the existing Product OS finalization path;
- owner-required ambiguity and owner answer remain bound to the exact question
  and authority revision;
- semantic responses are typed proposals until Arena-owned admission;
- package and gate facts come from current durable authority, not caller
  booleans or renderer payloads;
- stale package identity blocks Delivery admission;
- candidate work occurs in a non-authoritative worktree;
- canonical HEAD/status and protected acceptance are checked around workers;
- the independent verifier, candidate identity, and Apply checks remain
  authoritative;
- OpenCode cannot mark a candidate Verified or call Apply.

The failed attempts were retained as engineering evidence and not treated as
success: an invalid research URL was rejected and retried once; a scope-less
ValidationExperiment became a truthful terminal outcome; a feasibility result
exposed a missing generation binding; and a Delivery check exposed Python
bytecode creation after verification. The final fixture uses `python3 -B`; the
candidate-tree invariant was not weakened.

## Evidence classification

### CURRENT/IMPLEMENTED

- Durable `product_coordinator_runs` table in the existing TranscriptStore.
- Narrow Tauri API: start, status, owner answer, cancel, resume.
- Deterministic Research, ProductReview, Architecture, Package, Delivery, and
  Terminal phases.
- Typed semantic role work orders and bounded OpenCode JSON result parsing.
- Arena-owned Build Package admission into existing Delivery.
- Minimal founder UI for Discover → Decide → Deliver → Release state.

### SOURCE-CONFIRMED

- ProductAuthorityRecords remain product-truth authority.
- SessionRuntime remains live task/lifecycle authority.
- DeliveryState remains candidate, acceptance, verifier, and Apply authority.
- Renderer paths do not receive raw ProductAuthority or GateInput mutation.
- OpenCode permission controls are not an OS sandbox.

### RUNTIME-PROVEN

- Fresh production coordinator run from founder idea to verified candidate with
  the IDs and model above.
- Real WebDiscovery, independent verification, Product Director review,
  architecture competition, review roles, feasibility, gates, Delivery, and
  independent verifier.
- Canonical checkout preservation and no stray OpenCode/sleep process after
  completion.
- Dogfood command passed: 1 passed, 0 failed, 458 filtered; 963.57 seconds
  after binary build.

### UI-PROVEN

- React frontend build passed with coordinator status/question wiring.
- Owner-facing state is progressively disclosed and hides raw role plumbing.

### ENVIRONMENT-BLOCKED

- Native GUI runtime on the current Linux host remains blocked by the known
  WebKit/graphics limitation.
- Windows runtime/package qualification was not run in this milestone.

### STILL UNPROVEN

- Complete in-flight production-service restart/reconciliation matrix.
- End-to-end coordinator cancellation followed by a late semantic result.
- Explicit Safe Apply of the M07 candidate.
- True overlapping semantic-role execution; SessionRuntime serializes the wave.
- Production-scale performance and longer-running project recovery.
- Hostile same-user filesystem, full SSRF, and Git object-store isolation.

### DEFERRED/OPTIONAL

- Windows/package/native GUI release closure.
- GitHub MCP, ECC/gstack, frontier consultation execution, multiple providers,
  Dagu, and DSH replacement/repair.

## Founder interventions

The operator submitted the fresh founder idea, observed persisted status, and
supplied the owner-only bounded direction answer. No avoidable technical
coordination occurred: no prompt or URL relay, library selection, log moving,
role ordering, authority-record creation, package assembly, verifier operation,
or Delivery terminal work was manual.

## Verification record

- Real ignored production-coordinator dogfood with OpenCode/Muse Spark: passed.
- `cargo check`: passed with the historical warning baseline.
- Focused coordinator tests: 3 passed, 1 ignored (the real dogfood).
- `npm run build`: passed.
- Canonical checkout, process cleanup, diff, and secret review: performed for
  the final checkpoint.

The next large block is platform/release closure: Windows runtime/package
qualification, meaningful Linux install/launch, capable-host native GUI
dogfood, OpenCode prerequisite UX, and final security/release reconciliation.
