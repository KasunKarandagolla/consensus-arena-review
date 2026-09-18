# Milestone 05A — Research Runtime and Release Preflight

**Date:** 2026-09-18
**Scope:** research capability, evidence provenance, canonical-checkout detection, reusable procedure selection, provider/package/platform preflight
**Predecessor:** `622a2f725aeafcf128b7912d73269487b3727d8c`
**Status:** complete with bounded limitations; final M06 dogfood not executed

## Executive result

Milestone 05A adds typed research provenance to the existing Product OS evidence
records and adds a canonical-checkout snapshot around external Delivery workers.
The research contract now distinguishes an agent proposal from an independently
verified claim, contradiction, unresolved status, source identity, scope, and
revisit trigger. The canonical guard records original `HEAD` and complete
tracked/untracked Git status before and after worker execution. Any canonical
change is a fail-closed invalid result; Arena does not silently reset the
checkout.

The real read-only research route proven in this milestone is direct public
GitHub API/source retrieval plus official web-source retrieval. The official
GitHub MCP server was inspected and its read-only/toolset controls were
confirmed from upstream documentation, but it is not installed or runtime-
qualified in Arena. No external non-Zen provider or permitted consultation
provider was available for a new real task. M06 is therefore not a claim of
GitHub-MCP, frontier-consultation, Windows, or packaged GUI qualification.

## Evidence classification

### Documented upstream

- The official GitHub MCP server documents read-only mode, toolset allow-lists,
  and a lockdown/prompt-injection filter. Those controls are not treated as an
  Arena authorization boundary: [GitHub MCP repository](https://github.com/github/github-mcp-server)
  and [server configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md).
- OpenCode documents provider/model identifiers and custom OpenAI-compatible
  provider configuration: [providers](https://opencode.ai/v2/docs/providers) and
  [models](https://opencode.ai/v2/docs/models).

### Source-confirmed in Arena

- `EvidenceItem` now carries origin, evidence kind, verification disposition,
  source reference/URL/title/check time/scope, verifier work-order ID,
  contradiction references, decision impact, and revisit trigger.
- Product OS research proposals are forced to `Unverified`; a proposal cannot
  self-declare verifier origin or independent verification. Finalization
  requires an Arena-supplied verifier work-order ID and source metadata.
- Problem Research and Positioning gates accept only current
  `ResearchClaim` records with `IndependentlyVerified` status, a source, a
  verifier work-order ID, and no contradiction references.
- OpenCode and DSH execution paths snapshot canonical `HEAD` and complete
  `git status --porcelain=v2 --untracked-files=all` before and after the
  worker. The candidate remains a non-authoritative working directory, not an
  OS or security sandbox.
- Canonical mutation is rejected before Arena creates a candidate commit; no
  automatic reset/clean is performed.

### Runtime-proven

- GitHub research question: “What is the current official GitHub MCP server
  repository, license, release line, and documented read-only control?” A
  direct unauthenticated HTTPS request retrieved the official repository JSON,
  release/source metadata, and raw README without a credential value being
  emitted. Observed CLI inventory: `gh` 2.4.0+dfsg1; the authenticated `gh`
  path was not used as the secret-free qualification proof.
- A source-level research proposal/finalization test proves that an unverified
  claim does not satisfy the gate, an independently verified claim does, and a
  contradicted claim remains inspectable but cannot pass. This is the existing
  Product OS record path, not a new research database.
- Canonical mutation adversary: a Git fixture changed an unprotected tracked
  file in the canonical checkout; the before/after snapshot detected it and
  left the mutation for explicit operator recovery rather than silently
  masking it. The focused test passed.
- The existing real Muse/OpenCode qualification remains historical runtime
  evidence: OpenCode 1.17.18 with
  `opencode/muse-spark-1.2-contributor-free` previously executed bounded file
  work and the M02 walking skeleton. Two fresh 05A reruns of the larger
  adversarial walking-skeleton test safely failed closed (one non-zero worker
  result, one no-candidate-change result); neither is counted as a new model
  success.
- Linux frontend/production packaging completed through Tauri’s existing
  package path. The artifact is
  `src-tauri/target/release/bundle/deb/Consensus Arena_0.1.0_amd64.deb`,
  SHA256
  `c6c7b9b3697c1559bb51d750a1978b343edac95c02e8c3a5c7bebc373815e29d`,
  9,923,776 bytes. `dpkg-deb --info` and contents inspection succeeded; it
  declares `libwebkit2gtk-4.1-0` and `libgtk-3-0` and contains the Arena
  binary/desktop entry only. It was not installed or GUI-launched on this
  host.

### Integrated

- The new evidence provenance and canonical snapshot are integrated into the
  existing Product OS/Delivery/OpenCode source boundaries. Canonical failure
  state is persisted by the Delivery/OpenCode path before it returns to the
  caller. No second workflow, verifier, evidence database, MCP framework, or
  worktree manager was added.
- The focused Rust suites cover evidence-gate provenance, Product OS research
  proposal/finalization, canonical mutation detection, Delivery invariants,
  consultation validation, and OpenCode stale/cancelled/protected-result
  handling.

### Optional or unproven

- The Product OS research helpers are source-level and test-covered, but are
  not yet wired to a Tauri command or durable founder-facing research store.
  The direct GitHub/web retrieval observed in this preflight therefore does
  not yet constitute an Arena-produced durable research package. This is the
  primary M06 blocker.
- Official GitHub MCP runtime use, repository search through MCP, and MCP
  permission enforcement are unproven because no MCP server is configured.
  Direct public GitHub retrieval proves a narrow research capability, not MCP
  integration.
- A browser/search MCP or other single integrated web-search provider was not
  added. Official web documentation was retrieved during research, but no
  durable Product OS research package was produced through a Product OS UI.
- ECC/gstack are not present locally and were not installed. The selected
  reusable procedure is the local Superpowers `verification-before-completion`
  SOP, snapshot commit `1dc195897af4161d039b80d8471ec0a10c9bbc89`, procedure
  SHA256 `2befe7fc55bcadaa3d97dd9e8efeb633d2561c0ebe74c5a8b17c4d9e7e4520b`.
  It was used as a verification discipline, not copied into Arena or exposed
  as a worker authority.
- Shared consultation execution remains optional/unproven: the seam and
  budget/redaction rules are source-confirmed, but no permitted configured
  provider was invoked in this preflight.
- Non-Zen provider portability remains unproven. No NVIDIA key was requested.
- Windows current-source compilation/package/process qualification and native
  GUI launch remain unproven. Historical workflow/package records are not
  upgraded to current-SHA proof.
- Same-user hostile filesystem access and shared Git object-store isolation
  remain residual risks. A rejected candidate may still have caused Git object
  or filesystem effects before detection; secret scanning before Arena-created
  commits and release isolation remain required follow-up work.

## Real research contract exercised

The bounded GitHub question followed:

`question → source retrieval → source identity → claim proposal → independent
verification disposition → contradiction/unresolved handling`

The source register retained repository/release/README identity, URL, title,
retrieval time, and scope. A proposal is not accepted as fact merely because a
worker emitted it or because multiple workers agree. Material changing claims
must carry a version/date or scope and a revisit trigger. The stopping rule was
bounded at repository metadata, release metadata, README controls, and one
official OpenCode provider/model source pass; further searches were not needed
to change the current decision.

## Release preflight matrix

| Area | Status | Evidence boundary |
|---|---|---|
| Real web research | PROVEN (narrow direct official-source route) | Official source retrieval; no integrated search UI/MCP |
| GitHub research | PROVEN (direct read-only public route) | Official API/raw source; GitHub MCP itself unqualified |
| Fact verification | PROVEN (record/gate semantics) | Independent disposition is required; no broad research dogfood |
| Skill/SOP reuse | PROVEN | One pinned local verification SOP used; ECC/gstack deferred |
| Shared consultation | OPTIONAL-UNPROVEN | No permitted configured provider runtime |
| Non-Zen provider | UNPROVEN | No credential requested or used |
| Canonical checkout protection | PROVEN (detection/authority guard) | HEAD/status snapshots; not a same-user sandbox |
| Linux package | PROVEN / inspect-only | Existing Tauri `.deb` built and inspected; no install/launch claim |
| Windows build/package | PARTIAL | Existing workflow/history only, not current-SHA qualification |
| Native GUI | ENVIRONMENT-BLOCKED | Host lacks usable `/dev/dri` WebKit acceleration |
| Delivery authority | PROVEN | Existing Arena work-order, acceptance, verifier, Apply boundaries |
| Independent verifier | PROVEN | Existing verifier remains separate from worker |

## Independent review decision

**BLOCKED FOR M06 release closure.** The core authority path remains intact and
the narrow direct research route is real outside the Product OS runtime, but
the proposal/finalization helpers are not yet reachable through a durable
Arena command/store path. Final founder dogfood must not claim GitHub MCP,
integrated web-search, consultation, external-provider, Windows,
install/launch, or package-complete evidence. M06 can be reconsidered after a
narrow Arena-owned research-ingestion seam is implemented and verified, and
the final dogfood uses real research rather than simulated records.

## Reproduction commands

```text
cd /home/kasun/Music/arena/consensus-arena/src-tauri
cargo test evidence_gates::tests -- --nocapture
cargo test product_os::tests -- --nocapture
cargo test delivery::tests -- --nocapture
cargo test consultation::tests -- --nocapture
cargo test opencode_adapter::tests -- --nocapture
cd /home/kasun/Music/arena/consensus-arena
npm run build
npm run tauri build -- --bundles deb
```

No secret values, provider credentials, raw prompts containing credentials, or
environment snapshots are retained in this audit.
