# Delivery DSH V4 worker and Linux E2E qualification

**Date:** 2026-09-15 to 2026-09-16

**Scope:** qualify `deepseek-ai/deepseek-v4-flash-0731` through the installed
DSH path, use at most one control model, and continue into Arena Delivery if
the worker gate succeeds. Dagu was not implemented or tested.

## Result at a glance

The DeepSeek V4 Flash hosted route did not qualify as an Arena worker in this
session. The direct model-list request succeeded, but a bounded simple
completion timed out and the standalone DSH run did not perform a repository
change or produce a worker receipt.

The permitted control, `meta/muse-glimmer-30b`, completed a real coding task
through the same NVIDIA endpoint and DSH profile shape. It changed the
repository, passed the deterministic test, and produced a schema-version-1
`.arena-runtime/result.json` containing the fields Arena parses. This separates
the result from a general DSH/OpenAI-adapter failure, but does not qualify the
DeepSeek V4 route or authorize changing Arena's durable default model.

The genuine Arena Delivery UI flow was not reached. Native Tauri windows
launched on X11, but the managed development WebView remained blank; the
standalone debug binary without the Vite server showed the expected localhost
connection error. No Delivery state was advanced by manual substitution.

## Provider sanity

- Display was available as `DISPLAY=:0.0`; Wayland was unset.
- The existing NVIDIA credential reference was loaded in memory from the app
  settings store. Its value was never printed, persisted in evidence, or put in
  a repository file. `NVIDIA_API_KEY`, `OPENAI_API_KEY`, and
  `ARENA_DSH_API_KEY` environment references were unset in the shell.
- Endpoint: `https://integrate.api.nvidia.com/v1`.
- DeepSeek V4 Flash model-list request: HTTP 200.
- DeepSeek V4 Flash bounded non-streaming completion with no reasoning and a
  small output cap: timed out after 25 seconds without an HTTP response. No
  authentication failure was observed. No generic native tool-call result was
  used as an absolute DSH prerequisite.
- The single control request to `meta/muse-glimmer-30b` completed in about
  0.8 seconds and returned a structured `tool_calls` entry. Its message also
  contained `content`, `reasoning_content`, and `role` fields.

## DSH provider/adapter contract

The installed top-level package was `@deepseek-ai/dsh@0.1.5-rc.1`. Its
resolved transitive DSH bundles were `0.1.5-rc.2`, so this is an rc1 package
with a mixed resolved dependency tree; Arena does not currently pin or bundle
DSH.

The inspected adapter contract is:

- Arena's generated profile declares a custom `openai-completions` provider,
  `apiKeyEnv`, NVIDIA `baseURL`, model ID, context window, and `maxTokens`.
- The adapter uses pi-ai's streaming model path and maps native tool-call
  stream events into DSH Harness tool-call chunks. Tool results are replayed
  as provider tool-result messages.
- System prompts are passed as the provider system prompt. Arena's generated
  profile does not add a reasoning setting, developer-role override, alternate
  token-field setting, or custom stream protocol.
- The installed compatibility schema supports model metadata and fields such
  as developer-role, reasoning, token-field, and thinking-format compatibility,
  but no evidence required changing those fields for the control run.
- DSH itself does not create Arena's worker receipt. The bounded prompt must
  cause the model/worker to write `.arena-runtime/result.json`; Arena then
  validates schema 1 and its structured verification-command contract.

## Standalone worker qualification

### Primary: DeepSeek V4 Flash

Disposable repository initial HEAD:
`8629fd9f79db13846e545912e9e6ddd3ecb5e04e`.

The task was deterministic: change one function from `before` to `after`, keep
the existing test unchanged and passing, and write a complete schema-1 Arena
worker result. DSH was launched with the exact Arena-style headless invocation
and patch profile.

- Real coding change: **no**.
- Real tools: **not proven**; no repository mutation occurred.
- Valid worker result: **no**; `.arena-runtime/result.json` was absent.
- Clean termination: **no**; the bounded outer run reached 360 seconds and
  the exact worker process was terminated by the qualification harness.
- Observed DSH process RSS was approximately 136–139 MiB during the run.

### Control: Muse Glimmer

Disposable repository initial HEAD:
`40e2f6f405bfde0eed8cadf05bd059a582c6134a`.

- Real coding change: **yes**; `tiny.py` changed from `before` to `after`.
- Real tools: **yes**; DSH created/used the test-side file and the existing
  deterministic test passed with exit 0.
- Valid worker result: **yes**; result JSON had schema version 1, status
  `complete`, one acceptance item, and one verification command. The command
  fields matched Arena's `WorkerResultContract` shape.
- Clean termination: **yes, observed**; the DSH process was absent after the
  result was written. The qualification wrapper did not preserve a separate
  numeric parent exit-code line, so that detail remains unrecorded.
- Observed control DSH RSS ranged approximately 152–184 MiB. This does not
  replace earlier resource observations.

The control was used only because the primary route timed out through DSH and
the control's documented native tool path could distinguish a V4/provider
compatibility problem from a universal DSH adapter failure. No third model was
tested.

## Arena Delivery E2E

**Not proven.** A fresh disposable repository was prepared at initial HEAD
`e954bd10402f31f048137748377a80ee47a9a95f`, and the durable app model setting
was restored afterward. The full sequence

`UI → worktree → acceptance → implementation → verification → Verified → Apply`

was not executed. The app's native X11 window was visible, but its WebView was
blank under the managed `tauri dev` launch in this environment. The app was
not driven by manually invoking middle-stage commands, so no fabricated
Delivery ID, acceptance freeze, candidate SHA, receipt, or Apply result is
reported.

## Reliability branches

The following are **unproven in this session**: PASS, FAIL → repair → PASS,
INCONCLUSIVE resume, WaitingForUser, restart recovery, active-worker abort, and
Apply guards for dirty source, changed HEAD, and non-fast-forward.

The source and existing tests still confirm the intended protections:
CLEANBASE, WORKTREE, ACCEPTANCEFREEZE, PROTECTEDACCEPTANCE,
INDEPENDENTVERIFY, SAMECHECK, BOUNDEDREPAIR, DURABLEQUESTION, ASKDISMISS,
EXACTABORT, NOSECRETS, SAFEAPPLY, and CROSSPLATFORM. Runtime execution of
those branches remains a separate qualification requirement.

## Bugs and root causes

No Arena source defect was established, so no production source change was
made. The observed blockers are qualification/environment findings:

1. The V4 route's model-list endpoint was healthy, but bounded inference
   timed out; the root cause is not yet isolated between service latency,
   model overload, or route/request compatibility.
2. Direct execution of the debug binary without the configured Vite server
   renders a localhost connection error by design.
3. The managed `tauri dev` window launched but rendered blank on this X11
   session, with GTK/DRI diagnostics on stderr. Renderer/browser-console root
   cause remains unproven.

No Arena default, DSH source, provider framework, or workflow engine was
changed.

## Invariant and security review

- **Runtime proven:** native X11 window launch; the single control DSH run.
- **Test/source confirmed:** Delivery ownership, acceptance freeze, protected
  hashes, independent verification, durable question plumbing, abort, and
  Apply guards as implemented and covered by current source/tests.
- **Unproven:** DeepSeek V4 worker qualification, full Arena Delivery E2E, and
  all runtime reliability branches.
- **Concern:** DSH is not distributed by Arena; the installed qualification
  tree is mixed rc1/rc2; the current model route has no bounded coding proof.

No credential value, token, or secret-bearing output was recorded in this
audit, app state, source, result files, or Git.

## Decision and next milestone

Worker decision: **DSH remains promising but qualification incomplete.** The
control proves that the current DSH/OpenAI-completions boundary can perform a
bounded coding task and emit Arena's required receipt, while the requested
DeepSeek V4 route remains unqualified.

Immediate next substantial milestone: **finish Linux runtime reliability** so
the proven control profile can drive genuine Arena Delivery E2E and the
reliability branches can be exercised. Re-qualify V4 only with evidence-backed
request/latency findings; do not add Dagu or sweep models.
