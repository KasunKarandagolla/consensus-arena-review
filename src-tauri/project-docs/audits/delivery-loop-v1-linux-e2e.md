# Delivery V1 Linux Runtime and Authority Audit

**Date:** 2026-09-15
**Scope:** verifier authority hardening, admission rollback, DSH qualification,
native Linux launch, and a real Build UI dogfood attempt.
**Starting source checkpoint:** `5eac1add7f4bf4d8de817092cdf7269d705987b0`

This audit records only evidence obtained in this session. The permanent
2026-09-14/15 audits remain historical records and are not back-edited.

## Proven in this session

### Source and targeted tests

- Required verifier outcomes now have explicit precedence: a valid behavioral
  failure wins over inconclusive evidence; otherwise inconclusive evidence wins
  over pass; pass requires at least one executed required check and no failure
  or inconclusive result.
- Empty required-check profiles, duplicate command IDs, empty protected paths,
  and duplicate protected paths are rejected before verification.
- An inconclusive verifier receipt stops Delivery in an owner-visible Failed
  state without sending the receipt through the product-code repair worker.
  Resume retries the frozen verification when the last receipt was
  inconclusive.
- Admission rollback is bounded to the new session ID, candidate worktree,
  candidate branch, delivery row, and state-file bytes captured before
  admission. SessionRuntime remains the ownership authority.
- Focused verifier tests: 7 passed. Delivery tests: 4 passed. The full Rust
  suite passed 351/351, and `cargo build` completed for the native binary.

### DSH qualification

- Local history and project artifacts did not recover an exact prior DSH
  package version.
- The explicit qualification candidate was `@deepseek-ai/dsh@0.1.5-rc.1`,
  selected from official npm metadata as the current `latest` tag on the
  qualification date. It was installed only under `/tmp`, not into Arena and
  not as a product dependency.
- The executable reported `0.1.5-rc.1`.
- `dsh --help` and `dsh --profile headless --help` confirmed the expected
  positional task contract. A real patch-overlay boot using Arena's generated
  provider/model shape also reached the headless profile.
- A bounded disposable coding run reached the configured NVIDIA-compatible
  endpoint but returned `401` with no body before changing the fixture. No
  Arena result file was created, and no credential appeared in captured output.

### Native Linux application

- `npm run tauri dev` started Vite on `http://localhost:1420/` and launched the
  current native Tauri binary.
- The native window titled `Consensus Arena` appeared on X11. WebKit network
  and web-process children were observed.
- The current-source native debug link completed in `12m52s`; the earlier short
  timeout was therefore a slow-link observation, not a native build failure.
- The real Build UI was reached from the running application and rendered its
  `New session` screen with the Consult/Build selector.

## Failed / repaired

### Verification authority

The previous verifier treated mixed `FAIL + INCONCLUSIVE` evidence as
inconclusive because the inconclusive branch ran first. It also allowed an
empty profile to pass through an initially-true accumulator. Delivery sent
inconclusive receipts to the normal repair path. These were repaired in source
before runtime qualification and covered by focused tests.

### Admission failure window

The previous `start_delivery` path acquired SessionRuntime and then returned
from worktree, database, state-persistence, or launch failures without
reconciling all new artifacts. Bounded exact-target rollback now removes the
new worktree/branch, delivery row/session row, and restores the prior state
file/in-memory slot. No failure-path test was able to inject every filesystem
and SQLite error in this native session; source review and the targeted state
tests support the implementation, but exhaustive fault injection remains open.

### Frontend lane transition

The Build entry point did not reset `activeMode` or stale delivery state when
starting a new session. The fix keeps a new session in the Consult setup lane
until the owner explicitly selects Build, while preserving the existing
delivery recovery surface.

## Source-confirmed but not runtime-proven

- Clean-base admission, exact worktree ownership, acceptance authoring/freeze,
  protected hashes, independent verifier authority, same-check repair,
  durable questions, exact-owner abort, and conservative Apply remain
  confirmed by source and Rust tests, not by a completed model-backed GUI run.
- Rust-to-TypeScript Delivery state parsing uses `JSON.parse` for the
  JSON-serialized command result, and delivery-state listeners are registered
  with cleanup. WaitingForUser is persisted before `agent-ask-user`; persisted
  recovery provides a surface if the owner UI is absent.
- The Arena worker adapter uses explicit argv, candidate cwd, bounded output,
  timeout cleanup, `ARENA_DSH_API_KEY`, optional `ARENA_DSH_HOME`, and
  `.arena-runtime/result.json` schema version 1.

## Genuine Arena Delivery E2E

Not completed. The native Build UI was opened, but the run could not proceed to
folder selection/start because the external worker credential produced HTTP
401 in the independent DSH qualification. No GUI run is represented as
Verified, Applied, or successful. The earlier manual Git fixture remains
supporting Git/verifier evidence only and is not Arena orchestration proof.

The following paths therefore remain unproven in this session: acceptance
authoring through the GUI, implementation, independent verification,
FAIL-to-repair, INCONCLUSIVE GUI handling, owner question/restart, active abort,
and Apply refusal from dirty/changed original state.

## Invariant review

| Invariant | Status | Evidence |
|---|---|---|
| CLEANBASE | source-confirmed | admission validation and existing tests |
| WORKTREE | source-confirmed | explicit candidate cwd/worktree path |
| ACCEPTANCEFREEZE | source-confirmed | supervisor ordering and persisted commit |
| PROTECTEDACCEPTANCE | source-confirmed | protected hashes and restoration path |
| INDEPENDENTVERIFY | source + targeted tests | verifier owns receipt verdict |
| SAMECHECK | source-confirmed | frozen profile reused after repair |
| BOUNDEDREPAIR | source + targeted tests | three-attempt cap |
| DURABLEQUESTION | source-confirmed | persist before event and recovery path |
| ASKDISMISS | source-confirmed | all popup dismissal paths answer `Cancelled` |
| EXACTABORT | source-confirmed | SessionRuntime owner/child stop ordering |
| NOSECRETS | runtime smoke | credential not present in captured DSH output/evidence |
| SAFEAPPLY | source + prior manual fixture | no GUI Apply proof this session |
| CROSSPLATFORM | source-confirmed | no Linux-only shared Delivery primitive added |

## Resource and environment observations

Git 2.34.1, Cargo 1.95.0, Node 22.22.2, npm 10.9.7, Tauri CLI 2.11.2, X11,
Python 3, and the native Linux dependencies were available. Disk space was
ample. The machine reported about 3.7 GiB RAM and 4 GiB swap; during native
linking the Rust process reached roughly 1.2 GiB resident memory. The DSH
qualification package was large (498 packages initially, then required peer
resolution) and was kept outside the repository.

## Still unproven

- A valid working NVIDIA/OpenAI-compatible credential for the selected DSH
  candidate; the available local credential produced HTTP 401.
- Model-backed standalone DSH code change and result-file behavior.
- Genuine Arena GUI Delivery success and all requested negative/continuation
  branches.
- Windows native build/runtime and Windows process/worktree qualification.
- Whether DSH's current release cadence and external dependency footprint are
  acceptable for repeatable development qualification. Arena still does not
  package or install DSH.

## Dagu boundary

Dagu was not integrated. The real source/runtime review identifies the current
carrying-cost mechanics as Arena-owned persisted delivery state, restart
recovery, AskUser oneshot/event coordination, bounded repair transitions, and
subprocess ownership/cleanup. Those are candidates for a later falsifiable
Dagu comparison, but their replacement is out of scope until DSH credentials
and a genuine Delivery baseline are qualified.

## Cross-platform status

Linux native launch and Build UI rendering are now runtime-proven. Windows
qualification remains pending; no Linux-specific implementation was added to
the shared Delivery architecture.
