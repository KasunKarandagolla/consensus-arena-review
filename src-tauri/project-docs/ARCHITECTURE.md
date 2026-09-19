# Consensus Arena — Current Architecture

## Architectural principle

Arena is becoming a **thin product-authority layer over reusable execution systems**.

Current implementation is transitional: the old Consult lane is mature and retained, while a first Build/Delivery lane now exists in parallel.

```text
                    PRODUCT OWNER
                         │
                  Consensus Arena
        intent / decisions / acceptance authority
                         │
           ┌─────────────┴─────────────┐
           │                           │
        CONSULT                      BUILD
           │                           │
 frontier web models            isolated worktree
+ agent brain                  + bounded worker adapter
           │                           │
 expert reasoning               frozen acceptance
           │                           │
 blueprint/advice               independent verifier
                                       │
                              repair / AskUser / Verified
                                       │
                                explicit safe apply
```

## Lane A — Consult

Purpose: high-value independent reasoning and blueprint/advisory work.

Current mechanics:

- Tauri/Rust backend + React frontend.
- One persistent leader WebView.
- One shared navigating participant WebView.
- Seven built-in consumer-chat participants: ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, Kimi.
- Separate OpenAI-compatible `AgentBrain` interprets the leader and selects actions such as route, compare, ask user, blueprint, continue, complete, and later hackathon/parallel mechanisms present in current source.
- Existing browser reliability, diagnostics, recovery, memory, and session infrastructure remain relevant to this lane.

Future role: optional frontier consultation, not the mechanical SDLC runtime.

See `CONSULTATION.md`.

## Lane B — Build / Delivery V1

Purpose: carry a bounded product change from intent to independently verified candidate.

Latest audit establishes this sequence:

```text
clean repository
  ↓
record original HEAD
  ↓
create isolated arena-delivery worktree
  ↓
author acceptance material
  ↓
freeze acceptance commit + protected hashes
  ↓
bounded worker implementation attempt
  ↓
Arena runs frozen verification commands itself
  ├─ PASS → Verified candidate
  ├─ FAIL → bounded repair worker → same verification
  └─ NEEDS USER → persist WaitingForUser → existing AskUser → continue
  ↓
explicit Apply
  ↓
fast-forward original checkout only if still clean and unchanged
```

The worker cannot mark its own work Verified.

See `DELIVERY.md`.

## Evidence-gated project journey — Milestone 04

Arena now has a small, pure evidence-gate contract for the product journey.
The nine gate identifiers are records and deterministic predicates inside the
existing four broad phases; they are not nine services and do not introduce a
workflow engine:

```text
Discover: Vision → Problem research → Positioning → Ambiguity → Reuse
Decide:   Architecture → Build readiness
Deliver:  Implementation (current candidate + independent verifier)
Release:  Release (candidate/package/install/security/QA evidence)
```

The evaluator accepts only a current Arena-owned package. Stale package or
evidence revisions, missing evidence, unresolved owner decisions, unresolved
high ambiguity, incomplete architecture challenge, and verifier results tied
to another candidate revision fail closed. It does not persist the journey,
authorize a worker, or replace `SessionRuntime`, Delivery, the verifier, or
Safe Apply. Those boundaries remain the responsibility of the existing Arena
authority kernel and are future integration work.

The owner-facing Delivery view keeps the calm Delivery details and now places
them under the broader project journey label `Discover → Decide → Deliver →
Release`; the four-step Intent/Build/Verify/Apply sequence remains the
Delivery subsection. Technical evidence remains progressively disclosed rather
than becoming the primary experience.

### Durable Product OS research — Milestone 05B

Research now enters the product boundary through one Arena-owned work-order
seam, not through direct worker writes:

```text
Arena admits Researcher work order
              ↓
bounded primary-source retrieval
              ↓
current ProductAuthority record: ResearchClaim / Unverified
              ↓
Arena admits distinct FactVerifier work order
              ↓
independent source check + source scope
              ↓
IndependentlyVerified / Contradicted / Unresolved
              ↓
restart-safe Product OS snapshot and gate input
```

`TranscriptStore` is the durable home for Product OS records and work-order
metadata. `SessionRuntime` remains the live ownership and cancellation
authority. Unknown, wrong-role, cancelled, stale, superseded, and mismatched
results fail closed. Ambiguity admission forces an Arena-owned owner-required
question; only an adopted owner decision for the current question and
authority revision can resolve it.

The current runtime dogfood uses direct read-only official GitHub HTTPS
retrieval and does not claim GitHub MCP. M05C extends that Linux proof through
a real internal validation project: reopened verified research, typed
Product-Director scope/review evidence, reuse, a real bounded risk spike,
distinct architecture proposals, adopted owner direction, current Build
Package assembly, and pre-implementation gate evaluation. A material scope
update invalidates the old package. This is not market validation or final
founder dogfood. A candidate directory and
Git worktree remain non-authoritative working locations, not OS/security
sandboxes.

### Fresh founder dogfood — Milestone 06

The fresh Linux dogfood proves the first integrated Product OS path from a
newly admitted founder idea through WebDiscovery, independent fact
verification, an authority-derived Build Package and pre-implementation gates,
parallel bounded engineering roles, the existing frozen-acceptance Delivery
path, and an independent Verified candidate. The external worker remains a
replaceable execution substrate; Arena retains product truth, acceptance,
verification, and Apply authority. This does not claim native GUI, Windows
runtime, package installation, or hostile same-user filesystem isolation.

### Production coordinator — Milestone 07

M07 moves the sequencing out of the ignored integration harness and into the
Arena-owned `product_os_coordinator` module. The coordinator is a small
deterministic progression controller, not a workflow engine: it calls typed
Product OS operations, admits semantic role work orders to the existing
SessionRuntime/OpenCode adapter, persists minimal phase/reference state, and
hands only a current passing Build Package to the existing Delivery boundary.
AI supplies bounded semantic proposals; Arena adopts scope, owner decisions,
reuse, architecture, gates, verification, and terminal status.

The production API starts, observes, answers a genuinely owner-only question,
cancels, or resumes a project. It does not expose stage controls or raw
authority mutation to the renderer. M07 runtime-proves the fresh Linux
research-to-verified-candidate sequence. It does not yet prove concurrent
role execution, complete in-flight coordinator restart reconciliation, Windows
parity, packaging, native GUI launch, or Safe Apply.

## Durable state ownership

### Arena owns

- product intent and owner decisions;
- session/product history relevant to the user;
- Build delivery phase/state;
- acceptance contract/protected-material identity;
- candidate/evidence correlation;
- owner-facing questions and adopted answers;
- final acceptance/apply authority.

### Worker owns only a bounded attempt

Current DSH use is deliberately narrow. Worker process/session continuity is not treated as product truth.

### Git owns repository/candidate history

Use real commits/worktrees instead of inventing a parallel source-version system.

### Verification tool owns raw execution results

Arena interprets/records evidence but should not become a test runner or browser automation engine.

### Future workflow engine may own durable run mechanics

Dagu is the next candidate for waits/retries/run history/human tasks. It is not currently integrated.

## Memory boundary

Arena memory is for product truth, open questions, model reliability, patterns, and project/session continuity. Do not turn it into a universal autonomous-agent memory platform.

External workers may use their own procedural context/skills. Avoid duplicated authoritative state.

See `MEMORY.md`.

## AI resource boundary

### Programmatic path

A configurable OpenAI-compatible model can power Arena's orchestration/worker path. One endpoint should be enough for minimum mode.

### Consumer-web path

Authenticated web chats provide optional frontier reasoning without requiring separate paid frontier APIs. Browser transport remains isolated in Consult.

Model identity and transport should not be conceptually fused.

## Cross-platform boundary

- Current development/runtime evidence: Linux Lite.
- Product target: native Linux + native Windows.
- Use `PathBuf`, explicit argv vectors, and process APIs rather than shell pipelines/platform-specific paths.
- Do not require WSL.
- Any sandbox or native-test strategy must be qualified on Windows separately.

## Resource strategy

- Keep persistent Arena control state light.
- Run workers, browsers, test runners, and application processes on demand.
- Start with concurrency = 1 for Build.
- Measure the whole process tree rather than relying on package/binary size.
- Gate testing previously observed a DSH worker footprint around 185 MB RSS under one NIM-backed task; treat this as evidence for that configuration, not a universal forecast.

## Transition architecture: Dagu candidate

The accepted post-gates research recommendation is to test:

```text
Owner ↔ Arena product authority
            │
            ↕ run state / human task
           Dagu
            │
       bounded work order
            ↓
       replaceable worker adapter
            │
       candidate/blocker
            ↓
 project-native verifier
            │
       evidence to Arena
```

Dagu would own durable execution mechanics, not product meaning. Arena would adopt completed owner decisions into product truth.

**This diagram is a pending target, not the current implemented source.**
