# Consensus Arena — Project Documentation Index

**Last refreshed:** 2026-09-18
**Purpose:** Small, modular source-of-truth documents for humans, ChatGPT, and Codex working on Consensus Arena.

## Current product in one paragraph

Consensus Arena is a native Tauri 2 desktop application evolving from an AI expert-panel blueprint tool into a **product-authority and delivery system for a nontechnical product owner**. It now has two deliberately separate lanes:

1. **Consult** — the existing leader-driven frontier-model consultation system using authenticated consumer web chats and an OpenAI-compatible orchestration brain.
2. **Build / Delivery** — a new parallel lane whose current source can take bounded product work into an isolated Git worktree, author and freeze executable acceptance material, verify independently, repair with bounded attempts, persist owner questions, and apply only a verified candidate under strict Git preconditions. Its DSH worker prerequisite is currently materially challenged and blocks Build before admission.

The long-term direction is not to turn Arena into another coding-agent platform. Arena should own **product intent, owner decisions, progression policy, and acceptance authority**, while reusing external workers, workflow engines, Git, test runners, browser/native automation, deployment tools, and other commodity infrastructure.

## Milestone 05A current status

The 2026-09-18 preflight adds typed research provenance to the existing
Product OS evidence records and a canonical-checkout `HEAD`/complete-status
snapshot around external Delivery workers. Research proposals are unverified
until Arena records an independent verifier work-order and source metadata;
contradicted or stale claims cannot satisfy research gates. A candidate
working directory remains non-authoritative and is not an OS/security sandbox.

The narrow real research capability proven so far is direct read-only official
GitHub/web-source retrieval outside the Product OS runtime; the typed
proposal/finalization seam is not yet exposed through a durable Arena command
or founder-facing research store. The official GitHub MCP server, integrated
web search, ECC/gstack runtime procedures, shared consultation, and non-Zen
provider portability remain unqualified. Linux package production is proven
through the existing Tauri path, but installation/GUI launch are not claimed;
Windows current-SHA qualification and
native GUI launch remain unproven. See
`audits/product-os-transition/05A-research-runtime-release-preflight.md`.

## Reality hierarchy

When sources disagree, use this order:

1. **Current source and runtime evidence**
2. **Latest permanent audit in `audits/`**
3. **These project docs**
4. Historical prompts/specifications/research

Do not preserve a doc claim because it is written here. Correct the docs when current source proves otherwise.

The current programme audits through 2026-09-17 supersede older consultation-
only status language and earlier provisional substrate assumptions.

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

### Worker gate update — 2026-09-16

Two fresh npm installs from the newly preserved manifest/lock reproduced the
same dependency graph, DSH version, and root help output. Both then hung on
Arena's exact five-second `headless --help` prerequisite probe and were killed
by the bounded probe. The new lock is not the irrecoverable historical lock;
no model-backed task ran. The current worker decision is **DSH materially
challenged — Astra Trigger A**. The consultation packet is in
`audits/dsh-runtime-reproducibility/dsh-runtime-reproducibility.md`; no Astra
consultation response is claimed. Do not treat DSH as a qualified or durable
V1 prerequisite while this decision is open.

The final standalone Dagu closure also remains **INCONCLUSIVE — post-V1**.
Linux hard-interruption testing found an at-least-once side-effect risk and
required explicit recovery; Windows execution and DSH composition remain
unproven. No Dagu integration or Astra Trigger B consultation is justified.

The Linux host probe still reports no `/dev/dri`, llvmpipe, and no running
Arena/Vite process. Use the graphics-capable desktop procedure in
`RELIABILITY.md`; no GUI E2E is claimed.

The latest local post-implementation audit establishes a first Build vertical slice with these invariants at source/test level:

- Consult mode remains separate and unchanged.
- Build mode does not require leader/participants or model WebViews.
- Work occurs in an isolated `arena-delivery/<short-id>` Git worktree created from a clean base.
- The default DSH bounded-worker path remains materially challenged and blocked before admission; an opt-in OpenCode path now provides bounded Linux candidate execution behind an Arena-owned authority adapter.
- Executable acceptance material is authored **before implementation**, frozen, protected by hashes, and verified independently from worker narration.
- Required checks are rerun after repair; attempts are bounded.
- `needs_user` is persisted before the existing AskUser event is emitted; the source recovery path can re-present a persisted question after restart, but a separate production-service restart/answer runtime has not been qualified.
- Apply is explicit and source guards reject dirty, moved, or non-fast-forward bases; the successful production Apply path has not been runtime-qualified.
- DSH is not yet bundled/installed by Arena.
- Build setup performs a read-only DSH prerequisite check and blocks
  Delivery admission unless the expected `0.1.5-rc.1` executable exposes the
  `headless` profile. A newly preserved manifest/lock reproduces across two
  clean installs, but both hit the probe timeout. This runtime is not
  worker-qualified and the exact historical lock remains unavailable.
- Native Linux Tauri launch was reconfirmed, but the managed WebView was blank
  during the latest qualification session, so no Delivery UI dogfood run was
  started in that session.

See `DELIVERY.md`, `audits/delivery-loop-v1-linux-runtime-qualification.md`,
and the current qualification record in
`audits/delivery-dsh-v4-worker-and-e2e.md`.

The current OpenCode authority-adapter and walking-skeleton evidence is in
`audits/product-os-transition/02-product-os-authority-walking-skeleton.md`.
It qualifies only the installed Linux OpenCode 1.17.18 + Muse Spark path for
bounded candidate execution. Windows, packaging, unrestricted authority, and
production-scale performance remain unproven.

Milestone 04 adds a source-level evidence-gate evaluator and a minimal
founder-facing Intent → Build → Verify → Apply summary. The evaluator is a
pure Arena-owned contract with stale/missing-evidence fail-closed behavior;
it is not yet a durable journey store or a replacement for Delivery and
SessionRuntime. GitHub MCP, pinned ECC/gstack procedures, and external
consultation execution remain research capabilities rather than runtime-
qualified features.

Milestone 05 adds the bounded Arena-owned Build Package handoff: current
authority records are assembled and fingerprinted in existing Delivery state,
then rechecked before OpenCode admission. Existing SessionRuntime, candidate
worktree, frozen acceptance, independent verifier, repair, and Safe Apply
remain authoritative. The candidate directory is non-authoritative, not an OS
or security sandbox. Windows, packaging, hostile same-user isolation, broad
provider portability, GitHub MCP, ECC/gstack, and external consultation
execution remain unproven.

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

The standalone Dagu closure remains **INCONCLUSIVE — post-V1**. After hard
interruption, the active attempt required explicit reconciliation and repeated
an external side effect. Human-task recovery also needed explicit retry after
restart. DSH composition and Windows execution remain unproven. See
`audits/dagu-runtime-closure-2026-09-16.md`.

The follow-up prerequisite qualification is recorded in
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/delivery-v1-prerequisite-qualification-2026-09-16.md`.

See `audits/delivery-dsh-v4-worker-and-e2e.md` for the permanent evidence.

## Current strategic boundary

After the substrate gates and prior Astra consultation, the accepted
architecture is **not** a monolithic agent substrate. Arena keeps product
authority, frozen acceptance, independent verification, and safe Apply; its
bounded worker selection is now open under required Astra Trigger A.

> Arena product authority + reusable execution/workflow mechanics where proven useful + a bounded worker + independent deterministic verification.

**Dagu** remains a post-V1 workflow-engine candidate because it offers local durable runs, retries/history, root human tasks, and REST/CLI control. Its standalone result is **INCONCLUSIVE**: a hard interruption required explicit reconciliation and repeated an external side effect; DSH composition and Windows execution remain unproven. **Dagu is not part of the current Build lane.** Do not spend further V1 release effort on Dagu integration. The current DSH failure independently triggers the targeted Astra consultation recorded in its audit.

## Documentation maintenance

After a meaningful milestone:

1. verify source/runtime first;
2. update only affected modular docs;
3. add or retain a permanent audit under `src-tauri/project-docs/audits/`;
4. update `DECISIONS.md` if a durable architecture/product decision changed;
5. update this index only if the document map/current transition point changed.
