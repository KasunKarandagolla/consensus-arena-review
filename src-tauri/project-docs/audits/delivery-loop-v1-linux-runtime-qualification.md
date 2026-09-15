# Delivery Loop V1 — Linux Runtime Qualification Audit

**Date:** 2026-09-15
**Starting checkpoint:** `19a264a6996bee619407784ade4e523c0284e759`
**Branch:** `codex/arena-dev-temp`
**Result:** Authentication was qualified at the configured HTTP endpoint, but
the external DSH worker and native GUI Delivery loop were not qualified.

This is a permanent record of the qualification attempt. It does not replace
the earlier implementation and Linux E2E audits.

## Environment

- Linux Lite development machine, approximately 4 GiB RAM with swap.
- Repository and source were kept at the starting checkpoint plus the scoped
  DSH worker patch documented below.
- Disposable DSH fixture was placed outside the repository under `/tmp` and
  was not added to Git.
- The current native-launch environment exposed `DISPLAY=:0.0`, but the X
  server was unavailable. `xwininfo -root` failed to open that display and
  the native binary failed during GTK initialization.

## DSH candidate/version

- Candidate: `@deepseek-ai/dsh@0.1.5-rc.1`.
- Exact executable version was confirmed as `0.1.5-rc.1`.
- `--profile headless --help` succeeded.
- The current Arena invocation shape, `--profile headless --patch
  <patch-file> <prompt>`, was accepted by the CLI.
- DSH home was isolated in a disposable directory. The fixture was a clean
  Git repository with initial commit
  `4f42a504b9aa3c84f1a75966fc52408e0fac572b`.

## Authentication qualification

The current Arena settings were inspected using allowlisted metadata only.
The configured endpoint was `https://integrate.api.nvidia.com/v1` and the
current primary model was `poolside/laguna-xs-2.1`. The credential was present
without its value being printed. A small direct chat-completions request using
that endpoint, model, and credential returned HTTP 200 and a known response.

This distinguishes the previous HTTP 401 from a missing environment handoff
in the current setup: the current stored credential/model pair is accepted by
the endpoint. The old Nemotron model was not used as the current Arena model
for the qualification decision.

Source inspection also confirmed that Arena writes the credential reference as
`apiKeyEnv: ARENA_DSH_API_KEY` and passes the credential to the child process
under that exact name. DSH's configuration contract resolves arbitrary
`apiKeyEnv` names from the inherited launch environment. `DSH_HOME` is passed
only when `ARENA_DSH_HOME` is configured.

## Standalone DSH evidence

The independent DSH runs used the disposable fixture as the process cwd.
They did not complete the required coding contract:

- Current model with the original uncapped profile: exit 1 after roughly 18
  seconds; DSH reported `ResourceExhausted` with a local request-limit
  diagnostic; no fixture code change and no result receipt were produced.
- Current model with the explicitly capped `maxTokens: 4096` profile: the
  same disposable coding task still failed with the provider/tool interaction
  exhausting the local request budget; no fixture code change and no result
  receipt were produced.
- A simple no-tool prompt succeeded with a smaller cap, but it was not run in
  the disposable fixture and therefore does not prove coding, cwd, or Arena's
  result contract.
- Additional diagnostic model probes were not qualification passes: Nemotron
  returned a temporary service-overload error through DSH, while a GPT-OSS
  route returned malformed remaining message-header tokens. A direct
  provider-shaped request for the current model returned tool-call markup as
  ordinary content rather than a native tool call. These results are evidence
  of provider/DSH tool-loop incompatibility or service behavior, not evidence
  that Arena's verifier is wrong.

The required standalone contract is therefore **not proven**: no genuine
coding change, `.arena-runtime/result.json`, parsed result, or successful
cleanup-after-success was observed. Captured DSH stdout, stderr, and result
paths were checked for credential leakage; no credential value appeared.

## Native Arena GUI evidence

The actual debug binary was launched from the native project build. It did not
reach the window because GTK failed to initialize against the unavailable X
display. The prior permanent audit's successful native launch and Build UI
render remain valid historical evidence, but this session did not reproduce
them and did not click through the Build UI.

## Full Delivery E2E evidence

The chain

`UI → worktree → acceptance → implementation → verifier → Verified → Apply`

was **not proven** in this session. No major product segment was manually
bypassed or represented as successful. Consequently there are no new runtime
identities to report for delivery/session ID, acceptance freeze SHA, protected
hashes, candidate SHA, verification receipt, Verified state, or Apply result.

## FAIL/repair evidence

Not run. The worker did not produce a candidate in the standalone fixture, so
there was no legitimate behavioral failure to submit to Arena's verifier and
no repair attempt to measure. Existing source/tests and earlier audits still
cover the bounded-repair design; this session adds no runtime proof.

## INCONCLUSIVE evidence

Not run through the product UI. The standalone DSH failures were classified as
worker qualification failures, not converted into a product verification
receipt. No candidate was marked Verified, and no product-code repair budget
was consumed by these probes.

## WaitingForUser evidence

Not run. No worker reached a real `needs_user` result in the disposable
fixture, so there is no new proof of question persistence, answer delivery,
dismissal, or continuation.

## Restart recovery evidence

Not run. No product Delivery reached a persisted `WaitingForUser` state in
this session.

## Abort evidence

Not run through the native UI. The standalone DSH processes exited or were
bounded by the qualification harness; this does not prove Arena's exact-owner
abort semantics or stale-owner isolation.

## Apply safety evidence

Not run in a product Delivery. Dirty-original, changed-HEAD, and
non-fast-forward refusal behavior remain source/test-confirmed and covered by
prior audits, but were not exercised from this native GUI session.

## Resource observations

- Observed DSH process RSS was approximately 168–175 MiB across the failed
  standalone probes.
- Failed-run durations were approximately 17–30 seconds.
- A whole child-process-tree measurement was not captured.
- These are a few development-environment observations, not a memory budget
  qualification or a forecast for a successful coding run.

## Defects found/fixed

Arena's generated DSH provider patch previously omitted an explicit model
output cap, leaving DSH's larger default budget in effect. The scoped source
repair adds `maxTokens: 4096` and a regression test for the generated patch.
The cap is supported by the installed DSH profile schema, but it did not by
itself make the current disposable coding task pass.

The previous 401 was not reproduced with the current stored Arena credential
and current model at the direct endpoint. The evidence points downstream to
DSH/provider request-budget and tool-protocol behavior. No unsupported DSH
upgrade or Dagu integration was attempted.

## Source-confirmed but runtime-unproven

- Clean-base admission, worktree isolation, acceptance freeze, protected
  acceptance hashes, independent verification, deterministic outcome
  precedence, bounded repair, durable questions, exact abort ownership, and
  safe Apply remain source/test-confirmed by the current code and prior
  permanent audits.
- Arena's child-process cwd, explicit argv, inherited credential-reference
  name, output bounds, timeout, and `kill_on_drop` behavior are source-confirmed.
- The new generated profile output cap is compile-checked; its targeted test
  harness run timed out during the resource-heavy full binary test build on
  this machine.

## Still unproven

- Successful standalone DSH coding with a real change and valid result
  contract.
- Full native Linux GUI Delivery, including Apply.
- FAIL → repair → PASS, real INCONCLUSIVE, WaitingForUser, restart recovery,
  abort, and Apply refusal branches from the product UI.
- Native Windows runtime parity.
- DSH packaging/install strategy.

## Windows status

No Windows evidence was collected. Linux results do not establish Windows
support.

## Security review

No credential value was added to source, prompts, state, logs, fixtures,
audit files, or Git. One exploratory local diagnostic used an insufficient
redaction filter and exposed a stored credential in tool output; it was not
written to the repository, an evidence file, or Git history, and the value is
not reproduced here. Future diagnostics must use allowlisted metadata only.

## Dagu status and carrying-cost observations

Dagu was not integrated or prototyped. The current implementation's carrying
costs that a later Dagu gate should measure against are durable Delivery state
transitions, bounded retry history, persisted owner-question coordination,
exact child-process ownership/cleanup, and recovery without requiring a live
worker process. Those are the real baseline; this audit does not recommend a
Dagu change before the external worker and native product loop are qualified.
