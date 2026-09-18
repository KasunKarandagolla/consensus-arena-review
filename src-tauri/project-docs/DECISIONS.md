# Consensus Arena — Active Decisions

**Last refreshed:** 2026-09-17
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

**Architecture accepted; DSH selection reopened by current evidence.** Arena
should use a bounded worker rather than a monolithic execution substrate. DSH
previously completed one real Muse task, but the latest clean-install pair
reproduced only package resolution and root CLI help: the exact Arena
headless-profile probe timed out in both installs. No current DSH worker
baseline is qualified, and DSH materially challenges its current worker
assumption. Trigger A consultation is required before selecting DSH, a fork,
or another worker. Keep Arena's work-order, frozen-acceptance, verifier, and
Apply contracts stable during that decision.

DSH is **not** the owner of Arena product decisions, acceptance authority, or durable workflow policy.

## D-2026-08 — Dagu is a post-V1 workflow-engine candidate, not current implementation

**Not implemented; keep post-V1.** Dagu is attractive because it provides local durable runs, retries/history, processless root human tasks, REST/CLI control, and an existing DSH harness composition. The completed standalone closure remains **INCONCLUSIVE**: after a hard interruption the attempt required explicit reconciliation and repeated an external side effect; DSH composition and native Windows execution remain unproven.

Complete only the currently authorized hard-interruption and standalone
Windows qualification gates before recording a final verdict. Do not integrate
Dagu or broaden V1 qualification work. The inconclusive result does not
trigger Astra Trigger B. Preserve Dagu as a post-V1 candidate.

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

## D-2026-16 — Evidence gates are lightweight Arena-owned exit contracts

**Accepted for the week-one Product OS architecture.** Use one versioned
Arena-owned evidence package and deterministic predicates for Vision, Problem
Research, Positioning, Ambiguity, Reuse, Architecture, Build Readiness,
Implementation, and Release. Keep these gates inside the existing
Discover/Decide/Deliver/Release phases; do not introduce nine services, a
generic workflow engine, or a parallel authority model.

Stale or missing evidence fails closed. Worker, MCP, skill, and consultation
output may supply evidence but cannot authorize a gate, expand disclosure, or
replace owner decisions, the independent verifier, or Safe Apply. Durable
journey persistence and full work-order lifecycle binding remain later work
where the existing SessionRuntime/Delivery contracts can carry them safely.

## D-2026-17 — Build Package admission is Arena-owned

**Accepted for Milestone 05.** Product OS records are assembled into one
versioned, fingerprinted Build Package through existing persisted Delivery
state. Owner decisions and architecture evidence are referenced by identity;
worker, renderer, consultation, and runtime completion claims cannot construct a
passing package. A current package must pass the Architecture and Build
Readiness predicates before existing Delivery/OpenCode execution is admitted.
SessionRuntime remains live lifecycle authority; Delivery remains acceptance,
independent-verification, repair, Verified, and Safe Apply authority. The
candidate worktree is non-authoritative and is not a security sandbox.

## D-2026-18 — Research evidence requires Arena provenance and independent verification

**Accepted for Milestone 05A.** Research workers and external read-only tools
may propose claims and source references, but they cannot construct a verified
fact or expand authority. The existing Product OS evidence records carry the
claim origin, kind, source identity, verification disposition, contradiction
references, verifier work-order identity, scope, and revisit trigger. Research
gates accept only current independently verified claims with reconciled source
metadata. Direct official-source retrieval is the current narrow proven route;
GitHub MCP, integrated web search, consultation execution, and broad provider
portability remain separate qualification work.

## D-2026-19 — External workers must detect canonical checkout mutation

**Accepted for Milestone 05A.** Arena snapshots canonical `HEAD` and complete
Git status before and after external worker execution. Any canonical mutation
invalidates the work order and blocks candidate commit, Verified, and Apply.
Arena does not silently repair the canonical checkout. This is an authority
and detection guard, not a same-user security sandbox or Git-object-store
isolation mechanism.

## D-2026-20 — Durable research authority composes existing stores and runtime

**Accepted for Milestone 05B.** Research and fact-verification tasks use one
Arena-owned Product OS work-order record persisted in the existing
`TranscriptStore` database; live ownership remains `SessionRuntime`. Research
workers submit only sanitized unverified proposals. Arena resolves a distinct
current Fact Verifier work order before adopting an independent verification
disposition. Unknown, wrong-role, cancelled, stale, superseded, or mismatched
results fail closed. Ambiguity classification is Arena-owned and defaults to
owner-required; only a current adopted owner decision bound to the exact
question and authority revision can resolve it. This does not yet claim a
complete durable research-to-BuildPackage gate or M06 readiness.

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
