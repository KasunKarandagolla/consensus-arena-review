# Delivery Windows-readiness pre-audit

**Date:** 2026-09-16

**Scope:** Source-level audit of the current Delivery worker, verifier, Git,
state, and frontend paths. This is not native Windows runtime proof.

## Source-ready

- Delivery derives worktree/evidence locations with `PathBuf`, uses
  `current_dir`, and passes worker/verifier arguments as vectors rather than
  shell strings.
- Candidate worktrees are derived from the Tauri app-data directory and a
  generated delivery ID.
- Verification rejects absolute, drive-qualified, and parent-traversing
  relative paths.
- Delivery contains no `/tmp`, Bash, Unix process-group, or Unix-signal
  dependency.
- The folder picker is provided by the Tauri dialog plugin.

## Fixed in this milestone

- Verification command IDs are now restricted to safe receipt-file stems,
  including Windows device-name protection.
- DSH timeout handling now explicitly kills and waits for the child before
  continuing cleanup.
- INCONCLUSIVE resume preserves the uncommitted candidate so the frozen
  verifier can be rerun against the candidate it previously identified.
- Delivery UI state events reject a different session ID once a current run is
  established, and the initial state fetch cannot update an unmounted view.

## Concerns requiring native Windows proof

- `dsh.exe` versus npm `.cmd` shim resolution, plus `npm.cmd`, `npx.cmd`, and
  `gradlew.bat` verification invocation.
- Git worktrees and app-data paths containing spaces, Unicode, drive letters,
  long paths, or different volumes.
- Antivirus/indexer file locks during worktree operations, state/evidence
  writes, cleanup, and Apply.
- Process-tree cleanup for DSH, Node, Cargo, npm, and verifier descendants
  after timeout and abort.
- Windows WebView2 startup, Vite localhost lifecycle, and folder-picker
  behavior.
- SQLite/restart behavior under Windows file locking.

## Remaining source concern

Git operations used by Delivery are still unbounded. Hooks, signing prompts,
file locks, or antivirus interference can therefore wait indefinitely. The
next source hardening block should add bounded Git execution with explicit
cleanup, followed by native Windows qualification. The current environment
cannot prove either behavior.

## Evidence boundary

## Native Windows qualification update

Workflow [35200869335](https://github.com/KasunKarandagolla/consensus-arena-review/actions/runs/35200869335)
completed successfully on Windows Server 2025. It ran the full Rust suite
(400 passed, 2 ignored), Windows `cargo check`, frontend build, formatting,
the process containment fixtures, Git and SQLite path tests, and produced the
unsigned NSIS installer artifact `Consensus Arena_0.1.0_x64-setup.exe`
(5,568,641 uploaded bytes). The pinned Node `v22.22.2`/npm `10.9.7` DSH
prerequisite install also passed without a provider credential. This proves
Windows build/backend/packaging qualification; model-backed DSH execution and
WebView2 GUI E2E remain unproven.
