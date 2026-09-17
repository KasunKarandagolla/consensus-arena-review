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
