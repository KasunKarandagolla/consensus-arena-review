# preview.html Redesign Post-Audit

Date: 2026-07-06  
Scope: documentation audit of the completed production React port of
`/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`.

## Files inspected

Documentation read completely:

- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/AGENTS.md`
  (the requested root AGENTS.md was absent before this task)
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/CLAUDE-PROJECT-INSTRUCTIONS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/ARCHITECTURE.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/DECISIONS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/PROCESS.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/FRONTEND.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/PROCESS-md-codex-patch.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/BACKEND.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/IPC.md`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/README.md`

Production frontend inspected:

- `/home/kasun/Music/arena/consensus-arena/src/index.css`
- `/home/kasun/Music/arena/consensus-arena/src/App.tsx`
- `/home/kasun/Music/arena/consensus-arena/src/main.tsx`
- `/home/kasun/Music/arena/consensus-arena/src/stores/useAppStore.ts`
- `/home/kasun/Music/arena/consensus-arena/src/hooks/useIpcListeners.ts`
- `/home/kasun/Music/arena/consensus-arena/src/lib/agents.ts`
- `/home/kasun/Music/arena/consensus-arena/src/lib/tauri.ts`
- `/home/kasun/Music/arena/consensus-arena/src/lib/theme.ts`
- all files under `/home/kasun/Music/arena/consensus-arena/src/components/`
- `/home/kasun/Music/arena/consensus-arena/src/panels/SettingsPanel.tsx`
- `/home/kasun/Music/arena/consensus-arena/index.html`
- `/home/kasun/Music/arena/consensus-arena/src-tauri/src/commands.rs` return
  paths relevant to JSON/plain-string frontend handling
- `/home/kasun/Music/arena/consensus-arena/src-tauri/project-docs/mockup/preview.html`

The tracked frontend diff against HEAD was also inspected. It reports
19 files changed, 404 insertions, and 4,324 deletions. Four new frontend
support files are untracked and therefore are not included in that tracked
shortstat: `Topbar.tsx`, `lib/tauri.ts`, `lib/theme.ts`, and the local font
assets/public directory.

## PASS/FAIL results

### PASS — Templates button removed

`InputBar.tsx` renders only the disabled attachment control and Send/Stop.
No production source contains a Templates button or `layout-template` icon.
The reference mockup still contains the button, so this is an intentional
production divergence.

### PASS — Seven models represented

`lib/agents.ts` defines chatgpt, claude, gemini, deepseek, qwen, glm, and
kimi. Setup participants/leader, sidebar health dots, Settings connected
accounts, priming rows, and active-status rows consume that shared registry
or the selected IDs derived from it.

### PASS — AskUser close paths invoke provide_user_answer

`AskUserPopup.tsx` routes option click, custom button, custom Enter, Escape,
and backdrop dismissal through one guarded `answer()` function. Dismissals
send `"Cancelled"`; the store is cleared in `finally`.

### PASS — JSON-string commands parsed correctly

Current call sites parse JSON-string results for agent brain config,
fallback config, secondary config, agent health, session list/details, and
recovery state. These were cross-checked against `commands.rs` serialization.

### PASS — Plain strings are not JSON.parse'd

`get_prompt_template` values are assigned directly. `export_blueprint`
results are used directly as file paths. Neither is JSON-parsed.

### FAIL — “No backend source changed” cannot be proven from current diff

The aggregate worktree contains existing modifications under
`/home/kasun/Music/arena/consensus-arena/src-tauri/src/` from earlier backend
batches. There is no isolated redesign commit from which to prove historical
file attribution. The redesign implementation inspected here is frontend-
scoped, and this documentation task changed no backend source or dependency,
but the literal claim that the current overall diff has no backend changes is
false and is not recorded as verified.

### PASS — npm run build

Most recent direct run:

```text
> consensus-arena@0.1.0 build
> tsc && vite build
✓ 1707 modules transformed.
✓ built in 1m 37s
```

This was completed after local Inter/JetBrains Mono assets were added, so it
did not emit the earlier missing-font warnings. It was not rerun during this
documentation-only task.

### PASS (recorded result, not rerun) — cargo check

The completed backend/redesign state records `cargo check` as passing with
existing unused/dead-code warnings only. This documentation-only task did not
rerun it and did not edit backend source.

### PASS — git diff --check

Run after documentation edits; exit 0 with no output.

### PASS — visual smoke test

The current frontend was smoke-tested headlessly at 1440×1000. The Settings
panel, sidebar labels, input placeholder, suggestion chips, Save buttons, and
hello presentation rendered correctly with local Inter. Screenshot:
`/tmp/consensus-arena-settings-font-smoke.png`.

The supplied status mentioned 1440×960; that exact viewport was not rerun in
this documentation task, so only the directly reproduced 1440×1000 result is
claimed here.

## Additional verified implementation facts

- Primary, fallback, and secondary brain Settings sections are present.
- Rate-limit decisions include `lighter` and render “Use lighter model”.
- Priming uses `sessionAgentIds` selected during setup, not hardcoded models.
- The production hello uses preview.html's actual Bézier path/control points
  and transforms in inline SVG/CSS. It does not use the Lottie CDN runtime.
  Fixed stroke width and reduced gradient stops are documented differences.
- Local variable Inter and JetBrains Mono files exist under
  `/home/kasun/Music/arena/consensus-arena/public/fonts/`.
- No source or dependency file was changed by this documentation task.
