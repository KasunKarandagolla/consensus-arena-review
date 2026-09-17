# Post-V1 compiler warning and call-graph follow-up

**Date:** 2026-09-17
**Source:** successful `cargo check` after the programme closure source edits.

## Warning inventory

The check completed with **108 warnings**:

| Class | Count | Disposition |
|---|---:|---|
| `dead_code` | 87 | Mostly retired consultation/agent experiments, compatibility helpers, and unused store conveniences. Review by subsystem before removing anything. |
| Unused variables | 11 | Concentrated in the unreachable legacy setup-priming block in `session_runner.rs`. |
| Unused imports | 4 | Scattered across older live modules; no new credential or diagnostics warning was introduced. |
| Unused `mut` | 1 | Existing local binding. |
| Unreachable statement | 1 | The legacy setup-priming block follows an unconditional `continue`; inspect and remove or restore deliberately in a later Consult cleanup. |
| Unreachable pattern | 1 | Redundant exhaustive `SetupCompletionProof` arm. |
| Redundant field patterns | 2 | Style-only `text: text` patterns in `response_router.rs`. |

No dependency deprecation, generated-code, or test-only warning class was
reported by this `cargo check`. No broad warning cleanup was attempted during
release closure.

## Call-graph finding requiring product follow-up

`TranscriptStore::record_turn` and `update_session_status` have no production
callers in current source. Session creation and persisted Delivery state have
separate callers, but no writer was found for consultation `TurnRecord`s.
Therefore `get_transcript` and any export based on it may return no turn
history for ordinary Consult runs. This is a source/call-graph finding; this
sprint did not add transcript persistence or claim session transcript
continuity.

`delivery::transition` also has no production caller; current Delivery
supervision updates phase state directly. Review that helper with the V1
controller before choosing one transition authority.

## Suggested post-V1 work order

1. Decide whether full Consult turn history is still a supported product
   promise. If yes, add one production persistence boundary and lifecycle tests
   for write, reopen, export, and delete; avoid adding a writer in multiple
   router branches.
2. Audit `session_runner.rs`'s unreachable priming block and its associated
   unused bindings. Preserve active priming behavior; remove only proven-dead
   code.
3. Review legacy dead modules (`agentic_manager`, `capability_registry`,
   `persona_manager`, `proxy_manager`, `resource_monitor`, `signals`, and
   `turn_manager`) with reference/call-graph evidence before any deletion.
4. Remove harmless imports, `mut`, redundant patterns, and the unreachable
   match arm in focused Consult maintenance work.
5. Review unused `SessionRuntime` helpers and `delivery::transition` against
   actual ownership/state-machine invariants before changing reliability code.

The compiler warning count is a maintenance signal, not a release qualification
or proof that the listed subsystems are broken.
