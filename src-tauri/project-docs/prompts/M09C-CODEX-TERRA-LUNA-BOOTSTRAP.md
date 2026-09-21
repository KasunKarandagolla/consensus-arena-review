# M09C Codex Bootstrap — Terra Lead + Luna Workforce

Prepend this to the existing M09C mandatory preflight and M09C session prompt.

## Mandatory first action

Run `CODEX-MULTIAGENT-STARTUP-GATE.md`.

Do not edit source until the named-agent smoke test passes.

## Delegation contract

The Terra parent must explicitly delegate bounded work. Do not merely say "use subagents."

At the start of each major investigation wave:

1. spawn `source_scout` agents for distinct non-overlapping call paths;
2. spawn one `verification_planner`;
3. wait for their results;
4. Terra synthesizes the integration plan;
5. assign narrow edits to `implementation_worker` / `fixture_worker`;
6. keep shared-state integration with the Terra parent;
7. near checkpoint exit, spawn exactly one `pipeline_reviewer`;
8. Terra resolves reviewer findings before commit/push.

## Cost rule

Do not spawn `pipeline_reviewer` repeatedly.

Do not use Terra subagents for:
- grep/call-site discovery;
- straightforward fixtures;
- docs mapping;
- routine bounded code edits.

Those are Luna tasks.

## Failure rule

If multi-agent spawning stops working mid-session:
- do not silently collapse the remaining planned parallel audit into the parent;
- finish only the currently safe bounded edit if necessary;
- report the configuration/runtime failure;
- do not claim the planned multi-agent independent review happened.

## Verification economy

Retain the existing M09 Verification Economy protocol.

Subagents do not independently launch full regression or live-provider tests.
