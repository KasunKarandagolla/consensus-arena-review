# Codex Multi-Agent Startup Gate — Consensus Arena

Run this **before M09C edits in a new Codex session**.

## Preconditions

1. Repository is trusted so project-local `.codex` configuration is enabled.
2. `.codex/config.toml` exists at the repository root.
3. `.codex/agents/*.toml` exists.
4. Start a NEW Codex session after these files are installed.

## Mandatory spawn test

Before reading the long M09C implementation prompt, explicitly spawn these named agents in parallel:

- `source_scout`
- `verification_planner`
- `docs_scout`

Give each a tiny read-only task.

Wait for all three.

Required markers:

- `SOURCE_SCOUT_CONFIG_LOADED`
- `VERIFICATION_PLANNER_CONFIG_LOADED`
- `DOCS_SCOUT_CONFIG_LOADED`

Then spawn `pipeline_reviewer` with a tiny read-only smoke task and require:

- `PIPELINE_REVIEWER_CONFIG_LOADED`

## Fail-closed rule

If any named agent:
- cannot be found;
- cannot be spawned;
- silently falls back to the parent;
- does not return its configuration marker;
- is denied by the current permission/trust mode;

STOP BEFORE EDITING SOURCE.

Report:
- Codex version;
- whether project-local config is trusted/loaded;
- which named agent failed;
- exact spawn/config error;
- current parent model;
- whether multi-agent tools are available.

Do NOT continue M09C solo.

## Runtime policy after smoke passes

Parent/lead:
- Terra high reasoning.

Default bounded workers:
- Luna medium.

Use Luna for:
- source maps;
- test/fixture creation;
- routine implementation;
- docs drift;
- verification planning.

Keep Terra parent responsible for:
- architecture/composition decisions;
- shared-state integration;
- authority changes;
- final diff synthesis.

Spawn `pipeline_reviewer` (Terra) only once near a coherent checkpoint/session boundary, not after every edit.

## Heavy-operation discipline

Subagents may think in parallel.

Only one parent-designated Verification Coordinator may launch heavy local operations:
- heavyweight Rust test linking;
- live OpenCode/provider runs;
- browser family;
- full frontend build;
- milestone cargo check.

This prevents subagents from duplicating expensive verification.
