# Milestone 03 — Research, Reuse, Skills, and Shared Consultation

Date: 2026-09-17
Host: Linux development machine
Scope: bounded research/reuse evidence and an Arena-owned consultation contract

This audit follows the Milestone 02 authority-adapter checkpoint. It does not
upgrade external consultation material into Arena source-of-truth, and it
does not broaden the OpenCode qualification beyond the exact Linux Zen/Muse
Spark path already recorded in the Milestone 02 audits.

## Preflight closure before expansion

The four requested source-review findings were closed before the Milestone 03
implementation:

1. `start_delivery` now always reads configured securely stored credentials
   through the existing `configured_credentials` path and redacts the
   objective before it enters Delivery state. OpenCode still skips only the
   legacy Agent Brain configuration requirement. The regression test
   `opencode_objective_redacts_configured_secret_before_worker_invocation`
   uses a generated synthetic value, proves it is removed, and persists no
   credential value.
2. `execute_candidate` now inspects candidate/canonical protected state and
   persists sanitized failure evidence before it stages or creates an
   Arena-created candidate commit. Invalid candidate changes are reset and
   discarded. The real Muse attack exercise passed after this change and
   asserted the rejected candidate remained at its base HEAD; focused tests
   separately prove a verifier PASS cannot rescue an invalid work order.
3. Current wording calls the worktree a non-authoritative candidate working
   directory. It does not call it an OS/security sandbox and explicitly leaves
   arbitrary same-user filesystem access and shared Git object-store isolation
   as unproven.
4. External-provider portability was attempted once per available owner-
   relevant path without printing credentials: Gemini `google/gemini-2.5-flash`
   was unavailable to new users, Gemini `google/gemini-3.6-flash` timed out,
   and DeepSeek `deepseek/deepseek-flash` timed out after 59.50 seconds with
   no session/tool result or fixture mutation. Provider neutrality therefore
   remains unproven; no NVIDIA key was requested or used.

## Research evidence register

| Claim | Source / scope | Origin | Status | Decision impact / revisit |
|---|---|---|---|---|
| GitHub maintains an official MCP server with read-only mode, toolset allow-lists, and lockdown mode | [github/github-mcp-server](https://github.com/github/github-mcp-server), checked 2026-09-17 | Official GitHub repository | SOURCE-CONFIRMED | Adapt as a future read-only reconnaissance adapter; never treat MCP or lockdown as Arena authority. Revisit when a bounded GitHub task is needed. |
| OpenCode has provider, MCP, skills, agents, and permissions surfaces | [OpenCode documentation](https://opencode.ai/docs/), version-sensitive V1/V2 material | Official OpenCode docs/source | SOURCE-CONFIRMED; current Arena runtime proof remains narrow | Reuse through the existing adapter; pin configuration to OpenCode 1.17.18 until a version change is qualified. |
| OpenCode permissions are tool policy, not an OS/security sandbox | [OpenCode skill/permission source](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/plugin/skill/customize-opencode.md) | Official upstream source | SOURCE-CONFIRMED | Preserve Arena process containment, candidate boundary, protected checks, verifier, and Safe Apply. |
| ECC and gstack are suitable Arena assets | Prompt/source-register proposals only; no pinned local checkout found | External consultation input | STILL UNPROVEN / REJECT for this milestone | Do not install blindly. Revisit only with an exact pinned revision, license, hooks, and side-effect review. |
| Current Tauri/WebKit two-WebView transport should be replaced now | Local Consult source and current docs; no runtime replacement spike | Arena source | NOT ESTABLISHED | REUSE current transport. Playwright is an ADAPT candidate for bounded future QA, not a Consult migration. |
| OpenCode is provider-neutral in the current Arena adapter | M01/M02 runtime audits plus bounded external-provider attempts | Arena runtime | STILL UNPROVEN | Qualification remains Linux OpenCode Zen/Muse Spark only. A successful owner-relevant external task is required before widening the claim. |

The single web-search path used for this milestone was the web search tool,
restricted to primary GitHub/upstream sources for decision-critical claims.
No GitHub MCP server was installed or granted repository credentials.

## Reuse decisions

- **REUSE:** existing Hackathon OpenAI-compatible chat transport for the
  consultation seam; existing Arena credential storage; existing
  `SessionRuntime` and Delivery authority concepts.
- **ADAPT:** official GitHub MCP as a future read-only, minimally tool-profiled
  reconnaissance source; Playwright for bounded future browser QA spikes.
- **REJECT for now:** unpinned ECC/gstack assets, consumer consultation as a
  critical-path research transport, a GitHub crawler, a new MCP framework, a
  browser framework migration, or a generic consultation scheduler.

## Shared consultation seam

`src-tauri/src/consultation.rs` defines one Arena-owned request/result contract
for callers intended to be authorized non-Consult work. The request includes request/work-order
identity, origin, question, curated evidence, disclosure scope, selected
provider configuration ID, allowed transport, deadline, and token budget. The
result retains request/work-order correlation, provider/transport, answer,
curated source references, timestamp, status, and sanitized error.

The operation reuses `hackathon::call_hackathon_model`; it does not copy API
keys into the request or result, and it does not grant the provider authority
over product intent, acceptance, verification, candidate identity, or Apply.
Disclosure scope is validated before provider invocation, and known stored
credentials are redacted from the prompt and returned answer. The contract is
callable by Delivery research, engineering, reviewer, or Consult origins; it
does not synthesize a top-level Consult session. Its validated
`budget_tokens` is passed to a consultation-specific transport variant as the
provider `max_tokens` limit; unrelated Hackathon callers retain the historical
1024-token default.

The seam is contract/runtime-source proven, not provider-runtime proven: no
configured Hackathon model was invoked during this milestone. It is not yet a
durable work-order runtime integration: `execute` does not itself bind a
request to `SessionRuntime`, persist/reconcile results, enforce cancellation or
supersession, or carry candidate/version/verification identity. A future caller
must perform those Arena-owned admission and lifecycle checks before invoking
it. The existing browser participant path remains unchanged and is not used as
a hidden critical path.

## Verification

Source/unit evidence completed:

- `cargo test commands::tests::opencode_objective_redacts_configured_secret_before_worker_invocation -- --nocapture` — passed;
- `cargo test opencode_adapter::tests -- --nocapture` — 4 passed, 1 ignored;
- real Muse `real_muse_authority_boundary_and_walking_skeleton` — passed after
  the pre-commit rejection change, with the attack candidate left at base
  HEAD and the legitimate candidate independently verified;
- `cargo test consultation::tests -- --nocapture` — passed (5 tests);
- parallel read-only source/reuse/IPC reviews completed; no subagent edits
  were accepted;
- no credential values were added to source, prompts, tests, evidence, or
  documentation.

Full milestone verification still includes the final Rust check, frontend
build where affected, diff/secret scans, and working-tree review before the
checkpoint.

## Still unproven

- successful execution of the new consultation seam against a configured
  external model;
- Arena-side admission, cancellation/supersession, durable result persistence,
  restart reconciliation, and candidate/version/verification binding for the
  new seam;
- provider-neutral OpenCode execution beyond the tested Zen/Muse Spark path;
- Windows consultation/worker execution;
- packaged build and native packaged OpenCode execution;
- production-scale consultation concurrency for this new seam;
- official GitHub MCP runtime qualification inside Arena;
- pinned ECC/gstack reuse and license/hook qualification;
- browser transport replacement; current Tauri/WebKit remains the active
  Consult implementation.

## Handoff

At audit creation, the working tree contains the pre-existing historical-audit
deletions and untracked prompt/consultation trees, which remain outside this
milestone's reviewed change. The next prompt is:

`src-tauri/project-docs/prompts/04-evidence-gates-architecture-team.md`

Read it after this checkpoint; do not infer its scope from this audit.
