# Delivery Loop V1 Runtime Dogfood Audit

Date: 2026-09-15

## Scope

This session inspected the current Build/Delivery implementation, attempted the
native Linux launch path, exercised a disposable Git/verification fixture, and
repaired concrete lifecycle and IPC failure surfaces. Dagu was not integrated.

The repository was already dirty at session start. Existing user changes,
including the current Delivery implementation and historical-audit deletions,
were preserved; the historical audit deletions were not included in the
checkpoint commit.

## Proven in this session

### Runtime prerequisites

- Repository root: `/home/kasun/Music/arena/consensus-arena`.
- Git `2.34.1`, Cargo `1.95.0`, Node `v22.22.2`, npm `10.9.7`, and Tauri CLI
  `2.11.2` are available.
- `DISPLAY` is set and the host reports an X11 session.
- `dsh` is not available on `PATH`. `ARENA_DSH_EXECUTABLE`, `ARENA_DSH_HOME`,
  `NVIDIA_API_KEY`, `OPENAI_API_KEY`, and `ARENA_DSH_API_KEY` were unset.
  No secret value was printed.
- Source expects DSH to accept `--profile headless --patch <patch-file>
  <prompt>`, use the candidate worktree as its current directory, and write
  `.arena-runtime/result.json` with schema version 1. No DSH version is pinned
  by the current source.
- Source resolves Delivery persistence from the Tauri app-data directory:
  `delivery-state.json`, `delivery-worktrees/`, `dsh-runtime/`, and
  `delivery-evidence/`.

### Build and test baselines

- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check`
  exited 0. It produced warnings only.
- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo test delivery -- --nocapture`
  exited 0: 3 Delivery tests passed.
- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo test verification_output_is_bounded -- --nocapture`
  exited 0: 1 verifier-output test passed.
- `cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo test -- --nocapture`
  exited 0: 349 tests passed, 0 failed.
- `cd /home/kasun/Music/arena/consensus-arena/src && npm run build` exited 0.
  TypeScript and Vite completed; Vite transformed 1711 modules and emitted
  the production bundle.
- `cd /home/kasun/Music/arena/consensus-arena && git diff --check` passes
  after the current documentation whitespace was corrected.

### Disposable fixture

Fixture path: `/tmp/consensus-arena-delivery-dogfood-20260915`.

- Starting fixture `HEAD`: `1a20f48a68d8470d91a94448dc596c8f6e4ea287`.
- Product request: change `greet.py` to print `hello arena`.
- Deterministic acceptance command: `python3 tests/test_greeting.py`.
- Isolated candidate branch: `arena-delivery/dogfood`.
- The acceptance check failed against the starting candidate with exit 1 and
  output `hello`, demonstrating that the check distinguishes the requested
  behavior.
- The same check passed with exit 0 after the candidate change.
- Candidate commit: `9db29bb76f3e85872b41626df75047be4504ca0c`.
- A fast-forward Apply in the disposable fixture reached the candidate commit
  and left the original fixture clean. The candidate worktree was then removed
  as disposable cleanup; the candidate branch and commit remain in that
  fixture.

This fixture was manually driven and did not prove that Arena authored or froze
the acceptance material. It is supporting Git/verifier evidence only.

## Source-confirmed but not runtime-proven

- Build starts through the React Setup view and `start_delivery` IPC, without
  participant/WebView setup.
- `start_delivery` admits only a clean repository root, records `HEAD`, and
  creates an `arena-delivery/<uuid>` worktree.
- Acceptance authoring, `.arena-runtime` removal, acceptance commit, protected
  file hashes, conservative verification profile validation, independent
  verifier receipts, bounded three-attempt repair, durable questions, restart
  recovery, exact-owner abort, and conservative Apply are implemented in the
  inspected source.
- Rust command returns and frontend parsing match for Delivery state: Rust
  returns a JSON-serialized `String`, and the frontend explicitly calls
  `JSON.parse()`.
- AskUser options, custom submission, Escape, and backdrop dismissal all use
  `provide_user_answer`; failed answer IPC now leaves the modal available for
  retry instead of hiding a potentially pending waiter.
- Protected acceptance hashes and frozen verification commands are used for
  the verifier-to-Verified decision; worker prose and worker exit status are
  not sufficient.

## Failed / repaired

### Admission side effects could race

Before repair, `start_delivery` created the candidate worktree and persisted
state before taking `SessionRuntime` admission. Concurrent Build invocations
could therefore create an orphaned candidate before one launch was rejected.
Admission is now reserved before worktree, session-row, or delivery-state side
effects, then handed to the spawned Delivery task.

### Verification output and timeout handling were weaker than documented

Before repair, verifier checks used `Command::output()` without an explicit
kill-on-drop child or bounded pipe readers. A noisy or timed-out check could
consume unbounded memory and leave timeout cleanup ambiguous. Verification now
uses piped concurrent bounded readers, `kill_on_drop(true)`, explicit kill and
wait on timeout, and an `inconclusive` receipt status for timeouts and
infrastructure failures. The top-level receipt now preserves `inconclusive`
when any required check lacks valid executable evidence instead of collapsing it
into `fail`.

### AskUser answer failure could hide the modal

Before repair, any `provide_user_answer` IPC error cleared the modal in
`finally`, even though the backend sender might still be waiting. The modal now
clears only after a successful answer IPC call and resets its local guard on
failure. Its focus timer also has cleanup.

The first post-repair Rust compile caught a moved mutable task borrow in the
new bounded-reader helper; that was corrected before the successful targeted
tests, full suite, and `cargo check` above.

## Native GUI and DSH result

`timeout 120s npm run tauri dev` successfully started Vite at
`http://localhost:1420` and began compiling the Tauri binary, but timed out
during the final native compilation at the `consensus-arena` target. A direct
`timeout 300s cargo build --no-default-features` also expired during native
binary linking. No completed current-source native GUI session was available
for interaction.

Because DSH is absent and no configured compatible AI endpoint was available,
the session did not run acceptance authoring, implementation, repair, or
Arena-owned verification through a real DSH worker. No GUI proof is claimed.

## Delivery invariant review

- CLEANBASE: source-confirmed; fixture started clean. Arena GUI runtime not
  proven.
- WORKTREE: source-confirmed; fixture used an isolated branch/worktree.
- ACCEPTANCEFREEZE: source-confirmed only; not runtime-proven through Arena.
- PROTECTEDACCEPTANCE: source-confirmed only; no real worker run.
- INDEPENDENTVERIFY: source-confirmed and verifier unit-tested; no Arena run.
- SAMECHECK: source-confirmed only; fixture reran the same command manually.
- BOUNDEDREPAIR: source-confirmed; no real repair cycle.
- DURABLEQUESTION: source-confirmed only; no real worker question.
- ASKDISMISS: source-confirmed from the frontend paths; no native modal run.
- EXACTABORT: source-confirmed from `SessionRuntime`; no active GUI task.
- NOSECRETS: source/config inspection found no printed or fixture secret.
- SAFEAPPLY: source-confirmed; disposable fixture demonstrated a fast-forward
  Apply equivalent, not the Arena IPC command.
- CROSSPLATFORM: source uses native path APIs and explicit process arguments;
  only Linux compile/fixture evidence exists.

## Still unproven

- Full `real Build UI → start_delivery → DSH → verification → Verified →
  explicit Apply` path.
- Actual DSH version compatibility and NVIDIA/OpenAI-compatible network run.
- Acceptance authoring and protected-hash enforcement against a real worker.
- Failure/repair, owner question/restart, abort, and Apply refusal paths through
  the native app.
- Native Windows build and runtime qualification.
- Resource behavior of the complete Tauri + DSH path on the target machine.

## Cross-platform status

Linux source compilation, frontend production build, Git fixture behavior, and
native launch initiation were exercised. Windows remains pending; no Linux-only
shared architecture was added in this session.

## Dagu carrying-cost signal

Inspection shows meaningful Arena-owned carrying cost in durable phase
orchestration, state persistence/recovery, root-question handoff, exact-owner
abort coordination, bounded retry bookkeeping, verifier evidence correlation,
and conservative Apply preconditions. These mechanics remain current V1
authority and were not refactored toward Dagu. They define the baseline a later
narrow Dagu qualification must beat.
