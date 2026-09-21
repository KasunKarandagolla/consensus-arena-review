# M10 Specialist Runtime, Research Capability, and Continuity Audit

Date: 2026-09-21
Branch: `opencode/arena-m10-specialist-runtime`
Baseline: `890beb1d84fef4f7263e7c103f62a4fe1b77c412`
Implementation checkpoints: `e5301c5`, `4f6e855`

## Status vocabulary

- **CURRENT/IMPLEMENTED** — present in the current source and covered by the
  relevant deterministic check.
- **RUNTIME-PROVEN** — exercised against the real external capability in this
  environment, not merely by a fixture.
- **SOURCE-CONFIRMED** — behavior checked against the upstream source/docs.
- **ENVIRONMENT-BLOCKED** — the implementation is bounded, but this host lacks
  the external prerequisite for runtime qualification.
- **PENDING VALIDATION** — requires independent source/diff/runtime review or
  a target-specific run.

## CURRENT/IMPLEMENTED

1. First-level role families are persisted in `SettingsStore`. The catalog
   records model/provider/source/free status, probe timestamp, latency,
   failures, health status, and bounded error classification.
2. Free model choices become selectable only after a successful bounded
   `opencode run --model ...` marker probe. Fallback resolution is free-only
   and blocks instead of silently choosing a paid or unknown model.
3. Custom API model metadata is public-only; the API key is saved through the
   existing credential store and never enters Product OS, evidence, memory, or
   normal DTO output. Selection requires a real bounded response probe.
4. New semantic role execution binds a durable `ResolvedModelPolicy` and a
   bounded `ExecutionContextBundle` before the worker starts. Delegated role
   and channel work inherit the root snapshot; worker model hints are ignored.
5. `ExecutionContinuityState` fences work-order identity, authority revision,
   and execution epoch. Restarted running work becomes reconciliation-
   required, owner guidance advances the epoch, and stale results are rejected.
6. Agent Reach doctor JSON is consumed as a capability report, including
   array/map channel registries, active backend, observed time, health, and
   unavailable reason. Current capability observations are persisted without
   credentials.
7. TikTok is a bounded direct `tt` adapter using JSONL, `--limit`, timeout,
   output bounds, process cleanup, and faithful exit status mapping. Exit 4 is
   unavailable/inconclusive, not empty evidence.
8. ECC is a versioned allowlist with upstream attribution and an explicit
   warning that imported procedural text is untrusted. Curated resources are
   subordinate to Arena authority and inherit root model policy.
9. Settings exposes only first-level role assignment and compact Agent Reach /
   TikTok health; descendant model selectors and raw logs are not exposed.

## SOURCE-CONFIRMED

- The Agent Reach upstream documentation identifies a broad channel registry,
  backend/doctor capability model, and non-installing status/installation
  separation. Arena does not auto-install system dependencies.
- The `tt` upstream README documents operations including search, video, user,
  posts, comments, replies, hashtag, sound, trending, and discover; JSON/JSONL
  output; `--limit`; and exit codes 3 (valid empty), 4 (walled), and 6 (not
  found). See <https://github.com/tamnd/tiktok-cli>.
- The ECC upstream repository was inspected for the curated catalog metadata;
  the imported catalog records repository, commit SHA, source paths, license,
  and Arena capability tags. See <https://github.com/affaan-m/ECC>.

## RUNTIME-PROVEN

- Rust baseline focused Product Authority test: PASS.
- Full Rust binary suite before the final continuity-only additions: **554
  passed, 0 failed, 6 ignored**.
- Specialist policy/continuity/doctor fixture suite: **12 passed, 0 failed**.
- TikTok adapter contract suite: **4 passed, 0 failed**.
- Backend `cargo check`: PASS after the implementation checkpoint.
- Rust format check and `git diff --check`: PASS at the verified checkpoint.
- OpenCode `1.18.31` version check: PASS.
- `opencode models --refresh`: PASS.
- Real bounded `opencode run --model
  opencode/muse-spark-1.3-contributor-free --format json` probe: PASS; the
  correlated response was exactly `ARENA_MODEL_PROBE_OK`.

These Rust results prove Arena contracts and bounded parsing only. They are
not proof that Agent Reach, `tt`, or a live model provider is installed.

## ENVIRONMENT-BLOCKED

- `agent-reach --version` / `agent-reach doctor --json`: executable not found
  on this host. No system-modifying installation was attempted.
- `tt version`: executable not found on this host. No public TikTok query was
  run.
- Frontend build was attempted but this checkout has no installed `tsc`
  (`npm run build` failed with `tsc: not found`). Dependencies were not
  installed without owner approval.

## PENDING VALIDATION

- Independent source/diff/runtime review of the pushed branch.
- Live verified-free model catalog refresh and response probe on a qualified
  OpenCode/Zen environment.
- Agent Reach doctor observations on Linux and Windows, including mandatory
  channel admission and backend persistence.
- `tt` public success/empty/walled/not-found runs on a host with the binary.
- Whole-app restart/recovery dogfood, Windows packaging/WebView2 parity, and
  settings UI build/runtime review.
- Full M10 product gate demonstrating that Agent Reach, `tt`, ECC, and worker
  output cannot independently mark ProductAuthority, Verified, or Safe Apply.

## Integrity boundary review

The implementation preserves clean-base admission, ProductAuthority owner
questions and decisions, independent verification, bounded delegation,
`SessionRuntime` live ownership, Delivery acceptance/verifier/Apply authority,
secret scanning, and safe Apply invariants. No merge to main and no rewrite or
push to `chatgpt/arena-m09cd-rebuild` was performed.
