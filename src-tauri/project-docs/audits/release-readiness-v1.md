# V1 release readiness — 2026-09-16

**Scope:** Source-to-artifact and first-run audit. This is not a release
approval, native GUI qualification, or security sign-off.

## Build and distribution state

- Tauri product version is `0.1.0`; the configuration declares one desktop
  window and no updater, deployment target, or bundled DSH/Node runtime.
- A local Linux Debian bundle build was started with
  `npm run tauri build -- --bundles deb`, but was interrupted during optimized
  Rust compilation; no `.deb` artifact was produced. Even a successful package
  build proves compilation/packaging only, not that the artifact launches or
  its WebView renders correctly.
- The current frontend production output is approximately 455 kB before the
  native bundle (about 130 kB gzip for JS+CSS). There is no current-source
  Windows artifact result yet.
- Tauri's Linux package requires the target system's WebKitGTK/GTK runtime and
  related native libraries; see the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
  and [Debian bundle requirements](https://v2.tauri.app/distribute/debian/).
  Windows packaging uses WebView2 and is not native GUI proof by itself; see
  [Tauri Windows installer guidance](https://v2.tauri.app/distribute/windows-installer/).

## First-run prerequisites and failure behavior

- Git is an external prerequisite for Build mode. Production admission uses
  the selected repository's Git root, clean-base checks, worktree creation,
  and Apply. A missing/broken Git executable rejects admission rather than
  qualifying a Delivery session.
- DSH remains the provisional external V1 prerequisite: the owner/admin would
  be responsible for installing and maintaining the expected runtime; Arena
  does not install, bundle, or auto-update it. The frozen compatibility target
  is `@deepseek-ai/dsh@0.1.5-rc.1`, declared DSH component ranges
  `^0.1.5-rc.1`, `@mstar-harness/dsh@3.8.3`, Node `v22.22.2`, model
  `meta/muse-glimmer-30b`, `headless` profile, and schema-1
  `.arena-runtime/result.json`. The required lockfile SHA256 is
  `1297ec9257567a85c5a653734979256e6958a2c1235079c62fdc5bb9f2505887`.
- Runtime resolution uses `ARENA_DSH_EXECUTABLE` or `PATH`, optionally
  `ARENA_DSH_HOME`. Current detection checks `dsh --version` against the
  expected top-level version and requires `--profile headless --help` to exit
  successfully; it does **not** prove the Node version, dependency tree, or
  worker result compatibility. Missing/incompatible DSH is reported in Build
  setup and disables Start; backend admission repeats the check. No supported
  owner installation guide or post-qualification update policy exists yet.
- The exact frozen lockfile/runtime was not reproduced, and both required Muse
  runs were therefore not attempted (see
  `delivery-v1-backend-runtime-qualification.md`). This is a release gate, not
  a reason to package an arbitrary latest DSH.
- Build mode requires a primary Agent Brain API key, base URL, and model. The
  setup view now makes this requirement visible and saves the entered
  configuration before starting. Consult additionally requires a system
  prompt. Missing configuration should stop start-up with an actionable
  message; no API key value is included in diagnostics here.
- DSH/model-backed Build was not run in this qualification because the exact
  worker runtime was unavailable. First-run provider failure, worker UX, and
  model-backed completion therefore remain unproven.

## State, evidence, diagnostics, and security gaps

- Tauri app-data stores application settings, Delivery state/evidence, and
  rotating debug logs. Logs are written to the OS app-data location; the
  product does not yet provide a user-facing diagnostics export/location or
  a documented retention/cleanup policy.
- The current primary Agent Brain credential is stored in SQLite as ordinary
  text. A `keyring` plugin appears in configuration but is not registered as
  a working credential-storage path. Before nontechnical-owner release, either
  implement and verify secure credential storage or make an explicit,
  informed acceptance of this risk. Do not imply that the configured plugin
  declaration protects the saved key.
- No automatic updater is configured. Release/update assumptions, signing,
  provenance, and a supported upgrade/migration path remain product/release
  decisions.
- Raw verifier stdout/stderr are discarded after capture; the named evidence
  files contain only buffered-byte-count notices (which may include a
  truncation marker), not verifier diagnostics. Other evidence sources still require
  sensitivity handling and are not covered by this measure.
- DSH is launched with explicit argv, a bounded timeout, and the API key in its
  required process environment. It runs with the owner's filesystem identity;
  this is not an OS sandbox, and killing/waiting for the direct process does
  not yet prove that all descendant tools have exited.

## V1 external DSH decision boundary

The current source recommendation is to retain DSH as an **external
prerequisite** rather than bundle Node, DSH, models, and their update/security/
legal surface in the V1 installer. This is not a durable DECISIONS.md change
or a release-ready contract: the exact package tree/lock must be reproducible,
version detection must cover that tree, and setup/update guidance and failure
messages must be tested on supported OSes. If the installation burden is not
acceptable for the intended owner, present the packaging-versus-external-
prerequisite fork for owner review; do not silently add an installer.

The local `x86_64-pc-windows-msvc` cross-target check also stopped in `ring`'s
custom build because this Linux host lacks Microsoft's `lib.exe`; the Arena
crate was not reached, so this is not a Windows compile result. The focused
Windows workflow is present but awaits a reviewed/pushed checkpoint. A final
Debian package result, artifact size/dependencies, native Windows evidence,
and final source SHA remain pending. No release is approved by this audit.

## Qualification amendment — 2026-09-17

The current DSH reproducibility gate established a clean-install A/B package
tree match under the new lock documented in
`audits/dsh-runtime-reproducibility/dsh-runtime-reproducibility.md`, but the
headless help probe timed out and no model-backed Muse task was completed.
The historical lock remains unreproduced; DSH is therefore materially
challenged and triggers the Astra consultation packet rather than a durable
external-prerequisite decision.

Windows workflow [35200869335](https://github.com/KasunKarandagolla/consensus-arena-review/actions/runs/35200869335)
passed the full Rust suite, Windows `cargo check`, frontend build, path and
SQLite tests, process fixtures, the pinned Node/npm DSH prerequisite probe
without credentials, and unsigned NSIS packaging. The uploaded installer was
`Consensus Arena_0.1.0_x64-setup.exe` (5,568,641 bytes). This is Windows
build/backend/package evidence; model-backed DSH and WebView2 GUI E2E remain
unproven. The Linux Debian package build is still running on the qualification
host and has not yet produced an install-tested artifact.

The secure credential migration and diagnostics-retention work are covered by
the permanent secure-storage audit. Linux native keyring round-trip passed;
Windows keyring access and a real installed GUI remain separate evidence
items. No release approval is implied by this amendment.
