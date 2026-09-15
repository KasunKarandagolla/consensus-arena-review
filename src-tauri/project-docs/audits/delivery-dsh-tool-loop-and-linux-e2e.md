# Delivery DSH Tool-Loop and Linux E2E Qualification Audit

**Date:** 2026-09-15
**Starting source checkpoint:** `86f751b816b4e0cec80e7865f6fa687797e05008`
**Branch:** `codex/arena-dev-temp`

This audit records the direct provider qualification and native Linux checks
performed after the prior DSH runtime qualification. It does not alter the
historical audits and does not represent any unproven Delivery stage as
successful.

## Direct provider tool-call qualification

The direct diagnostic used the configured endpoint
`https://integrate.api.nvidia.com/v1` and read the stored credential only
in-memory. Diagnostic output reported only endpoint, model, HTTP status, and
response-shape metadata.

### Primary candidate: Nemotron 3 Super

- Model: `nvidia/nemotron-3-super-120b-a12b`.
- Protocol: OpenAI-compatible `POST /v1/chat/completions`.
- Request: one deterministic `add_numbers` function, `tools`, a required or
  automatic tool choice, bounded `max_tokens`, and no streaming.
- NVIDIA’s current model API schema was checked before probing. The documented
  reasoning controls are `reasoning_effort` (`none`, `low`, `high`) and
  `reasoning_budget`; the generic NVIDIA function-calling documentation
  requires `tools` together with `tool_choice`.
- The first named-tool request returned HTTP 500. A `required` request with
  the documented temperature/top-p defaults returned HTTP 200, but the
  assistant message had no native `tool_calls` array and did have ordinary
  content. `auto` returned HTTP 500.
- Additional reasoning-mode checks returned HTTP 400 for the older
  `chat_template_kwargs` form and HTTP 500 for the documented
  `reasoning_effort` forms. None returned native `message.tool_calls`.

**Qualification:** **FAIL for the hosted route.** The endpoint was reachable,
but no expected native OpenAI `tool_calls` structure was proven.

### Single permitted fallback

`moonshotai/kimi-k2-instruct` was selected because NVIDIA’s official model
documentation contains an OpenAI-compatible native tool-calling loop that
passes the assistant tool-call message back and appends a `role: tool` result.
Its direct request returned HTTP 410. No tool call or continuation was
available to execute.

No broader model sweep was performed, and no Arena model default was changed.

## Direct two-turn tool loop

**Not proven.** The primary candidate produced no native tool call, and the
single fallback candidate was unavailable. Therefore no local tool execution,
tool-result message, or final continuation response can be claimed.

## Standalone DSH qualification

**Not run by design.** DSH `@deepseek-ai/dsh@0.1.5-rc.1` was not launched in
this session because the required direct native tool loop did not pass. There
is consequently no new evidence for DSH tool acceptance, repository change,
`.arena-runtime/result.json`, Arena parser compatibility, or clean process
completion. Dagu was not integrated.

## GUI environment and native Build UI

- `DISPLAY=:0.0` was set, `WAYLAND_DISPLAY` was unset, and the X11 root was
  reachable.
- The current native binary was built with `cargo build --no-default-features`
  successfully. The link observed approximately 470 MiB RSS at peak in the
  development process; this is not a full application memory qualification.
- `npm run tauri dev` launched the actual Tauri window titled `Consensus Arena`.
- The current home screen rendered. A real click on `New session` rendered the
  current `New session` setup view, and a real click on `Build` rendered the
  Build setup UI with `Desired outcome`, `Project folder`, and `Choose folder`.
- No repository was selected and no Build/Delivery run was started.

**GUI qualification:** native launch and Build UI rendering are **runtime
proven** in this environment. Model-backed Delivery is not.

## Genuine Arena Delivery E2E

**Not proven.** The required chain

`UI → worktree → acceptance → implementation → verification → Verified → Apply`

was not started because the provider/DSH gate failed. No delivery/session
identity, starting HEAD, worktree, acceptance freeze, protected hashes,
verification profile, candidate SHA, receipt verdict, Verified state, or Apply
result was generated in this session.

## Runtime branch evidence

- **PASS:** not run.
- **FAIL → repair → PASS:** not run; no candidate reached independent
  verification.
- **INCONCLUSIVE:** not run through the product UI.
- **WaitingForUser:** not run.
- **Restart recovery:** not run.
- **Abort:** not run through a Delivery task.
- **Apply refusal:** not run through a product Delivery.

## Defects and changes

No concrete product defect was found or changed in this session. No durable
model default, DSH dependency, or Arena source was modified. The only
repository changes are this audit and current-status corrections in
`README.md` and `DELIVERY.md`.

The prior scoped `maxTokens: 4096` DSH patch remains in source; this session
did not change or requalify it.

## Security and temporary artifacts

- The credential variable was reported only as set/not set. Its value was not
  printed, serialized, logged, or written to the repository or audit.
- Direct request failures were reported by HTTP status/category only; provider
  error bodies were not captured in repository evidence.
- Diagnostic screenshots were stored temporarily outside the repository and
  remain outside Git after inspection. No DSH home, fixture repository, cache,
  or build output was added to Git.

## Delivery invariant review

| Invariant | Status | Evidence |
|---|---|---|
| CLEANBASE | source-confirmed | no product Delivery admission this session |
| WORKTREE | source-confirmed | no product Delivery admission this session |
| ACCEPTANCEFREEZE | source-confirmed | no worker run this session |
| PROTECTEDACCEPTANCE | source-confirmed | no worker run this session |
| INDEPENDENTVERIFY | source + targeted tests | no candidate this session |
| SAMECHECK | source-confirmed | no repair this session |
| BOUNDEDREPAIR | source + targeted tests | no repair this session |
| DURABLEQUESTION | source-confirmed | no owner question this session |
| ASKDISMISS | source-confirmed | no modal task this session |
| EXACTABORT | source-confirmed | no active Delivery task this session |
| NOSECRETS | runtime diagnostic review | credential value absent from captured output/evidence |
| SAFEAPPLY | source + prior fixture | no product Apply this session |
| CROSSPLATFORM | source-confirmed | no shared Delivery change this session |

## Verification performed

- Direct Nemotron and Kimi provider probes: completed with the statuses and
  response shapes above; no native tool-loop pass.
- `cargo build --no-default-features`: passed; warnings only.
- Native `tauri dev` launch and Build UI screenshot: passed for GUI rendering.
- DSH coding smoke: intentionally not run because its prerequisite failed.

The final repository baseline and diff checks passed after this audit's
documentation changes.

## Still unproven

- A genuinely native-tool-calling NVIDIA-hosted endpoint/model suitable for
  the DSH OpenAI-completions adapter.
- A complete direct tool loop.
- Standalone DSH coding with a real repository change and valid worker result.
- Arena worker-result parser compatibility in a live worker run.
- Full Arena Delivery E2E, including acceptance authoring/freeze, verification,
  Verified, Apply, and all requested reliability branches.
- Native Windows build/runtime parity and complete memory qualification.

## Next gate

Do not broaden model research. The next substantial qualification should be a
narrow, evidence-led provider/DSH distribution gate using a currently
available, NVIDIA-documented native tool-calling route, followed by the same
standalone DSH contract. Until that passes, Dagu remains out of scope and the
Arena Delivery baseline remains source/test-confirmed rather than
model-backed runtime-proven.
