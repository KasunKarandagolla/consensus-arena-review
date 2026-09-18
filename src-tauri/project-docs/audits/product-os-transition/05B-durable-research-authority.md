# Milestone 05B — Durable Research + Work-Order Authority

**Date:** 2026-09-19
**Predecessor:** `a3949e54e6ea2ca9c9a32d856b2bc84b6de0e8d8`
**Platform exercised:** Linux development host
**Decision:** **BLOCKED FOR M06**

## Scope

This checkpoint adds the narrow Arena-owned admission boundary needed for
research and fact verification. It composes the existing SQLite-backed
`TranscriptStore` and `SessionRuntime`; it does not add a database, scheduler,
workflow engine, generic role system, MCP server, provider, or browser.

The production path is:

```text
Arena work order
  → bounded read-only source retrieval
  → unverified Product OS evidence proposal
  → distinct Arena fact-verifier work order
  → current-source verification disposition
  → durable Product OS records
  → restart/reconciliation
```

## DOCUMENTED

- Arena owns product authority, owner decisions, evidence-gate evaluation,
  Delivery acceptance, independent verification, and Safe Apply.
- Research output is a proposal and is not a verified fact.
- Existing Product OS Build Package gates require complete current authority
  records; a research-only project must not invent architecture or build
  evidence.
- The source route exercised here is direct official GitHub HTTPS retrieval.
  This is not GitHub MCP qualification.

## SOURCE-CONFIRMED

- `ProductWorkOrder` persists project/session identity, role, lifecycle,
  authority revision, parent relationship, evidence/result references,
  cancellation state, and run generation.
- `TranscriptStore` persists Product OS authority and work orders in the
  existing transcript database, with authority/work-order updates written in
  one SQLite transaction where both change together.
- Live execution is admitted through `SessionRuntime`; no competing lifecycle
  authority was introduced.
- Research admission forces `ResearchClaim` + `Unverified` and rejects
  self-verification. Fact-verifier admission requires a current unverified
  claim and a completed originating researcher order.
- Finalization is reachable through the runtime wrapper only after a current
  `FactVerifier` order is found in the same project, source, revision, and
  lifecycle state. Unknown, wrong-role, cancelled, superseded/stale, and
  mismatched results fail closed.
- Ambiguity admission ignores caller resolver classification and records an
  Arena-admitted owner-required question. The owner decision operation binds
  the answer to the exact question and admission revision.
- A resolver payload cannot remove the Arena-owned owner-required marker.
  Stale/cancelled/technical decisions cannot satisfy the current owner gate.
- Restart reconciliation changes persisted `Running` work to
  `ReconciliationRequired`; it never fabricates completion.
- Renderer-facing commands return sanitized JSON strings and expose snapshots
  and semantic operations, not direct ProductAuthority record mutation.

## RUNTIME-PROVEN

### Real research question

The Linux runtime test asked:

> What default branch does the official GitHub MCP repository currently report?

Source used:

`https://api.github.com/repos/github/github-mcp-server`

The bounded source adapter retrieved official repository metadata, parsed the
repository identity and `default_branch`, and retained only sanitized source
identity/scope. It did not persist the raw response body or any credential.

The observed lifecycle was:

1. Arena admitted a `Researcher` work order.
2. The real source retrieval produced an `Unverified` research proposal.
3. Arena admitted a distinct `FactVerifier` work order tied to that evidence.
4. The verifier independently retrieved the same primary source.
5. Arena finalized the claim as `IndependentlyVerified` with source scope and
   verifier work-order identity.
6. Arena admitted an owner-required ambiguity and adopted a matching owner
   decision. A wrong question ID was rejected.
7. The SQLite store was closed and reopened. Verified evidence, completed
   verifier identity, resolved ambiguity, and adopted owner decision remained
   bound to the same project.

Additional adversarial/runtime tests proved:

- researcher identity cannot be used as a fact verifier;
- cancelled verifier work cannot finalize evidence;
- verifier admitted at an old authority revision is rejected and marked
  failed, while the claim remains unverified;
- a persisted pending order reopens as `ReconciliationRequired`;
- a cancelled research order rejects a late run;
- non-official or query-bearing source URLs are rejected.

## INTEGRATED

- The runtime seam is exposed through the minimum Tauri commands for research
  admission/execution, verifier admission/execution, cancellation, snapshot,
  ambiguity admission, and owner decision.
- Delivery displays the current Product OS evidence disposition and unresolved
  owner-required ambiguity without changing the existing owner-facing layout.
- The existing pure gate evaluator still consumes only an Arena-assembled
  package. Its source-level tests prove unverified research is missing and
  independently verified research can pass when a complete current package
  exists.

## STILL UNPROVEN / BLOCKER

M06 admission is blocked because the real durable research dogfood starts with
an intentionally minimal research authority record. No accepted architecture,
Build Package, or product decision record was fabricated merely to make the
research gate pass. Therefore the following exact criterion is not yet
runtime-proven through the new production seam:

> a real verified research result survives reopen and satisfies the
> appropriate BuildPackage-backed evidence gate.

The source-level gate path is proven, but the complete authoritative
Product-OS-package-to-gate handoff remains the next prerequisite. Other
unproven boundaries are:

- native Windows runtime and packaging;
- packaged Linux installation/launch qualification;
- full GUI-driven research dogfood;
- GitHub MCP, upstream ECC/gstack procedures, and shared consultation
  execution;
- non-Zen provider portability;
- production-scale performance;
- active in-flight HTTP cancellation observation (the work-order cancellation
  and fail-closed late-result boundary is proven);
- explicit supersession runtime exercise;
- hostile same-user filesystem and shared Git-object-store isolation;
- full research-to-Delivery Build Package admission and Safe Apply run.

The candidate directory remains a non-authoritative working directory, not an
OS/security sandbox. The direct source adapter is read-only and bounded; it is
not a general research database or crawler.

## Verification record

- `cargo test product_os_runtime::tests -- --nocapture`: **7 passed**.
- `cargo test product_os::tests -- --nocapture`: **13 passed** in the focused
  Product OS run.
- `cargo check`: passed with the existing warning baseline.
- `npm run build`: passed; TypeScript and Vite build completed.
- `rustfmt --edition 2024 --check src/product_os_runtime.rs`: passed.
- `git diff --check`: passed before checkpoint review.
- Secret scan: no credential-pattern matches in changed source/audit scope.

No Windows or package qualification is claimed. No provider credential is
stored in source, tests, work orders, evidence, logs, or this audit.
