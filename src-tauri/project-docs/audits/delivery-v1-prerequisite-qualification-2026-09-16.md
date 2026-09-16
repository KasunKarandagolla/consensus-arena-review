# Delivery V1 prerequisite and Linux rendering qualification — 2026-09-16

**Branch:** `codex/arena-dev-temp`

**Starting checkpoint:** `cd3f45716d0f93d892a9b7bfac7ef4e5582fea4d`

## Outcome

This block adds the minimum external-DSH prerequisite experience and records a
bounded Linux WebKitGTK qualification attempt. It does not runtime-qualify
the worker or Arena Delivery.

## DSH prerequisite contract

Current source evidence establishes:

- Arena resolves `ARENA_DSH_EXECUTABLE`, falling back to `dsh` on `PATH`.
- The qualified version policy is exact `0.1.5-rc.1`, based on the prior real
  Muse control qualification.
- Arena probes `--version` and `--profile headless --help` with a five-second
  timeout and bounded output handling.
- `get_dsh_prerequisite` returns only safe metadata and an owner-facing setup
  message. It never returns API keys, raw probe output, or package internals.
- `start_delivery` repeats the same check before worktree/session creation.
- Arena does not install, download, package, or mutate DSH/system state.

On this machine, `dsh` was not resolvable from `PATH` and
`ARENA_DSH_EXECUTABLE` was not configured. The real prerequisite result is
therefore unavailable, and no DSH coding run was attempted.

## Linux environment evidence

- X11 session with `DISPLAY=:0.0`; Wayland was unset.
- WebKitGTK 2.50.4 and GTK 3.24.33 were installed.
- Tauri uses `webkit2gtk = 2.0.2` in the Rust source contract.
- OpenGL reports Mesa llvmpipe software rendering.
- `/dev/dri` is absent; EGL reports `DRI2: failed to authenticate`.
- Vite served the expected frontend at `http://localhost:1420/`.
- Native Tauri windows launched, but the Arena WebView surface remained
  blank under the baseline, `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
  `WEBKIT_DISABLE_COMPOSITING_MODE=1`, `LIBGL_ALWAYS_SOFTWARE=1`, and the
  combined flags.
- The flags were tested as development-environment experiments only and were
  not baked into Arena.

The strongest current classification is a Linux graphics/WebKit runtime
blocker in this session. A graphics-capable native session or a separately
reproducible supported software-rendering environment is still required for
the genuine UI gate.

## Verification

- `cargo fmt --check`: passed after formatting.
- `cargo check`: passed; existing warnings only.
- `cargo test dsh_worker::tests`: 5 passed, 0 failed.
- `npm run build`: passed.
- `git diff --check`: passed.

## Programme boundary

The following remain unproven: two independent Muse coding runs, genuine
Arena `UI → worktree → acceptance → implementation → verification → Verified
→ Apply`, Delivery reliability runtime scenarios, Phase 1 memory runtime
qualification, native Windows runtime, and the conditional Dagu gate. The
new prerequisite check is source/test confirmed, not a substitute for any of
those runtime gates.
