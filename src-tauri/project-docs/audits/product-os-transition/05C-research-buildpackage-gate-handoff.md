# M05C — Verified research to Build Package gate handoff

Date: 2026-09-19
Scope: internal Product OS validation only; not market validation, release
qualification, or final founder dogfood.

## Decision

**READY FOR M06.** The M05B blocker is closed on one durable internal
validation project. No provider, MCP, browser, workflow, release, or delivery
capability was added.

## DOCUMENTED

- A Build Package is assembled from current Product Authority records and
  evaluated by the existing evidence-gate evaluator.
- NarrowBuild is intended to be owner/product-authority controlled, while
  Delivery remains the separate acceptance, verification, and Apply authority.

## SOURCE-CONFIRMED

- ProductScopeAdmission is a typed scope operation; it validates required
  fields, increments project/vision revision, and clears any old
  decision outcome/direction decision.
- ProductAuthorityRecords.product_direction_decision_id binds NarrowBuild to
  an adopted OwnerDecisionRecord for the current product-direction question.
  assemble_build_package() rejects a bare enum, missing decision, wrong
  question, or wrong option.
- Product Director work orders are validated as current/admitted before typed
  scope/review evidence is persisted. Reviews cannot submit a ResearchClaim,
  raw ProductAuthority record, or GateInput.
- Reuse and architecture admissions validate current typed evidence references.
  Architecture additionally requires two distinct completed Product Director
  work orders, a reuse review, hard-constraint review, risk experiment,
  red-team review, and dissent.
- The existing package fingerprint/revision rule makes a package stale after a
  material authority change.

## RUNTIME-PROVEN

Executed Product OS and Product OS runtime focused test suites. Results: 15
Product OS tests passed; 8 Product OS runtime tests passed.

The real M05C sequence used project
m05c-internal-github-metadata and official primary source
https://api.github.com/repos/github/github-mcp-server.

1. Arena admitted research work order
   c1a37253-03de-4097-9ffd-653e551d9fe5; its source-backed proposal was stored
   Unverified.
2. Distinct Fact Verifier work order
   fd8e095e-00cc-45ad-bcbd-989ad2ca020d independently retrieved the same
   official endpoint and finalized the claim as IndependentlyVerified.
3. The TranscriptStore was closed and reopened. The verified claim and
   verifier binding remained current.
4. A Product Director admitted the bounded internal scope: a read-only utility
   reporting selected public repository identity/default branch with bounded
   failures and no writes/authentication. The project explicitly records that
   it is not market validation.
5. Separate Product Director review work orders admitted
   architecture-direct-http, architecture-existing-retrieval,
   reuse-existing-retrieval, constraints-bounded-read-only,
   red-team-github-metadata, and dissent-no-general-client.
6. A real SessionRuntime-owned risk spike retrieved the official endpoint and
   recorded RiskExperiment evidence. The reuse decision was REUSE of the
   existing bounded official-GitHub retrieval; no fake BUILD claim was used.
7. An owner-required ambiguity was admitted and resolved only through its
   current OwnerDecision record. Arena then adopted NarrowBuild through the
   product-direction decision path.
8. The store was reopened again. It assembled
   m05c-internal-github-metadata:build-package:16, revision 16, fingerprint
   197bdc567671a586daaa4813e5050fbd933b1ad684b50aaa22f02b8ce3e488d2.
   Vision, ProblemResearch, Positioning, Ambiguity, Reuse, Architecture, and
   BuildReadiness each returned PASS from that same package.
9. A material typed scope change advanced authority and cleared the direction.
   The old BuildReadiness evaluation returned STALE; the old package was no
   longer current.

## INTEGRATED

- Durable research/fact-verifier work orders from M05B now feed the same
  reopened Product Authority state used by the package assembler.
- Package facts remain Arena-derived. No renderer, worker, researcher, or
  runtime completion can provide a passing GateInput.
- The existing Delivery handoff remains unchanged: it consumes only an
  Arena-assembled, current package and independently rechecks Architecture and
  BuildReadiness.

## Adversarial coverage

- Bare decision_outcome NarrowBuild without an adopted direction decision:
  rejected.
- Material scope change after package assembly: old package/gates stale and
  direction cleared.
- Duplicate architecture proposal identity: rejected by existing assembler.
- Missing architecture/red-team/risk evidence: rejected by existing tests and
  typed architecture admission.
- Unverified/contradicted research, wrong-role/cancelled/stale verifier, and
  owner-ambiguity downgrade: remain covered by the M05B/runtime suites.

## STILL UNPROVEN

- Final founder dogfood, Delivery implementation through this exact upstream
  package, and native GUI execution.
- Windows runtime/package qualification and production-scale performance.
- GitHub MCP (this proof used direct official HTTPS), external web search,
  shared frontier consultation execution, ECC/gstack runtime use, and broad
  provider portability.
- Same-user filesystem sandboxing or Git-object-store isolation. Candidate
  worktrees remain non-authoritative working directories, not security
  sandboxes.
