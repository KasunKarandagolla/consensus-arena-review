# Consensus Arena — Product Direction

**Status:** Active product direction after the 2026-09 post-gates research and first Delivery implementation.

## Product thesis

Consensus Arena is evolving into a **product-authority and delivery system for a nontechnical product owner**.

The owner should primarily supply:

- product intent;
- requirements and priorities;
- subjective product choices;
- permissions and risk decisions;
- business judgment.

The system should carry as much of the technical journey as practical:

`intent → clarification → accepted requirements → bounded work → implementation → run/test → evidence → repair → re-verification → explicit apply/release`

The user should not remain the routine prompt courier, build runner, screenshot messenger, log collector, or coordinator between AI tools.

## Evidence-gated progression

The product journey uses lightweight Arena-owned exit contracts rather than
assuming that activity is progress. Vision, problem research, positioning,
ambiguity, and reuse are discovery evidence; architecture and build
readiness are decision evidence; implementation requires the existing
independent verifier; release requires actual candidate, package, install,
security, and QA evidence. Missing or stale evidence reopens the relevant
gate.

Research may recommend `stop`, `pivot`, a validation experiment, or a narrow
build. It is not market validation by itself. Worker, MCP, skill, or future
consultation output is evidence only after Arena correlates it with the
current project/work-order authority; it cannot expand disclosure, acceptance,
verification, or release authority.

The current durable research seam now follows that rule in runtime: an
Arena-admitted researcher produces an unverified proposal, and a distinct
Arena fact-verifier work order must independently check the primary source
before the claim can become verified. Owner-required ambiguities are admitted
by Arena and resolved only by a current adopted owner decision bound to the
question and authority revision. This Linux proof uses direct official
GitHub HTTPS retrieval; it does not claim GitHub MCP or broad provider
portability.

## Two product lanes

### Consult

Use frontier consumer chat models when independent high-quality reasoning materially helps:

- ambiguous architecture;
- product strategy;
- difficult debugging hypotheses;
- security challenge;
- adversarial review;
- high-impact decisions.

Consultation is selective. A committee is not the default for routine mechanical work.

The same advisory capability is a future option for a bounded Delivery
research, engineering, or reviewer work order when the question benefits from
frontier reasoning. A future Arena-owned caller must admit the request, derive
the permitted provider and disclosure scope, and reconcile its lifecycle
before treating the response as evidence. The owner-facing product state
remains Arena-owned; a consultation cannot mark a candidate accepted,
verified, or eligible for Apply.

### Build / Delivery

Use bounded programmatic workers and deterministic tools to carry product work forward. Current V1 already proves the core separation:

- Arena owns the product/acceptance boundary.
- The worker proposes and implements.
- Git isolates the candidate.
- Acceptance material is frozen before implementation.
- Project-native verification decides PASS/FAIL, not worker prose.
- The owner is asked only when a durable product decision is required.

## AI access model

Arena is **not** a “zero API” product.

Minimum mode may use **one configurable programmatic AI endpoint/model** to power the core reasoning/worker path. That endpoint may be a free hosted API, local/open model, compatible gateway, or paid endpoint the user voluntarily provides.

Arena also retains consumer-web frontier access for optional specialist consultation. These are two complementary transports, not competing philosophies.

The product must not require separate paid frontier APIs for every consultant.

## What Arena should uniquely own

Arena should own the semantics that generic execution tools cannot safely infer:

- authoritative product intent;
- adopted owner decisions;
- product progression policy;
- owner-facing durable questions;
- acceptance policy;
- correlation between candidate, scenario, attempt, and evidence;
- explicit subjective waivers/approvals;
- concise product history/continuity;
- a simple owner experience.

M05C proves this ownership on an internal validation slice: source-backed
research can survive reopen, inform a bounded product direction, and enter a
current Build Package only through Arena-owned review, owner-decision, and
gate operations. This is not a market-validation claim or the final
founder-to-release experience.

## What Arena should not rebuild

Prefer mature existing infrastructure for:

- generic agent loops;
- tool registries;
- durable workflow execution;
- Git/worktrees;
- browser/native test runners;
- assertion engines;
- deployment providers;
- generic sandboxes;
- schedulers/CI;
- generic vector memory;
- autonomous agent teams unless proven necessary.

Reuse is not an excuse to aggregate many tools. Prefer the **highest mature reusable abstraction that removes more complexity than it introduces**.

## UI philosophy

The product should feel closer to ChatGPT/Grok than a war room:

- simple primary surface;
- familiar navigation;
- low cognitive load;
- progressive disclosure;
- product progress rather than agent telemetry;
- technical evidence available when needed, not permanently exposed.

Do not redesign exact pixels without a separate frontend task. The current implemented `preview.html`-based design remains the visual baseline until explicitly changed.

## North-star relationship

The intended relationship is increasingly:

> “This project is under Arena's responsibility; ask me when my judgment or authority is required.”

not:

> “Ask Arena another question and manually move the answer to the next tool.”

This north star does **not** imply Arena itself must own every technical subsystem.
