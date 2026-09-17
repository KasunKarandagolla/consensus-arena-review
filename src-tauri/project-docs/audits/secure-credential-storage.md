# Secure credential storage qualification — 2026-09-17

## Implementation

Agent Brain primary, fallback, and secondary API keys, plus Hackathon model
keys, now pass through the `CredentialStore` boundary in
`/home/kasun/Music/arena/consensus-arena/src-tauri/src/credentials.rs`.
Production uses pinned `keyring = 4.2.0` with its v1 platform backends: Windows
Credential Manager and Linux Secret Service. Arena adds no bundled-key
encryption and does not move plaintext to another application file.

Test code injects an in-memory credential store. Native round-trip tests use
only generated synthetic values and delete them after verification; Linux's
test is ignored in ordinary tests because it requires an active Secret
Service session. The Windows native round-trip runs in the focused Windows
test workflow.

## Migration safety

At startup, the settings store detects legacy direct Agent Brain keys and keys
embedded in Hackathon JSON. For each key it:

1. reads any existing OS-store value;
2. writes the legacy secret and reads it back for equality;
3. removes SQLite rows/JSON secret fields with `secure_delete` and a
   transaction, then attempts a best-effort `VACUUM`;
4. stores only non-secret migration metadata.

If secure storage is unavailable or read-back fails, existing SQLite data is
retained, prior vault values are restored when possible, and the application
sets a visible unavailable/pending state. The legacy value is retained for a
later migration retry. Any fallback read from that retained row leaves
migration pending, and session start, Delivery, and exports are blocked until
the OS-store copy is validated. A missing OS-store entry with a retained row
also marks migration pending and refuses that read. Migration errors do not
log secret values. A failure after the vault write but during SQLite cleanup
can leave both copies temporarily; it does not silently discard the original
copy.

Saving a blank Agent Brain key retains the saved credential. Settings provides
explicit primary/fallback/secondary removal. Deleting a Hackathon model removes
its OS-store entry; empty round-trips for an existing model preserve its key.

## Renderer, export, and diagnostic boundary

- Primary/fallback/secondary config serialization always emits `api_key: ""`
  and `api_key_configured`; it never serializes the stored key.
- `get_hackathon_config` returns the existing API-key-free safe projection.
- Diagnostic snapshots reduce Agent Brain configuration to presence booleans
  and redact current credential literals from retained browser evidence before
  serialization is returned. Diagnostic briefs apply the same exact-value
  redaction. Safe-DOM URLs are also redacted at retention time, including
  authentication query values and fragments.
- Memory export is written to a temporary sibling staging file, scanned, then
  renamed into place. Memory restore scans both the input and its pre-restore
  backup. Ordinary error paths attempt explicit cleanup and report a possible
  leftover path if cleanup fails; guard destructors retry and log an OS error
  without logging file contents. A process crash or forced termination can
  still leave a temporary file. Diagnostics use the same guarded cleanup and
  scan before returning. Session/blueprint export does not serialize the
  settings store.
- Delivery scans current primary, fallback, secondary, and Hackathon keys in
  worker results, staged files, committed trees, and frozen verifier metadata
  before candidate verification. Matches block candidate admission and cause
  the worktree/index to be reset.
- Credential matching checks the currently configured raw value and its
  JSON-escaped string representation. Memory's write filter is heuristic, and
  prior memory containing a rotated or removed key cannot be identified from
  current settings alone.
- Git linked worktrees share the original repository's object database. A
  rejected credential-bearing blob may remain as an unreachable local Git
  object after Arena resets the candidate. The scanner prevents the blob from
  reaching verification or Apply; it does not erase shared objects. Isolated
  candidate object storage remains a release-security requirement.
- Diagnostics are owner-triggered after Maintenance mode is enabled and remain
  local. The exported bundle contains browser activity evidence; it does not
  include dated app log files or Delivery receipts. Arena checks the bundle
  against currently configured credential values before retaining it. Up to
  five matching export directories are targeted for retention, with pruning
  when a new export starts; the app does not track whether each directory is a
  valid bundle. Dated app logs are pruned after 14 days, and the rolling
  appender caps retained daily log files at 15. There is no automatic upload
  or telemetry, and individual log files do not currently have a hard
  byte-size cap.

The Settings panel reports the app-data path in the diagnostic brief and
surfaces the export directory after it is written. If explicit cleanup fails,
the command reports that a partial artifact may remain and the guard retries
on drop. A process crash can still leave a temporary artifact. Credential
store error messages are generic and do not include key values.

## Verification

The focused source tests cover legacy migration for all key families,
secure-store failure/preservation, partial migration rollback, raw serialized
config exclusion, raw SQLite-file sentinel exclusion after successful
migration, blank-key preservation, explicit removal, and Hackathon model
deletion. `MemoryCredentialStore` keeps unit tests independent of desktop
keyring state.

Native platform outcomes for this checkpoint:

- Linux Secret Service: **PASS** — the ignored native round-trip test stored,
  read back, and removed a generated synthetic credential in the available
  Secret Service session.
- Windows Credential Manager: **pending native Windows workflow**.
- Provider/API credential used: **no**.

This source implementation and mock-backed tests are not by themselves a
cross-platform secure-storage qualification. Release readiness must keep each
native platform result separate.
