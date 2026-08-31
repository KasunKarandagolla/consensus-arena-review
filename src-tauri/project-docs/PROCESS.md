# Consensus Arena — Development Process

## The Core Rule

**Never write code without a complete audit first. Never integrate code without
a complete audit after.**

Mini surgery loops — fix one bug, create another, fix that, create another —
are the primary failure mode of this project. They happen when code is written
without understanding the full state of the system. This process eliminates them.

---

## How Tasks Are Executed

**CURRENT PROCESS:** Codex CLI is the execution and independent verification
tool. It reads the real worktree, plans scoped edits, applies them directly,
and runs the relevant checks. Claude may provide planning/review context,
but documents and hand-written specifications never override current source.
Cline and complete-file delivery sections below are retained only as process
history; they are not the default workflow.

### Current Codex Workflow

1. Confirm the working directory and read root `AGENTS.md`.
2. Read every affected source/document completely and inspect the dirty
   worktree before editing.
3. State a scoped plan. Preserve unrelated user changes.
4. Apply the smallest coherent edit; do not add dependencies or delete files
   without approval.
5. Run the task's real checks (`npm run build`, `cargo check`, targeted
   audits, and always `git diff --check` as applicable).
6. Report the diff and exact verification output. The user decides whether
   to create a git checkpoint.

Audit-before-edit and post-change verification remain non-negotiable.

Phase 1 Memory used this current Codex flow end to end: pre-audit,
implementation, real verification, post-audit, and checkpoint (`f0847c0`).

### Legacy Model Roles (historical Cline infrastructure workflow)

| Role | Who | Does What |
|------|-----|-----------|
| Design & code authorship | Claude | Writes all code, designs all specs, authors all Cline prompts |
| File execution | Cline | Receives Claude's complete code, writes files to disk, runs cargo check |
| Audits | Capable model via Cline | Reads files, reports findings — never writes code, never edits files |
| User | Human | Says go/stop at major decision points only |

**The user never manually edits files. Claude never asks the user to paste code.
Cline writes everything directly.**

### Legacy Cline Model Requirements

Cline requires a model capable of reliable agentic tool-calling with correct
parameter schemas. Not all models are suitable. Confirmed working:
- Claude Sonnet (via Anthropic API) — best
- Gemini 2.5 Pro (via Google AI Studio) — very good, free
- Mistral-Nemotron (via NVIDIA NIM, model: mistralai/mistral-nemotron) — working

Confirmed incompatible:
- Nemotron Super (nvidia/nemotron-3-super-120b-a12b) — thinking mode breaks
  tool-call parser causing infinite retry loops. If using NVIDIA API, switch
  model to mistralai/mistral-nemotron.
- Any reasoning/thinking model without thinking mode explicitly disabled

If Cline enters an infinite retry loop on simple commands, the model is
incompatible. Switch models — do not attempt to fix with prompts.

---

## Legacy Cline Task Structure

Every implementation task follows this structure. Steps are combined into one
comprehensive Cline prompt where possible to minimise round trips — but the
logical sequence never changes.

### Step 1 — Pre-Implementation Audit

Before any code is written, the relevant files are read completely.
The audit model reads and reports. It does not suggest fixes or write code.

Audit prompt must specify:
- Exact files to read (absolute paths)
- Exact things to report (struct fields, function signatures, event names, etc.)
- Named risks to check (see Named Risks section below)
- Output saved to: project-docs/audits/[task-name]-pre.md

### Step 2 — Implementation Design (Claude)

Using the pre-audit findings, Claude produces the complete implementation:
- Every function signature
- Every struct definition
- Every channel type
- Every event name verified against IPC.md
- Every constraint from project docs verified

The design is complete — not sketched. Cline executes exactly what Claude specifies.

### Step 3 — Implementation (Cline with Claude's code)

Claude provides the **complete file content** inside the Cline prompt.
Cline writes the file using the absolute path, then immediately runs cargo check.
If errors appear, Cline reads them, applies the fix, and re-runs.
Cline does not return until: file written + cargo check 0 errors.

### Step 4 — Post-Implementation Audit

Audit model reads the newly written code against the spec and named risk checklist.
Output saved to: project-docs/audits/[task-name]-post.md
Every item is PASS or FAIL with exact file and line numbers.

### Step 5 — Integration and Optional Git Checkpoint

Code is integrated only when post-audit is clean.
If any FAIL items exist, they go back to Step 3 with specific failures listed.
After clean integration and explicit user approval, run:
  git add -A && git commit -m "checkpoint: [task description]"
The user is never asked to debug.

---

## Comprehensive Prompt Strategy

For straightforward tasks, Steps 1–5 are combined into a single Cline prompt:

```
STEP 1 — READ: [files to read — use absolute paths]
STEP 2 — PRE-AUDIT: [create pre-audit file with findings]
STEP 3 — WRITE: [complete file content provided by Claude]
STEP 4 — VERIFY: [cargo check — fix errors and re-run until clean]
STEP 5 — POST-AUDIT: [create post-audit file — PASS/FAIL each item with line numbers]
Report: all steps in one output.
```

**Prompt quality is critical.** Every Cline prompt must be:
- Structured as numbered steps — one action per step
- Using absolute paths for all file operations
- Providing complete file content inline — never partial code or diffs
- Unambiguous — no room for the model to infer intent

Vague prompts produce vague implementations. Complete inline code produces
correct implementations regardless of model capability.

---

## Critical Prompt Rules — Learned From Experience

### Rule 1: Always use absolute paths
Relative paths cause files to be written to wrong directories when Cline's
working directory differs from expected. Every file path in every Cline prompt
must be absolute:
  /home/kasun/Music/arena/consensus-arena/src-tauri/src/filename.rs

Never: src-tauri/src/filename.rs
Always: /home/kasun/Music/arena/consensus-arena/src-tauri/src/filename.rs

### Rule 2: Provide complete file content inline
Never ask Cline to modify specific lines or apply diffs. Always provide the
complete corrected file content for Cline to write. This eliminates ambiguity
and prevents partial writes.

### Rule 3: Git checkpoint after review and user approval
After checks pass and the user approves the diff, run:
  cd /home/kasun/Music/arena/consensus-arena && git add -A && git commit -m "checkpoint: [description]"
This provides rollback points. Absence of checkpoints enables codebase collapse.

### Rule 4: Stop immediately if Cline loops
If Cline repeats the same tool call more than 3 times without progress,
stop the task immediately. Do not let it continue. The model is incompatible
or the prompt is malformed. Paste the error output to Claude for diagnosis.

### Rule 5: One file write per Cline step
Each WRITE step should target one file. Multiple file writes in a single step
increases the chance of a partial write leaving the codebase in an inconsistent
state. If multiple files must change, write them in sequential steps.

---

## Task Sizing Rules

Each task must:
- Touch the **minimum number of files** necessary to produce one working unit
- Have a **single clear completion condition** (cargo check 0 errors + post-audit clean)
- Be **independent** — completing it must not require another task to be done first

Split tasks when they touch logically unrelated files.
Combine tasks when they are a single atomic unit of work.

---

## Named Risks — Check Every Audit

These specific patterns must be checked in every audit:

**RISK-BLOCKING:** `blocking_lock()` inside `on_navigation` closure or any async fn.
Causes UI freeze on WebKitGTK. Zero tolerance.

**RISK-CHANNEL:** Using `tokio::sync::mpsc` inside `on_navigation` closure.
Must use `std::sync::mpsc` because `on_navigation` is synchronous.

**RISK-EVENTMATCH:** Backend emits field named X, frontend listens for field named Y.
Check every `app.emit()` call against IPC.md. Field names must match exactly.

**RISK-UNWRAP:** `unwrap()` or `expect()` in any path reachable during a session.
Silent panic crashes the session. Only permitted in: test code, compile-time
constants, .setup() closure startup (acceptable for unrecoverable init failures).
`unwrap_or_default()` is acceptable when empty string is a safe fallback.

**RISK-STALERESPONSE:** Response from turn N captured as turn N+1 response.
wait_for_response() must check BOTH agent_id AND turn number before accepting.

**RISK-INITSCRIPT:** `initialization_script` hardcoded for specific agent.
GENERIC_INIT_SCRIPT must remain a static constant, generic across all services.

**RISK-NAVCLOSURE:** `on_navigation` closure captures agent-specific variables by value.
Agent identity must come from URL or `window.__ca_agentId` — never from closure capture.

---

## Audit File Structure

```
src-tauri/project-docs/
└── audits/
    ├── fix1-settings-path-pre.md
    ├── fix1-settings-path-post.md
    ├── fix2-browser-backend-pre.md
    ├── fix2-browser-backend-pre-v2.md
    ├── fix2-browser-backend-post.md
    ├── fix3-agent-brain-pre.md
    ├── fix3-agent-brain-post.md
    ├── fix4-atomic-save-pre.md
    ├── fix4-atomic-save-post.md
    ├── fix5-response-router-pre.md
    ├── fix5-response-router-post.md
    ├── fix5-verification.md
    ├── fix5-corrective-post.md
    ├── fix5-final-post.md
    ├── full-compliance-audit.md
    ├── cleanup-post.md
    ├── final-absolute-audit.md     ← comprehensive final audit (to be created)
    └── [future audits...]
```

Every audit file is preserved permanently. Never deleted.
Audit files form a permanent history of what was verified and when.

---

## User Intermediation Points

The user decides at exactly these points:

1. **Major architecture decisions** — if something fundamental changes
2. **Go/stop at task completion** — review audit results, say integrate or revise
3. **Design approval** — approve Google Stitch UI design before frontend implementation

The user is never asked to:
- Edit files manually
- Paste code
- Debug errors
- Judge whether code is correct
- Make technical implementation decisions

---

## Chat Session Management

At significant milestones, Claude notifies the user to start a new chat in the
same Claude project. Before starting the new chat, Claude identifies which
project files need updating and produces updated versions. This prevents hitting
context limits mid-task and ensures the new chat has accurate project state.

Significant milestones:
- Major feature complete (e.g. entire backend done)
- Full audit completed
- Frontend phase complete
- Any time context is approaching limits

Before starting a new chat:
1. Update all relevant project files (DECISIONS.md, BACKEND.md, etc.)
2. If the user approves, commit: git add -A && git commit -m "checkpoint: [milestone]"
3. Notify user to start new chat in same Claude project

---

## The Setup-Send Detection Special Case

During setup phase, detecting that the user pressed Send requires ALL four conditions:

1. Input field was not empty before send
2. Message count in conversation increased by exactly 1
3. Input field is now empty
4. No page reload occurred (document.readyState check)

Fires `arena://sent/{agent_id}` only. Never fires `arena://response`.

False trigger test cases that must pass:
- User types then deletes text and presses Enter → must NOT trigger
- User refreshes the page → must NOT trigger
- User presses Enter on empty input → must NOT trigger
- User sends actual message → MUST trigger within 2 seconds

Note: Current GENERIC_INIT_SCRIPT implements send detection via a polling
interval that checks input field value changes. The four conditions above
are the specification — verify the implementation matches during the
GENERIC_INIT_SCRIPT deep review.
