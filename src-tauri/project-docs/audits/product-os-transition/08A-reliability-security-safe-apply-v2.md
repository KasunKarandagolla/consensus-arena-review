# 08A — Reliability, Security, Concurrency + Safe Apply Closure

Date: 2026-09-20

Repository: `/home/kasun/Music/arena/consensus-arena`

Base reviewed: `016a8e0f7c3e8281d9580b40f95b6d54e0dbe77f` (`codex/arena-dev-temp`).

This audit closes the 08A runtime preflight without reopening the accepted Q1/Q2
quality-stack design. Arena still chooses profiles; workers cannot choose a
stronger profile; `env_clear()` is the default child boundary; derived quality
outputs cannot mutate ProductAuthorityRecords, acceptance, verification, or
Apply authority; and the deterministic verifier remains the sole source of a
Verified verdict.

## INTEGRATED

### Coordinator concurrency and recovery

- `run_architecture` now persists Architect A/B and the Reuse,
  Constraints, and Red-team work orders before starting their light semantic
  executions. The independent executions use `tokio::try_join!`; results are
  admitted in deterministic role order after completion. A failed role returns
  an error and cannot fabricate consensus.
- Work-order generation, current authority revision checks, cancellation, late
  result rejection, and restart reconciliation remain in the existing
  `SessionRuntime`/`TranscriptStore` path. No scheduler or second authority
  store was introduced.
- The existing persisted Delivery state and candidate-worktree boundary remain
  the restart seam before and after Delivery admission. OpenCode and quality
  subprocesses remain owned by the contained child lifecycle.

### Safe Apply and Git boundary

The existing production `apply_verified_candidate` path was exercised against a
disposable real Git repository. It requires all of:

| Precondition | Disposition |
| --- | --- |
| `Verified` phase and candidate commit | INTEGRATED; missing/unverified state is rejected |
| explicit Apply command path | INTEGRATED; no worker or quality tool calls Apply |
| clean unchanged canonical checkout | RUNTIME-PROVEN; dirty and moved HEAD cases rejected |
| current candidate SHA and verifier receipt | RUNTIME-PROVEN; changed candidate and stale receipt rejected |
| protected paths unchanged | RUNTIME-PROVEN through verifier/Apply guards |
| non-forcing fast-forward | RUNTIME-PROVEN; divergent base rejected |
| resulting original checkout | RUNTIME-PROVEN by the existing production fixture |

Canonical-checkout mutation detection, staged/committed/ignored secret scans,
protected acceptance restoration, and worktree cleanup remain in the existing
Delivery code. Git object-store confidentiality against a malicious same-user
process is not presented as a sandbox guarantee; authority integrity and
pre-Apply policy/secret checks are the V1 controls.

### Web and prerequisite policy

- Product research uses bounded hosted `websearch` through the WebResearch
  profile. The production path records `webfetch=not_used_in_production_path`;
  arbitrary fetch is not needed for the current V1 research contract.
- Public-source URL validation rejects non-HTTPS, loopback/private/link-local,
  credential-bearing, and credential-like query URLs. This is a narrow policy,
  not a general SSRF framework; OS/network policy outside the process is not
  claimed.
- Context7 remains an on-demand, bounded technical-documentation MCP input
  keyed by library/framework, version when known, and a short question. It is
  not Product OS memory or acceptance evidence.
- OpenCode version health requires `1.18.31`. Repo intelligence is optional
  derived cache data keyed by repository identity, base HEAD, and analyzer
  version. Rust/TypeScript LSP, selected skills, Context7, Playwright MCP,
  and native Tauri WebdriverIO are optional-degradable or target-specific;
  they do not block basic deterministic Product OS work unless acceptance
  explicitly requires that surface.

### Resource policy

Measured current-host values:

| Capability | Peak RSS |
| --- | ---: |
| OpenCode official single request | `544448 KB` |
| Arena-equivalent minimal OpenCode request | `509948 KB` |
| Implementation + selected skills | `536128 KB` |
| DebugRepair + selected skills | `713528 KB` |
| rust-analyzer | `174628 KB` / about `170.5 MiB` |
| TypeScript LSP | `65540 KB` / about `64.0 MiB` |
| agent-analyzer incremental update | `27460 KB` / about `26.8 MiB` |
| successful Rust test link | `1374656 KB` / about `1.31 GiB`; final positive-Apply rebuild `1514764 KB` / about `1.45 GiB` |
| WebdriverIO embedded qualification | `148440 KB` |
| WebdriverIO external qualification | `218952 KB` |

The successful Rust test-link number is a development/test-link constraint, not
normal Arena runtime. The low-spec policy is one heavy model/LSP/browser task at
a time, one analyzer refresh per repository HEAD, and no silent fan-out of
heavy profiles. Optional helpers fall back to project-native diagnostics,
frontend build/typecheck, deterministic verifier checks, or an explicit
environment status.

## RUNTIME-PROVEN

- Plain OpenCode 1.18.31, an Arena-equivalent temporary config, and a `plan`
  request were compared without credentials. The plain and minimal paths both
  returned exact `READY` with exit `0`.
- Real disposable model-backed Implementation and DebugRepair tasks used the
  selected pinned Superpowers procedures, made/check-repaired bounded changes,
  and passed deterministic checks. Arena's verifier remained external to the
  worker and no skill output was used as a verdict.
- Matching rust-analyzer 1.95.0 and pinned TypeScript language-server 4.3.3
  produced diagnostics and navigation/reference results on disposable targets;
  cancellation left no language-server process.
- Playwright MCP `0.0.82` inspected and acted on a disposable page through its
  MCP HTTP server, discovered locators, and observed the deterministic page
  result `Hello Arena`. The result is exploratory/non-authoritative.
- Chrome DevTools MCP `1.9.0` started and completed bounded navigation and
  evaluation. It is not accepted as default V1 evidence.
- The rebuilt low-memory Rust test binary executed deterministic Q1 policy,
  cache, environment, verifier, Safe Apply, cancellation, reconciliation,
  process-cleanup, candidate-review, and workflow tests. The corrected
  Product OS reopen/build-package test passed. The positive Apply test
  fast-forwarded a current verified OpenCode candidate in a disposable Git
  repository; the dirty, moved-HEAD, changed-candidate, stale-receipt,
  protected-state, and non-fast-forward cases remained rejected.
- The analyzer qualification remains standalone `agent-analyzer` v0.8.1,
  MIT, commit `719badc74731cd127a03b2d52bc51812b0c50c30`. The cached map
  reported stale when checked against a newer HEAD, and the source-level
  snapshot key test proves HEAD invalidation; bounded slices remain capped.
- No OpenCode, language-server, MCP, browser, or Tauri-driver process remained
  after the probes.

## UPSTREAM-REGRESSION

- OpenCode 1.18.31's CandidateReview-shaped background/profile path returned
  `403 FreeTierError: OpenCode's free tier can only be used from within
  OpenCode` while plain CLI, minimal plan, Implementation, and DebugRepair
  paths succeeded. This is recorded as a path-specific OpenCode/free-tier
  regression, not an Arena authority or profile-selection defect.

## ENVIRONMENT-BLOCKED

- Native Tauri WebdriverIO smoke execution is blocked on this Linux host. The
  current binary can be spawned, but the embedded provider has no registered
  `tauri-plugin-wdio-webdriver`; the external provider has no `tauri-driver` or
  `webkit2gtk-driver`; and `/dev/dri` is absent with the known WebKitGTK/EGL
  limitation. Playwright evidence is not substituted for native evidence.
- Full model-backed CandidateReview remains blocked by the upstream/provider
  403 above. Its receipt is not treated as acceptance evidence. The selected
  Superpowers runtime proofs that were available are recorded separately and
  are not generalized to CandidateReview.
- A bounded pair of light profile-equivalent model probes timed out at 90 s
  after the successful single-run probes; cleanup was clean. The coordinator
  concurrency seam is integrated, but a full end-to-end model-backed
  architecture run is not claimed from that timeout.

## REJECTED/NOT REQUIRED

- OpenCode 1.18.28 side-by-side fallback was not required because 1.18.31
  passed the plain and implementation-side probes. Active V1 health remains
  pinned to the qualified 1.18.31 version.
- Chrome DevTools MCP is rejected as a default V1 dependency because it adds no
  required capability beyond the proven Playwright MCP exploratory path.
- Hostile same-user object-store confidentiality and arbitrary same-user
  filesystem sandboxing are outside the V1 threat model; fail-closed authority,
  canonical-checkout, protected-state, precommit secret, and Safe Apply checks
  remain required.
- Full AgentSys orchestration, global Superpowers bootstrap, arbitrary skill
  installation, Sentry MCP, AgentSys skillers, and Flow-Next runtime dependency
  remain rejected/not required.

## Verification record

- `cargo check`: passed after the concurrency and fixture changes.
- Low-memory focused/full-targeted test strategy: passed; peak RSS was
  `1514764 KB` on the final positive-Apply rebuild (`1374656 KB` on the
  earlier Q1 closure link), with successful Q1 closure tests recorded above.
- Frontend build, Context7 bounded health/query proof, Playwright Test repeat
  proof, scoped secret scan, `git diff --check`, and orphan-process inspection
  remain passed from the Q1/Q2 closure run; native Tauri GUI evidence remains
  environment-blocked as stated above.
