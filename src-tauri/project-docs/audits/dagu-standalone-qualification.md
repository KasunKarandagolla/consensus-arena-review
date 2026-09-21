# Dagu standalone falsification qualification — 2026-09-16

**Scope:** Disposable local Dagu experiment only. No Arena integration,
production-state migration, or frontend change was performed.

**Verdict: INCONCLUSIVE.** Several important durable mechanics passed, but the
full falsification gate did not: automatic reconciliation after a hard
interruption during an active run and successful Dagu→DSH composition remain
unproven. Do not describe this as a Dagu qualification for Arena or begin
integration from this evidence alone.

## Environment and carrying cost

- Dagu `2.16.6`, official Linux amd64 distribution; downloaded archive checksum
  was checked against the published checksum, then run locally.
- Linux server RSS measured approximately **80–109 MiB** across observed states.
- Disposable run/state fixture was approximately **1.1 MiB**.
- Linux binary was approximately **161 MiB**.
- Official Windows archive checksum and PE32+ artifact type were inspected;
  no Windows execution was available.
- Upstream source is GPL-3.0-or-later. Upstream distinguishes a separately
  executed CLI/server from importing/linking its experimental embedded Go API;
  distribution and legal obligations need review before Arena bundles or
  embeds it. This audit is not a legal opinion.

## Gate results

| Area | Result |
|---|---|
| Local install, start, CLI and REST control | Exercised |
| Dependency scheduling | Passed in disposable DAGs |
| Retry and durable history visibility | Passed; retry visibility was observed in run history |
| Root human task before protected action | Exercised; no command-attached approval was used |
| Workflow attempt release while waiting | Passed; root run entered Waiting and relinquished its execution attempt |
| Controller termination/restart while waiting | Passed; pending root task remained discoverable after restart |
| Identical duplicate answer | Passed; idempotent “already completed” response |
| Conflicting answer | Passed; rejected with HTTP 409 |
| Resume after answer | Passed; same root run continued through a fresh bounded continuation |
| Linked continuation for a second blocker | Passed using a new root continuation |
| Root-task limits | Confirmed: human tasks are root-only and cannot use step retry/timeout or root retry policy as if they were ordinary executable steps |
| Hard interruption during an active attempt | **Not proven**; explicit CLI reconciliation/cleanup was needed after the injected interruption |
| Dagu + DSH/Muse composition | **Not proven**; two attempts stopped before a worker result because the disposable composition lacked the required worker credential/runtime boundary. No result, deterministic verifier outcome, human continuation, or resumed worker was fabricated. |
| Native Windows execution | **Not run**; artifact inspection only |

The standalone exercise is consistent with upstream human-task documentation:
completion persists input before enqueueing a retry, identical canonical input
is idempotent, conflicting input is rejected, and human tasks are root-DAG-only
and processless. See [Dagu Human Tasks](https://docs.dagu.sh/writing-workflows/human-tasks),
[Dagu installation](https://docs.dagu.sh/getting-started/installation/),
[Dagu durable execution](https://docs.dagu.sh/writing-workflows/durable-execution),
and [upstream licensing](https://github.com/dagucloud/dagu/blob/main/LICENSING.md).

## Decision boundary

Dagu visibly supplies retry/history/control and human-task scheduling, but the
experiment did not show that its carrying cost removes more Arena-specific
control than it adds. In particular, Arena still needs to own product meaning,
acceptance authority, receipt correlation, owner-facing semantics, and safe
worker/verifier decisions. No Dagu code was added to the product.

No Astra consultation was initiated: the overall gate did not strongly qualify
Dagu as an Arena candidate. A future packet should be considered only after
the exact DSH runtime is reproducible and the missing composition and active-
run recovery tests complete.
