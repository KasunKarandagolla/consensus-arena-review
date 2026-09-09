# Consensus Arena — Development Process

## Purpose

This file describes the **current efficient workflow** for audits and implementation.
Legacy Cline/complete-file-delivery procedures are removed from the active process because they consumed context and no longer describe how this project is worked on.

Historical process details remain available in Git history/audits if needed.

---

## Core Principle

**Understand the connected mechanism before changing it. Then verify the real result.**

Avoid the project's historical failure pattern:

`patch symptom → create new interaction bug → patch again → lose architectural understanding`

For reliability work, treat the pipeline as one state machine rather than a collection of isolated files.

---

## Source Hierarchy

1. Current worktree/source and current runtime evidence.
2. `DECISIONS.md`.
3. Relevant current project docs.
4. Historical audits/comments.

If docs conflict with source, source wins and docs should be corrected after the work stabilizes.

---

## Two Work Modes

### Mode A — Read-only forensic audit

Use when the goal is to discover bugs/weaknesses, not implement fixes.

Rules:

- no source edits;
- no dependency changes;
- no speculative patches;
- record exact HEAD/worktree first;
- reconstruct call/state graph;
- derive invariants;
- adversarially simulate failure paths;
- prove findings with exact source evidence;
- separate `PROVEN`, `HIGH`, `MEDIUM`, and `NEEDS_RUNTIME` findings;
- create one concise durable audit report;
- do not spend time on full builds unless a targeted check resolves a real uncertainty.

For the Astra pipeline audit, only the requested audit Markdown file may be created.

### Mode B — Implementation/repair

Use only after the bug set/invariants are sufficiently understood.

Rules:

1. Record HEAD/dirty worktree.
2. Read every file to be edited completely.
3. Read directly connected code required to preserve contracts.
4. Implement one coherent reliability unit.
5. Add deterministic regression/stress tests.
6. Run real verification.
7. Review diff.
8. User decides whether to checkpoint/push.

Do not use one Codex session per symptom when several symptoms share one root state-machine defect.

---

## Token / Quota Discipline

High-capability model quota is scarce.

### Spend tokens on

- cross-module control flow;
- concurrency/event ordering;
- side-effect idempotency;
- response integrity;
- recovery boundaries;
- reviewer/Blueprint/Complete guarantees;
- AskUser/Hackathon races;
- deterministic stress simulations;
- exact source evidence.

### Do not spend tokens on

- repeating historical batch narratives;
- command/module/AppState counts unless directly relevant;
- reading old audits before independent findings are frozen;
- frontend pixel polish during a backend pipeline audit;
- re-auditing unrelated Memory CRUD/Skills/stub modules;
- long internal-reasoning transcripts;
- giant build logs;
- full-file echoing;
- speculative implementation during a read-only audit;
- duplicating the same root cause as many findings.

### Audit priority under quota pressure

1. P0/P1 physical active-turn correctness.
2. Event/concurrency/recovery.
3. Panel review/Blueprint/Complete correctness.
4. AskUser/Hackathon/OAuth.
5. Performance/security/prompt behavior that affects pipeline reliability.
6. P2/P3 cleanup only if budget remains.

If time/quota becomes tight, finish a high-quality partial P0/P1 report rather than burning the budget on low-value breadth.

---

## Efficient Source-Reading Strategy

### First pass

Read canonical docs once:

1. `DECISIONS.md`
2. `ARCHITECTURE.md`
3. `BACKEND.md`
4. `IPC.md`
5. `PROCESS.md`
6. `AGENTS.md`

Then discover the real call graph from source.

### Core pipeline files

Read each large core file completely once:

- `response_router.rs`
- `browser_backend.rs`
- `browser_harness.rs`
- `session_runner.rs`
- `orchestrator.rs`
- `agent_brain.rs`
- `checkpoint.rs`
- `session_vault.rs`
- `hackathon.rs` when Hackathon is in scope.

After the complete first read, revisit by function/line range rather than rereading entire large files repeatedly.

Read frontend/memory/support modules only when the call graph reaches them.

---

## Reliability Audit Method

### Step 1 — Reconstruct state ownership

For every state variable/event/channel answer:

- who creates it?
- who mutates it?
- who consumes it?
- what identity correlates it?
- what persists across navigation?
- what persists across restart?
- what happens if it is late/duplicate/dropped?

### Step 2 — Define invariants

Examples:

- after physical SubmitConfirmed, no automatic duplicate Send;
- critical Response cannot be dropped behind telemetry;
- hard timeout is absolute;
- old generation cannot satisfy current turn;
- successful response is fully intact before brain use;
- attempted consultation is not successful review;
- Blueprint/Complete cannot bypass backend-known required work;
- AskUser close cannot leave an orphan waiter;
- restart cannot repeat an already-confirmed side effect.

### Step 3 — Trace every violation path

Search all call sites capable of:

- Send/injection/navigation;
- response completion;
- queue draining;
- retry;
- Blueprint/Complete;
- checkpoint save/resume;
- AskUser/Hackathon creation/cleanup;
- OAuth popup/new-window decisions.

### Step 4 — Simulate adversarial timing

Do not require a human runtime reproduction when source deterministically permits failure.

Use compact branch simulations such as:

- ACK lost after successful Send;
- queue full when Response arrives;
- unrelated events near timeout;
- late old-generation response;
- 15-second streaming pause;
- provider `/new → /chat/id` after Send;
- restart after SubmitConfirmed;
- reviewer rate-limited before Blueprint;
- Stop while AskUser waits.

### Step 5 — Classify findings

`PROVEN` — deterministic source violation.

`HIGH` — strong cross-module failure path.

`MEDIUM` — credible weakness needing more evidence.

`NEEDS_RUNTIME` — cannot responsibly establish from static source.

Do not present hypotheses as proven bugs.

---

## Named Risks

Every relevant audit/implementation should consider:

- **BLOCKING** — blocking lock/work in async/browser callback.
- **CHANNEL** — wrong channel type/location or lossy critical traffic.
- **EVENTMATCH** — frontend/backend command/event mismatch.
- **IPCPARSE** — JSON-string vs plain-string mismatch.
- **UNWRAP** — production panic path.
- **ACTIVEKEY** — missing generation/stale event acceptance.
- **CRITICALDROP** — result/submit event lost behind diagnostics.
- **SUBMITIDEMPOTENCY** — duplicate Send after successful side effect.
- **TIMEOUT** — event traffic resets overall deadline.
- **RESPONSEINTEGRITY** — old/partial/truncated/corrupt response accepted.
- **CONTINUITY** — unnecessary navigation/lost conversation URL.
- **REVIEWGUARD** — attempted review counted as successful / early Blueprint.
- **CHECKPOINT** — resume repeats or forgets safety-critical state.
- **ASKCHANNEL / ASKDISMISS** — stranded/double AskUser.
- **OAUTH** — unsafe host trust or broken provider callback flow.
- **INITSCRIPT** — document-start/provider-specific model runtime regression.
- **NAVCLOSURE** — stale agent captured in long-lived callback.

---

## Implementation Safety Pattern

### Before side effects

Establish exact operation identity and baseline.

### After side effects

Record physical evidence immediately.

### Retry rule

Before retrying any side effect, ask:

**Could the previous attempt already have succeeded?**

If yes, do not repeat unless source has strong proof it did not happen.

### Post-submit rule

After SubmitConfirmed, retries may observe/recover the response but may not automatically re-submit the same turn.

---

## Verification

For backend/pipeline changes, typical full checks:

```bash
cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo fmt --check
cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check
cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo test
cd /home/kasun/Music/arena/consensus-arena && npm run build
cd /home/kasun/Music/arena/consensus-arena && git diff --check
```

If infrastructure prevents a check:

- report exactly which check did not run;
- do not transfer a PASS from an earlier commit;
- do not call the new work fully verified.

A compile/test PASS does not replace the seven-provider GUI acceptance matrix for browser behavior.

---

## Git Checkpoints

Before a risky reliability batch, create/push a clean checkpoint when the user asks.

Do not use `git add .` blindly in a large development tree.
Prefer staging known tracked changes and intentional audit files.

No commit/push from an audit/implementation session unless explicitly authorized.

---

## Documentation Updates

Update docs **after** source/checks stabilize.

Do not copy transient implementation details such as command counts or exact large enums unless they are long-lived public contracts.

When a doc claims something is DONE, it should mean current source/tests/runtime support that claim—not merely that an implementation exists.

Historical audits remain preserved; canonical docs should describe current reality.

---

## Astra Read-Only Audit Efficiency Plan

For a five-hour/high-capability audit window, target roughly:

### ~55% — core physical pipeline

- browser/event state;
- Submit/response;
- timeouts/retries;
- transport;
- continuity;
- recovery.

### ~25% — logical orchestration

- brain decisions;
- review accounting;
- Blueprint/Complete;
- AskUser;
- Hackathon;
- checkpoint behavior.

### ~10% — OAuth/performance/security/prompt interactions

Only where they affect pipeline reliability.

### ~10% — independent contradiction pass + concise report

Do not use the final 10% generating a long narrative.

If earlier phases uncover many P0/P1 issues, reduce P2/P3 breadth rather than truncating the evidence/report.

---

## Audit Deliverable

Preferred durable audit format:

`ID | severity | confidence | invariant | evidence | failure simulation | impact | minimal repair | preferred repair | regression test | dependencies`

One concise Markdown report is more valuable than a verbose terminal conversation or hidden reasoning transcript.
