# 00 — File Inventory (Phase 0.2 raw output)

Generated: 2026-09-06
Commit: a3ab85f (branch forensics/browser-auth-diagnostics, ahead of origin by 2)

## Backend Rust files (excluding target/node_modules)

```
./src-tauri/build.rs
./src-tauri/src/agent_brain.rs              532 lines
./src-tauri/src/agentic_manager.rs           66 lines
./src-tauri/src/blueprint_store.rs          198 lines
./src-tauri/src/browser_backend.rs         6389 lines
./src-tauri/src/browser_harness.rs         1556 lines
./src-tauri/src/capability_registry.rs       40 lines
./src-tauri/src/commands.rs                3011 lines
./src-tauri/src/context_manager.rs          149 lines
./src-tauri/src/db_helpers.rs                61 lines
./src-tauri/src/errors.rs                    72 lines
./src-tauri/src/hackathon.rs               1304 lines
./src-tauri/src/main.rs                     182 lines
./src-tauri/src/memory_store.rs            1474 lines
./src-tauri/src/orchestrator.rs             248 lines
./src-tauri/src/persona_manager.rs           61 lines
./src-tauri/src/proxy_manager.rs             19 lines
./src-tauri/src/resource_monitor.rs          60 lines
./src-tauri/src/response_router.rs         2453 lines
./src-tauri/src/session_runner.rs          1099 lines
./src-tauri/src/session_vault.rs            180 lines
./src-tauri/src/settings_store.rs           348 lines
./src-tauri/src/signals.rs                   65 lines
./src-tauri/src/token_budget.rs              59 lines
./src-tauri/src/transcript_store.rs         235 lines
./src-tauri/src/turn_manager.rs              52 lines
```

Total .rs files (real source, excluding target): **25** (plus build.rs = 26). Comparable BACKEND.md claim must be verified in 07-doc-mismatches.

Total Rust source lines (src-tauri/src only, excluding build.rs): **19,913** — re-verified via `wc -l` sum in session 2 (session 1's ~17,563 was arithmetically inconsistent with its own per-file listing which summed to 19,913; session 2's direct `find ... -exec wc -l` confirms 19,913). With build.rs (3 lines): 19,916. Browser_backend alone 32% of total.

Largest files & depth budget tier:
- Tier 3 (>500 lines, highest scrutiny): browser_backend.rs (6389), commands.rs (3011), response_router.rs (2453), browser_harness.rs (1556), memory_store.rs (1474), hackathon.rs (1304), session_runner.rs (1099), agent_brain.rs (532)
- Tier 2 (150-500): orchestrator.rs (248), transcript_store.rs (235), blueprint_store.rs (198), main.rs (182), session_vault.rs (180), context_manager.rs (149)
- Tier 1 (<150): errors.rs (72), agentic_manager.rs (66), signals.rs (65), db_helpers.rs (61), persona_manager.rs (61), resource_monitor.rs (60), token_budget.rs (59), turn_manager.rs (52), capability_registry.rs (40), proxy_manager.rs (19)

## Frontend files (excluding node_modules/dist/target)

```
./src/App.tsx                                80 lines
./src/components/hackathon/HackathonMiniWindow.tsx 502 lines
./src/components/layout/Sidebar.tsx           34 lines
./src/components/layout/Topbar.tsx            32 lines
./src/components/overlays/AskUserPopup.tsx    13 lines
./src/components/overlays/CaptchaOverlay.tsx   8 lines
./src/components/overlays/RateLimitOverlay.tsx  9 lines
./src/components/shared/DebugPanel.tsx       274 lines
./src/components/shared/InputBar.tsx          47 lines
./src/components/shared/Toast.tsx              4 lines
./src/components/views/ActiveView.tsx         33 lines
./src/components/views/EmptyView.tsx          26 lines
./src/components/views/PrimingView.tsx        21 lines
./src/components/views/SetupView.tsx         101 lines
./src/hooks/useIpcListeners.ts               377 lines
./src/index.css                              234 lines
./src/lib/agents.ts                           92 lines
./src/lib/tauri.ts                            82 lines
./src/lib/theme.ts                            23 lines
./src/lib/utils.ts                             5 lines
./src/main.tsx                                13 lines
./src/panels/MemoryPanel.tsx                 211 lines
./src/panels/SettingsPanel.tsx               538 lines
./src/stores/useAppStore.ts                  305 lines
./src/vite-env.d.ts                            1 line
./vite.config.ts                              <20 lines
```

Total frontend files: **26** (25 src + vite.config.ts). FRONTEND.md File Structure section must be compared.

Total frontend lines: **~2831** (TS/TSX total, per wc) + 234 CSS.

## Root-level files

```
./AGENTS.md
./BETA_RELEASE_COMPREHENSIVE_AUDIT.md   (untracked, prior audit)
./BROWSER_FORENSICS_IMPLEMENTATION_REPORT.md
./BROWSER_RELIABILITY_OBSERVABILITY.md
./components.json
./FRESH_INSTALL_BROWSER_FORENSICS.md
./.gitignore
./HACKATHON_MODE_DESIGN.md
./index.html                            320 bytes, 12 lines — NO font CDN (clean)
./package.json
./package-lock.json
./postcss.config.js
./tailwind.config.js
./tsconfig.json
./tsconfig.node.json
./vite.config.ts
```

`index.html` content verified: no `<link>` to fonts.googleapis.com, only root div + script tag. PASS for font-CDN regression check.

## Config/manifest files

```
./package.json
./src-tauri/Cargo.toml
./src-tauri/tauri.conf.json
./tsconfig.json
./tsconfig.node.json
./vite.config.ts
```

## Line counts note

Depth-budget scaling: Tier 3 files collectively account for ~17,193 of 19,913 Rust lines (86%). Response_router.rs (2453) and browser_backend.rs (6389) alone demand the highest scrutiny per Phase 0 scaling rule — no skimming of back halves permitted.

**Correction note (session 2):** Session 1's ~17,563 total was arithmetically inconsistent with its own per-file listing (which summed to 19,913); re-verified against real `wc -l` output in session 2: **19,913** (19,916 with build.rs).

## Untracked files worth noting

- `BETA_RELEASE_COMPREHENSIVE_AUDIT.md` — prior comprehensive audit, untracked
- `HACKATHON_MODE_DESIGN.md` — design doc
- `src-tauri/project-docs/audits/*` — 8 audit subdirs for previous tasks
- `src-tauri/src/hackathon.rs` + `src/components/hackathon/` — Hackathon Mode implementation, untracked relative to base but present on this branch
- `dist/` — build artifact, excluded
