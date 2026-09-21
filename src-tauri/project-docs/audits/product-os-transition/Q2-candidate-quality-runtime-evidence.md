# Q2 — Candidate Quality + Runtime Evidence

Date: 2026-09-20

Repository: `/home/kasun/Music/arena/consensus-arena`

Base checkpoint: `d9ee05d1e49f9e8dd27795c2c2686d64949bb6d6`.

This audit preserves the Q1 architecture. Arena still selects profiles,
workers cannot select stronger profiles, `env_clear()` remains the child
boundary, repo intelligence is derived and HEAD-keyed, and ProductAuthority,
verification, and Apply remain outside quality tools.

## INTEGRATED

- `candidate_review.rs` defines bounded `SemanticReviewReceipt` records,
  exact candidate/acceptance identity, finding dispositions, deduplication,
  stale-receipt checks, and advisory-only tests.
- OpenCode candidate Delivery now runs relevant semantic lenses before the
  deterministic verifier when the provider is available. It supplies only a
  bounded diff, frozen acceptance summary, optional bounded repo-intelligence
  slice, and exact candidate identity. Candidate mutation or identity drift
  fails closed. Review output cannot create PASS/Verified.
- `quality_workflows.rs` makes browser evidence policy explicit: frozen
  Playwright Test and target-specific native Webdriver evidence are distinct
  from exploratory Playwright MCP and Chromium DevTools diagnostics.
- `doc_drift.rs` provides a bounded, deterministic source-path drift report.
  The scanner does not edit or commit documentation; the documentation worker
  can only propose a correction for review.
- `PerformanceProcedure` encodes scenario → baseline → constraint →
  hypothesis → measurement → one bounded change → rerun → evidence. It is a
  policy type, not an additional workflow owner.

## RUNTIME-PROVEN

### Playwright Test

- Qualified outside the Arena package with pinned `@playwright/test 1.54.0`
  and system Chromium `/usr/bin/google-chrome` version `147.0.7727.137`.
- A frozen disposable web scenario ran once successfully and then with
  `--repeat-each=2`: `2 passed (41.2s)`. The run produced a screenshot and
  Playwright trace artifacts. The browser process was cleaned up; only the
  pre-existing interactive Chrome crashpad process remained.
- This proves deterministic web-target automation only. It does not prove the
  native WebKitGTK/Tauri shell.

### Q1 carry-forward runtime tools

- Matching rustup `rust-analyzer 1.95.0` protocol proof: four diagnostics, one
  definition, one reference; maximum RSS `174628 KB`.
- Pinned `typescript-language-server 4.3.3` with TypeScript `5.4.5` protocol
  proof: two diagnostics, one definition, two references; maximum RSS
  `65540 KB` using a bounded disposable `noLib` target.
- These LSP results are advisory and do not enter acceptance or verifier
  authority.

## ENVIRONMENT-BLOCKED

- Real model-backed CandidateReview, Implementation/TDD, and DebugRepair/
  systematic-debugging proofs could not complete. The qualified OpenCode
  `1.18.31` path returned provider `403 FreeTierError: OpenCode's free tier
  can only be used from within OpenCode`; the configured local OmniRoute
  endpoint `http://localhost:20128/v1` was unreachable. Superpowers runtime
  influence is therefore not inferred from copied files.
- Playwright MCP and Chrome DevTools MCP were not available through the current
  configured MCP/tool surface. No exploratory result is claimed as PASS.
- Native Tauri/WebdriverIO runtime automation was not installed in this
  environment. Existing native Tauri policy is preserved; no Playwright claim
  is substituted for WebKitGTK/Tauri evidence.

## SOURCE-CONFIRMED

- Q2 semantic reviewers use the existing OpenCode CandidateReview profile and
  the existing heavy-profile single-slot policy; no second coordinator or
  generic plugin system was introduced.
- The verifier continues to inspect the candidate after semantic review and
  owns the only Verified transition. Browser exploration, LSP, Context7,
  repo-intelligence, skills, and performance prose remain advisory.
- Review procedures remain narrow adaptations of the pinned/inspected
  selected procedures. Full AgentSys workflow orchestration, global
  Superpowers bootstrap, Sentry MCP, and Flow-Next runtime integration were
  not added.

## REJECTED / NOT REQUIRED

- Sentry MCP: **REJECTED / NOT REQUIRED AS CORE V1**. It is useful only for a
  target project that already uses Sentry; it is not a founder-to-release V1
  dependency.
- AgentSys skillers: **REJECTED FOR V1**. Arena uses only pinned/reviewed
  procedures; no automatic learned-skill adoption exists.
- Flow-Next: **REJECTED AS RUNTIME DEPENDENCY**. Arena's existing coordinator,
  Product OS, Delivery, and verifier remain workflow authorities.
- Chrome DevTools MCP is rejected from the default V1 profile unless a future
  qualification demonstrates a material capability beyond Playwright MCP.

## ADVERSARIAL / DETERMINISTIC COVERAGE

- Review receipts reject stale candidate SHA/acceptance identity and collapse
  duplicate findings without a universal score.
- Review mutation is checked after the profile run; mutated or dirty candidate
  state cannot proceed to verification.
- Review findings cannot set `Verified`; a deterministic verifier receipt is
  still required.
- Exploratory browser evidence is explicitly non-deterministic policy data.
- Seeded source rename/document stale-reference proof executed through
  `doc_drift::scan_documents`; the report was bounded and the document bytes
  were unchanged.
- Performance completion requires an evidence reference and actual command
  fields; model prose alone cannot complete the procedure.

## Verification record

- `cargo check`: passed with the existing repository warning baseline.
- The supported one-job/debuginfo-disabled Cargo strategy passed the new
  execution-profile (3), candidate-review (4), documentation-drift (1),
  workflow-policy (2), repo-intelligence (2), environment-allowlist (2), and
  OpenCode authority (3) tests. Peak RSS for the successful timed link was
  `1,434,664 KB` over `228.01 s`.
- Frontend `npm run build`: passed (`tsc` and Vite, 1,712 modules).
- agent-analyzer v0.8.1 cache status was valid at base HEAD
  `d9ee05d1e49f9e8dd27795c2c2686d64949bb6d6` before this commit; a new
  committed HEAD invalidated that prior map (`status: stale`), proving stale
  snapshot rejection. Incremental update processed one commit and emitted a
  valid map at `64fe9a56857c7e8f609737bde5250c241b0f8af2` in `5.66 s` with
  peak RSS `26,640 KB`.
- Context7 anonymous MCP health/query proof and the pinned Playwright
  repeat proof passed. Existing OpenCode authority regression passed.
- Scoped secret scan, `git diff --check`, touched-file formatting checks, and
  orphan-process inspection passed; full repository cargo-format check remains
  blocked by pre-existing unrelated formatting differences.
- Playwright qualification was external to the repository so no dependency
  was added to Arena's package manifests.
