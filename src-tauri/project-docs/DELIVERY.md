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
command and `start_delivery` admission both apply this policy before any
Delivery worktree/session mutation.

Arena does **not** currently install/package DSH.

The reproducibility qualification found one exact top-level
`@deepseek-ai/dsh@0.1.5-rc.1` package with rc2 transitive components selected by
declared semver ranges. The tested external runtime is reproducible from its
package manifest and frozen pnpm lockfile, subject to registry/store and
external profile availability. Arena does not treat the mixed tree as a reason
to upgrade or downgrade DSH without a new qualification.

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

If protected acceptance content changes:

- restore it from the acceptance commit;
- fail the attempt.

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

When worker output indicates `needs_user`:

- persist `WaitingForUser` first;
- then emit the existing owner-question UI event;
- use existing AskUser answer plumbing;
- dismissal is recorded as `Cancelled` and fails safely;
- restart recovery can re-emit the persisted delivery question.

This intentionally avoids requiring a worker process/tool call to remain suspended across the human decision.

### 9. Abort

`SessionRuntime` ownership protects exact-task abort semantics. Child ownership provides worker cleanup. State is marked Cancelled only after stopping the correct owner.

### 10. Verified candidate and Apply

Verified work remains isolated until explicit user Apply.

Apply requires:

- candidate is Verified;
- original checkout is clean;
- original checkout `HEAD` is unchanged from delivery start;
- non-forcing fast-forward succeeds.

No merge conflict automation, push, or deployment in V1.

Worktree/evidence are intentionally retained.

## Security / secret handling

Audit establishes:

- no API key stored in delivery state, prompts, evidence, or logs;
- DSH configuration under app data references an environment variable;
- native paths and explicit argv are used;
- no `/tmp`, shell pipeline, Unix-process-group, or platform-specific path assumption was added.

## Current known limitations

- DSH must already be available and compatible; Arena does not
  package/install it. Build setup reports a clear prerequisite state before
  start and admission repeats the check defensively.
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
- A source/test hardening pass now preserves an uncommitted candidate during
  INCONCLUSIVE verification resume, constrains receipt IDs, explicitly waits
  for DSH timeout cleanup, and filters delayed Delivery UI events by session.
- Delivery V1 currently implements its own bounded orchestration. The post-gates strategy selects Dagu as a candidate to remove more durable workflow/wait/retry mechanics, but Dagu is not integrated.

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
10. no secrets in durable/log/evidence state;
11. explicit Apply with unchanged clean base;
12. native Linux/Windows-compatible path/process design.
