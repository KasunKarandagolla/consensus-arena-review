# Delivery Loop V1 Post-Implementation Audit

Date: 2026-09-14

## Scope

This audit covers the first Build vertical slice in the current source tree.
Consultation, browser transport, participant injection, and the existing AskUser
popup remain separate paths.

## Invariant review

- Consult mode still uses `start_session` and the existing session runner.
- Build mode has no participant or leader requirement and does not open model
  WebViews.
- `start_delivery` accepts only a clean Git repository root, records `HEAD`,
  and creates one `arena-delivery/<short-id>` worktree under app data.
- DSH is launched as an independent `tokio::process::Command` with
  `--profile headless`, the worktree as cwd, bounded output, timeout cleanup,
  and `kill_on_drop(true)`. No shell wrapper or worker-session resume is used.
- The authoring run is required to create executable acceptance material before
  implementation. Arena removes `.arena-runtime`, commits the acceptance
  freeze, records protected paths/hashes, and validates structured commands.
- Arena runs the frozen commands directly from a conservative program
  allowlist. Worker prose, exit code, and result summary cannot mark Verified.
- Protected acceptance changes are restored from the acceptance commit and the
  attempt is forced to fail.
- The same profile and command list are rerun after repair. Implementation and
  repair are capped at three attempts.
- `needs_user` is persisted as `WaitingForUser` before the existing
  `agent-ask-user` event is emitted. `provide_user_answer` consumes the shared
  oneshot sender with `take()`. Restart recovery re-emits the persisted
  question; dismissal is recorded as `Cancelled` and fails safely.
- `SessionRuntime` is the delivery task ownership guard. Abort stops the exact
  owner before marking the state `Cancelled`; child ownership provides worker
  cleanup.
- Verified work is committed only in the isolated worktree. Apply requires a
  verified candidate, clean original checkout, unchanged original `HEAD`, and
  a successful non-forcing fast-forward.
- No API key is placed in delivery state, prompts, evidence, or logs. DSH
  configuration is written under app data and references an environment
  variable.
- Native `PathBuf` and explicit program/argument vectors are used; no `/tmp`,
  shell pipeline, Unix process group, or platform-specific path assumption was
  added.

## Verification evidence

The source-level unit coverage includes delivery-state JSON round-trip,
deterministic phase transitions, attempt limits, worker complete/needs-user
result parsing, verification program/cwd rejection, and protected-file hash
change detection. Final command results are reported with the implementation
handoff.

## Known V1 limitations

- DSH is a prerequisite; Arena does not install or package it. The executable
  may be supplied with `ARENA_DSH_EXECUTABLE`, and an existing DSH home may be
  supplied with `ARENA_DSH_HOME`.
- Apply updates the original checkout only after explicit user action. The
  evidence/worktree are intentionally retained.
- The current runtime has not been exercised through a full Tauri GUI session;
  the external DSH/NVIDIA smoke path is reported separately if available.
