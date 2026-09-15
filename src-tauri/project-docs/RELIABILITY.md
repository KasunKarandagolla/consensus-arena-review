# Consensus Arena — Reliability and Safety Invariants

## Source priority

Runtime/source evidence > latest audit > docs > old plans.

Permanent audits are never deleted. They record what was actually proven at a point in time.

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
No API keys in prompts, delivery state, evidence, or logs.

### SAFEAPPLY
Apply only verified candidate, clean original checkout, unchanged original HEAD, successful non-forcing fast-forward.

### CROSSPLATFORM
No Linux-only assumptions in shared Build architecture.

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

## Required-scenario protection

If using self-healing/generative testing tools, required scenarios must not silently disappear or become skipped while still yielding a green build.

Arena acceptance policy should validate required scenario coverage, not only process exit status.

## Permission rule

Protected external/irreversible action requires owner permission **before** action.

When using external workflow tools, verify their approval semantics rather than trusting feature names.

## OS containment

Git worktrees isolate repository history, not the operating system.

Do not claim untrusted-code safety without a real OS containment boundary. Reuse an existing sandbox where practical; Windows parity remains a qualification item.
