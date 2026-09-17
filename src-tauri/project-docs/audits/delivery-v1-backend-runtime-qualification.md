# Delivery V1 backend/runtime qualification — 2026-09-16

**Branch:** `codex/arena-dev-temp`

**Boundary:** These results separate production service/runtime tests from a
native GUI run. No result below is called GUI E2E.

## DSH worker reproducibility

Local Node is `v22.22.2`; npm is `10.9.7`. Registry metadata confirms
`@deepseek-ai/dsh@0.1.5-rc.1` declares `@deepseek-ai/dsh-headless` and
`@deepseek-ai/dsh-base` in the `^0.1.5-rc.1` range. The frozen external contract
also names `@mstar-harness/dsh@3.8.3` and `meta/muse-glimmer-30b`.

The exact documented lockfile SHA256,
`1297ec9257567a85c5a653734979256e6958a2c1235079c62fdc5bb9f2505887`, and its
runtime could not be recovered. pnpm `10.33.0` and `10.33.2` each reconstructed
a different candidate lock with SHA256
`88a26a4d1f31bdffd465bf081f5d02638f0f42982aeb235ff7f4d1365a004773`; that
candidate had 568 package records and 234 DSH rc2 records versus the frozen
audit's 230. A legacy isolated tree returned the top-level version but did not
prove the exact tree/headless execution contract. The mismatched candidate was
not installed or run.

**Muse tasks:** zero of the required two independent runs were started. No
random model was substituted. Therefore DSH/Muse is **not runtime-repeatable**
on current evidence; the earlier one-run Muse result remains historical
single-run evidence. DSH RSS and process-tree RSS were not measured in this
pass. The configured provider credential was only checked for presence and
never printed or passed to a worker.

The upstream [DSH package](https://www.npmjs.com/package/%40deepseek-ai/dsh)
and [headless profile reference](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/bundle/headless/README.md)
describe the current command/profile surface, but do not replace recovery of
the pinned lockfile.

## Production-path Delivery harness

Source now exposes a narrow backend dogfood entry that acquires the real
`SessionRuntime` owner/permit, executes the same Delivery supervisor and
services, persists Delivery/transcript state, and uses real Git, DSH, verifier,
and Apply code. It omits only Tauri `AppHandle` event emission. The model-backed
Rust test is marked ignored unless an approved key/runtime is explicitly
supplied. **That test compiled but was not run.** Thus this pass did not execute
the full sequence:

`admission → worktree → DSH acceptance → acceptance freeze → implementation → verification → Verified → Apply`

through the production Delivery supervisor. No session/receipt/candidate from
that model-backed path is claimed.

The source/test changes close several evidence-boundary hazards:

- all production Delivery Git invocations use a 120-second bounded runner with
  128 KiB per-stream capture; reader/join/capture failures return errors rather
  than empty output;
- verifier receipts correlate `session_id`, `attempt_id`, unique
  `verification_id`, `acceptance_commit`, `candidate_sha`, and frozen
  `profile_hash`;
- each verifier execution writes to its own evidence directory, so Resume does
  not overwrite a previous verification record;
- raw verifier stdout/stderr are discarded after capture; evidence files carry
  byte-count notices only, so command output is not written into app evidence;
- command IDs are case-insensitively unique before evidence filenames are
  created, avoiding case-folding collisions on Windows filesystems;
- the verifier compares candidate HEAD and Git-visible worktree status before
  and after checks; persistent visible mutation fails, but transient
  mutate-then-restore and ignored-file changes are not detected or excluded;
- Verified records the exact candidate commit that earned PASS and performs no
  later commit;
- terminal repair-budget failure is persisted;
- worker modification of protected acceptance is restored from the frozen
  commit and rejects the attempt without rewriting the verifier's own receipt
  verdict.

These are source changes backed by the tests below; they do not imply a
model-backed Delivery run or a Windows process-tree guarantee. Git hooks/helpers
and concurrent user edits may create descendants/races outside the direct-child
timeout boundary. Apply checks were exercised sequentially, not under a
concurrent external editor.

## Real production-component tests executed

The full Rust suite completed:

```text
cargo test --manifest-path src-tauri/Cargo.toml --no-fail-fast
366 passed; 0 failed; 1 ignored; 19.03 s test runtime
```

The ignored case is the DSH-backed Delivery dogfood test described above.
Relevant executed cases include:

- `production_admission_and_worktree_creation_handle_paths_with_spaces` —
  production clean-base and worktree helpers against disposable real Git repos;
- `production_protected_acceptance_restoration_uses_frozen_git_content` —
  detects tampering, restores actual file content from the acceptance commit,
  and checks the restored worktree;
- `production_apply_guards_reject_dirty_moved_and_non_fast_forward_bases` —
  invokes production Apply logic and confirms all three unsafe-base refusals;
- `production_verifier_records_pass_fail_timeout_and_attempt_identity` —
  real verifier child processes yield PASS, exit-7 FAIL, and timeout
  INCONCLUSIVE records with candidate/profile/attempt correlation;
- `production_verifier_fails_when_a_passing_check_mutates_candidate_tree` —
  a child exits successfully after mutating a tracked candidate, and the
  production verifier records FAIL rather than accepting that tree;
- `protected_paths_reject_symlink_identity` — rejects a protected-file
  symlink rather than following a link to equal bytes elsewhere;
- `production_verifier_records_pass_fail_timeout_and_attempt_identity` also
  emits a sentinel on stdout and confirms the evidence file stores only the
  omission notice, not child output;
- `memory_store::tests::phase1_runtime_round_trip_and_repair` — file-backed
  MemoryStore write/search/reopen/export/restore/index-repair exercise;
- the full `SessionRuntime` unit set — ownership/admission/stop/completion state
  semantics. These tests do not prove termination of a real DSH/Node child tree.

The production verifier now resolves Node and npm from the same host PATH
installation (or explicit `NODE_EXE`) outside the candidate worktree, then
checks exact Node `v22.22.2` and npm `10.9.7` before invoking `npm-cli.js`
without a command shell. Candidate-controlled `where.exe`/npm shims are not
used. Windows workflow 35200869335 now passes the Windows-only smoke tests,
including the npm no-shell path, Job Object descendant fixtures, full Rust
tests, and unsigned NSIS packaging. Other `.cmd`/`.bat` verifier tools remain
unqualified. Model-backed Windows DSH and GUI/WebView2 behavior remain
unproven.

The real verifier PASS/FAIL/INCONCLUSIVE cases and real Git checks are
production-component runtime evidence, but they are **not** the complete
Delivery runtime path and do not substitute for real DSH runs.

## Reliability matrix

| Scenario | Evidence/status |
|---|---|
| Worker PASS → Verified | Full worker/supervisor path **not run**; direct production verifier PASS behavior tested |
| FAIL → repair → PASS | **Not run**; no exact DSH worker available |
| INCONCLUSIVE → resume | State-preservation unit test and unique evidence-run IDs; no real persisted supervisor restart/resume |
| Protected acceptance | Production Git restore helper tested; worker-triggered supervisor attempt not run |
| WaitingForUser | No production worker question/answer runtime; harness without UI safely cannot answer |
| Restart recovery | **Not run** as separate Delivery service lifecycle |
| Abort | SessionRuntime ownership tests pass; no DSH child-process kill/wait test |
| Dirty Apply | Production Apply refusal tested |
| Changed HEAD Apply | Production Apply refusal tested |
| Non-fast-forward Apply | Production Apply refusal tested |
| Successful Apply | Exists in the ignored model-backed path only; **not runtime-proven here** |

## Other checks and GUI boundary

- `cargo check --manifest-path src-tauri/Cargo.toml`: passed on the final
  source (107 existing/unrelated warnings remain).
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.
- `npm run build`: passed; generated frontend output totals approximately
  455 kB (about 130 kB gzip for JS+CSS).
- `git diff --check`: passed on the review handoff snapshot.
- Linux host probe: GTK `3.24.33`, WebKitGTK `2.50.4`, `/dev/dri` absent,
  `llvmpipe (LLVM 15.0.7, 128 bits)`. No Vite or Arena process was running
  during the probe. The prior EGL DRI2-authentication and blank-WebView audit
  remains the relevant product GUI evidence. The new host script is
  `/home/kasun/Music/arena/consensus-arena/scripts/qualify-linux-native-runtime.sh`.

The complete reliability result, lockfile blocker, and unrun scenarios are
also indexed in `DELIVERY.md` and `RELIABILITY.md`. Historical audits have not
been modified.
