# M05D — Durable web discovery qualification

Date: 2026-09-19
Predecessor: 80b5e90dab350680fb422e2d8035aa070d27ef50
Status: **READY FOR M06**

## Decision

The owner-authorized OpenCode runtime upgrade closed the narrow M05D runtime
blocker. OpenCode `1.18.31`, using the existing OpenCode Zen Muse Spark free
model and its hosted Exa web-search path, executed real bounded web research
on Linux. Arena now admits that output only through an Arena-owned WebDiscovery
work order, stores source-backed proposals as `Unverified`, and requires a
distinct current FactVerifier work order for final disposition.

This qualifies bounded Linux web research behind Arena authority. It does not
qualify OpenCode generally, Windows, packaging, unrestricted web fetching, or
the final founder/release dogfood.

## DOCUMENTED

- OpenCode websearch and webfetch are separate capabilities. The websearch
  permission is the relevant policy action for the integrated research role.
- Search results are untrusted research input. A search result or researcher
  response is not an independently verified fact.
- OpenCode permission policy is not an OS or same-user filesystem sandbox.

## SOURCE-CONFIRMED

- Arena's existing known-source research path remains narrow: it accepts a
  known, query-free official GitHub API locator and is not an arbitrary URL
  fetcher.
- Existing Product OS authority admits unverified proposals, binds finalization
  to a current independent verifier work order, persists reopenable state, and
  derives gate/package inputs from Arena records.
- The adapter continues to use `SessionRuntime`, contained process execution,
  bounded result parsing, and semantic Tauri commands. No crawler, search
  engine, browser framework, or second workflow authority was added.

## RUNTIME-PROVEN

### Owner-authorized runtime upgrade

Pre-upgrade executable path:

    /home/kasun/.opencode/bin/opencode

The existing installation was identified as the standalone curl installation;
the existing config and session locations were preserved. The configured model
identifier was `opencode/muse-spark-1.2-contributor-free`. No provider or
search credential was printed or added.

The supported pinned command was:

    opencode upgrade v1.18.31

OpenCode reported that it used the curl method and upgraded from `1.17.18` to
`1.18.31`. The executable path remained unchanged, and `opencode --version`
reported `1.18.31` afterward. A real bounded Muse inference succeeded with
session `ses_f498bbdf6ffe2mBVnZNLuAcOmW` and the existing configuration.

### Web capability

A disposable-directory probe made a real model-backed `websearch` call. The
runtime reported the search provider/path as `exa`; no separate search API key
was requested. The result included the public source URL
`https://opencode.ai/v2/docs/websearch/` and a source title/claim. A separate
probe also made a real `webfetch` call for that URL.

The integrated Arena path deliberately uses `websearch` only. It does not
generalize Arena into an arbitrary URL fetcher. Source URLs are normalized and
bounded before admission, while source claims remain unverified until the
independent verifier path completes.

### Research-role permission boundary

The built-in OpenCode `plan` role was tested with the existing global
configuration. It performed the real websearch call and denied a harmless
local file mutation attempt; the attempted file was absent afterward. The
production adapter invokes `opencode run --agent plan` for WebDiscovery and
FactVerifier work.

A custom permission-profile probe was rejected by the free-tier runtime with
HTTP 403 because that profile was not accepted in the hosted free-tier
context. It was not made part of the production path. The built-in role proof
is policy evidence only, not proof against hostile same-user processes.

### Durable two-flow dogfood

The final real run used:

    ARENA_OPENCODE_EXECUTABLE=/home/kasun/.opencode/bin/opencode
    ARENA_OPENCODE_MODEL=opencode/muse-spark-1.2-contributor-free
    cargo test real_web_discovery_user_problem_and_competitor_flows_survive_reopen -- --ignored --nocapture

The test passed in 210.20 seconds without manual URL/result relay. It created
project `m05d-real-web-research` at final revision `13` and ran two production
WebDiscovery flows: a user/problem question and a competitor/status-quo
question. The work-order IDs were:

    7e7bdc03-cfec-44da-8ebc-bd8233e92829
    953f3de0-03da-4510-a924-dc3b74863ab9
    4c5a5073-f1ec-4709-a61a-e67ad2a4514a
    89952244-32f2-4135-a4a3-3595871176dc

The run admitted seven user/problem proposals and three competitor/status-quo
proposals as durable `Unverified` evidence. A distinct FactVerifier work order
independently finalized one claim in each flow as `IndependentlyVerified`; the
remaining claims stayed `Unverified` rather than being promoted by researcher
consensus. The store was closed and reopened, and the evidence/status and
work-order identity remained reconciled.

## INTEGRATED

- `ProductResearchMode` distinguishes the existing `KnownSource` route from
  `WebDiscovery`; the category is bounded to the M06-needed research classes.
- WebDiscovery work orders carry Arena project/revision, role, category,
  runtime session, evidence references, and terminal state.
- OpenCode output is parsed through a bounded structured contract. URL,
  proposal-count, field-size, duplicate, and HTTPS/public-host checks apply
  before evidence admission. Raw conversations are not Product Authority.
- Every researcher proposal is forced through the existing Arena admission
  path as `ResearchClaim` + `Unverified`. Researcher-supplied verified states,
  owner decisions, gate status, and package state are ignored/rejected.
- A distinct current FactVerifier work order independently uses `websearch` and
  can finalize only a current matching claim through the existing authority
  path as `IndependentlyVerified`, `Contradicted`, or `Unresolved`.
- Cancellation, stale/currentness checks, bounded failure handling, and reopen
  reconciliation remain in the Arena work-order path.
- The owner-facing Delivery view shows bounded research evidence/status without
  exposing raw tool traces as the primary product view.

## VERIFICATION

- Product OS focused suite: **11 passed, 0 failed, 1 ignored**.
- Real OpenCode/Muse authority-adapter regression after the upgrade, including
  protected candidate, legitimate candidate, verifier, and cleanup: **passed**
  in 226.03 seconds.
- Real two-flow WebDiscovery/reopen dogfood: **passed** in 210.20 seconds.
- `cargo check`: **passed** with the existing historical 81-warning baseline.
- `git diff --check`: passed during the reviewed change.
- The focused rustfmt check reports formatting differences already present in
  the large legacy `product_os_runtime.rs` module; the file was not globally
  reformatted so unrelated historical changes were not absorbed.
- Process inspection after the runtime tests found no stale OpenCode, shell, or
  sleep descendant.

## STILL UNPROVEN

- Windows runtime qualification, Windows packaging, Linux packaged install,
  and native GUI dogfood.
- Production-scale performance and resource qualification beyond the measured
  bounded Linux runs.
- Hostile same-user filesystem isolation and complete SSRF/network isolation;
  OpenCode permissions are not a security sandbox, and the integrated path
  does not claim unrestricted `webfetch` safety.
- Broad provider portability beyond the already qualified Linux Zen/Muse path.
- GitHub MCP, pinned ECC/gstack runtime procedures, and shared frontier
  consultation remain separate unqualified/optional capabilities.
- The final M06 founder-to-release dogfood and release closure.

## M06 ADMISSION

**READY FOR M06.** The essential M05D criteria are runtime-proven: Arena can
create WebDiscovery work orders; the upgraded permitted runtime performs real
websearch without manual relay; proposals enter durable Unverified evidence;
distinct FactVerifier work orders independently produce final dispositions;
status survives reopen; failures remain bounded/truthful; web content cannot
directly mutate Product Authority; both required real research flows passed;
and the evidence uses the existing Product OS authority seam rather than a new
crawler, search framework, or workflow authority.

This is capability readiness for the next bounded milestone, not market
validation and not a claim that the final product is release-qualified.
