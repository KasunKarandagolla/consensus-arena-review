# M05D — Durable web discovery qualification

Date: 2026-09-19
Predecessor: ab45d3369e4fb8ffb6336f5bc36afcd25e9bf191
Status: **BLOCKED FOR M06**

## Decision

Arena cannot currently perform the required autonomous web-discovery research
flow. The installed OpenCode runtime is version 1.17.18. Its currently
configured OpenCode Zen free-tier provider rejected a bounded, model-backed
research request before model/tool execution, reporting that OpenCode 1.18.0
or newer is required for the free tier.

No OpenCode upgrade, provider credential, paid search provider, browser
automation, crawler, or substitute research framework was added in this
milestone.

## DOCUMENTED

- The installed OpenCode SDK types expose websearch and webfetch permission
  keys, together with allow/ask/deny policy values.
- Those permission declarations are tool policy, not an OS/security sandbox.

## SOURCE-CONFIRMED

- The existing durable research runtime accepts only a known, query-free
  official GitHub repository API locator. It deliberately is not an arbitrary
  URL fetcher.
- Existing Product OS authority retains Unverified proposal admission,
  independent FactVerifier finalization, durable reopen, contradiction state,
  and gate/package derivation. None of those operations permit a researcher
  to self-verify or directly change Product Authority.
- No existing configured alternative autonomous search integration was found.

## RUNTIME ATTEMPT

Command surface:

    /home/kasun/.opencode/bin/opencode --version
    1.17.18

A disposable directory was created at
/tmp/arena-m05d-web-qOPrKo. The bounded invocation used the historical
qualified model identifier:

    opencode run --dir /tmp/arena-m05d-web-qOPrKo
      --model opencode/muse-spark-1.2-contributor-free --format json

The prompt required exactly one websearch call and explicitly prohibited bash,
local reads, edits, writes, skills, MCP, and non-web tools. The provider
returned HTTP 426 before a model response or tool call:

    OpenCode 1.18.0 or newer is required to use the free tier

Observed session identity: ses_f499940a4ffeTyWMCx2p1J5Lr5.

No search result, source proposal, evidence record, repository mutation,
credential, tool trace, or authority transition was produced. Consequently,
websearch/webfetch presence, provider search account requirements, tool
correlation, read-only permission enforcement, timeout/cancellation through a
real web tool, and network/SSRF behavior remain unproven.

## SECURITY BOUNDARY

The attempted role instruction denied all local mutation and credential
actions, but no model/tool execution occurred, so this is not proof of
permission enforcement or prompt-injection containment. Arena's existing
authority boundary remains unchanged: untrusted research content could only
become an Unverified proposal through Arena admission and would still require
a distinct FactVerifier.

## M06 ADMISSION

The essential M05D criterion that the current permitted runtime performs real
web search without manual relay failed. M06 must not use manually copied web
results or claim autonomous user/problem or competitor research.

The exact minimal blocker is a compatible, owner-authorized OpenCode runtime
and a requalified Zen web-search path (or another already-configured
autonomous search integration). This audit does not authorize upgrading
OpenCode, changing providers, requesting keys, or adding a search service.

## STILL UNPROVEN

- Real autonomous WebDiscovery work order and structured result ingestion.
- Independent web verifier work order and reopen proof for web evidence.
- User/problem and competitor/status-quo autonomous research flows.
- Prompt-injection, cancellation, duplicate, malformed, oversized, zero-result,
  source-freshness, and SSRF/network-boundary web-tool adversaries.
- Any M06 founder dogfood or release closure.
