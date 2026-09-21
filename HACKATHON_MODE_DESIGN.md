# Consensus Arena — Hackathon Mode (Design Document)

## Status: DESIGN COMPLETE — PRE-IMPLEMENTATION

This document is the single source of truth for the Hackathon Mode feature
as brainstormed and agreed between Kasun and Claude. It has not yet been
scoped into a technical specification (struct definitions, IPC events,
commands) or handed to a coding agent for implementation. Read this
document completely before writing any specification or code against it.

This feature is **independent of and additive to** the existing
WebView-based multi-model panel described in ARCHITECTURE.md, BACKEND.md,
FRONTEND.md, IPC.md, and DECISIONS.md. It does not replace, modify, or
depend on any existing session flow except at the two integration points
explicitly named below (the setup toggle, and the point where a hackathon
result is injected back into the main leader's context). Do not conflate
this document with the separately-discussed OpenCode integration idea
(project-folder generation and build handoff after a blueprint is
finalized) — that is explicitly out of scope here and was intentionally
not merged into this design.

---

## 1. What This Feature Is

Hackathon Mode is an optional, API-key-driven "wide idea generation burst"
that the main discussion panel's leader can trigger when it hits a hard
problem, or that the user pre-enables at session setup. Where the existing
panel is slow, high-quality, and resource-heavy (real browser sessions,
capped at 2 WebViews, human-reviewable), Hackathon Mode is cheap, wide,
and fast (plain HTTP calls to API-based models, no WebView, no login
state, potentially dozens of models running at once) because it is
network/API-bound rather than local-compute-bound — it does not stress
the 4GB/Celeron hardware constraint the way additional WebViews would.

**The relationship between the two systems:** the main panel's leader
remains the sole authority. Hackathon Mode does not make decisions on the
leader's behalf — it produces raw material (multiple groups' worth of
synthesized output) that gets folded back into the main leader's ongoing
context, exactly the same way a Route response or a RouteCompare response
does today. The main leader reads it, evaluates it, and decides what to
do next using its existing decision-making (Blueprint / Route / AskUser /
Continue / Complete), unchanged.

**Why this doesn't need an external framework or plugin:** every
sub-problem this feature requires (parallel fan-out to many API
endpoints, collecting responses, maintaining a running conversation
history per group, one model directing others and deciding when to stop)
is already solved, in a harder form, by the project's existing
`agent_brain.rs` + `response_router.rs` + `context_manager.rs` pattern.
Hackathon Mode is not a new orchestration engine — it is N small,
parallel instances of the orchestration pattern the project already has,
running for a bounded burst, feeding into the one large instance that
already exists. External multi-agent frameworks (LangGraph, CrewAI,
Agent Squad, RuFlo, etc.) were evaluated and rejected: nearly all are
Python/TypeScript-native and would require a subprocess bridge from the
Rust backend (new memory footprint, new failure surface, on hardware
that is already tight), and the one Rust-native option found
(`openai-agents` crate) only wraps a single API call — it does not solve
fan-out, health-check discovery, group formation, or hierarchical
report-up, which are this feature's actual hard parts.

---

## 2. Relationship to Existing Systems — What This Is NOT

- This is **not** a modification to the existing 7-model WebView roster
  (chatgpt, claude, gemini, deepseek, qwen, glm, kimi) or their
  browser-automation mechanism. Hackathon participants are a completely
  separate, user-configured roster of API-key-based models.
- This is **not** a replacement for the existing `AgentDecision` enum
  (Route / Blueprint / Continue / Complete / RouteCompare / AskUser) used
  by the main panel's agent brain. Hackathon groups use their own,
  smaller internal decision contract (see Section 6).
- This is **not** the OpenCode/project-folder-generation idea discussed
  separately (Claude's proposed later feature: after the main panel
  reaches full consensus on a blueprint, capture it as markdown docs in a
  local project folder and hand off to OpenCode/Codex for actual code
  build). That idea was mentioned only as background context in case it
  revealed synergy — it does not change anything about this design and
  is not being designed here. The only noted connection: this document's
  author flagged that if Hackathon Mode's group outputs need to
  eventually feed a blueprint that gets handed to a builder agent, the
  main leader's synthesis of hackathon output should stay markdown-clean
  — this is a note for later, not a current design constraint.
- This is **not** a grading/reliability-scoring system. An AI-judged
  contribution-grading approach was proposed and explicitly rejected in
  favor of user-controlled manual ordering (see Section 4 and Section 9).

---

## 3. Model Access Patterns (Supported, Both)

Hackathon participants are accessed via API key, not browser automation.
Two configuration patterns must both be supported:

1. **Shared key/endpoint covering many models** — e.g. NVIDIA NIM's free
   tier, where one API key and one base URL exposes 50+ distinct model
   names. The user pre-selects and saves which of those model names to
   actually use.
2. **Separate API key per model** — each model has its own key and base
   URL (e.g. a model accessed via its own dedicated provider account).

Both patterns must be representable in the same saved-model list; the
system does not need to know or care which pattern a given saved entry
uses — each saved model entry independently carries its own
`api_key`/`base_url`/`model_name`, whether or not that `api_key`/`base_url`
pair happens to be identical to another entry's.

---

## 4. Configuration — The Hackathon Mini-Window

All Hackathon Mode configuration happens in a dedicated mini-window,
separate from the main new-session Setup screen, opened when the user
toggles Hackathon Mode on during session setup.

### 4.1 Saving models

The user registers each hackathon-participant model by saving:
- Model name
- Base URL
- API key
- Group assignment (see 4.2)

### 4.2 Group assignment and ordering

- The user creates groups (arbitrary number, no fixed cap discussed).
- Each saved model is assigned to exactly **one** group at save time —
  models do not rotate between groups and are not shared across multiple
  groups. A model assigned to Group A exists only in Group A for the
  lifetime of that configuration.
- Within a group, the user orders members top-to-bottom using
  **up/down arrows** next to each saved model. This order is the single
  mechanism that determines:
  - **Leadership:** the top-ranked (position 1) member is that group's
    intended leader.
  - **Leader fallback:** if the position-1 member fails to respond — at
    invitation time, or mid-hackathon at any point — leadership passes
    to the next live member down the list (position 2, then 3, and so
    on). The group is never abandoned solely because its acting leader
    failed to respond; it demotes down the ordered list until it finds a
    live member. This is one rule applied uniformly at both the
    invitation stage and during the hackathon run itself — there is no
    separate fallback rule for each stage.

  This ordering mechanism was deliberately chosen over an AI-graded
  contribution-scoring system (see Section 9) — it solves both "who
  leads" and "who leads if the leader fails" with a single, simple,
  user-controlled, mechanically trivial construct (an ordered list),
  with no AI judgment call, no bias risk, and no extra API calls needed.

### 4.3 Per-group invite toggle

Each group has its own checkbox in the mini-window. Only groups with
their checkbox ticked are included when the user sends invitations.
Groups left unticked do not participate in that hackathon run at all.

### 4.4 Send Invitations (health-check / participant discovery)

- Button: **"Send Invitations."**
- On click: the system sends a lightweight test prompt, in parallel, to
  every saved model belonging to every ticked group. This is a
  concurrent fan-out — one request per model, all in flight
  simultaneously — not a sequential loop. Expected wall-clock time for
  the whole invitation round is roughly the response time of the
  slowest responding (or timing-out) model, not the sum of every
  model's response time; for a batch on the order of tens of models,
  the whole round is expected to typically resolve within roughly
  10–20 seconds, most responses landing within the first few seconds.
- **Live-updating team card, per group:** as each model's invitation
  response comes back, that model is highlighted as a confirmed
  participant and the group's displayed list re-sorts so responders
  float to the top, preserving their originally-saved relative order.
  This updates live as responses stream in — the user is not staring at
  a frozen screen for the duration of the round.
- Non-responders are not included as participants for that run. This is
  evaluated fresh on every "Send Invitations" click — a model that
  failed to respond in a previous run is not permanently excluded.
- Each saved model has an associated icon (provided by Kasun) shown on
  its card for visual identification in the live team-card view.

### 4.5 Zero-response groups

If **no** member of a ticked group responds to the invitation, that
group's checkbox becomes **inactive** (cannot be toggled on or off) —
this is a UI-level state change, not a separate runtime rule. This
single mechanism is also the resolution for "what happens if an entire
group ends up with zero live members" — it is prevented from ever being
selectable in a broken state in the first place, rather than being
allowed to run and failing at hackathon execution time.

### 4.6 Max questions per teammate

A dropdown in the mini-window, customizable in number (candidate values:
1 / 2 / 3 / 5 / Unlimited — exact list to be finalized at
implementation time), labeled **"Max questions per teammate."**

- This caps how many times **each individual non-leader member** can be
  routed to by their group's leader, total, across that group's entire
  internal hackathon session.
- **Not applicable to the leader** — the leader itself has no cap on how
  many routing/synthesis decisions it makes; only its use of each
  individual teammate is bounded.
- **Rationale (why per-teammate cap, not a total leader-routing
  budget):** a per-teammate cap guarantees every member the user
  deliberately assigned to a group is actually consulted, up to the
  bound. A total-budget-to-spend-anywhere alternative was considered and
  rejected, because it would let a group leader silently spend its
  entire budget on one favored member and never consult the others at
  all — defeating the purpose of having a multi-member team in the first
  place. The per-teammate cap keeps the "team" meaningfully a team while
  still letting the leader lean on a stronger member more heavily within
  the per-member bound.

### 4.7 The "Go" button

Once the user is satisfied with the confirmed participants (visible via
the live team cards), they leave the desired groups' checkboxes ticked
and press **"Go."** This closes the mini-window and returns to the main
new-session Setup screen, where the rest of session configuration
(project brief, session type, leader, etc. — all existing, unchanged)
continues as normal.

---

## 5. Trigger

Two trigger points were discussed; both are in scope:

1. **User pre-enabled at session setup** — a toggle in the new-session
   Setup screen turns Hackathon Mode on for that session, with
   configuration happening via the mini-window described in Section 4
   before the main session even starts.
2. **Main leader-initiated mid-session** — the existing main panel's
   leader, during a live session, can decide a hackathon is warranted
   when it encounters a hard problem needing wider input. (Note: the
   exact mechanism by which the main leader signals this decision — e.g.
   a new `AgentDecision` variant, versus some other signal — was raised
   as an open question during brainstorming and has not yet been
   resolved or specified. This is a known gap for the implementation
   specification phase, not a decision made in this document.)

In both cases, once a hackathon run actually starts, the mechanism from
Section 6 onward is identical regardless of which trigger initiated it.

---

## 6. Task Distribution

- Every participating group receives the **same task brief** for a given
  hackathon run. This was an explicit decision: the goal is divergent,
  independent takes on one problem from different groups, not the main
  leader splitting one problem into different sub-pieces handed to
  different groups.
- The task brief itself is a plain string, conceptually equivalent to
  the existing `project_brief` field — its exact derivation (verbatim
  from the main leader's current problem statement, vs. some
  transformation of it) is not yet specified.

---

## 7. Group Execution — Hierarchical Model (Confirmed Approach)

Two shapes were discussed for how a group produces its output: a flat
version (every member answers the task brief once, responses
concatenated with no internal leader) and a hierarchical version (a
group-internal leader manages a real back-and-forth with its own
members before submitting one synthesized output). **The hierarchical
version is the confirmed approach** — Kasun explicitly wants group
leaders that run their own internal discussion, not flat one-shot
concatenation, despite the added complexity being acknowledged.

### 7.1 Mechanism, per group, once a hackathon run starts

Each ticked, non-degraded group runs its own independent, simultaneous
loop (groups do not wait on each other — see Section 8):

1. The group's task brief becomes the first message in that group's own,
   private, accumulating conversation history (conceptually identical
   to what `context_manager.rs` already does for the main panel, scoped
   to one group instead of the whole session).
2. The group's current acting leader (starting at position 1 in the
   user's saved order, per Section 4.2/4.5's fallback rule) receives the
   full accumulated history and produces a decision: route a specific
   prompt to a specific teammate, or submit the group's final output.
3. If routing: the routed prompt is appended to the group's history, the
   target teammate receives the full accumulated history and responds,
   and that response is appended to the group's history in turn. This
   respects the per-teammate cap from Section 4.6 — a teammate who has
   already been routed to the maximum configured number of times is not
   selected again by the leader for the remainder of that group's run.
4. The loop returns to step 2 — the leader now sees the full history,
   including the newest teammate response, when making its next
   decision.
5. This repeats until the acting leader decides to submit. There is no
   fixed round cap on the leader's own decision-making — only the
   per-teammate consultation count is bounded (Section 4.6). (Whether an
   additional hard safety cap on total group rounds is needed, the way
   the main panel has no iteration cap either, was raised as an open
   question during brainstorming and left unresolved — flagged as a gap
   for the implementation specification phase.)
6. If the acting leader fails to respond at any point in this loop
   (not just at the initial invitation stage), leadership passes to the
   next live member in the saved order, per Section 4.2's fallback rule,
   and that new leader continues the loop using the group's existing
   accumulated history — the group's conversation is not restarted from
   scratch on a leader handoff.

### 7.2 Group leader's decision contract

The group-internal leader uses its own decision contract, distinct from
and smaller than the main panel's `AgentDecision` enum — conceptually
only two outcomes are needed at this layer (route to a specific
teammate with a specific prompt, or submit/complete). The main panel's
richer decision types (Blueprint, AskUser, RouteCompare, Continue) were
not discussed as applying inside a group's internal loop; this document
does not assert whether they are needed or excluded, only that they were
not part of the brainstormed group-leader mechanism. Exact naming and
structure is left to the implementation specification phase.

---

## 8. Concurrency

- All ticked, viable (non-degraded) groups run their internal loops
  **simultaneously**, not sequentially. This was an explicit design
  decision, arrived at by directly addressing the question of whether
  group count affects total hackathon time.
- **Group count is not the primary driver of total hackathon time.**
  Because groups run in parallel, total time is roughly bounded by the
  slowest single group's internal loop, not the sum of every group's
  time. What actually drives time is **group depth** — how many
  rounds a given group's leader chooses to run before submitting, and
  how many live members are in the slowest group — not how many groups
  exist.
- This concurrency model is consistent with why Hackathon Mode does not
  stress the 4GB/Celeron hardware constraint the way additional WebViews
  would: the cost of running more groups in parallel is more simultaneous
  network/API calls, not more local compute or memory.

---

## 9. Rejected Alternative: AI-Judged Contribution Grading

An earlier proposal (from Claude, during brainstorming) suggested
automatically sorting/ranking saved models based on a grading system
that evaluated how much each model's input contributed to its group's
final solution. This was explicitly **rejected by Kasun** in favor of
the manual up/down ordering described in Section 4.2, for reasons
including:

- It avoids the fuzzy, bias-prone problem of having an AI judge quality
  of contribution — a genuinely hard evaluation problem, not a
  mechanical one.
- It avoids an entirely separate scoring AI call per group (extra cost,
  extra latency, extra prompt-design surface, extra reliability
  questions) that the grading approach would have required.
- The manual-ordering solution incidentally and elegantly solves a
  second problem (leader fallback on non-response) that the grading
  approach did not address at all — one simple mechanism instead of two
  separate ones.

This is recorded here so a future session does not reintroduce automatic
grading without deliberately revisiting this rejection and the reasoning
behind it.

---

## 10. Open Questions — Explicitly Unresolved

These were surfaced during brainstorming and intentionally left open.
Do not assume an answer to any of these when writing an implementation
specification — confirm with Kasun first.

1. **Mid-session trigger mechanism.** How exactly does the main panel's
   leader signal "start a hackathon now" during a live session — a new
   `AgentDecision` variant, a separate signal, or something else? Not
   discussed in enough depth to specify.
2. **Group size floor.** Can a group legally consist of just one member
   at configuration time? If so, and that single member fails to
   respond, does the group simply produce no output for that run (same
   as the zero-live-members case in Section 4.5), or is single-member
   group creation itself disallowed in the mini-window UI? Raised, not
   answered.
3. **Hard round cap on a group's total internal loop.** Section 4.6's
   per-teammate cap bounds how often any one member is consulted, but
   does not bound how many total decision cycles a group's leader can
   run before submitting. Raised as a parallel to the main panel's own
   lack of a fixed iteration cap, but not explicitly confirmed either
   way for groups.
4. **Task brief derivation.** Exactly how the task brief handed to
   hackathon groups is derived from the main leader's current context
   (verbatim vs. transformed/summarized) is not specified.
5. **Exact dropdown value list for "Max questions per teammate."**
   Candidate values suggested during discussion (1/2/3/5/Unlimited) are
   illustrative, not confirmed final.
6. **Report-up formatting detail.** Section 7 assumes each group's final
   submitted output gets packaged in a form analogous to the existing
   RouteCompare arm's "[X said: ...]" concatenation pattern before
   reaching the main leader, but the literal format was not specified
   in depth — only that the general shape should follow existing
   precedent in the codebase.
7. **API key storage/security for hackathon-saved models.** The existing
   `session_vault.rs` pattern (AES-256-GCM encrypted storage) was
   referenced during earlier general project discussion as the
   established precedent for storing sensitive credentials, but whether
   hackathon-participant API keys should reuse that exact mechanism was
   not explicitly discussed or confirmed as part of this feature's
   brainstorming.

---

## 11. Relationship to Existing Named Risks and Constraints

Not yet evaluated against this feature and flagged here so the
implementation specification phase does not skip this step:

- RISK-UNWRAP, RISK-BLOCKING, RISK-CHANNEL (see AGENTS.md / PROCESS.md)
  apply to any new Rust code this feature requires, same as everywhere
  else in the codebase — not discussed specifically for this feature but
  not exempted either.
- RISK-IPCPARSE's pattern (any new command returning a struct/collection
  must return `serde_json::to_string(&value)`, every frontend caller
  must `JSON.parse()` it) will apply to any new commands this feature
  introduces (e.g. saving/loading the hackathon model+group
  configuration, triggering invitations, retrieving live team-card
  state) — not discussed specifically for this feature but expected to
  apply by the project's existing established pattern.
- The 4GB RAM / 2 WebView constraints were explicitly reasoned about
  during this brainstorming (Section 8) and found not to conflict with
  this feature, since it is API/network-bound rather than
  WebView/local-compute-bound.

---

## 12. Summary — Confirmed Design in One Pass

1. User saves hackathon-participant models (name, base_url, api_key) via
   a dedicated mini-window, supporting both shared-key-many-models and
   separate-key-per-model configurations.
2. User assigns each saved model to exactly one group, and orders each
   group's members via up/down arrows — order determines leader (top)
   and fallback-on-failure sequence (next down), applied identically at
   invitation time and during the hackathon run.
3. User ticks which groups participate, and clicks "Send Invitations" —
   a parallel health-check fan-out to every model in every ticked group,
   with a live-updating, auto-sorting team card per group as responses
   arrive.
4. Groups with zero responders have their checkbox locked inactive.
5. User sets "Max questions per teammate" (dropdown, customizable),
   applying to non-leader members only.
6. User clicks "Go" — mini-window closes, returns to normal session
   setup (toggle-triggered) or the hackathon simply becomes available
   for the main leader to invoke later (leader-triggered — mechanism
   unresolved, see Section 10.1).
7. On an actual hackathon run: every ticked, viable group receives the
   same task brief, and runs its own hierarchical internal loop
   simultaneously with every other group — acting leader routes to
   teammates (respecting the per-teammate cap), accumulates a private
   running conversation, and eventually submits.
8. Each group's submitted output is packaged and reported to the main
   panel's leader, in a form analogous to the existing RouteCompare
   pattern, and the main leader resumes its normal decision-making with
   that new context folded in.
9. Total hackathon time is bounded by the slowest group's depth, not by
   how many groups exist, because groups run in parallel — consistent
   with the project's hardware constraints.
10. Manual, user-controlled ordering replaces automatic AI-judged
    grading entirely, for both leader selection and leader-fallback.

---

## 13. Explicitly Out of Scope for This Document

- The OpenCode/project-folder-generation/build-handoff idea (mentioned
  once as background context, not designed here).
- Any concrete Rust struct definitions, IPC command/event names, SQLite
  schema, or frontend component structure — this is a design document,
  not an implementation specification. A separate specification pass
  (matching the depth of PHASE1_MEMORY_FINAL_v10.md's treatment of the
  memory system) is expected before this reaches Codex CLI.
