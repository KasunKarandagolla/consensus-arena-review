# Consensus Arena — Reliability and Safety Invariants

## Source priority

Runtime/source evidence > latest audit > docs > old plans.

Permanent audits are never deleted. They record what was actually proven at a point in time.

## Current programme closure — 2026-09-17

- **DSH worker:** materially challenged. Two clean current installs matched,
  but Arena's five-second headless profile probe timed out twice. Neither Muse
  task nor model-backed Delivery ran. Trigger A is ready; no Astra response is
  claimed.
- **Delivery:** source/targeted-test boundaries are retained, but the complete
  production path and worker-dependent reliability matrix remain unrun.
  Production Git tests cover clean admission/worktree paths, protected-file
  restoration, and dirty/changed/non-fast-forward Apply refusals.
- **Process containment:** Arena clears the DSH environment and forwards an
  allowlist plus its configured Agent Brain credential. Unix workers run in a
  process group held by a live guard until group termination; Windows workers
  use a Job Object. Linux synthetic tests cover normal leader exit with an
  orphan descendant, timeout termination, drop, and SessionRuntime owner
  abort. The fixture descendant stopped within the test's three-second poll
  after collect returned; this does not prove every descendant was reaped or
  had stopped before return. Process groups and Job Objects are cleanup
  boundaries, not sandboxes against a same-user worker that escapes or kills
  the guard. No real DSH process tree was observed. Windows process tests
  remain pending native CI.
- **Credentials:** primary, fallback, secondary, and Hackathon model secrets
  are migrated to OS credential storage by source. Mock-store tests cover
  successful and failed migration, safe serialization, preservation, and
  removal. Native Windows coverage awaits CI; native Linux Secret Service
  requires an active unlocked desktop session.
- **Dagu:** standalone result is **INCONCLUSIVE — post-V1**. Linux recovery
  needed explicit reconciliation after hard interruption and repeated an
  external side effect. Windows and DSH composition remain unproven.

## Consult-lane named risks

### BLOCKING
No `blocking_lock()` in async Rust or navigation callbacks.

### CHANNEL
Navigation callback is synchronous; do not use Tokio mpsc there where current design requires std sync channel.

### EVENTMATCH
Backend event names/payloads must exactly match frontend listeners.

### UNWRAP
No casual `.unwrap()` / `.expect()` in production live-session paths.

### STALERESPONSE
Accept browser responses only for the expected model/agent and turn/run identity.

### INITSCRIPT
Generic browser init script stays static/generic, not provider-specific logic baked into a global string.

### NAVCLOSURE
Do not capture stale agent identity into navigation callbacks; read runtime identity.

### ASKCHANNEL
Consume pending AskUser sender exactly once; avoid double-send/dead channel.

### ASKDISMISS
Every modal-close path must answer, including `Cancelled`.

### IPCPARSE
Frontend must parse JSON-string command results when Rust serializes them as strings.

## Delivery V1 invariants

### CLEANBASE
Do not start bounded delivery on a dirty base repository.

### WORKTREE
Implementation occurs in isolated candidate worktree, not original checkout.

### ACCEPTANCEFREEZE
Acceptance material is authored/frozen before implementation.

### PROTECTEDACCEPTANCE
Worker changes to protected acceptance content force attempt failure and restoration.

### INDEPENDENTVERIFY
Worker self-report cannot produce Verified.

### SAMECHECK
After repair, rerun the same frozen required checks/profile.

### BOUNDEDREPAIR
Current V1 caps implementation/repair attempts.

### DURABLEQUESTION
Persist `WaitingForUser` before emitting owner UI; restart re-presents persisted question.

### EXACTABORT
Abort exact `SessionRuntime` owner and clean up child worker before final cancelled state.

### NOSECRETS
Arena does not put the configured API key in structured Delivery state,
acceptance prompts, or worker summaries. Raw verifier stdout/stderr are not
persisted; evidence files contain only a buffered-byte-count notice (possibly
including a truncation marker) plus command status and exit metadata. The DSH
worker path uses a Unix process-group guard and a Windows Job Object; Linux
synthetic descendant tests cover normal exit, timeout, and owner abort, while
native Windows process-tree execution remains unproven. This does not establish
that every other evidence source is secret-free.

### SAFEAPPLY
Apply only verified candidate, clean original checkout, unchanged original HEAD, successful non-forcing fast-forward.

### CROSSPLATFORM
No Linux-only assumptions in shared Build architecture.

### RECEIPTIDENTITY
Every persisted verification receipt must identify the Delivery session,
implementation attempt, unique verification run, frozen acceptance commit,
candidate commit, and frozen profile hash. A PASS must remain tied to the same
clean Git candidate that the verifier executed; do not create a new Verified
commit after the checks.

### GITPROCESS
Delivery Git subprocesses have a bounded timeout and bounded captured output.
Output-reader failure is an error, never an empty/clean status. Killing the
direct Git process is not proof that hooks/helpers or other descendants have
terminated; native Windows process-tree qualification remains open.

## Product OS research authority invariants — Milestone 05B

### RESEARCHPROPOSAL
Researcher output enters Product OS only through Arena admission and is forced
to current `ResearchClaim` + `Unverified`. It cannot self-declare a verified
fact or write accepted authority directly.

### FACTVERIFIER
Finalization requires a distinct current Arena Fact Verifier work order in the
same project, source scope, authority revision, and lifecycle. Unknown,
wrong-role, cancelled, stale, superseded, or mismatched work fails closed.

### OWNERAMBIGUITY
Ambiguity admission is Arena-owned and conservatively owner-required. A
serialized resolver value or worker recommendation cannot downgrade the
Arena-owned classification. Only an adopted owner decision for the exact
question and admission revision can resolve it.

### PRODUCTRESTART
Product OS records and work-order metadata are persisted in the existing
`TranscriptStore`. On reopen, persisted work without a live `SessionRuntime`
lease becomes `ReconciliationRequired`; no completion or evidence adoption is
fabricated. A late result cannot attach to a cancelled or stale record.

This milestone proves the Product OS research boundary on Linux. The later M06
dogfood proves the fresh research-to-BuildPackage-to-Delivery path, but neither
milestone claims a same-user filesystem sandbox, Windows runtime parity,
package installation, or complete SSRF isolation.

## Product OS package currentness — Milestone 05C

The package handoff is fail-closed: reviewed scope invalidates a previously
adopted NarrowBuild direction; a current package requires the corresponding
adopted Owner decision, current typed architecture/reuse/risk references, and
the existing fingerprint/revision check. M05C proves the seven applicable
pre-implementation gates from reopened durable authority and proves a material
scope change returns the old package evaluation as `Stale`. It does not
qualify Delivery execution, GUI, Windows, packaging, or same-user sandboxing.

## Production coordinator invariants — Milestone 07

The coordinator is deterministic code around existing authorities, not a
second workflow engine. A run is durable before execution; semantic role
outputs remain proposals until typed Arena admission; an owner question is
persisted before the frontend is expected to answer; only a current passing
Build Package can enter Delivery; and Delivery alone can produce Verified.
Unknown, stale, cancelled, superseded, protected, or mismatched evidence
continues to fail closed through the existing Product OS and Delivery checks.

The fresh Linux M07 dogfood runtime-proves the complete production
research-to-Verified-candidate path. The coordinator's complete in-flight
restart/reconciliation matrix, end-to-end coordinator cancellation exercise,
explicit Apply, Windows process behavior, packaging, and GUI runtime remain
unproven. SessionRuntime currently serializes the semantic role wave; no
parallel-overlap claim is made.

## Verification semantics

Use three outcomes conceptually:

- **PASS** — identified candidate ran all required accepted checks and they passed.
- **FAIL** — a valid required check ran and demonstrated wrong behavior.
- **INCONCLUSIVE** — infrastructure error, missing evidence, skipped required scenario, incomplete coverage, malformed receipt, or inability to prove the candidate/check identity.

An owner waiver remains a waiver, not a fake PASS.

Delivery applies this precedence deterministically: a valid behavioral FAIL
wins over any INCONCLUSIVE result; otherwise INCONCLUSIVE wins over PASS. A
profile with zero required executable checks is rejected, and zero executed
checks can never produce PASS. INCONCLUSIVE stops the current run without
consuming a product-code repair attempt; Resume reruns the frozen checks.

Verifier evidence is written under a unique verification-run directory so a
resume does not overwrite a prior receipt. If a verification command leaves
the candidate worktree dirty or moves its HEAD, the receipt verdict is FAIL and
the supervisor refuses both repair-on-that-mutated-tree and Verified. The
HEAD/status comparison is sampled only before and after checks; transient
mutate-then-restore and ignored-file changes can evade it without OS isolation.

## Required-scenario protection

If using self-healing/generative testing tools, required scenarios must not silently disappear or become skipped while still yielding a green build.

Arena acceptance policy should validate required scenario coverage, not only process exit status.

## Permission rule

Protected external/irreversible action requires owner permission **before** action.

When using external workflow tools, verify their approval semantics rather than trusting feature names.

## OS containment

Git worktrees isolate repository history, not the operating system.

The DSH process group and Windows Job Object provide process cleanup boundaries,
not a sandbox for untrusted code: a same-user worker can attempt to leave its
group or interfere with the guard. Use a stronger filesystem/network sandbox
before claiming hostile-code safety. Windows cleanup parity also needs native
runtime qualification.

## Native Linux host qualification

For a graphics-capable native desktop follow this reproducible sequence:

1. In that desktop user's logged-in session, launch
   `npm run tauri dev` from
   `/home/kasun/Music/arena/consensus-arena`.
2. In a second terminal run
   `/home/kasun/Music/arena/consensus-arena/scripts/qualify-linux-native-runtime.sh`.
3. To include safe application-log aggregates, set `ARENA_LOG_FILE` to the
   current `consensus-arena.log` path before running the script. It prints only
   counts for IPC/invoke-related and error lines, never raw log contents.
4. Manually record whether the leader window and shared participant window
   render, whether the first frontend IPC calls complete, and any sanitized
   startup error code. This host check is not GUI E2E unless the Delivery
   sequence itself is completed through the rendered product.

The script reports GTK/WebKitGTK versions, `/dev/dri`, GL renderer, Vite HTTP
health, and process presence. It applies no rendering workaround flags and is
not a product startup change.

### Final Linux native dogfood instructions

This is pending until both a graphics-capable desktop and a qualified bounded
worker are available. On the logged-in Linux desktop:

1. From `/home/kasun/Music/arena/consensus-arena`, run `npm run tauri dev`.
2. In a second terminal run
   `/home/kasun/Music/arena/consensus-arena/scripts/qualify-linux-native-runtime.sh`.
   Confirm the leader and shared participant windows render and first frontend
   IPC calls complete. Record GTK/WebKit versions and GL renderer.
3. Create a new disposable Git repository whose parent directory contains
   spaces. Add one tiny deterministic test, commit a clean initial state, and
   keep the fixture outside the product repository.
4. In the rendered Arena Build UI, choose that repository and request one
   bounded, observable change. Do not invoke middle-stage commands manually.
5. Capture the actual Delivery/session ID, original HEAD, acceptance-authoring
   worker result, acceptance file paths, freeze commit, protected paths and
   hashes, verification profile/hash, later candidate commit, verifier receipt
   identity/verdict, and Apply result. Confirm the original checkout stayed
   clean and at its original HEAD until Apply, then fast-forwarded only after
   Arena showed Verified.
6. Record screenshots and sanitized app logs locally. Do not place credentials
   or raw provider output in the audit. This test proves GUI E2E only if the
   complete sequence is driven through the visible app and actual OS dialogs.

The current host does not satisfy these prerequisites. This procedure is
prepared for the final dogfood block and is not qualification evidence.

## Q1 process and resource safeguards

OpenCode profile configuration is temporary and injected through an explicit
allowlist after environment sanitization. The existing process-group and Job
Object containment remains the cancellation boundary. Repository analysis is
one cached run per repository HEAD and query output is bounded. Heavy LSP
profiles are intended to run one at a time on the target low-resource machine;
semantic and web roles do not start LSP.

On the current Linux host, `agent-analyzer` used approximately 25.7 MiB RSS
for an incremental update. A bounded OpenCode version probe measured about
101.6 MiB RSS. The matching rustup `rust-analyzer` now qualifies at about
170.5 MiB RSS for a disposable diagnostic/navigation session. The pinned
TypeScript language server qualifies at about 64.0 MiB RSS when its disposable
target uses bounded `noLib` configuration; full frontend build/typecheck
remains the fallback on constrained hardware. Heavy LSP profiles remain
single-slot only, and no second LSP-equipped worker is launched silently.
Plain OpenCode 1.18.31 and the Implementation/DebugRepair Superpowers tasks
are runtime-proven. The CandidateReview-shaped background path still returns
the upstream free-tier 403 and remains explicitly advisory/unavailable for
that run; no acceptance or Verified state depends on it.

The successful Rust test-link strategy uses one Cargo job, test debuginfo
disabled, GCC, and `--reduce-memory-overheads`; it measured `1374656 KB`
(about `1.31 GiB`) peak. This is a test-link measurement, not normal Arena
runtime. No concurrent heavy model/LSP/browser tasks are launched silently.

## 08B platform and build-resource closure

The current low-resource policy remains runtime-focused: heavy LSP/model/
browser work is single-slot, optional helpers degrade to project-native checks,
and the native GUI path is blocked on this host's missing `/dev/dri` graphics
device despite GTK/WebKitGTK being installed. The release `.deb` build is a
developer/package-link measurement and is recorded separately from normal
runtime RSS; it must not be used as a Product OS runtime performance claim.

Windows process cleanup and NSIS remain current-HEAD Windows-runner evidence.
Linux package extraction/launch does not prove Windows WebView2 or native
Tauri functional parity. Every external quality capability has an explicit
required, optional-degradable, target-specific, or environment-blocked status
in the 08B audit.

## M09A controller and test-link reliability

The controller now keeps `SessionRuntime` as the sole live session owner and
uses a subordinate serial resource claim for semantic roles. Claims are bound
to work-order ID and execution epoch; stale releases fail closed. Architecture
proposals and review packets are independent records but are executed
serially, so the low-memory host never receives overlapping exclusive role
holders. Snapshot is a pure read; restart/resume calls explicit reconciliation.

The supported focused Rust test command uses one Cargo job, disabled test
debuginfo, GCC, and `--reduce-memory-overheads`. The successful M09A pipeline
run measured `1,526,488 KiB` peak RSS (about `1.46 GiB`) and executed the new
controller logic. This is a test-link measurement, not an Arena runtime
performance claim. Heavy LSP/model/browser helpers remain single-slot and
degrade to project-native checks when resources are insufficient.

Decision-critical research claims are independently verified as a required
set; incidental verified claims cannot mask an unverified critical claim.
Typed owner decisions, packet-bound synthesis, and bounded remediation remain
advisory/controller inputs only. ProductAuthority, verifier, and Apply
authority boundaries are unchanged.

Restart reconciliation is represented explicitly by `Reconciling`; cancellation
uses `Cancelling` before `Cancelled`, and stop/pivot/block states are not
overloaded as successful completion. Stale packet hashes and execution epochs
fail closed. Bounded experiments are admitted only through a typed contract
with protected paths and deterministic result semantics.

## M10 specialist and capability reliability

Verified model selection is fail-closed: only free catalog entries with a
successful bounded response probe are selectable, and fallback resolution
never crosses into a paid or unknown model. Custom API model tests return only
public status/latency/classification; the key is held by OS credential storage.

Agent Reach and `tt` are optional external capabilities. Doctor/backend
observations and TikTok exit statuses are capability/evidence inputs, not
authority. Agent Reach absence, malformed doctor output, TikTok walled
responses, and probe failures degrade honestly. External installation is never
implicit.

Continuity fences bind work-order identity, authority revision, and execution
epoch. Restarted in-flight work is reconciled rather than auto-completed;
owner guidance advances the epoch, and stale results cannot overwrite newer
guidance. These deterministic tests do not prove external binaries or a live
model account are installed.
