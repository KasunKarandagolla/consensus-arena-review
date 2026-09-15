# Consensus Arena — Active Decisions

**Last refreshed:** 2026-09-15
This file contains **current durable decisions**, not the full historical diary. Historical details belong in audits and source history.

## D-2026-01 — Arena is now dual-lane

**Accepted.** Preserve two separate lanes:

- **Consult**: existing leader-driven frontier-model consultation/browser system.
- **Build / Delivery**: parallel bounded execution and independent-verification path.

Do not route Build through the legacy browser consultation loop.

## D-2026-02 — Arena owns product authority, not generic worker internals

**Accepted.** Arena owns product intent, adopted decisions, progression policy, acceptance authority, and the owner-facing experience.

External systems may own bounded execution, workflow mechanics, Git, test execution, sandboxing, deployment, and other commodity capabilities.

## D-2026-03 — Worker continuity does not require process continuity

**Accepted after substrate gates.** A valid continuation can be:

`worker reports blocker → worker exits → durable owner question → answer recorded → fresh bounded worker continues from repository/task/decision state`

Arena does not require a coding worker to hold a pending same-turn human tool call indefinitely.

## D-2026-04 — Human permission precedes protected actions

**Accepted.** A protected/irreversible action must not happen first and ask for approval afterward.

When evaluating workflow engines, distinguish a true pre-action human task from “approval after command execution.”

## D-2026-05 — Acceptance is independent of implementation narration

**Accepted and implemented in Delivery V1.** Worker prose, exit code, or self-reported success cannot mark a candidate Verified.

PASS requires execution of the frozen required checks against the identified candidate. Missing/incomplete evidence is not PASS.

## D-2026-06 — Acceptance material is frozen before implementation

**Accepted and implemented in Delivery V1.** The authoring phase creates executable acceptance material before implementation. Arena freezes/protects it, records hashes, and rejects worker attempts that alter protected acceptance material.

The worker may propose changes to requirements/tests, but cannot silently redefine success during a repair cycle.

## D-2026-07 — Bounded workers, not a monolithic substrate

**Accepted.** DSH is currently retained narrowly as a bounded coding/debug/repair worker because real gate testing proved useful execution with NVIDIA NIM on modest hardware.

DSH is **not** the owner of Arena product decisions, acceptance authority, or durable workflow policy.

## D-2026-08 — Dagu is the next workflow-engine candidate, not current implementation

**Accepted as next validation direction. Not implemented yet.** Dagu is attractive because it provides local durable runs, retries/history, processless root human tasks, REST/CLI control, and an existing DSH harness composition.

Do not code Dagu into Arena without a focused integrated slice proving the actual wait/restart/duplicate-answer/repair semantics on Linux and then Windows.

If Dagu fails materially, keep the stable work-order/worker/verifier contracts and fall back to a small Arena-owned controller. Do not reopen the entire agent-substrate search automatically.

## D-2026-09 — One programmatic AI endpoint is sufficient for minimum mode

**Accepted.** Core Build must remain conceptually operable with one compatible AI endpoint/model plus deterministic tools. Additional APIs or frontier web consultants improve quality but are not mandatory for routine execution.

## D-2026-10 — Consumer web frontier models remain valuable specialists

**Accepted.** The existing browser-backed ChatGPT/Claude/Gemini/etc. path is retained for selective high-value reasoning. It should not perform routine build/test/log/repair mechanics when programmatic/deterministic tools are better.

## D-2026-11 — Native Linux + native Windows are product requirements

**Accepted.** Current development machine is Linux Lite on modest hardware. The product must remain architecturally viable on native Linux and native Windows without requiring WSL.

Modest hardware is an optimization target, not a blanket reason to reject useful architecture. Measure actual resource use.

## D-2026-12 — Two WebViews remain a Consult-lane constraint, not a universal future law

**Accepted.** The legacy browser consultation lane still uses at most two WebViews (persistent leader + shared navigation window). Do not casually add more there.

Do not use that historical optimization to reject unrelated Build-lane architecture without measurement.

## D-2026-13 — Current frontend baseline remains, future UX stays minimal

**Accepted.** Existing `preview.html`-derived React design is the implemented baseline. Future Build/Delivery UI should remain simple, calm, product-focused, and progressively disclose technical details. Do not turn Arena into a control-panel/war-room UI.

## D-2026-14 — Current Delivery V1 apply is explicit and conservative

**Accepted and implemented.** Verified work remains in the isolated worktree until the user explicitly applies it. Apply requires a clean original checkout, unchanged original HEAD, a verified candidate, and a successful non-forcing fast-forward.

No automatic merge/push/deploy in V1.

## D-2026-15 — Reuse current Arena code only when it still deserves responsibility

**Accepted.** Existing code is not privileged because six months were spent building it. Keep code that fits the future boundary; replace/delete it when a mature reusable component removes more carrying cost.

Likewise, do not rewrite useful code merely for theoretical elegance.

---

# Superseded / rejected assumptions

The following are **not current architecture requirements**:

- Arena is only a multi-model blueprint generator.
- “Zero API keys” means no programmatic AI endpoint may be required.
- Every frontier participant must have a paid API.
- Every SDLC step should run through consumer web chats.
- A worker must preserve the same process/agent turn across every human question.
- DSH + Morning Star should be Arena's full durable substrate.
- Fusion should be Arena's full substrate; current tested release lacked active behavioral-verification wiring.
- OpenHands should be Arena's full substrate; gate testing found host/result and NIM compatibility defects for the required path.
- Existing Arena runtime/memory/orchestration code must be preserved just because it exists.
- The entire future product must stay below a hard global 2 GB limit or exactly two WebViews. These remain important optimization/current-lane facts, not universal architecture laws.

See `SUBSTRATE_RESEARCH.md` for concise gate conclusions.
