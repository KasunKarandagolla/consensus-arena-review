# Q1 — Quality Execution Foundation

Date: 2026-09-19

Repository: `/home/kasun/Music/arena/consensus-arena`

Resolved predecessor/current base: `ee003a96dfb66ab9b59c59880eb27a503b3d32fa`
(`feat: add production Product OS coordinator`). The working branch is
`codex/arena-dev-temp`.

## DOCUMENTED

- Arena-owned profiles are defined in
  `/home/kasun/Music/arena/consensus-arena/src-tauri/src/execution_profiles.rs`:
  `SemanticNoTools`, `WebResearch`, `Implementation`, `DebugRepair`,
  `CandidateReview`, `BrowserQa`, `DocsMaintenance`, and
  `PerformanceInvestigation`.
- Arena maps authoritative Product OS roles to profiles. Workers do not receive
  a profile-selection input.
- Profile configuration is temporary, non-secret, and authority-free. It can
  select OpenCode agent/mode, bounded tools, selected skills, native LSP,
  Context7, timeout/result limits, and resource policy only.
- `ProjectIntelligenceSnapshot` is a derived cache keyed by repository
  identity, base HEAD, and analyzer version. Only metadata and bounded query
  slices are sent to workers; the full map is not Product OS authority.
- Implementation, DebugRepair, and CandidateReview are the only profiles that
  enable OpenCode LSP. Semantic and web roles do not start LSP.
- The selected Superpowers procedures are limited to TDD,
  verification-before-completion, systematic-debugging,
  requesting-code-review, and receiving-code-review.

## SOURCE-CONFIRMED

- OpenCode adapter source remains the Delivery boundary and still performs
  canonical-checkout protection, candidate isolation, evidence correlation,
  and project-native verification handoff.
- `dsh_worker` still supplies Unix process-group and Windows Job Object
  containment. The new configuration path calls `env_clear()` and adds only
  exact explicit OpenCode variables:
  `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR`,
  `OPENCODE_DISABLE_LSP_DOWNLOAD`, and
  `OPENCODE_EXPERIMENTAL_LSP_TOOL`.
- Ambient provider/API variables are not copied. Provider tokens are not
  written to profile configuration.
- Product OS and Delivery remain the only authorities for product records,
  acceptance, verification, and Apply.
- OpenCode documentation/source supports `OPENCODE_CONFIG`,
  `OPENCODE_CONFIG_DIR`, native LSP configuration, and the Rust/TypeScript
  built-in LSP path: [OpenCode configuration](https://dev.opencode.ai/docs/config/)
  and [OpenCode LSP](https://dev.opencode.ai/docs/lsp/).

## RUNTIME-PROVEN

### OpenCode

- Executable: `/home/kasun/.opencode/bin/opencode`.
- Version: `1.18.31`, matching Arena's qualified adapter version.
- A bounded `/usr/bin/time timeout 10s opencode --version` probe measured
  maximum RSS `104040 KB` (approximately 101.6 MiB). The probe was bounded and
  left no retained Arena process after cleanup.

### agent-analyzer

- Upstream release: v0.8.1, MIT, commit
  `719badc74731cd127a03b2d52bc51812b0c50c30`.
- Static musl archive SHA-256:
  `ca7acd32f09e3c5b6b36f65e00a4441735a22f2170147b2f1bc0a43234f03e6a`.
- The actual repository was analyzed. Qualified outputs included project
  metadata, Rust/TypeScript/HTML/JavaScript language counts, entry points,
  `commands.rs`/`browser_backend.rs`/`opencode_adapter.rs` hotspots, coupling,
  ownership/bus-factor signals, and symbol/import mappings.
- `repo-intel status` returned:
  `{"status":"valid","analyzed_up_to":"ee003a96dfb66ab9b59c59880eb27a503b3d32fa","total_commits":67}`.
- An incremental update after no new commits processed zero commits while
  rebuilding AST/co-change indexes.
- Measured incremental update: 9.22 seconds, maximum RSS `25660 KB`
  (approximately 25.7 MiB).
- Arena's runtime cache is bounded by map size and HEAD/version key. A changed
  HEAD produces a different snapshot key, preventing stale reuse.

### Context7

- Path: anonymous remote MCP at `https://mcp.context7.com/mcp`, configured only
  for DocsMaintenance and PerformanceInvestigation profiles.
- Runtime proof completed without credentials: MCP initialize, `tools/list`,
  `resolve-library-id` for OpenCode, and `query-docs` for the bounded question
  “How do I configure native LSP servers and disable automatic LSP downloads in
  OpenCode?”
- The response identified `/anomalyco/opencode`, returned source links, and
  discussed `lsp: false`, custom LSP entries, and `autoupdate: false`.
- Context7 output is not persisted as Product OS memory or acceptance evidence.
  Upstream: [Context7](https://github.com/upstash/context7).

### Containment/profile regression coverage

- Profile unit tests cover role policy, selective LSP, selected skill names,
  denial of authority tools, and denial of arbitrary skills.
- Environment tests cover removal of unrelated API/cloud secrets and require
  explicit profile overrides for OpenCode configuration.
- Existing process-group/Job Object tests remain in `dsh_worker`; the new
  file-backed stdout path uses the same contained child and cleanup lifecycle.

### Authority/process adversarial coverage

- A worker cannot select a profile: the only profile mapping is the internal
  `for_product_role` function, and Delivery fixes `Implementation`.
- Ambient arbitrary variables and provider secrets are removed after
  `env_clear()`; OpenCode variables are accepted only through the typed
  override struct.
- Repo-intel data is explicitly delimited as derived/untrusted and is stored
  outside ProductAuthorityRecords. Snapshot keys include HEAD, so a changed
  HEAD cannot reuse the prior snapshot.
- Profile config denies `apply`, `verify`, `acceptance`, and arbitrary skills;
  skill instructions are explicitly advisory and cannot set Arena verdicts.
- Context7 is read-only documentation input and has no Product OS persistence
  or acceptance ingestion path.
- LSP is exposed only to candidate/code roles and its result is not connected
  to verification or Apply. Candidate canonical/protected-state checks remain
  in the existing Delivery path.
- Existing contained-child cancellation tests cover descendant cleanup; the
  file-backed analyzer path uses that same cleanup boundary.

## INTEGRATED

- Product Director and Architect prompts receive a bounded repository-intel
  slice when the analyzer is available, with an explicit untrusted/non-
  authoritative delimiter and fail-soft fallback.
- Existing semantic Product OS work orders use `SemanticNoTools`; existing web
  research uses `WebResearch`; OpenCode candidate Delivery uses
  `Implementation`.
- Temporary profile configuration and any copied selected skill files are
  removed by the profile workspace guard after the contained invocation.
- The verifier still owns the `Verified` transition, and no new capability
  writes ProductAuthorityRecords directly.
- No new dependency was added and no second orchestrator, Arena LSP server, or
  full Superpowers/AgentSys workflow was introduced.

## ENVIRONMENT-BLOCKED

- Rust LSP runtime proof is blocked on this host: `/home/kasun/.cargo/bin/rust-analyzer`
  resolves through rustup, but `rustup component list --installed` has no
  `rust-analyzer` component and the executable reports that it is unknown.
  Fallback: project-native `cargo check` and bounded source queries.
- TypeScript LSP runtime proof is blocked: neither
  `typescript-language-server` nor a global `tsserver` executable is installed
  in the current environment. Fallback: the repository's frontend build.
  OpenCode's native TypeScript path remains source-confirmed only.
- A real model-backed Superpowers Implementation and DebugRepair task was not
  claimed as proven in this audit because no disposable target/provider run
  was authorized for this qualification pass. The pinned skill files were
  inspected and selected skill injection is integrated, but runtime influence
  remains blocked rather than inferred from configuration.
- The GNU agent-analyzer archive was not usable on this host because it
  requires GLIBC 2.38; the qualified static musl archive was used instead.

## REJECTED / NOT REQUIRED

- Full AgentSys workflow orchestration: rejected; standalone deterministic
  repo intelligence is sufficient for this milestone.
- Global Superpowers bootstrap or worker-selected arbitrary skills: rejected.
- Arena-owned LSP server/client: not required; OpenCode native configuration is
  the supported path.
- Context7 as Product OS memory or acceptance evidence: rejected.
- Worker ability to select a stronger profile, alter acceptance/verification,
  mark `Verified`, mutate Apply authority, or write outside the candidate:
  rejected/fail-closed by policy and existing Delivery checks.
- Inline credentials in OpenCode config: rejected.

## Verification record

- `cargo check`: passed.
- `cargo test --bin consensus-arena execution_profiles::tests -- --nocapture`
  was attempted as the focused unit run. The binary test harness exceeded ten
  minutes and approximately 2.06 GiB RSS while linking on this 4 GiB-class
  host, so it was stopped with exit 130 before executing tests. This is an
  environment/resource block, not a passing test claim.
- Context7 bounded MCP proof: passed as recorded above.
- `cargo fmt --all -- --check`: baseline repository is not formatting-clean;
  it reports unrelated pre-existing formatting differences in commands,
  consultation, delivery, and other source files. Changed Rust regions were
  checked separately; no unrelated source was reformatted.
- Required frontend build, full targeted binary tests, secret scan, final
  orphan-process inspection, commit, and remote confirmation are recorded by
  the delivery handoff for this milestone.

External qualification references: [agent-analyzer](https://github.com/agent-sh/agent-analyzer),
[Superpowers](https://github.com/obra/superpowers), and
[Context7](https://github.com/upstash/context7).

## Follow-up closure and Q2 integration — 2026-09-20

This section preserves the original checkpoint claims above. It records the
environment work performed after that checkpoint and the small Q2 policy seam
added without changing Arena's authority model.

### SOURCE-CONFIRMED / INTEGRATED

- The active rustup toolchain is `stable-x86_64-unknown-linux-gnu`, Rust
  `1.95.0 (59807616 2026-04-14)`. The matching rustup component now provides
  `/home/kasun/.cargo/bin/rust-analyzer`, version `1.95.0 (5980761
  2026-04-14)`. OpenCode's Arena profile configuration points at native LSP;
  no Arena LSP client or server was added.
- TypeScript tooling is reproducibly installed outside the repository at
  `/home/kasun/.local/share/consensus-arena-tools/typescript-language-server-4.3.3`:
  `typescript-language-server 4.3.3` with bundled/pinned TypeScript `5.4.5`.
  The repository's existing frontend build remains the fallback and no
  package.json dependency was added.
- Q2 now has an Arena-owned bounded `SemanticReviewReceipt` policy. Receipts
  carry exact candidate SHA and acceptance commit, deduplicate findings, and
  are explicitly advisory. CandidateReview runs before deterministic
  verification when the qualified execution runtime is available; a stale or
  mutating review is rejected and no review can mark PASS/Verified.
- Browser evidence policy distinguishes frozen Playwright/native Webdriver
  commands from exploratory Playwright MCP and Chromium DevTools output.
  Performance workflow data requires measured evidence before it is complete.
  These are policy types, not a second coordinator.

### RUNTIME-PROVEN

- Rust LSP protocol proof on disposable target
  `/tmp/arena-q1-rust-proof-20260919` produced four syntax diagnostics, one
  definition result and one references result through the installed
  rust-analyzer stdio server. Maximum RSS was `174628 KB` (about `170.5 MiB`)
  over `19.55 s`; the server was terminated and no rust-analyzer process
  remained.
- TypeScript LSP protocol proof on disposable target
  `/tmp/arena-q1-ts-proof-20260919` produced two type diagnostics, one
  definition result and two references results through the pinned
  `typescript-language-server 4.3.3` path. The bounded target used `noLib` to
  avoid irrelevant standard-library acquisition on this constrained host.
  Maximum RSS was `65540 KB` (about `64.0 MiB`) over `23.41 s`; the spawned
  tsserver children were terminated and orphan inspection was clean.
- These raw LSP proofs are advisory source-intelligence evidence only. They do
  not enter ProductAuthorityRecords or verification receipts.

### ENVIRONMENT-BLOCKED

- An actual model-backed OpenCode profile run was attempted with the exact
  Arena-style LSP configuration. The qualified executable reached provider
  startup but returned the exact provider error `403 FreeTierError: OpenCode's
  free tier can only be used from within OpenCode`. The configured local
  OmniRoute endpoint at `http://localhost:20128/v1` was also unreachable.
  Therefore OpenCode tool-call proof and real Superpowers Implementation and
  DebugRepair influence remain blocked by provider access, not inferred from
  copied skill files. No credential was placed in temporary configuration.
- The TypeScript proof on the full frontend project was resource-heavy on this
  host; the disposable bounded `noLib` target is the successful qualification
  case. Project-native `npm run build` remains the default frontend fallback.

### Q1 test-resource closure status

- The focused binary test was rerun with one Cargo job, test debuginfo removed,
  GCC as linker, and `--reduce-memory-overheads`. The dependency build was
  completed successfully. The previous `2.06 GiB` failed linker attempt
  remains historical evidence and is not described as product runtime
  performance.
- The supported low-resource command is:
  `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0 -C linker=gcc -C link-arg=-Wl,--reduce-memory-overheads' cargo test --bin consensus-arena -- --nocapture`.
  It is intentionally serial and should be used for Q1 policy/cache tests on
  the target host.
- The first low-memory link completed the execution-profile tests (3 passed).
  The timed cached-profile run completed the new candidate-review tests (4
  passed) with peak RSS `1,434,664 KB` (about `1.37 GiB`) over `228.01 s`.
  Direct execution of the cached binary also passed the drift (1), workflow
  policy (2), repo-intelligence (2), environment allowlist (2), and existing
  OpenCode authority (3) tests. This closes the Q1 test-linking proof gap;
  it does not claim that the full suite is cheap on this host.

### Q2 evidence status

- Semantic-review policy is **INTEGRATED**, with pure parser, exact-SHA,
  deduplication and advisory-authority tests. Real model-backed candidate
  review and bounded repair are **ENVIRONMENT-BLOCKED** by the provider error
  above.
- Playwright Test, Playwright MCP, Chrome DevTools MCP, and native Tauri
  WebdriverIO runtime qualification are recorded separately in the Q2 audit;
  no browser result is accepted as a deterministic verdict unless it is a
  frozen VerificationCommand for the target it actually exercises.
- Sentry MCP is **REJECTED / NOT REQUIRED AS CORE V1**; AgentSys skillers are
  **REJECTED FOR V1**; Flow-Next is **REJECTED AS RUNTIME DEPENDENCY**.

### Closure verification — 2026-09-20

- `cargo check`: passed after the closure changes.
- Focused low-memory tests: passed as recorded above. The source-only
  `cargo fmt --all -- --check` baseline remains noisy; touched new Rust files
  were checked with edition-2024 rustfmt.
- Frontend `npm run build`: passed (`tsc` and Vite, 1,712 modules).
- The qualified analyzer cache reported valid at the current base HEAD before
  the closure commit; its status was keyed to `d9ee05d1e49f9e8dd27795c2c2686d64949bb6d6`.
  After the closure commit it reported `stale`; an incremental update processed
  one commit and emitted a valid new map at the new HEAD in `5.66 s` with peak
  RSS `26,640 KB`. The post-commit stale-HEAD check is recorded in the Q2 audit.
- Context7 anonymous MCP health/query proof, Playwright repeat proof, scoped
  secret scan, and orphan-process inspection passed. Model-backed
  Superpowers and OpenCode LSP tool-call proof remain provider-blocked as
  stated above.

## Q1 runtime closure addendum — 2026-09-20

This addendum preserves the historical checkpoint statements above and records
the current runtime disposition after the 08A preflight. It does not change the
Q1 execution-profile, authority, or derived-evidence architecture.

### RUNTIME-PROVEN

- OpenCode `/home/kasun/.opencode/bin/opencode` `1.18.31` passed a plain
  official CLI request on `/tmp/arena-08a-official-target`: exact `READY`, exit
  `0`, peak RSS `544448 KB`, elapsed `52.91 s`.
- The same model passed an Arena-equivalent temporary configuration with the
  `plan` agent and denied authority/tools: exact `READY`, exit `0`, peak RSS
  `509948 KB`, elapsed `49.27 s`. This is the minimal profile/config path used
  to compare the earlier provider failure; no credential was exposed.
- A bounded Implementation proof ran in
  `/tmp/arena-08a-implementation-repo` from baseline
  `d1f705447a4ad6e1fcbc74aba39fa601f0ec1903` to candidate
  `9cc57aa740b3e85e52b16279f0311d0281973ed0`. The worker loaded the pinned
  Superpowers `test-driven-development` and
  `verification-before-completion` procedures, demonstrated red then green
  `npm run check` (5/5), and did not claim Arena authority. Peak RSS was
  `536128 KB` over `141.68 s` for the completing continuation.
- A bounded DebugRepair proof ran in `/tmp/arena-08a-debug-repo` from seeded
  defect commit `c6d0f3b7a22966e4f649ceb57d9ba99d312714c4`. The worker loaded
  `systematic-debugging` and `verification-before-completion`, reproduced the
  failing check, identified the percentage arithmetic defect, repaired it, and
  reran the same check successfully (1/1). Peak RSS was `713528 KB` over
  `193.68 s`. The independent check also passed after the worker returned.
- The pinned Superpowers revision is
  `5bf4e78011075bcfc0dc295f0724994cd123ee71`, MIT licensed. Only the five
  selected procedures remain exposed; workers cannot install or select others.
- Rust LSP now uses the matching rustup component, `rust-analyzer 1.95.0
  (5980761 2026-04-14)`, and the previous disposable OpenCode-native protocol
  proof remains valid: diagnostics, definition, and references, peak RSS
  `174628 KB` (about `170.5 MiB`). TypeScript remains pinned at
  `typescript-language-server 4.3.3` with TypeScript `5.4.5`; its disposable
  proof produced diagnostics, definition, and references at peak RSS
  `65540 KB` (about `64.0 MiB`). Both are advisory and cleanup was verified.
- Playwright MCP `@playwright/mcp 0.0.82` started in a disposable isolated
  server and handled a real page: navigation, snapshot locator discovery,
  form fill, click, observed `Hello Arena`, and close. Its reported bundled
  Playwright was `1.64.0-alpha-1789764292000`; the server and page server were
  stopped cleanly.
- Chrome DevTools MCP `1.9.0` (Apache-2.0) started and completed a bounded
  `navigate` plus `evaluate` probe. Its server was stopped cleanly; it adds no
  required V1 capability beyond the proven Playwright MCP path.
- `@wdio/tauri-service 1.4.0` with WebdriverIO `9.31.9` was installed in a
  disposable tools location and `wdio --version` passed. Native launch was
  attempted through both embedded and external providers; the exact outcomes
  are recorded as environment-blocked below.
- The successful low-memory Cargo strategy executed the new deterministic
  tests. The rebuilt test binary passed execution-profile policy (3), repo
  intelligence (2), Safe Apply rejection matrix (1), verifier mutation/
  timeout (2), Product OS runtime/reopen/cancellation (12 with 2 explicitly
  ignored provider-backed cases), OpenCode late-result/identity (2), process
  containment (13), candidate-review policy (4), and workflow policy (2).
  The successful link peak was `1374656 KB` (about `1.31 GiB`) over `298.85 s`.
- The final rebuild including the positive OpenCode Safe Apply test peaked at
  `1514764 KB` (about `1.45 GiB`) over `346.61 s`; both successful links used
  the same supported one-job/debuginfo-disabled strategy.
- The previous `2.06 GiB` failed test-link attempt remains historical test
  resource evidence, not an Arena runtime-performance claim.

### INTEGRATED

- The coordinator now creates and persists Architect A/B and the light
  independent review work orders before executing them with `tokio::try_join!`.
  The execution-profile mapping, `env_clear()` boundary, single heavy-LSP
  slot, and all Product OS/verifier/Apply authority boundaries are unchanged.
- The existing real-Product-OS handoff fixture was corrected to create its two
  architecture proposals as `ArchitectA` and `ArchitectB`; the production
  admission rule was not weakened.
- The supported test command on the low-spec host is:
  `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0 -C linker=gcc -C link-arg=-Wl,--reduce-memory-overheads' cargo test --bin consensus-arena -- --nocapture`.
- Resource policy is now explicit: one heavy model/LSP/browser task at a time;
  light semantic roles may overlap only when measured headroom permits; repo
  analysis is cached by repository identity/HEAD/analyzer version; optional
  LSP, analyzer, MCP, and browser helpers fail soft to project-native checks.

### UPSTREAM-REGRESSION

- A CandidateReview-style run using the same `1.18.31` model, bounded plan
  configuration, LSP-enabled profile shape, and selected review skills returned
  the exact provider response: `403 FreeTierError: OpenCode's free tier can
  only be used from within OpenCode`. This is path-specific: the plain CLI,
  minimal plan request, Implementation proof, and DebugRepair proof succeeded.
  Arena is not downgraded to `1.18.28`; the active supported runtime remains
  `1.18.31` for the proven paths, while CandidateReview reports the provider
  failure and remains advisory/non-authoritative.

### ENVIRONMENT-BLOCKED

- Native Tauri WebdriverIO could not execute a smoke scenario on this host.
  The embedded provider spawned the current binary but its WebDriver server did
  not become ready because `tauri-plugin-wdio-webdriver` is not registered;
  the external provider reported missing `tauri-driver` and
  `webkit2gtk-driver`. The host also has no `/dev/dri`, matching the existing
  WebKitGTK/EGL graphics limitation. Embedded and external WDIO probes peaked
  at `148440 KB` and `218952 KB`, respectively, and both cleaned up.
- A concurrent pair of lightweight profile-equivalent model probes timed out
  at the 90-second bound after the earlier successful single-run probes; they
  left no process. The coordinator overlap is source-integrated, but no full
  model-backed Product OS architecture run is claimed as runtime proof from
  that failed pair.
- Full frontend TypeScript LSP remains resource-sensitive; the bounded
  disposable `noLib` qualification and `npm run build` fallback remain the
  supported evidence on this machine.

### REJECTED/NOT REQUIRED

- Side-by-side OpenCode `1.18.28` fallback was not installed because
  `1.18.31` passed the plain, minimal, Implementation, and DebugRepair probes.
- Chrome DevTools MCP is not a default V1 dependency because Playwright MCP
  already supplies the required exploratory inspect/locator/action surface.
- Full AgentSys orchestration, global Superpowers bootstrap, arbitrary worker
  skills, Context7 as Product OS memory, LSP as acceptance authority, and
  browser/MCP output as PASS remain rejected or not required.
