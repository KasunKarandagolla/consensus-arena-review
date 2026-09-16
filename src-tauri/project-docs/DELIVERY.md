# Consensus Arena — Build / Delivery Lane

**Current evidence:** `audits/delivery-v1-programme-qualification-2026-09-16.md`
(2026-09-15 to 2026-09-16), with
`audits/delivery-dsh-tool-loop-and-linux-e2e.md`,
`audits/delivery-loop-v1-linux-runtime-qualification.md` and
`audits/delivery-loop-v1-post.md` retained as preceding audits.

## Purpose

Build/Delivery is the first concrete implementation of Arena's new product philosophy:

> product intent and acceptance remain Arena-owned; a bounded worker changes code; deterministic project checks decide whether the candidate actually works.

It is intentionally parallel to the old Consult/browser path.

## Current V1 flow

### 1. Admission

- Build does not require a leader or participant selection.
- It does not open model WebViews.
- Existing `SessionRuntime` is the task-ownership/concurrency guard.

### 2. Repository isolation

`start_delivery` semantics established by audit:

- input must be a clean Git repository root;
- record original `HEAD`;
- create one candidate branch/worktree under app data named like `arena-delivery/<short-id>`;
- main checkout is not implementation workspace.

### 3. Bounded worker

Current worker is DSH in headless mode:

- independent `tokio::process::Command`;
- candidate worktree as `cwd`;
- explicit argv/process launch rather than shell wrapper;
- bounded output/timeouts;
- `kill_on_drop(true)` cleanup;
- current worker-session resume is not used/required.

Executable resolution:

- `ARENA_DSH_EXECUTABLE`, or
- `PATH`.

Optional existing DSH home may be supplied via `ARENA_DSH_HOME`.

The current source expects a DSH CLI invocation of
`--profile headless --patch <patch-file> <prompt>` and a schema-version-1
`.arena-runtime/result.json` worker receipt. Arena's current qualified
compatibility policy is exact DSH `0.1.5-rc.1` plus a successful
`--profile headless --help` probe. The read-only `get_dsh_prerequisite`
command and `start_delivery` admission apply this version/help probe before
any Delivery worktree/session mutation. It does not verify Node, the frozen
dependency tree, or a schema-1 worker result; the documented lock/runtime was
not reconstructed and current two-run Muse repeatability is unproven.

Arena does **not** currently install/package DSH.

The earlier audit records one exact top-level
`@deepseek-ai/dsh@0.1.5-rc.1` package with rc2 transitive components selected by
declared semver ranges. In the 2026-09-16 closure attempt, pnpm `10.33.0` and
`10.33.2` resolved the documented top-level DSH and `@mstar-harness/dsh@3.8.3`
packages to lock SHA256
`88a26a4d1f31bdffd465bf081f5d02638f0f42982aeb235ff7f4d1365a004773`, not the
documented frozen SHA256
`1297ec9257567a85c5a653734979256e6958a2c1235079c62fdc5bb9f2505887`. The
candidate lock had 568 package records and 234 DSH rc2 records; the frozen audit
describes 230. The exact original runtime/lock was not found. Do not run a
model-backed qualification against the mismatched candidate or call the current
baseline runtime-repeatable. The earlier successful Muse run remains historical
single-run evidence only. See
`audits/delivery-v1-backend-runtime-qualification.md`.

### 4. Acceptance authoring and freeze

Before implementation, the authoring run must create executable acceptance material.

Arena then:

- removes `.arena-runtime` from the candidate acceptance material;
- commits the acceptance freeze;
- records protected paths/hashes;
- validates structured verification commands;
- uses conservative program/cwd validation.

The implementation worker cannot silently weaken the acceptance contract.

### 5. Independent verification

Arena executes the frozen commands directly.

Worker prose, worker exit code, or result summary **cannot** mark Verified.

Verifier receipts now correlate the session, attempt ID, unique verification
run ID, frozen acceptance SHA, candidate SHA, and verification-profile hash.
Each verification gets a unique evidence subdirectory. The verifier compares
candidate HEAD and Git-visible worktree status before and after checks, so
persistent visible mutation cannot yield PASS. This does not prove that the
candidate stayed unchanged at every instant: mutate-then-restore and ignored
file changes can evade the sampled end state without stronger OS isolation.
The supervisor marks the exact already-verified commit Verified and does not
create a new candidate commit after checks pass.

If protected acceptance content changes:

- restore it from the acceptance commit;
- fail the worker attempt; preserve the verifier's own receipt rather than
  rewriting its PASS/FAIL result to represent Arena's separate policy decision.

### 6. Independent outcome handling

- PASS requires at least one executed required check, with every required
  check passing and protected acceptance unchanged.
- FAIL requires valid required-check evidence demonstrating incorrect behavior;
  a worker exit code or prose is not enough.
- INCONCLUSIVE represents infrastructure, missing, malformed, skipped,
  incomplete, or uncorrelated evidence. It stops the current bounded run in
  an owner-visible Failed state and does not consume a product-code repair
  attempt. Resume reruns the frozen verification.
- Mixed outcomes are deterministic: FAIL takes precedence over INCONCLUSIVE,
  which takes precedence over PASS.

### 7. Bounded repair

- implementation + repairs are bounded to three attempts in current V1;
- after repair, Arena reruns the **same frozen profile/commands**;
- PASS/FAIL is evidence-driven.

### 8. Durable owner question

When worker output indicates `needs_user`, the production source path:

- persist `WaitingForUser` first;
- then emit the existing owner-question UI event;
- use existing AskUser answer plumbing;
- dismissal is recorded as `Cancelled` and fails safely;
- restart recovery can re-emit the persisted delivery question.

This intentionally avoids requiring a worker process/tool call to remain
suspended across the human decision. These persist/notify/recovery mechanics
are implemented, but a production-service restart, answer, and continuation
run has not been qualified in the current runtime matrix.

### 9. Abort

`SessionRuntime` ownership protects exact-task abort semantics. Child ownership
provides worker cleanup. State is marked Cancelled only after stopping the
correct owner. Unit tests cover ownership semantics; killing/waiting for a
real DSH descendant tree has not been runtime-qualified.

### 10. Verified candidate and Apply

Verified work remains isolated until explicit user Apply.

Apply source requires:

- candidate is Verified;
- original checkout is clean;
- original checkout `HEAD` is unchanged from delivery start;
- non-forcing fast-forward succeeds.

No merge conflict automation, push, or deployment in V1.

Production Apply tests exercised dirty, changed-HEAD, and non-fast-forward
refusals. Successful production Apply remains unproven because the only
model-backed path that reaches it is opt-in and has not run.

Worktree/evidence are intentionally retained.

## Security / secret handling

Audit establishes:

- no configured API key in structured delivery state, acceptance prompts, or
  worker summaries;
- raw verifier stdout/stderr are discarded after capture; their evidence files
  contain only buffered-byte-count notices (which may include a truncation
  marker), not command output or diagnostics;
- DSH configuration under app data references an environment variable;
- native paths and explicit argv are used;
- no `/tmp`, shell pipeline, Unix-process-group, or platform-specific path assumption was added.

## Current known limitations

- DSH must already be available and compatible; Arena does not
  package/install it. Build setup reports prerequisite status before start and
  admission repeats the check defensively. The frozen package tree remains
  unreproduced in the current environment, so exact worker compatibility is a
  release blocker until the documented lock/runtime is recovered or the audit
  is corrected from new primary evidence.
- DeepSeek V4 Flash `deepseek-ai/deepseek-v4-flash-0731` reached the NVIDIA
  model-list endpoint, but bounded inference timed out and its standalone DSH
  run did not produce a coding change or worker receipt. The single permitted
  control, `meta/muse-glimmer-30b`, completed a real DSH coding task and
  emitted an Arena-shaped schema-1 worker receipt. The current default model
  was not changed. See `audits/delivery-dsh-v4-worker-and-e2e.md`.
- Native X11 launch was reconfirmed, but the managed `tauri dev` WebView was
  blank in this qualification session. No genuine Delivery UI run was started
  and no middle-stage manual substitution was used.
- Windows runtime parity remains a separate qualification requirement.
- Windows verifier launch now routes `npm` through a resolved `node.exe` and
  its `npm-cli.js`, avoiding direct execution of the `npm.cmd` shim. When an
  npm verification command is used, the resolver fails closed unless Node is
  `v22.22.2` and npm is `10.9.7`. This source change has not run on Windows;
  other `.cmd`/`.bat` tools remain unqualified.
- A source/test hardening pass now preserves an uncommitted candidate during
  INCONCLUSIVE verification resume, constrains receipt IDs, explicitly waits
  for DSH timeout cleanup, and filters delayed Delivery UI events by session.
- Delivery V1 currently implements its own bounded orchestration. The Dagu
  standalone gate now supports several durable run/wait/retry/history
  mechanics, but overall remains **INCONCLUSIVE** because hard-interruption
  reconciliation and successful Dagu→DSH continuation are unproven. Dagu is
  not integrated; see `audits/dagu-standalone-qualification.md`.

## Runtime qualification boundary

The opt-in Rust test `delivery::tests::backend_qualification_runs_production_delivery_path`
uses real production SessionRuntime admission, clean-base validation, Git
worktree creation, acceptance authoring/freeze, DSH supervision, the Arena
verifier, durable state/transcript writes, and production Apply. It is
supporting **production-path backend Delivery runtime qualification**, not GUI
E2E. It requires an explicitly supplied worker credential and the exact DSH
runtime; no successful invocation has yet been recorded. Credential values
must never be placed in test output, evidence, or Git.

## Next architecture candidate: Dagu

Accepted research direction:

- Dagu owns durable run/wait/retry/history mechanics;
- root human tasks own pending-answer mechanics;
- DSH remains a bounded worker;
- Arena owns product decision meaning and acceptance;
- project-native tools own verification.

Important Dagu constraints from research:

- human tasks are root-workflow checkpoints, not arbitrary nested agent-turn questions;
- repeated/nested human interactions may require linked bounded root work orders;
- its separate “approval” behavior can execute an attached command before waiting, so permission-gated actions should use a **preceding human task**, not retrospective approval;
- Dagu is GPL; distribution/embedding terms must be settled before shipping. Start with external CLI/REST boundary.

## Non-regression invariants

Any Delivery change must preserve:

1. clean-base isolation;
2. no automatic modification of original checkout during implementation;
3. acceptance frozen before implementation;
4. protected acceptance cannot be silently rewritten;
5. independent verifier controls Verified;
6. same required scenario/profile reruns after repair;
7. bounded attempts;
8. owner question persisted before UI notification;
9. abort stops the exact task/child;
10. no configured API key in structured durable state; raw verifier output is
    discarded and other local evidence is treated as potentially sensitive;
11. explicit Apply with unchanged clean base;
12. native Linux/Windows-compatible path/process design.
