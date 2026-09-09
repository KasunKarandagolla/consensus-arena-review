# Consensus Arena — Frontend

## Status

The React/Tauri frontend is substantially built and visually based on:

`src-tauri/project-docs/mockup/preview.html`

For the current reliability phase, frontend work should be limited to behavior that can affect or misrepresent the backend pipeline.

Do not spend pipeline-audit time on visual polish unless a UI path can strand, duplicate, or incorrectly signal session state.

---

## Design Ground Truth

- Native Tauri application.
- React + TypeScript + Zustand + Lucide.
- Blue / Light / Dark themes.
- Local Inter + JetBrains Mono fonts; no runtime font CDN.
- Current production components primarily use CSS variables / inline styles rather than shadcn primitives.
- Main content presents Blueprint sections, not raw model conversations.
- Live status drawer can expose model/session progress without turning the product into a debate dashboard.

`preview.html` remains the approved visual reference unless explicitly replaced.

---

## Pipeline-Relevant Views / Surfaces

### Setup

User selects project/session/participant/leader/brain configuration.

**Semantic correction:** backend setup is now readiness/authentication only.
The historical UI name `PrimingView` / `priming` state may still exist for compatibility, but it must not be interpreted as “setup sends role priming messages”.

First role priming is carried by each model's first useful active task.

### Active session

Relevant actions:

- Stop/abort;
- user input where supported;
- status drawer;
- Blueprint rendering;
- pause/resume controls if exposed;
- AskUser;
- CAPTCHA/rate-limit recovery.

### Connected Accounts

Uses backend browser-window management and provider login/OAuth paths.

Do not assume successful account flow for Claude means provider-neutral OAuth is complete. Manual testing showed Google OAuth remains unreliable for other providers such as GLM/DeepSeek.

### AskUser popup

Must be mounted at an application level where it cannot disappear during view transitions without resolving the backend sender.

Required close behavior:

- option click → `provide_user_answer`;
- custom submit → `provide_user_answer`;
- Escape → cancellation answer;
- backdrop/close → cancellation answer.

### CAPTCHA / challenge overlay

The overlay informs the user that verification must be completed in the provider window.
`captcha_resolved` requests backend re-evaluation; it does not fabricate Ready.

### Rate-limit recovery

UI options must map exactly to backend recovery semantics.
A user retry/continue choice must not cause a second Send if the original model request was already physically accepted.

### Recovery

Distinguish:

- incomplete-session Blueprint replay (`recover_session`);
- paused active-session checkpoint resume (`resume_session`).

The frontend must not present these as the same operation.

---

## Participant Registry

Current backend checkpoint source supports:

- seven built-in participants;
- persisted custom participants;
- merged participant list commands.

The frontend should consume the backend participant registry rather than maintaining an independently drifting hardcoded list where practical.

Built-in baseline IDs:

- chatgpt
- claude
- gemini
- deepseek
- qwen
- glm
- kimi

Kimi baseline source URL at checkpoint `0cc76c9` is `https://kimi.ai/`.

---

## IPC Rules

### Source hierarchy

For a frontend command/listener change, inspect:

1. Rust command/event source.
2. `main.rs` registration.
3. `IPC.md` intended contract.
4. frontend invoke/listen call.

Do not copy return semantics from another command.

### JSON-string return hazard

Many Rust commands return a String containing `serde_json::to_string(...)`.
Frontend must `JSON.parse()` these before treating them as objects/arrays.

Known command families with structured JSON include brain configs, participant/session collections, recovery/checkpoint state, health/memory collections, Hackathon state, and some diagnostics.

Plain-text/Markdown/path-returning commands must not be parsed merely because other commands are.

Before adding any `invoke<T>()`, read the actual Rust return type and implementation.

### Listener cleanup

Every `listen()` registration must be unsubscribed on cleanup/remount.
Audit React StrictMode/dev remount behavior and duplicate event handlers.

---

## Pipeline-Critical Events

Frontend should correctly handle current source events in these families:

### Session/setup

- `session-status`
- `setup-agent-ready`
- `setup-agent-complete`
- `setup-agent-failed`
- `setup-complete`
- `session-checkpoint`
- `session-complete`

`setup-agent-complete` now means browser/setup readiness complete, not “priming message was sent”.

### Agent/browser status

- `agent-state-change`
- `active-turn-state`
- `browser-diagnostic`
- `agent-routing`
- `route_started`
- `boss-message`

### Blueprint/brain

- `blueprint-update`
- `blueprint-section-added`
- `blueprint_emitted`
- `agent_brain_decision_started`
- `agent_brain_decision_failed`
- `agent_brain_decision_fallback`

### Human intervention

- `agent-ask-user`
- `captcha-detected`
- `rate-limit-reached`

### Memory/Hackathon

Use current IPC/source names; do not rely on this file as an exhaustive payload schema.

---

## Diagnostics UX

Routine debugging should prefer the compact Diagnostic Brief rather than rendering/copying giant raw snapshots.

Reason:

- smaller memory footprint;
- smaller clipboard/UI payload;
- less token waste when shared with an AI;
- easier comparison across models.

Advanced raw timeline/export remains available for forensic escalation.

Diagnostics UI must never expose:

- API keys;
- cookie values;
- OAuth codes/tokens;
- raw authentication URLs with sensitive query parameters.

---

## Current Runtime Evidence Relevant to Frontend

### Claude

Recent manual session:

- model window loaded;
- first leader priming/task envelope was injected;
- no re-injection observed;
- response captured.

### Qwen

Recent manual sessions:

- Qwen became ready and accepted first prompts;
- visible response could be produced while backend remained `active_waiting_for_response`;
- when Qwen was leader, Claude participant loaded but was never routed because the pipeline had not captured Qwen's leader response yet;
- loading was no longer falsely labelled login/verification in the successful classifier test, although sparse hydration may still become empty-shell too early.

These are backend/browser pipeline findings, not UI rendering defects.

---

## AskUser Audit Checklist

Astra/frontend audit should verify:

- popup appears for every `agent-ask-user`;
- answer command is sent once;
- Escape/backdrop/close resolve with cancellation;
- Stop while popup open cannot strand backend wait;
- stale event from old session cannot reopen/answer current AskUser;
- listener remount does not create duplicate popup/action handlers.

---

## Stop / Pause / Resume Checklist

Verify:

- Stop maps to real abort semantics;
- active UI returns to correct state on every backend exit path;
- pause/request-pause command naming matches actual source usage;
- resume cannot be invoked twice concurrently;
- a failed resume does not leave UI permanently “running” or “resuming”;
- incomplete-session recovery banner does not imply active-loop resume when it only replays persisted Blueprint content.

---

## Frontend Areas Outside Current Astra Audit Scope

Unless directly connected to pipeline correctness, do not spend high-capability audit budget on:

- pixel-level mockup differences;
- hello animation;
- typography;
- theme polish;
- decorative layout;
- Blueprint Markdown styling;
- historical light-theme bug;
- shadcn/Tailwind cleanup.

---

## File Map — Pipeline Relevant

Use current source rather than treating this as exhaustive.

- `src/App.tsx` — root overlays/listener mounting.
- `src/hooks/useIpcListeners.ts` — Tauri event subscriptions/cleanup.
- `src/stores/useAppStore.ts` — UI/session state.
- `src/components/views/SetupView.tsx` — session setup.
- `src/components/views/PrimingView.tsx` — historical name; verify current readiness semantics.
- `src/components/views/ActiveView.tsx` — active Blueprint/status surface.
- `src/components/shared/InputBar.tsx` — Start/Send/Stop behavior.
- `src/components/overlays/AskUserPopup.tsx` — AskUser channel completion.
- `src/components/overlays/CaptchaOverlay.tsx` — challenge recovery.
- `src/components/overlays/RateLimitOverlay.tsx` — rate-limit recovery.
- `src/panels/SettingsPanel.tsx` — Connected Accounts, brain/prompts/diagnostics.
- `src/panels/MemoryPanel.tsx` — memory controls; only pipeline interactions are current audit scope.

---

## Current Priority

Frontend is not the primary blocker.

The next reliability audit should inspect frontend only where it participates in:

- IPC correctness;
- human recovery;
- AskUser;
- Stop/pause/resume;
- Connected Accounts/OAuth;
- stale/duplicate listener behavior;
- diagnostic safety.

Do not reopen general redesign work until the backend/session pipeline is beta-reliable.
