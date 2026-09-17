# DSH current runtime reproducibility and worker gate

**Date:** 2026-09-16
**Branch:** `codex/arena-dev-temp`
**Starting checkpoint:** `dee98a487378bc41900fb6e3c6eb0d98f2432ef0`

## Verdict

**DSH CURRENT WORKER ASSUMPTION MATERIALLY CHALLENGED — ASTRA TRIGGER A.**

The new npm install baseline reproduced across two clean installs, but Arena's
production prerequisite probe for the `headless` profile timed out in both.
The bounded gate stopped before model-backed work. No Muse task, production
Delivery run, worker RSS measurement, or DSH descendant-tree run was attempted.

The new lock is a deterministic install artifact, not a qualified worker
runtime and not a reproduction of the historical lock.

## Pinned environment and install inputs

- Platform: Linux x86_64.
- Node: `v22.22.2`.
- npm: `10.9.7`.
- Top-level DSH: `@deepseek-ai/dsh@0.1.5-rc.1`.
- Harness: `@mstar-harness/dsh@3.8.3`.
- Model selected for the two-task gate: `meta/muse-glimmer-30b`.
- Planned provider config: OpenAI-compatible NVIDIA endpoint
  `https://integrate.api.nvidia.com/v1`; this endpoint was not contacted in the
  current gate.
- Profile: `headless`.
- npm manifest SHA256: `2a227dd10076bfe86a0a453ee9b6da163087210e6f9e354f484906ecb13a4baa`.
- npm lock SHA256: `563f454a9e732e1090be474dfde3261fc479b648fa295ad0809f88056f712582`.

The pinned inputs are preserved beside this audit as `package.json` and
`package-lock.json`. They contain no `node_modules`. The manifest is a
qualification harness input and is not part of the product dependency graph.

This lock differs from the unreproduced historical SHA256
`1297ec9257567a85c5a653734979256e6958a2c1235079c62fdc5bb9f2505887` and from
the earlier candidate SHA256
`88a26a4d1f31bdffd465bf081f5d02638f0f42982aeb235ff7f4d1365a004773`.
Do not describe it as the historical lock.

## Clean install A/B evidence

Two newly created install directories used byte-identical copies of the
manifest and lock. Both ran normal lifecycle scripts with
`npm ci --no-audit --no-fund --engine-strict`, without provider credentials.
Each install exited successfully with 528 packages added. DSH's subprocess
helper `postinstall`, `node-pty`, and protobufjs postinstall steps exited 0.

The exact manifests and resolved lock graphs matched. Both resolved the same
588 lock records, including one top-level DSH rc1 package and 233 DSH rc2
components. The lock-resolved tree SHA256 matched at
`e7cb23f3832f7fa246e405f2bace26e18829a7da41ad0bdf90e7a4cc87179630`; the
installed tree SHA256 matched at
`4df7e7ee4e69fb64d3f256a9db71502c9eca0fe5cac86ef8f934913b8d23ef4f`; and
the normalized `npm ls --all` graph SHA256 matched at
`b171c8ab2de09732751fe8f2a270de6b07fa30ac28f435190df04a39252e8ea0`.
Both graphs contained 2,130 nodes and the same two optional-package notices.

`dsh --version` reported `0.1.5-rc.1` in both installs with matching output.
Root `dsh --help` exited 0 in about 1–1.5 seconds with identical output.
Arena's actual profile capability check, `dsh --profile headless --help`,
produced no output and did not exit within Arena's five-second timeout in
either install. The harness killed each timed-out process after its one-second
kill grace (observed exit 137). Both headless probes therefore failed the
same production capability check even though package resolution was
reproducible.

The earlier diagnostic pair used `--ignore-scripts`; it is not qualification
evidence because it skipped a required helper `postinstall`. The normal pair
above is the final install gate. No further setup combinations were tried.

## Muse tasks and production Delivery

Required Muse task 1: **not started**.
Required Muse task 2: **not started**.

The exact Arena probe failed before model credentials were needed. No provider
credential was supplied, read, or printed. No DSH model call, tool use, repo
edit, deterministic test, schema-1 result file, Arena parser acceptance,
candidate worker, RSS sample, or post-worker process-tree observation exists
from this gate.

Production path
`admission → acceptance → implementation → verification → Verified → Apply`:
**not run**. No production Delivery result is claimed.

## Arena contract and release implications

Current Arena worker contract, owned outside DSH:

- `SessionRuntime` owns admission and cancellation.
- Arena validates a clean Git repository, captures original HEAD, and creates
  the isolated worktree.
- Acceptance is authored and frozen before implementation; protected paths
  and hashes remain Arena-owned.
- Invocation is `dsh --profile headless --patch <patch-file> <prompt>` from
  the candidate worktree. Arena clears the inherited environment and supplies
  an allowlist of runtime/path variables, the configured DSH home, and the
  Agent Brain credential as `ARENA_DSH_API_KEY`. Preflight probes use the
  same filter without a provider credential. DSH and its child tools still
  need access to the configured Agent Brain credential to call the model.
- Worker output must contain `.arena-runtime/result.json` schema 1. Arena
  parses the structured result and independently runs frozen verifier
  commands. DSH cannot declare the candidate Verified.
- Arena owns correlated receipts, bounded repair, durable owner questions,
  and explicit safe Apply.

Consequences:

- The exact production prerequisite check rejects both clean installs, so V1
  Build is blocked before worktree/session creation on this baseline.
- Historical one-run Muse success remains useful evidence that the adapter
  can work under one prior environment. It does not override the current
  probe, establish reproducibility, or meet the two-run gate.
- The prior V4 Flash request timed out; it is not a substitute worker route.
- Windows DSH package installation, executable resolution, and headless
  capability passed on the native runner (workflow 35200869335). Windows npm
  `.cmd` shims resolve to the package's Node entrypoint without a shell. This
  was prerequisite evidence only; no provider secret or model-backed Windows
  worker run was used.
- Current `kill_on_drop` evidence concerns the tracked direct child only.
  DSH descendant cleanup remains unproven on Linux and Windows.

## Earlier evidence and rejected/watchlisted alternatives

The permanent audit `delivery-dsh-v4-worker-and-e2e.md` records one earlier
`meta/muse-glimmer-30b` run against the NVIDIA OpenAI-compatible endpoint. It
performed a real edit and deterministic test, wrote an Arena-shaped schema-1
receipt, and observed approximately 152–184 MiB DSH RSS. Its wrapper did not
record a separate numeric parent exit status. This is single-run historical
evidence, not current qualification.

The old documented lock was not reproducible. The newly generated current npm
lock is reproducible, but the five-second Arena headless probe fails. No
replacement worker was selected in this gate. Existing rejected or watchlisted
directions remain as recorded in `SUBSTRATE_RESEARCH.md`: OpenHands, OpenCode,
Pi, Morning Star, Fusion, Factory Missions, and controller candidates such as
LangGraph/XState/DBOS have not been requalified as drop-in bounded workers.
Dagu is a separate orchestration candidate and remains INCONCLUSIVE; it is not
a worker replacement.

## Astra consultation packet — Trigger A

Consultation is required before selecting a replacement architecture. The
available toolset in this execution environment did not expose an Astra
consultation connector, so no Astra response is claimed. This packet is ready
to submit:

### Evidence

- Historical DSH evidence: one Muse control run succeeded with real tool use,
  repository edit, deterministic test, schema-1 result, and 152–184 MiB
  observed RSS; exact parent exit code was not retained.
- Historical DSH lock
  `1297ec9257567a85c5a653734979256e6958a2c1235079c62fdc5bb9f2505887` could
  not be reproduced. The prior candidate lock `88a26a4...` was a different
  package tree.
- New manifest/lock install A and B match byte-for-byte and both install
  successfully with scripts enabled. The new lock is
  `563f454a9e732e1090be474dfde3261fc479b648fa295ad0809f88056f712582`.
- In both clean installs, `dsh --version` and root `--help` pass, but the
  exact Arena capability probe `dsh --profile headless --help` times out and
  is killed. No model-backed tasks were attempted.
- Resource evidence is limited to the historical one-run RSS and the current
  install/probe runs. No current worker RSS, active-tool process tree, or
  descendant cleanup was observed.
- Windows build qualification is being pursued independently. Windows DSH
  setup/probe and provider-backed execution are not proven; no secret was
  placed in CI.
- Arena requires bounded worker invocation, worktree-only edits, protected
  frozen acceptance, a valid schema-1 result, independently correlated
  deterministic verification, bounded repair, needs-user result support,
  kill/wait cleanup, and native Linux/Windows behavior. Arena remains owner of
  admission, acceptance authority, receipt correlation, Verified, and Apply.
- OpenHands has been rejected for missing external result submission, NIM
  `prompt_cache_key` incompatibility, and unproven repair/restart/Windows.
  OpenCode is only a reserve candidate; Pi should be reconsidered only after
  this material DSH failure. Dagu remains INCONCLUSIVE and must not be treated
  as a worker replacement. Other alternatives above are watchlisted, not
  qualified.

### Question

> With Arena's work-order, frozen-acceptance and verifier contracts now stable, should V1 replace DSH, maintain a narrow compatibility layer/fork, or qualify a different bounded worker? Optimize for reproducibility, native Linux/Windows, one OpenAI-compatible endpoint, low carrying cost, and modest hardware.

Do not redesign Arena's product or acceptance contracts as part of this
consultation.
