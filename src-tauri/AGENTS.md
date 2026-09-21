# AGENTS.md — Consensus Arena

This file is intended for Codex/AI coding agents working in the repository.

## Start here

1. Read `src-tauri/project-docs/README.md`.
2. Read `src-tauri/project-docs/DECISIONS.md`.
3. Use the README task map to read only relevant modular docs.
4. Read all affected source files completely before editing.
5. Read the latest relevant permanent audit.

**Source/runtime reality beats docs.** If source conflicts with docs, implement against source and update the affected doc/audit.

## Current product state

Consensus Arena is no longer only a multi-model blueprint tool.

It has:

- a mature **Consult** lane using consumer frontier web chats + AgentBrain;
- a new parallel **Build / Delivery V1** lane using isolated Git worktree + bounded DSH worker + frozen acceptance + independent verification + bounded repair + durable owner questions + explicit safe Apply.

Dagu is the next workflow-engine candidate to validate. **It is not yet implemented.**

## Working rules

- Smallest coherent change.
- No unrelated refactor.
- Ask before adding dependencies unless the user/task explicitly authorizes them.
- Ask before deleting files; prove dead references.
- Never claim fixed without real verification.
- Show diff/summary after changes.
- Do not commit unless the user's workflow authorizes the checkpoint.
- Never expose/store API keys in source, prompts, logs, evidence, or durable state.

## Cross-platform

Current development machine: Linux Lite / modest hardware.
Product target: native Linux + native Windows.

Do not introduce WSL-only, `/tmp`-only, Bash-only, Unix-signal-only, or hardcoded path assumptions into shared architecture.

## Consult-lane non-negotiables

- Maximum two WebViews in current consultation transport.
- No `blocking_lock()` in async/navigation callback paths.
- Navigation callback uses the established synchronous-channel design; do not casually substitute Tokio mpsc.
- No stale agent/turn acceptance.
- Generic init script remains static/generic.
- Runtime agent identity must not be captured as a stale closure value.
- Every AskUser close path answers; dismissal sends `Cancelled`.
- Exact backend/frontend events and fields must match.
- Check real Rust command return type before writing `invoke<T>()`; many commands return serialized JSON strings.

## Delivery-lane non-negotiables

Read `DELIVERY.md` before any Build change.

Preserve:

- clean repository admission;
- isolated `arena-delivery` worktree;
- original checkout untouched during implementation;
- DSH launched as bounded child, not product authority;
- acceptance authored/frozen before implementation;
- protected acceptance hashes;
- worker cannot self-mark Verified;
- Arena executes frozen checks independently;
- same checks rerun after repair;
- repair attempts bounded;
- `WaitingForUser` persisted before owner UI event;
- restart re-presents persisted question;
- exact-owner abort and child cleanup;
- no secrets in durable/log/evidence data;
- explicit Apply only after verified candidate + clean unchanged original HEAD + non-forcing fast-forward;
- no automatic push/deploy in V1.

Do not add Dagu until the focused Dagu validation passes.

## Reuse philosophy

Do not build generic infrastructure if a mature external component removes more complexity than it introduces.

Do not force standalone software into a plugin role it was not designed for.

Do not preserve old Arena subsystems purely because they already exist.

## Verification

Backend:

```bash
cd src-tauri && cargo check
```

Frontend:

```bash
npm run build
```

Plus task-specific tests/runtime validation and `git diff --check`.

Reliability/Delivery changes require a post-change invariant audit proportional to risk.
