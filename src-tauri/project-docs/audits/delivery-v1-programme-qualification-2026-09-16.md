# Delivery V1 programme qualification — 2026-09-16

**Branch:** `codex/arena-dev-temp`

**Starting checkpoint:** `003c42cf8a584c7e7416fb7edeb0ef7793ba291f`

## Outcome

This milestone advanced the source and test baseline, but did not runtime
qualify Arena Delivery. The required UI-to-Apply path was not substituted or
claimed: the native WebKitGTK window launched, while both managed `tauri dev`
and direct debug WebViews rendered blank in this Linux session.

## Worker baseline

The proven external control remains `meta/muse-glimmer-30b` through
`@deepseek-ai/dsh@0.1.5-rc.1`. The installed tree contains one rc1 top-level
package and 230 rc2 transitive DSH packages. The mixed tree is expected: the
top-level package pins rc1 while its `^0.1.5-rc.1` dependencies and the
`@mstar-harness/dsh@3.8.3` peer range resolve compatible rc2 components.
The external runtime has a byte-identical pnpm lockfile and is reproducible
assuming registry/store availability, but Arena does not install or bundle it.

The prior successful Muse run remains the only runtime-proven worker control.
This session reconfirmed endpoint reachability and the generated Arena patch
contract, but a second clean successful run was not completed. DeepSeek V4
Flash remains unqualified after the prior bounded timeout/no-receipt run.

## Native Linux UI

- `tauri dev` started Vite on `http://localhost:1420/` and created the native
  `Consensus Arena` window.
- WebKitGTK emitted `libEGL warning: DRI2: failed to authenticate`.
- `/dev/dri` was absent and `glxinfo -B` did not complete within the bounded
  diagnostic window.
- The same blank result occurred in the direct debug binary, so Vite/Tauri
  URL agreement and React bootstrap were not identified as the cause.
- No frontend console bridge was available. No source workaround was
  introduced, and no Delivery UI run was claimed.

The blank WebView is currently classified as a Linux graphics/runtime
environment blocker. A software-rendering launch workaround remains a
development-machine experiment, not product architecture.

## Delivery and reliability matrix

| Scenario | Status |
|---|---|
| UI → worktree → acceptance → implementation → verification → Verified → Apply | Unproven |
| Normal PASS through Arena | Unproven |
| FAIL → repair → PASS | Unproven |
| INCONCLUSIVE → resume | Source defect fixed; runtime unproven |
| WaitingForUser persistence/answer | Source/test confirmed; runtime unproven |
| Restart recovery/re-presented question | Unproven |
| Active-worker abort and child cleanup | SessionRuntime source/tests confirmed; Delivery runtime unproven |
| Protected-acceptance modification | Source/test path confirmed; runtime unproven |
| Dirty Apply refusal | Source-confirmed; Arena runtime unproven |
| Changed-HEAD Apply refusal | Source-confirmed; Arena runtime unproven |
| Non-fast-forward refusal | Supporting Git behavior only; Arena runtime unproven |

The adversarial review also found that the prior INCONCLUSIVE resume path could
hard-reset away an uncommitted candidate; this milestone preserves that
candidate for frozen verification. Receipt IDs are now constrained for
evidence-path safety, DSH timeout cleanup is explicit, and delayed Delivery UI
events are filtered by session identity after a current run is known.

## Memory qualification

Phase 1 memory is source-implemented and schema-smoke-tested. Source confirms
session/project/global tables, provenance/ranking, FTS triggers/search/repair,
export/restore, async `run_blocking` access, non-fatal router handling, and
frontend JSON/event plumbing. Functional CRUD, context injection, reliability
writes, AskUser resolution, FTS repair UI, export/restore, and cross-restart
behavior remain runtime-unproven. The only dedicated memory test is the schema
smoke test.

## DSH distribution recommendation

Keep DSH external for current V1. Arena should detect/configure the prerequisite
and document the tested runtime contract before considering app-data install,
bundling, or embedding. A distribution change would introduce Node/runtime
carrying cost, update/security policy, license review, Windows packaging work,
and offline support obligations that are not justified by the current single
successful control run.

## Dagu gate

Not run. The Linux Delivery happy-path gate was not reached, so no Dagu
standalone qualification or Arena integration decision is made.

## Security and resource boundary

No credentials were copied into source, evidence, or this audit. One external
diagnostic process listing exposed a credential-bearing command argument;
rotation remains recommended if that credential has not already been rotated.
The successful prior Muse control observed roughly 152–184 MiB DSH RSS.
Compiler RSS during the full Rust test build is not a product-runtime measure.

## Next qualification gate

First establish a repeatable software-rendering development launch or a
graphics-capable Linux runtime, then complete a genuine Arena Delivery run with
the proven Muse profile. Only after that gate should the reliability scenarios
and the narrow standalone Dagu falsification experiment begin.
