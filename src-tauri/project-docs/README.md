# Consensus Arena — Project Documentation Index

**Last refreshed:** 2026-09-16
**Purpose:** Small, modular source-of-truth documents for humans, ChatGPT, and Codex working on Consensus Arena.

## Current product in one paragraph

Consensus Arena is a native Tauri 2 desktop application evolving from an AI expert-panel blueprint tool into a **product-authority and delivery system for a nontechnical product owner**. It now has two deliberately separate lanes:

1. **Consult** — the existing leader-driven frontier-model consultation system using authenticated consumer web chats and an OpenAI-compatible orchestration brain.
2. **Build / Delivery** — a new parallel lane that can take bounded product work into an isolated Git worktree, use a bounded DSH worker, freeze executable acceptance material, verify independently, repair with bounded attempts, persist owner questions, and apply only a verified candidate under strict Git preconditions.

The long-term direction is not to turn Arena into another coding-agent platform. Arena should own **product intent, owner decisions, progression policy, and acceptance authority**, while reusing external workers, workflow engines, Git, test runners, browser/native automation, deployment tools, and other commodity infrastructure.

## Reality hierarchy

When sources disagree, use this order:

1. **Current source and runtime evidence**
2. **Latest permanent audit in `audits/`**
3. **These project docs**
4. Historical prompts/specifications/research

Do not preserve a doc claim because it is written here. Correct the docs when current source proves otherwise.

The post-gates Delivery audits dated 2026-09-14 supersede the older consultation-only status language in the previous project docs.

## Read only what the task needs

Always read `DECISIONS.md` first after this index, then use this map.

| Task | Read next |
|---|---|
| Product direction / scope / user experience | `PRODUCT.md`, `DECISIONS.md`, `SUBSTRATE_RESEARCH.md` |
| Overall system architecture | `ARCHITECTURE.md`, then lane-specific doc |
| Legacy expert-panel / browser consultation | `CONSULTATION.md`, `BACKEND.md`, `IPC.md`, `RELIABILITY.md` |
| Build / Delivery work | `DELIVERY.md`, `BACKEND.md`, `IPC.md`, `RELIABILITY.md` |
| Dagu / DSH / substrate decisions | `SUBSTRATE_RESEARCH.md`, `DELIVERY.md`, `DECISIONS.md` |
| Frontend / UX | `FRONTEND.md`, `PRODUCT.md`, `IPC.md` |
| IPC / Tauri commands / events | `IPC.md`, then real Rust command/event source |
| Memory | `MEMORY.md`, `BACKEND.md` |
| Reliability / audits / recovery | `RELIABILITY.md`, relevant lane doc, latest audit |
| Coding or implementation | `PROCESS.md`, root `AGENTS.md`, plus relevant technical docs |

Do **not** load every project document for a narrow task. This documentation is intentionally modular to save context.

## Current verified transition point

The latest local post-implementation audit establishes a first Build vertical slice with these invariants at source/test level:

- Consult mode remains separate and unchanged.
- Build mode does not require leader/participants or model WebViews.
- Work occurs in an isolated `arena-delivery/<short-id>` Git worktree created from a clean base.
- DSH is currently a bounded external worker, not Arena's durable product authority.
- Executable acceptance material is authored **before implementation**, frozen, protected by hashes, and verified independently from worker narration.
- Required checks are rerun after repair; attempts are bounded.
- `needs_user` is persisted before the existing AskUser event is emitted; the source recovery path can re-present a persisted question after restart, but a separate production-service restart/answer runtime has not been qualified.
- Apply is explicit and source guards reject dirty, moved, or non-fast-forward bases; the successful production Apply path has not been runtime-qualified.
- DSH is not yet bundled/installed by Arena.
- Build setup now performs a read-only DSH prerequisite check and blocks
  Delivery admission unless the expected `0.1.5-rc.1` executable exposes the
  `headless` profile. This version/help probe does not establish the frozen
  dependency tree or worker-result compatibility; the expected runtime is not
  yet reproducible or runtime-qualified.
- Native Linux Tauri launch was reconfirmed, but the managed WebView was blank
  during the latest qualification session, so no Delivery UI dogfood run was
  started in that session.

See `DELIVERY.md`, `audits/delivery-loop-v1-linux-runtime-qualification.md`,
and the current qualification record in
`audits/delivery-dsh-v4-worker-and-e2e.md`.

The current qualification record adds a direct DeepSeek V4 Flash provider
sanity check and a real DSH comparison. V4 Flash reached model listing but
bounded inference timed out; its standalone DSH task did not change a
repository or emit a worker receipt. The single permitted Muse Glimmer control
did change a repository and emit a schema-1 receipt, but that is historical
single-run evidence: the documented lock could not be reconstructed in the
latest pass, and neither of the two required independent Muse runs was
attempted. DSH repeatability, the complete Arena Delivery runtime path, and
reliability branches therefore remain unproven.

The 2026-09-16 programme qualification fixed the INCONCLUSIVE resume
candidate-preservation defect, hardened evidence receipt IDs and DSH timeout
cleanup, and added a stale Delivery-event guard. A follow-up backend/runtime
closure pass added a production-path dogfood boundary, bounded Git execution,
immutable verifier/candidate correlation, real-Git admission/Apply/protected
restoration tests, and a host qualification script. Its model-backed Delivery
test remains opt-in and has not passed: the exact documented DSH lockfile could
not be reconstructed, and two independent Muse runs were therefore not
attempted. This is backend runtime evidence, not GUI E2E.

Native WebKitGTK still renders blank on the current Linux session because
`/dev/dri` is absent and EGL DRI2 authentication fails; prior software-rendering
and compositing flags did not restore the Arena surface. The host script is at
`/home/kasun/Music/arena/consensus-arena/scripts/qualify-linux-native-runtime.sh`.
No UI-to-Apply run was claimed. See
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/delivery-v1-programme-qualification-2026-09-16.md`.

The standalone Dagu falsification gate has now been run without Arena
integration. Its local durable-run, retry, and root-human-task mechanics were
supported, but the Dagu→DSH composition and automatic reconciliation after a
hard interruption were not proven. Overall verdict: **INCONCLUSIVE**; no Dagu
integration or Astra consultation is authorized by this result. See
`audits/dagu-standalone-qualification.md`.

The follow-up prerequisite qualification is recorded in
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/delivery-v1-prerequisite-qualification-2026-09-16.md`.

See `audits/delivery-dsh-v4-worker-and-e2e.md` for the permanent evidence.

## Current strategic next layer

After seven substrate qualification sessions and a post-gates Astra consultation, the accepted architecture is **not** a monolithic agent substrate. The current recommendation is:

> Arena product authority + reusable durable workflow mechanics + bounded worker + independent deterministic verification.

**Dagu** remains a deferred workflow-engine candidate because it offers local durable runs, retries/history, root human tasks, and REST/CLI control. Standalone mechanics are now partly qualified, but the current overall falsification verdict is inconclusive: successful DSH composition and crash reconciliation remain unproven. **Dagu is not part of the current implemented Build lane.** Resolve the exact DSH runtime and complete backend Delivery reliability evidence before reconsidering integration; native Linux GUI proof remains a separate final gate.

## Documentation maintenance

After a meaningful milestone:

1. verify source/runtime first;
2. update only affected modular docs;
3. add or retain a permanent audit under `src-tauri/project-docs/audits/`;
4. update `DECISIONS.md` if a durable architecture/product decision changed;
5. update this index only if the document map/current transition point changed.
