# 08B v2 — Platform, Packaging + Remaining Capability Closure

Date: 2026-09-20

Starting checkpoint: `f030a950c0eccbf65c9c71d37e39c2b6493d1d02`

This audit closes the platform and capability qualification boundary after
08A. It does not reopen execution profiles, Product OS authority, deterministic
verification, Safe Apply, or the accepted rejection of full AgentSys,
full-Superpowers orchestration, DSH, and Dagu.

The current working tree also contains unrelated pre-existing audit deletions
and untracked consultation/prompt material. Those paths were not changed or
included in this closure.

## Status vocabulary

Every current item is classified as one of:

- `PROVEN/INTEGRATED`
- `REJECTED/NOT REQUIRED`
- `ENVIRONMENT-BLOCKED`

Historical evidence is identified as historical and is not promoted to
current-HEAD proof.

## PROVEN/INTEGRATED

### Formatting and Linux quality gates

The Windows workflow's formatting gate was reproduced on the current checkout.
The smallest mechanical remediation formatted only the seven files reported by
rustfmt:

- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/consultation.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/delivery.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/main.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/opencode_adapter.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/product_os_runtime.rs`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/transcript_store.rs`

`cargo fmt --all -- --check` passes after that formatting-only change. No
authority or runtime logic was changed by the remediation.

The current Linux quality checks remain source/runtime-confirmed from 08A and
were rechecked at this closure: frontend build, backend check, execution
profile tests, contained-process tests, verifier/Delivery tests, and
`git diff --check`.

### Linux package

The current-HEAD package was built with:

```text
cd /home/kasun/Music/arena/consensus-arena
CARGO_BUILD_JOBS=1 npm run tauri build -- --bundles deb
```

The build used the existing Tauri configuration and no new repository
dependency. Exact artifact size, SHA-256, Debian dependencies, and safe
extraction/launch result are recorded below after the build completed.

The package is a Linux artifact only. It does not establish Windows NSIS,
WebView2, or native GUI parity.

Current artifact:

- path: `/home/kasun/Music/arena/consensus-arena/src-tauri/target/release/bundle/deb/Consensus Arena_0.1.0_amd64.deb`;
- size: `10833834` bytes;
- SHA-256: `a44054093eb88cebba1559d7e3f74076ffc9e87717d6bb0c4da7202d4e0b3aea`;
- package: `consensus-arena` version `0.1.0`, architecture `amd64`;
- installed size: `32407 KiB`;
- dependencies: `libwebkit2gtk-4.1-0`, `libgtk-3-0`;
- bundled binary: `33148128` bytes;
- desktop icon: `/usr/share/icons/hicolor/32x32/apps/consensus-arena.png`;

`dpkg-deb --extract` succeeded in a disposable directory. Launching the
extracted binary with `WEBKIT_DISABLE_COMPOSITING_MODE=1` reached native
startup and was bounded by the 20-second probe (`timeout` exit 124); it emitted
only the known GTK module/DRI warnings, and no product or WebKit process
remained afterward. A disposable `dpkg --root` install/purge probe also
succeeded without mutating the host package database. The release linker
reached a measured maximum RSS of `2130864 KiB`; this is a
developer/package-build measurement, not normal Arena runtime RSS.

### Existing Windows workflow and containment source

The current workflow is
`/home/kasun/Music/arena/consensus-arena/.github/workflows/windows-qualification.yml`.
It performs locked Node setup, frontend build, `cargo fmt --check`, Rust tests,
Windows `cargo check`, and NSIS packaging on `windows-latest`. The current
Linux host cannot execute that runner.

Source confirms that bounded DSH/OpenCode workers use explicit argv/current
directory, sanitized environment, Unix process groups, and Windows Job Objects
with kill-on-drop. Current Linux tests cover the available containment/path
fixtures; historical Windows workflow evidence covered Job Object descendants
and paths containing spaces. Verifier commands use a separate direct process
path, so worker containment proof is not generalized to arbitrary verifier
descendants.

### Official GitHub MCP interoperability

The official server was qualified in an isolated temporary tools location:

- repository: `github/github-mcp-server`;
- release: `v1.12.2`;
- commit: `85598ba6e1256f7ebf4867b95d63b833c4549264`;
- license: MIT;
- Linux x86_64 archive SHA-256:
  `95843162759da2c31dde082dd145be35db82164594796c294414b69790c2290e`;
- binary SHA-256:
  `b7a96bf79c68c0d4d0cdb5713e9ff36a1730b87ee3ae710e2e6189f398d7c1aa`;
- invocation: `github-mcp-server stdio --read-only`;
- protocol initialization: MCP `2024-11-05`;
- `tools/list`: 23 read-only tools;
- read proof: repository search and README read succeeded;
- write proof: `create_issue` was not exposed and returned `unknown tool` when
  challenged;
- cleanup: temporary server/config/credential material and process were
  removed.

This proves interoperability. It is rejected as a default V1 dependency in
favor of Arena's existing direct GitHub/OpenCode retrieval path.

### Skills and workflow closure

The selected Superpowers subset remains proven and integrated from 08A:

- revision `5bf4e78011075bcfc0dc295f0724994cd123ee71` / tag `v6.4.1`;
- license MIT;
- only TDD, verification-before-completion, systematic-debugging,
  requesting-code-review, and receiving-code-review are exposed;
- Arena retains founder discovery, planning, worktree, acceptance,
  verification, and Apply authority.

ECC source was inspected at revision
`934195f955cf0da847d59fcd6f68856bce112d8b`, package `2.2.2`, MIT. gstack was
inspected at revision `a6b3a57512ca6d5c6aa5b68f74f736195021f96e`, tag
`v1.87.4.0`, package `1.87.4`, MIT. Their broad installers, mutation-capable
orchestration, browser daemons, planning/shipping flows, and duplicated
procedures add no required V1 capability. Both are rejected below.

### DSH and Dagu disposition

DSH remains rejected for active V1. The exact current source/audits still show
the reproducibility and structured-worker boundary failures; the production
coordinator uses the qualified Arena OpenCode path instead.

Dagu remains rejected for active V1. The current coordinator already owns the
bounded lifecycle and authority boundary. Existing Dagu evidence includes
stale-running recovery, explicit retry, repeated external-side-effect risk,
and incomplete DSH composition. A standalone Windows Dagu script exists, but
that does not justify production integration.

### Capability distribution

| Capability | Distribution status | Current boundary/fallback |
|---|---|---|
| OpenCode 1.18.31 | `PROVEN/INTEGRATED` / required for OpenCode Delivery | External prerequisite; bounded version health and explicit executable override |
| agent-analyzer 0.8.1 | `PROVEN/INTEGRATED` / optional-degradable | Derived HEAD/version-keyed cache; missing executable fails soft for consumers |
| Selected Superpowers assets | `PROVEN/INTEGRATED` / optional-degradable | Pinned selected files only; no global bootstrap |
| rust-analyzer 1.95.0 | `PROVEN/INTEGRATED` / target-specific | Native OpenCode LSP; project-native diagnostics fallback |
| TypeScript LSP 4.3.3 | `PROVEN/INTEGRATED` / target-specific | Native OpenCode LSP; frontend build/typecheck fallback |
| Context7 4.1.1 path | `PROVEN/INTEGRATED` / optional-degradable | Bounded technical documentation input only |
| Playwright Test | `PROVEN/INTEGRATED` / target-specific | Deterministic web evidence when a candidate requires it |
| Playwright MCP 0.0.82 | `PROVEN/INTEGRATED` / optional-degradable | Exploratory evidence, never acceptance PASS |
| Native Tauri WebdriverIO | `PROVEN/INTEGRATED` / target-specific | Toolchain qualified; native execution blocked on this host |
| Chrome DevTools MCP 1.9.0 | `REJECTED/NOT REQUIRED` as default V1 | No material capability beyond existing Playwright paths |

OpenCode, analyzer, skills, LSP, Context7, and browser helpers are not silently
installed by the package. Their discovery/remediation state is explicit and
the quality path degrades to project-native checks where defined.

## ENVIRONMENT-BLOCKED

### Current-HEAD Windows qualification

The current Linux host cannot prove current-HEAD Windows frontend/Rust/Product
OS/quality-stack tests, Job Object runtime, paths/Unicode/long-path behavior,
OpenCode/analyzer/LSP discovery on Windows, WebView2 GUI behavior, or NSIS
packaging. `x86_64-pc-windows-msvc` cross-checking also stops at the missing
MSVC `lib.exe`; `makensis`, Wine, and PowerShell are unavailable here.

A historical Windows workflow run proved an earlier checkpoint's Rust suite,
Job Object/path fixtures, cargo check, and unsigned NSIS artifact. It is not
current-HEAD evidence after Q1/08A source changes. A Windows model task is also
blocked by the absent Windows runtime in this environment.

### Native GUI functional smoke

GTK 3.24.33 and WebKitGTK 2.50.4 are installed and X11 is available, but
`/dev/dri` is absent and the renderer is unaccelerated `llvmpipe`. A bounded
native bootstrap reached the Consensus Arena window with
`WEBKIT_DISABLE_COMPOSITING_MODE=1`; the complete founder-input → phase/status
→ owner-question/result flow could not be safely completed. No native
`consensus-arena` or WebKit process remained after cleanup. The functional
native GUI result is therefore environment-blocked, not substituted by
Playwright evidence.

### Provider portability and frontier consultation

Already configured non-Zen OpenCode paths were attempted without exposing
credentials:

- NVIDIA `nvidia/deepseek-ai/deepseek-v4-flash-0731`: bounded task timed out,
  peak RSS `306896 KB`, no mutation;
- Google `google/gemini-3.1-pro-preview`: bounded task timed out, peak RSS
  `604212 KB`, no mutation.

Neither returned a credential error, but neither produced a deterministic
inference/file-task result. Non-Zen portability and a model-backed frontier
consultation therefore remain environment-blocked by provider runtime
availability. They are not promoted to V1 dependencies.

### Native Tauri WebdriverIO execution

The disposable toolchain was `@wdio/tauri-service 1.4.0` with WebdriverIO
`9.31.9`. Embedded execution could not find a registered
`tauri-plugin-wdio-webdriver`; the external path lacked `tauri-driver` and
`webkit2gtk-driver`. The host also lacks `/dev/dri`. This is environment
blocked for native functional execution; Playwright results remain separate.

### Host package install/uninstall and full native launch

Disposable-root install/purge and extracted launch are proven. Installing into
the host package database and completing the native GUI functional sequence
require a capable disposable OS/graphics environment and are not claimed from
this host.

## REJECTED/NOT REQUIRED

- GitHub MCP as a default V1 dependency: interoperability is proven, but
  direct GitHub/OpenCode retrieval is sufficient and has lower carrying cost.
- ECC and gstack: source-qualified MIT assets are redundant or conflict with
  Arena-owned authority and workflow boundaries.
- Full AgentSys orchestration and global Superpowers bootstrap: rejected by
  the accepted Q1 architecture.
- DSH as the active V1 worker: reproducibility and structured runtime evidence
  remain materially challenged.
- Dagu as the active V1 coordinator: no current requirement reduces complexity
  enough to justify a second workflow engine.
- Chrome DevTools MCP as a default dependency: no material required capability
  beyond Playwright Test/MCP and existing browser evidence boundaries.
- Bundling large external model, analyzer, LSP, MCP, or browser runtimes into
  the Tauri package: the low-spec target needs explicit external prerequisites,
  optional degradation, and bounded remediation instead.

## Verification record

The closure verification set is:

- `cargo fmt --all -- --check` — PASS after narrow formatting remediation;
- `cd /home/kasun/Music/arena/consensus-arena/src && npm run build` — PASS;
- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check` —
  PASS with the existing warning baseline;
- focused Q1/08A execution-profile, containment, repo-intelligence,
  verifier/Delivery, coordinator, and authority regression tests — PASS as
  recorded in the 08A closure and rechecked after formatting;
- current-HEAD Linux `.deb` build/metadata/extraction probe — recorded above;
- disposable-root `.deb` install/purge probe — PASS;
- official GitHub MCP read-only initialize/list/search/read/no-write probe —
  PASS;
- scoped secret scan, `git diff --check`, and orphan-process inspection —
  PASS.

No claim in this audit grants ProductAuthority, acceptance, Verified, or Apply
authority to a quality tool, provider, MCP server, skill, analyzer, LSP, or
browser helper.
