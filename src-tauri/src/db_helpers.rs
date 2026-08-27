use crate::errors::AgentError;
use std::sync::Arc;
use std::time::Duration;

/// HIGH-5 / HIGH-6 (Task 9): run a blocking (synchronous `rusqlite`) closure
/// on Tokio's dedicated blocking thread pool instead of the async runtime
/// thread, with a short retry/backoff loop for transient failures such as
/// `SQLITE_BUSY` under write contention.
///
/// Shared by every call site that touches `TranscriptStore`, `BlueprintStore`,
/// or `SessionVault` from async code (`commands.rs`, `response_router.rs`,
/// `session_runner.rs`) so the spawn_blocking + retry pattern is written
/// once instead of duplicated per file.
///
/// `op` is invoked via `tokio::task::spawn_blocking` on each attempt — it
/// must be `Fn` (re-callable) rather than `FnOnce` so a failed attempt can
/// be retried with a fresh blocking-thread dispatch. Backoff sleeps between
/// attempts use `tokio::time::sleep` (async, non-blocking) since control
/// has returned to the async caller between `spawn_blocking` calls.
///
/// Note: this crate's `AgentError::DatabaseError` does not currently carry
/// the underlying `rusqlite::Error` variant, so this helper cannot detect
/// "is this specifically SQLITE_BUSY" — it retries any `DatabaseError` up
/// to 3 total attempts. That's a deliberately conservative approximation:
/// retrying a genuinely permanent DB error a couple of extra times costs
/// ~150ms total and changes nothing; retrying a transient busy/lock error
/// is exactly the behaviour HIGH-6 asks for.
pub async fn run_blocking<T, F>(op: F) -> Result<T, AgentError>
where
    F: Fn() -> Result<T, AgentError> + Send + Sync + 'static,
    T: Send + 'static,
{
    let op = Arc::new(op);
    let mut last_err: Option<AgentError> = None;

    for attempt in 0..3u32 {
        let op = op.clone();
        match tokio::task::spawn_blocking(move || op()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) => {
                last_err = Some(e);
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_millis(50 * (attempt as u64 + 1))).await;
                }
            }
            Err(join_err) => {
                // The blocking task itself panicked — retrying won't help.
                return Err(AgentError::DatabaseError(format!(
                    "blocking database task panicked: {}",
                    join_err
                )));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        AgentError::UnknownError(
            "run_blocking: exhausted retries with no error recorded".to_string(),
        )
    }))
}
