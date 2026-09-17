# Dagu standalone runtime closure — 2026-09-16

**Verdict: INCONCLUSIVE — keep Dagu post-V1.** The standalone Linux mechanics
are useful, but automatic recovery after a hard interruption is incomplete,
DSH composition is unproven, and the Windows workflow has not run. Arena was
not integrated into the experiment.

## Linux runtime evidence

- Dagu `2.16.6`, official Linux amd64 archive SHA256
  `06c3ed951fb58408313b1db25bc9f90ff2f427cbdbe68aaff55cd5465c167717`.
- During a running-step test, the process tree was inspected: Dagu PID 24448,
  shell PID 24460, watcher PID 24461, and `sleep 90` PID 24463. Dagu was
  SIGKILLed at about 22:49:59. The shell and sleep disappeared, but the
  persisted Dagu attempt remained `Running` and immediate retry failed as
  already running.
- Restarting Dagu `start-all` reconciled the stale attempt to `Failed` at
  about 22:51:43. Same-run retry then succeeded. External side-effect trace:
  `started`, `started`, `completed`; the interrupted step began twice.
- A separate root human task persisted `Waiting` through controller death and
  restart. Completing it caused `dag-run is not queued: waiting`; an explicit
  same-run `dagu retry` resumed only the pending downstream step and
  succeeded. The completed pre-wait step did not repeat.
- These runs show stale active-run reconciliation and explicit recovery, with
  at-least-once side-effect risk. They do not show automatic human-task resume
  after restart.
- Earlier standalone gates remain supported: durable root task, waiting
  attempt release, duplicate identical-answer idempotency, conflicting-answer
  rejection, history, retry, and linked continuation.
- Dagu→DSH→verifier→human task→fresh continuation was not completed. The
  bounded attempts stopped before a worker result because the disposable
  composition lacked the required worker runtime/credential boundary. No
  Arena integration or model-backed result is claimed.

Runtime fixtures and logs are under
`/tmp/arena-dagu-hard-interruption.UVQyWT` on the qualification host.

## Windows gate

An independent workflow and PowerShell harness were added at
`.github/workflows/dagu-windows-qualification.yml` and
`scripts/qualify-dagu-windows.ps1`. The harness pins Dagu `2.16.6`, verifies
the official Windows amd64 archive SHA256
`65193670d974ece9e14b2fd9c61a06dd3073f0b03a553445623ca03f162be7b8`, and is
designed to run a basic workflow from a path with spaces, a root human task,
controller restart, history, and continuation. YAML parsing passed. Workflow
run [35198945289](https://github.com/KasunKarandagolla/consensus-arena-review/actions/runs/35198945289)
then passed on Windows Server 2025 with Dagu `2.16.6` and the pinned archive
hash. The fixture path contained spaces; the root human task survived
controller restart, required the explicit retry recovery path, and the
continuation completed. The run did not use Arena integration or DSH.

## Recommendation

Do not integrate Dagu or spend further V1 release time on it. Revisit only if
the worker runtime is qualified and Dagu's measured carrying-cost reduction
justifies resolving automatic waiting-task resume, duplicate side effects,
Windows execution, and worker composition. This result does not trigger
Astra consultation.
