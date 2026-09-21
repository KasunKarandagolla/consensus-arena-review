# Consensus Arena — Frontend Guide

## Current release closure — 2026-09-17

The Setup view displays the DSH prerequisite result and primary Agent Brain
configuration requirement before Build can start. Current DSH installs pass
version/root-help checks but time out on the headless profile check, so the
UI stays blocked at setup; the owner sees plain status text with technical
probe details behind a disclosure. Build launch failures use product-language
guidance with the sanitized command error available as optional details. A
model-backed run was not substituted or claimed.
Agent Brain inputs return an empty key plus `api_key_configured`; a blank save
keeps the saved OS credential, and Settings offers explicit removal. An
unavailable or pending OS credential-store migration is shown as a setup
status. Current key-storage details and qualification boundaries are in
`audits/secure-credential-storage.md`.

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

The current Delivery surface includes a minimal founder journey summary with
four broad steps: Intent, Build, Verify, and Apply. It is a status aid, not a
second workflow state machine. Failure and cancellation remain visibly
terminal, while evidence and technical identifiers stay in the existing
details/status areas.

The production Product OS entry now starts through `start_product_project`.
`DeliveryView` polls the coordinator status and shows the broader
Discover → Decide → Deliver → Release progression, including evidence,
ambiguity/owner-question state, current package readiness, and Delivery
verification. It uses the existing calm visual language and keeps OpenCode
sessions, worktree paths, and raw tool events out of the primary owner view.
The owner question is answered through the semantic backend command; the
renderer cannot mutate ProductAuthorityRecords or gate input. M07 proves the
backend production run and frontend build, not native GUI runtime behavior on
the current graphics-limited host.

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
