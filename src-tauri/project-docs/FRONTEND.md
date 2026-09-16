# Consensus Arena — Frontend Guide

## Current baseline

The production frontend is React + TypeScript inside the Tauri application and was rebuilt from the project's `preview.html` reference. That visual baseline remains in force unless the user explicitly approves a redesign.

The product direction has changed, but this does **not** imply a greenfield frontend rewrite.

## UX philosophy

Arena should remain:

- simple;
- calm;
- familiar;
- product-focused;
- progressively disclosed;
- closer to ChatGPT/Grok interaction patterns than a control room.

Do not expose every worker, process, retry, queue, token count, or subsystem on the main screen.

The primary questions the UI should answer are:

1. What are we trying to achieve?
2. What is Arena doing now?
3. Does the owner need to decide anything?
4. What outcome/evidence exists?

## Two user modes

### Consult

Existing consultation experience remains available for expert reasoning/blueprint/advice.

### Build / Delivery

Build setup reads DSH prerequisite state and the saved Agent Brain's required
API key, base URL, and model. Missing primary configuration is surfaced beside
the existing Agent Brain controls; a rejected backend start must leave the
owner on setup rather than switching to an empty Build progress surface. This
is setup/error UX coverage only, not a native GUI runtime proof.

The new lane should present product progress at a high level, for example:

- preparing acceptance;
- implementing;
- verifying;
- repairing;
- waiting for your decision;
- verified;
- cancelled/failed.

Exact labels/events must match current source rather than this conceptual list.

Do not turn Build into an IDE. Technical evidence may be inspectable behind a details surface.

## Existing frontend foundations to preserve

- collapsible left sidebar/session history;
- shared top bar;
- main content area;
- setup/new-session experience;
- active-session status surface;
- Settings panel;
- AskUser modal;
- overlays/toasts;
- Blue / Light / Dark themes;
- local Inter + JetBrains Mono fonts;
- current hello animation implementation;
- Zustand state management;
- Tauri event listeners with cleanup.

## AskUser rule

Every UI path that dismisses an owner question must answer the backend:

- option click;
- custom submit;
- Escape;
- backdrop/close.

Dismissal currently uses `"Cancelled"` and must not leave a backend waiter hanging.

Build/Delivery may persist its question before emitting the modal so restart can re-present it.

## Build-lane presentation

The post-implementation audit proves frontend Build/status wiring compiles, but the audit is not a pixel/UI inventory. When editing Build UI:

1. read the current React source;
2. identify the actual Build entry/progress state already present;
3. extend minimally;
4. keep Consult and Build distinguishable without duplicating the entire app shell.

## Settings direction

Settings currently include connected consumer accounts and programmatic brain/model configuration. Future worker/workflow configuration should be hidden unless the user actually needs it.

Do not require a nontechnical user to understand DSH, Dagu, test runners, worktrees, or process topology during normal use.

## IPC parsing risk

This project has repeatedly shipped frontend bugs by assuming `invoke<T>()` returned a parsed object when the Rust command actually returned `serde_json::to_string`.

Before every new/changed invoke call:

- inspect real Rust return type;
- parse serialized JSON strings explicitly;
- do not parse commands that intentionally return plain text.

See `IPC.md` and `AGENTS.md`.

## Styling

Current real source uses CSS variables and component styles derived from the implemented mockup. Do not reintroduce external font CDNs or casually replace the design system with installed-but-unused UI libraries.

## Verification

For frontend-affecting work:

```bash
npm run build
```

from the actual project root/package location, plus real Tauri dev/runtime testing when behavior depends on native IPC or process integration.
