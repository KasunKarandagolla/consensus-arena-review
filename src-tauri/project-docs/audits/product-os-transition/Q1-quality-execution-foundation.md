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
