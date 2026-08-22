# Codex CLI Process Status

This former patch has been incorporated into
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/PROCESS.md`.
It remains as historical context. PROCESS.md and root AGENTS.md are the
authoritative current workflow. Audit before edits, source-over-docs, and
post-change verification remain unchanged.

Phase 1 Memory followed the Codex audit → implementation → verification →
post-audit → checkpoint flow and was checkpointed at `f0847c0`.

---

## How Tasks Are Executed

**Codex CLI replaces Cline as the current execution and verification tool
for coding and documentation tasks in this project.** Codex CLI is a materially
different tool than Cline was: it has its own real file/terminal access, can
independently read the actual current state of files, form its own plan,
run cargo check/npm run build itself, and see real compiler/build
output -- rather than blindly executing a hand-written prompt with no
independent verification. This closes exactly the gap that let stale
project docs and at least one real registration bug (pause_session/
resume_session missing from generate_handler!) go undetected for an
extended period.

### Model Roles (Updated)

| Role | Who | Does What |
|------|-----|-----------|
| Design & planning | Claude | Reads source files, forms the technical plan, writes out exactly what needs to change and why. Still makes all technical decisions autonomously. |
| Execution & independent verification | Codex CLI | Reads the real current file state itself, executes the plan, runs cargo check/npm run build, reports real output, shows the diff before/after |
| Occasional local experimentation | Codex CLI in --oss mode (Qwen Coder / Nemotron via Ollama) | ONLY for small, contained, low-risk tasks the user explicitly wants to test locally -- NOT the default execution path. This machine's 4GB RAM/Celeron constraint makes large local models risky to run alongside the Tauri dev build; do not default to this mode. |
| User | Human | Says go/stop at major decision points. Runs Codex CLI sessions. Reports results back to Claude when cross-referencing is needed. |

**Permission mode:** workspace-write + on-request approval is the
project default (per the user's own Codex CLI guide). Do NOT use
danger-full-access or --dangerously-bypass-approvals-and-sandbox on
this project -- a confidently-wrong autonomous edit in a codebase this
interconnected (IPC contracts, lock-type dependencies across 3+ files,
frontend/backend field-name matching) is exactly the failure mode
approval-gating exists to prevent.

**AGENTS.md** (at the project root and/or ~/.codex/AGENTS.md) carries the
project's non-negotiable constraints, named risks, and design ground truth
so every Codex session inherits them automatically. Keep this file in sync
with DECISIONS.md/BACKEND.md/FRONTEND.md's Core Constraints and Named
Risks sections -- if one changes, update the other.

### When Claude vs. Codex CLI does the work

- **Claude:** planning, reading files to build understanding across the
  whole codebase, writing out precise specifications, reviewing Codex's
  output when the user pastes it back, resolving cross-file architectural
  questions, updating project docs.
- **Codex CLI:** the actual file edits, running builds/checks, and -- this
  is the valuable part -- independently verifying claims rather than
  trusting them. If Claude says "this should now return a JSON string,
  parse it on the frontend," Codex CLI can directly read commands.rs to
  confirm that's actually true, rather than taking it on faith.
- **Direct Claude file delivery** (the previous method -- Claude hands the
  user complete replacement files to place by hand) remains available and
  is still appropriate for: doc rewrites, planning-only sessions, or any
  task where the user isn't running Codex CLI in that moment.

### Cline — legacy, infrastructure-only

Cline is no longer the primary execution tool for coding tasks but remains
available for pure infrastructure tasks it already handled well (initial
npm install, environment scaffolding) if Codex CLI isn't in use for some
reason. Cline should NOT be used for coding logic going forward -- Codex
CLI's independent verification capability is strictly better for that role.

### Codex CLI Session Checklist

At the start of a Codex CLI session on this project:
1. Confirm the working directory is correct (codex --cd
   /home/kasun/Music/arena/consensus-arena or equivalent) -- Codex works
   mainly inside whatever folder it's pointed at.
2. Confirm AGENTS.md is present at the project root so it's loaded
   automatically.
3. For any task involving claims about "current state" (is X built? does Y
   exist?) -- ask Codex to verify directly against the real files, not
   against what a project doc says.
4. After Codex makes changes: /diff, then cargo check/npm run build,
   then report the real output back -- don't just trust a "done" claim.
