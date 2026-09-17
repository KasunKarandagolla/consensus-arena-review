# Milestone 01 — Runtime + Provider Foundation

**Date:** 2026-09-17
**Branch:** `codex/arena-dev-temp`
**Starting HEAD:** `d364bdf800e5c6d2936efdbc4c817667052425a8`

## Verdict

**CONDITIONAL / NOT QUALIFIED as an Arena worker.** OpenCode is a credible
provider-neutral team-runtime candidate and its Linux control plane passed a
small disposable health probe. The executable hard gate was not passed, so no
production adapter, Product OS schema, frontend change, or DSH replacement was
implemented.

## Environment and exact artifact

- OS: Linux Lite 6.6, Linux 5.15.0-177-generic, x86_64.
- Node: `v22.22.2`; Cargo: `1.95.0`.
- Local executable: `/home/kasun/.opencode/bin/opencode`.
- Local executable SHA256:
  `0cbfb6de55aa4ce3c74da12d8516376033693a88abca6238c5be32bf98130636`.
- Runtime-reported version: `1.17.18`.
- Installed local SDK/plugin metadata: `@opencode-ai/sdk` and
  `@opencode-ai/plugin` `1.17.15`.
- Provider/model/protocol executed: none. No model credential was used and no
  provider request was made. The configured local Omniroute base URL was not
  running during the probe.

The public OpenCode release/source channels were inconsistent during review:
the official documentation exposed the current server/provider/agent/MCP/
skills/permission surfaces, while upstream package/release material referenced
newer `1.18.x` artifacts. Qualification must pin the exact binary actually
used rather than infer a version from documentation or a plugin package.

## Evidence classification

### SOURCE-CONFIRMED / DOCUMENTED

Official OpenCode documentation and source describe:

- `opencode serve`, loopback binding, OpenAPI `/doc`, health/version,
  sessions, async prompts, SSE events, session abort, and instance disposal:
  <https://opencode.ai/docs/server/>.
- Provider configuration through native providers and custom
  OpenAI-compatible endpoints, including documented DeepSeek, NVIDIA, Z.AI,
  and custom-provider paths:
  <https://opencode.ai/docs/providers/>.
- Primary/subagent roles, per-agent model/permission configuration, and child
  delegation:
  <https://opencode.ai/docs/agents/>.
- MCP server configuration, on-demand skills, and V1 permission controls:
  <https://opencode.ai/docs/mcp-servers/>,
  <https://opencode.ai/docs/skills/>,
  <https://opencode.ai/docs/permissions/>.
- The candidate provider families and protocol caveats were recorded by the
  provider scout; this is compatibility documentation, not Arena runtime
  proof.

### RUNTIME-PROVEN in this milestone

- `opencode --version` eventually returned `1.17.18` within a 45-second
  bounded probe.
- A disposable Git repository was created under `/tmp` and OpenCode was
  launched with that repository as its current directory.
- `opencode serve --hostname 127.0.0.1 --port 4198` answered:
  `{"healthy":true,"version":"1.17.18"}`.
- The server printed its loopback listener banner and was terminated through
  the explicit probe process. No OpenCode probe process remained afterward.
- A prior bounded server observation recorded approximately 315,308 KiB RSS
  for the OpenCode server while it was starting. This is a single Linux
  observation, not a concurrency or target-machine budget qualification.

### STILL UNPROVEN / FAILED TO QUALIFY

- model execution and at least one non-OpenAI provider;
- file read/write, shell/tool call, real repository edit, and deterministic
  test through OpenCode;
- two logical runtime roles performing tasks;
- child/subagent delegation and concurrent fan-out;
- MCP capability execution and skill loading/execution;
- session message lifecycle, SSE event correlation, resume/reconciliation;
- effective cancellation plus descendant process cleanup;
- OpenCode protected/out-of-scope write behavior;
- native Windows launch, paths, process cleanup, and packaging;
- memory/resource behavior across planned role concurrency;
- typed mapping from runtime completion to an Arena-neutral execution result.

The local provider credential store was not used by these tests. The existing
local OpenCode state is not a qualification fixture: its database is roughly
3.8 GB with a roughly 23 MB log. Future probes must use isolated XDG config,
data, state, and cache directories.

## Security and authority boundary

Arena's existing seams remain the required boundary:

- `SessionRuntime` owns admission, owner identity, and cancellation.
- `dsh_worker.rs` demonstrates the reusable containment seam: explicit argv,
  sanitized environment, bounded output, process groups on Unix, and Job
  Objects on Windows.
- Delivery owns clean-base admission, isolated worktrees, frozen/protected
  acceptance, independent verification, bounded repair, and explicit Apply.

OpenCode permission rules (`allow`/`ask`/`deny`) are tool policy, not an OS
sandbox. Process groups/Job Objects are cleanup boundaries, not hostile-code
filesystem or network isolation. No credential may be placed in argv, prompts,
worktrees, logs, or durable evidence. OpenCode must not be exposed directly to
the renderer and runtime completion must never imply `Verified`.

## Parallel scout/reviewer record

Read-only scouts A–F were launched concurrently. They agreed that OpenCode is
documented/source-confirmed but not Arena-qualified, with the primary risks
being provider execution, startup/resource cost, MCP/skills proof, cancellation
and process-tree cleanup, native Windows, packaging, and ambient config/state.

The requested independent reviewer launch was attempted after A–F, but the
agent-thread limit rejected the launch. The primary agent performed the
independent challenge review and reached the same conditional verdict; no
claim of a separate reviewer execution is made.

## Fallback and next qualification

No fallback runtime was evaluated. The milestone did not encounter a proven
intrinsic OpenCode protocol failure; it encountered missing provider execution
and incomplete safety/platform evidence. Do not broaden framework research or
start DSH recovery.

Before a production adapter is considered, run a bounded qualification with:

1. an exact pinned OpenCode artifact and isolated XDG directories;
2. one permitted provider/model, without recording credentials;
3. a disposable repository whose parent path contains spaces;
4. real read/write/shell/edit/test evidence and two role tasks;
5. one child/subagent delegation;
6. one deterministic local stdio MCP sentinel and one project-local skill;
7. protected-acceptance and out-of-scope-write attempts;
8. exact-owner abort, timeout, descendant cleanup, and instance disposal;
9. native Windows launch/path/package/process evidence;
10. bounded resource measurements at planned concurrency.

The fallback, if OpenCode fails an intrinsic requirement, is one bounded
comparison against that failed requirement only. Codex remains optional and is
eligible only for a Responses-compatible provider; it is not a replacement
decision in this audit.

## Verification and handoff

- Local runtime probes were bounded and all disposable OpenCode processes were
  terminated after the checks.
- No repository source files, dependencies, frontend files, or production
  runtime code were changed.
- Historical audit deletions and pre-existing consultation/prompt files were
  preserved.
- Handoff source HEAD before this checkpoint was
  `d364bdf800e5c6d2936efdbc4c817667052425a8`; the final checkpoint SHA is
  reported in the milestone handoff because a commit cannot embed its own
  final object ID.
- Next prompt: `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/prompts/02-product-os-authority-walking-skeleton.md`.
