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

The Linux source checks and frontend build pass, but no native Windows machine
was available. This document separates source readiness from the runtime proof
still required before Windows support is claimed.
