# Consensus Arena Project Documentation

## Current status

The backend is feature-complete for the documented scope. The production
React frontend has been rebuilt from
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`
while preserving the existing Tauri IPC contract. The preview design is the
current implemented baseline and must not be replaced without explicit
approval.

The backend and frontend builds passed during the Phase 1 post-audit. Phase 1
Memory is implemented and checkpointed at `f0847c0`. A real interactive Tauri
memory test remains before Phase 2 Skills: exercise Route, RouteCompare,
Blueprint, AskUser, export, and restore. Evidence is recorded in
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/audits/phase1-memory-post.md`.

## AGENTS.md
Current path: `/home/kasun/Music/arena/consensus-arena/AGENTS.md`.

Codex reads this automatically at the start of every session in that
project folder (per the Codex CLI guide's section 9). It bakes in every
non-negotiable constraint, named risk, and design ground truth from this
project so you don't have to repeat them by hand in every prompt.

## PROCESS-md-codex-patch.md
Historical patch retained for context. Its Codex CLI workflow is now
incorporated into PROCESS.md.

## CLAUDE-PROJECT-INSTRUCTIONS.md
Project context, constraints, current state, and named risks for Claude.

## Development launch

```bash
cd /home/kasun/Music/arena/consensus-arena
npm run tauri dev
```

## Verification

```bash
cd /home/kasun/Music/arena/consensus-arena/src && npm run build
cd /home/kasun/Music/arena/consensus-arena/src-tauri && cargo check
cd /home/kasun/Music/arena/consensus-arena && git diff --check
```

Read real source before changing status claims. As of the Phase 1 Memory
post-audit, the verified counts are 16 AppState fields and 38 Tauri commands
defined/registered; recount from source whenever a task depends on them.
