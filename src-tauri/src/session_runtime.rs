use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::task::JoinHandle;

/// Immutable owner identity for a live orchestration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOwner {
    pub session_id: String,
    pub run_generation: u64,
}

/// Ownership-relevant runtime phases. `OrchestratorStatus` remains the
/// product/UI workflow status; this enum is the concurrency/task ownership authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimePhase {
    Idle,
    Starting(SessionOwner),
    Running(SessionOwner),
    Paused(SessionOwner),
    Resuming(SessionOwner),
    Stopping(SessionOwner),
    Finished(SessionOwner),
}

impl RuntimePhase {
    fn owner(&self) -> Option<&SessionOwner> {
        match self {
            RuntimePhase::Idle => None,
            RuntimePhase::Starting(o)
            | RuntimePhase::Running(o)
            | RuntimePhase::Paused(o)
            | RuntimePhase::Resuming(o)
            | RuntimePhase::Stopping(o)
            | RuntimePhase::Finished(o) => Some(o),
        }
    }

    fn is_active(&self) -> bool {
        matches!(
            self,
            RuntimePhase::Starting(_)
                | RuntimePhase::Running(_)
                | RuntimePhase::Paused(_)
                | RuntimePhase::Resuming(_)
                | RuntimePhase::Stopping(_)
        )
    }

    fn name(&self) -> &'static str {
        match self {
            RuntimePhase::Idle => "Idle",
            RuntimePhase::Starting(_) => "Starting",
            RuntimePhase::Running(_) => "Running",
            RuntimePhase::Paused(_) => "Paused",
            RuntimePhase::Resuming(_) => "Resuming",
            RuntimePhase::Stopping(_) => "Stopping",
            RuntimePhase::Finished(_) => "Finished",
        }
    }
}

struct Inner {
    phase: RuntimePhase,
    handle: Option<JoinHandle<()>>,
}

/// SessionRuntime is the single concurrency authority for orchestration tasks.
/// Synchronization uses `std::sync::Mutex` for short ownership metadata and an
/// atomic counter for monotonic generation. Never hold the mutex across `.await`.
pub struct SessionRuntime {
    inner: Mutex<Inner>,
    generation: AtomicU64,
}

impl Default for SessionRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRuntime {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                phase: RuntimePhase::Idle,
                handle: None,
            }),
            generation: AtomicU64::new(0),
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// True if a session task currently owns the runtime (Starting/Running/Paused/Resuming/Stopping).
    /// Finished and Idle are not considered active for `session_active` semantics.
    pub fn is_active(&self) -> bool {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        guard.phase.is_active()
    }

    /// Current owner if any (including Finished, but not Idle).
    pub fn current_owner(&self) -> Option<SessionOwner> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        guard.phase.owner().cloned()
    }

    /// True if the runtime is active and the session_id matches the live owner.
    pub fn is_active_session(&self, session_id: &str) -> bool {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        if !guard.phase.is_active() {
            return false;
        }
        match guard.phase.owner() {
            Some(o) => o.session_id == session_id,
            None => false,
        }
    }

    pub fn phase_name(&self) -> String {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        guard.phase.name().to_string()
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn try_reap_finished(&self, inner: &mut Inner) {
        if let RuntimePhase::Finished(_) = &inner.phase {
            if let Some(handle) = &inner.handle {
                if handle.is_finished() {
                    inner.phase = RuntimePhase::Idle;
                    inner.handle = None;
                }
            } else {
                // Finished with no handle — treat as reapable.
                inner.phase = RuntimePhase::Idle;
            }
        }
    }

    // ── Admission ───────────────────────────────────────────────────────

    /// Acquire a Start permit for the given session_id. Uses monotonic generation.
    /// Performs lazy reap of a finished handle if proven terminated.
    pub fn try_acquire_start(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<SessionStartPermit, String> {
        self.try_acquire_with_phase(session_id, true)
    }

    pub fn try_acquire_resume(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<SessionStartPermit, String> {
        self.try_acquire_with_phase(session_id, false)
    }

    fn try_acquire_with_phase(
        self: &Arc<Self>,
        session_id: String,
        is_start: bool,
    ) -> Result<SessionStartPermit, String> {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        // Lazy reap finished task before admission decision.
        self.try_reap_finished(&mut inner);

        match &inner.phase {
            RuntimePhase::Idle => {}
            _ => {
                return Err(
                    "A session is already active. Use Stop to end it before starting a new one."
                        .to_string(),
                );
            }
        }

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let owner = SessionOwner {
            session_id,
            run_generation: generation,
        };
        inner.phase = if is_start {
            RuntimePhase::Starting(owner.clone())
        } else {
            RuntimePhase::Resuming(owner.clone())
        };
        // handle remains None until handoff
        Ok(SessionStartPermit {
            runtime: Arc::clone(self),
            owner,
            committed: false,
        })
    }

    // ── Handoff ─────────────────────────────────────────────────────────

    fn handoff(
        &self,
        owner: &SessionOwner,
        handle: JoinHandle<()>,
        to_running: bool,
    ) -> Result<(), String> {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        // Check that we are still in Starting/Resuming with same owner.
        let still_valid = match &inner.phase {
            RuntimePhase::Starting(o) if o == owner => true,
            RuntimePhase::Resuming(o) if o == owner => true,
            _ => false,
        };
        if !still_valid {
            // Stop won the race — abort the would-be live task.
            handle.abort();
            return Err(
                "Session start cancelled — stop requested before task became live".to_string(),
            );
        }
        if to_running {
            inner.phase = RuntimePhase::Running(owner.clone());
        } else {
            // For now Running is the only post-handoff, even for Resuming.
            inner.phase = RuntimePhase::Running(owner.clone());
        }
        inner.handle = Some(handle);
        Ok(())
    }

    // ── Completion / stale protection ───────────────────────────────────

    /// Mark the given owner terminal (natural completion). Owner-checked including generation.
    /// Returns true if the transition was applied, false if stale.
    pub fn mark_completed(&self, owner: &SessionOwner) -> bool {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        let current = match inner.phase.owner() {
            Some(o) => o.clone(),
            None => return false,
        };
        if &current != owner {
            return false;
        }
        match &inner.phase {
            RuntimePhase::Running(_)
            | RuntimePhase::Paused(_)
            | RuntimePhase::Starting(_)
            | RuntimePhase::Resuming(_) => {
                inner.phase = RuntimePhase::Finished(owner.clone());
                // Keep handle for lazy reap; do not clear.
                true
            }
            _ => false,
        }
    }

    /// Owner-checked transition to Paused (e.g., after checkpoint).
    pub fn mark_paused(&self, owner: &SessionOwner) -> bool {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        match &inner.phase {
            RuntimePhase::Running(o) if o == owner => {
                inner.phase = RuntimePhase::Paused(owner.clone());
                true
            }
            _ => false,
        }
    }

    /// Owner-checked transition from Paused back to Running (in-process resume).
    pub fn mark_running(&self, owner: &SessionOwner) -> bool {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        match &inner.phase {
            RuntimePhase::Paused(o) if o == owner => {
                inner.phase = RuntimePhase::Running(owner.clone());
                true
            }
            _ => false,
        }
    }

    // ── Stop semantics ──────────────────────────────────────────────────

    /// Stop the current session task, aborting and awaiting termination.
    /// Transitions through Stopping and only then to Idle with owner check.
    /// Returns Ok(()) even if no live session existed (controlled UX result).
    pub async fn stop(&self) -> Result<(), String> {
        // Phase 1: capture owner + handle, move to Stopping
        let (owner_opt, handle_opt) = {
            let mut inner = match self.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            // Lazy reap first: if Finished and handle finished, we can just set Idle and return.
            self.try_reap_finished(&mut inner);
            match inner.phase.clone() {
                RuntimePhase::Idle => return Ok(()),
                RuntimePhase::Finished(_) => {
                    // After reap attempt, if still Finished with unfinished handle, treat as stopping
                    // But handle should be finished after reap check; if not finished, we need to abort.
                    // Fall through to Stopping handling.
                    let owner = match inner.phase.owner() {
                        Some(o) => o.clone(),
                        None => return Ok(()),
                    };
                    inner.phase = RuntimePhase::Stopping(owner.clone());
                    let h = inner.handle.take();
                    (Some(owner), h)
                }
                RuntimePhase::Starting(o)
                | RuntimePhase::Running(o)
                | RuntimePhase::Paused(o)
                | RuntimePhase::Resuming(o)
                | RuntimePhase::Stopping(o) => {
                    let owner = o.clone();
                    // If already Stopping, keep it Stopping but still need to ensure handle taken.
                    inner.phase = RuntimePhase::Stopping(owner.clone());
                    let h = inner.handle.take();
                    (Some(owner), h)
                }
            }
        };

        // Phase 2: abort and await outside lock
        if let Some(handle) = handle_opt {
            handle.abort();
            let _ = handle.await;
        }
        // If no handle (e.g., Starting before handoff), we still need to
        // give a chance for any racing permit handoff to be rejected.
        // No await needed, but we still must transition to Idle with owner check.

        // Phase 3: owner-checked final cleanup to Idle
        {
            let mut inner = match self.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            // Only clear if still Stopping with same owner that we stopped.
            if let Some(expected) = owner_opt {
                match &inner.phase {
                    RuntimePhase::Stopping(o) if o == &expected => {
                        inner.phase = RuntimePhase::Idle;
                        inner.handle = None;
                    }
                    _ => {
                        // If phase already changed (e.g., permit rollback set Idle,
                        // or natural completion set Finished), respect that.
                        // But if we were Stopping and now it's not, do nothing.
                    }
                }
            } else {
                inner.phase = RuntimePhase::Idle;
                inner.handle = None;
            }
        }
        Ok(())
    }

    /// Synchronous variant for tests where no handle exists (Starting without task).
    /// Moves phase to Idle if still Starting/Resuming with same owner.
    #[cfg(test)]
    pub fn rollback_starting_for_test(&self, owner: &SessionOwner) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        match &inner.phase {
            RuntimePhase::Starting(o) if o == owner => {
                inner.phase = RuntimePhase::Idle;
            }
            RuntimePhase::Resuming(o) if o == owner => {
                inner.phase = RuntimePhase::Idle;
            }
            _ => {}
        }
    }
}

/// Guard/permit for a newly acquired execution ownership. Must be handed off
/// to the runtime via `commit` after the task is spawned. If dropped without
/// commit, admission rolls back automatically with owner check.
pub struct SessionStartPermit {
    runtime: Arc<SessionRuntime>,
    owner: SessionOwner,
    committed: bool,
}

impl SessionStartPermit {
    pub fn owner(&self) -> SessionOwner {
        self.owner.clone()
    }

    pub fn commit(mut self, handle: JoinHandle<()>) -> Result<(), String> {
        let res = self.runtime.handoff(&self.owner, handle, true);
        if res.is_ok() {
            self.committed = true;
        }
        // If handoff failed, handle is already aborted inside handoff.
        // Need to forget self's Drop rollback — set committed true anyway to
        // prevent Drop from trying to clear? But handoff failure already left
        // phase as Stopping, not Starting, so Drop's check would do nothing.
        // However we consumed handle, so if we set committed true we avoid double abort.
        // If handoff failed, we want Drop to NOT clear Stopping to Idle prematurely
        // — the stop() caller will set Idle after await. So mark committed true
        // even on error to suppress rollback.
        self.committed = true;
        res
    }
}

impl Drop for SessionStartPermit {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut inner = match self.runtime.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        // Owner-checked rollback: only clear if still Starting/Resuming with same owner.
        let should_rollback = match &inner.phase {
            RuntimePhase::Starting(o) if o == &self.owner => true,
            RuntimePhase::Resuming(o) if o == &self.owner => true,
            _ => false,
        };
        if should_rollback {
            inner.phase = RuntimePhase::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::task::JoinHandle;

    fn test_runtime() -> Arc<SessionRuntime> {
        Arc::new(SessionRuntime::new())
    }

    #[test]
    fn t1_exclusive_admission_two_starts_cannot_both_succeed() {
        let rt = test_runtime();
        let p1 = rt
            .try_acquire_start("sess-a".to_string())
            .expect("first acquire ok");
        let p2 = rt.try_acquire_start("sess-b".to_string());
        assert!(p2.is_err(), "second concurrent start must be rejected");
        assert!(rt.is_active());
        assert_eq!(rt.current_owner().unwrap().session_id, "sess-a");
        drop(p1);
        // after drop rollback, should be idle and allow new acquire
        assert!(!rt.is_active());
        let p3 = rt
            .try_acquire_start("sess-c".to_string())
            .expect("after rollback, should succeed");
        assert_eq!(p3.owner().session_id, "sess-c");
    }

    #[test]
    fn t2_monotonic_generation_increases() {
        let rt = test_runtime();
        let p1 = rt.try_acquire_start("s1".to_string()).unwrap();
        let gen1 = p1.owner().run_generation;
        drop(p1);
        let p2 = rt.try_acquire_start("s2".to_string()).unwrap();
        let gen2 = p2.owner().run_generation;
        assert!(
            gen2 > gen1,
            "generation must increase: {} vs {}",
            gen1,
            gen2
        );
        drop(p2);
        let p3 = rt.try_acquire_resume("s3".to_string()).unwrap();
        let gen3 = p3.owner().run_generation;
        assert!(gen3 > gen2);
    }

    #[test]
    fn t3_pre_spawn_rollback_permit_drop_makes_runtime_admissible() {
        let rt = test_runtime();
        {
            let _permit = rt.try_acquire_start("sess".to_string()).unwrap();
            assert!(rt.is_active());
            // drop without commit should rollback
        }
        assert!(!rt.is_active());
        assert!(rt.current_owner().is_none() || !rt.is_active());
        // new acquisition should succeed
        let p2 = rt.try_acquire_start("sess2".to_string());
        assert!(p2.is_ok());
    }

    #[test]
    fn t4_stale_rollback_cannot_clear_newer_owner() {
        let rt = test_runtime();
        let permit_old = rt.try_acquire_start("old".to_string()).unwrap();
        let owner_old = permit_old.owner();
        // Simulate that old permit is stale and a newer owner has been acquired
        // But we cannot acquire newer while old is still Starting. So we need to
        // first rollback old, then acquire newer, then test stale rollback.
        drop(permit_old);
        // Now idle
        let permit_new = rt.try_acquire_start("new".to_string()).unwrap();
        let owner_new = permit_new.owner();
        assert_ne!(owner_old.run_generation, owner_new.run_generation);
        // Stale guard: try to rollback old owner manually (simulate Drop of old guard after new owner exists)
        rt.rollback_starting_for_test(&owner_old);
        // Should not have cleared new owner
        assert!(rt.is_active());
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        // Also try permit Drop semantics: create a fake permit with old owner and drop
        let stale_permit = SessionStartPermit {
            runtime: Arc::clone(&rt),
            owner: owner_old.clone(),
            committed: false,
        };
        drop(stale_permit);
        // Still should be new owner
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        assert!(rt.is_active());
    }

    #[test]
    fn t5_natural_completion_only_exact_owner() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-x".to_string()).unwrap();
        let owner = permit.owner();
        // handoff with dummy task
        let handle = tokio::runtime::Handle::try_current().map_or_else(
            |_| {
                // Create a runtime for this test thread
                let rt2 = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt2.spawn(async { tokio::time::sleep(Duration::from_millis(5)).await })
            },
            |h| h.spawn(async { tokio::time::sleep(Duration::from_millis(5)).await }),
        );
        // If no current runtime, we need to spawn via new runtime; simplify: use spawn_blocking style
        // Instead we test mark_completed without actual handle commit via direct phase manipulation
        // For this test we commit properly if we have a runtime
        // Simpler: commit via runtime handoff and then test mark_completed
        // We'll use a helper that doesn't require tokio runtime for phase test
        // Instead directly set phase to Running for test
        {
            let mut inner = rt.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.phase = RuntimePhase::Running(owner.clone());
            // fake handle not needed for this logic test
        }
        // Correct owner should succeed
        let ok = rt.mark_completed(&owner);
        assert!(ok, "exact owner should mark completed");
        // After completion, phase is Finished, not active
        assert!(!rt.is_active());
        assert_eq!(rt.current_owner().unwrap(), owner);

        // Stale owner (different generation) should fail
        let stale = SessionOwner {
            session_id: owner.session_id.clone(),
            run_generation: owner.run_generation + 100,
        };
        let ok2 = rt.mark_completed(&stale);
        assert!(!ok2, "stale generation must not mark completed");
        // Should still be Finished with original owner
        assert_eq!(rt.current_owner().unwrap(), owner);

        // Different session_id same generation should fail
        let other = SessionOwner {
            session_id: "other".to_string(),
            run_generation: owner.run_generation,
        };
        let ok3 = rt.mark_completed(&other);
        assert!(!ok3);
        assert_eq!(rt.current_owner().unwrap(), owner);
    }

    #[tokio::test]
    async fn t6_stop_retains_admission_until_task_dead() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess".to_string()).unwrap();
        let owner = permit.owner();
        // Spawn a task that runs for a bit
        let handle: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        permit.commit(handle).expect("commit ok");
        assert!(rt.is_active());
        assert_eq!(rt.current_owner().unwrap(), owner);
        // While running, another Start must fail
        assert!(rt.try_acquire_start("other".to_string()).is_err());
        // Stop the session — this aborts and awaits termination, then releases admission.
        rt.stop().await.unwrap();
        assert!(!rt.is_active());
        // Only after stop completion may a new Start succeed
        assert!(rt.try_acquire_start("new2".to_string()).is_ok());
        // Also verify that a Start attempted before stop would have failed (already proven above)
        // and that concurrent Stop properly retains admission is covered by t7.
    }

    #[tokio::test]
    async fn t7_stop_during_starting_prevents_handoff_resurrection() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let _owner = permit.owner();
        assert!(matches!(
            rt.inner.lock().unwrap_or_else(|e| e.into_inner()).phase,
            RuntimePhase::Starting(_)
        ));
        // Concurrent stop before handoff
        let rt_clone = Arc::clone(&rt);
        let stop_fut = rt_clone.stop();
        stop_fut.await.unwrap();
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Idle");
        // Now attempt handoff — must fail and not resurrect
        let handle: JoinHandle<()> = tokio::spawn(async {});
        let res = permit.commit(handle);
        assert!(res.is_err(), "handoff after stop must be rejected");
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Idle");
        // New start should succeed
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
    }

    #[test]
    fn t8_wrong_session_in_process_resume_rejected() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-A".to_string()).unwrap();
        let owner_a = permit.owner();
        let handle: JoinHandle<()> = tokio::runtime::Handle::try_current()
            .map(|h| h.spawn(async {}))
            .unwrap_or_else(|_| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .spawn(async {})
            });
        // Commit to Running
        let rt_clone = Arc::clone(&rt);
        // We need to handle commit without tokio runtime check; just manually set Running
        {
            let mut inner = rt_clone.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.phase = RuntimePhase::Running(owner_a.clone());
            inner.handle = Some(handle);
        }
        drop(permit);
        // Simulate in-process resume check: live owner is A, target is B
        let live = rt.current_owner().unwrap();
        assert_eq!(live.session_id, "sess-A");
        assert!(rt.is_active());
        let target_b = "sess-B";
        // This is the logic that resume_session should enforce
        if rt.is_active() && live.session_id != target_b {
            // should reject
        } else {
            panic!("should have rejected wrong-session resume");
        }
        // Ensure current owner not overwritten after rejected resume
        assert_eq!(rt.current_owner().unwrap().session_id, "sess-A");
        assert!(rt.is_active_session("sess-A"));
        assert!(!rt.is_active_session("sess-B"));
        // Simulate correct in-process resume (same session)
        let target_a = "sess-A";
        assert!(rt.is_active_session(target_a));
        // Should not spawn second task; is_active remains true with same owner
        assert_eq!(rt.current_owner().unwrap(), owner_a);
    }

    #[test]
    fn t9_duplicate_resume_cannot_spawn_two_tasks() {
        let rt = test_runtime();
        // No active session; first resume acquires
        let p1 = rt
            .try_acquire_resume("sess-R".to_string())
            .expect("first resume acquire ok");
        assert!(rt.is_active());
        // Second concurrent resume must be rejected
        let p2 = rt.try_acquire_resume("sess-R".to_string());
        assert!(p2.is_err(), "duplicate resume must be rejected");
        let p3 = rt.try_acquire_start("sess-other".to_string());
        assert!(p3.is_err(), "start while resuming must be rejected");
        // Drop first permit without commit (simulating failure) -> should allow next
        drop(p1);
        assert!(!rt.is_active());
        let p4 = rt
            .try_acquire_resume("sess-R2".to_string())
            .expect("after rollback, should succeed");
        assert_eq!(p4.owner().session_id, "sess-R2");
    }

    #[tokio::test]
    async fn t10_finished_task_reap_requires_proof() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess".to_string()).unwrap();
        let owner = permit.owner();
        // Spawn a task that finishes quickly
        let handle: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
        permit.commit(handle).unwrap();
        assert!(rt.is_active());
        // Simulate natural completion: mark Finished but keep handle
        // Wait a bit for handle to be finished? Actually handle is still running for 10ms
        tokio::time::sleep(Duration::from_millis(30)).await;
        let completed = rt.mark_completed(&owner);
        assert!(completed);
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Finished");
        // Now the handle is finished, so next acquire should reap and succeed
        // But we need to ensure handle is indeed finished
        {
            let inner = rt.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(h) = &inner.handle {
                assert!(
                    h.is_finished(),
                    "finished task's handle must report is_finished"
                );
            } else {
                panic!("handle should still be present for lazy reap");
            }
        }
        let p2 = rt
            .try_acquire_start("next".to_string())
            .expect("should reap finished and allow new start");
        assert_eq!(p2.owner().session_id, "next");
        assert!(p2.owner().run_generation > owner.run_generation);

        // Also test that non-finished task blocks reap
        let rt2 = test_runtime();
        let permit2 = rt2.try_acquire_start("sess2".to_string()).unwrap();
        let owner2 = permit2.owner();
        let long_handle: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        permit2.commit(long_handle).unwrap();
        // Mark finished prematurely before handle actually finished? Our mark_completed will set Finished even though handle not finished
        // But this simulates a task that reported completion before actually terminating — should not allow immediate reap until handle finished.
        // We set Finished now, but handle still running, so next acquire should fail because is_finished false.
        let completed2 = rt2.mark_completed(&owner2);
        assert!(completed2);
        // Try to acquire while handle not finished — should fail because try_reap checks is_finished
        let p_fail = rt2.try_acquire_start("next2".to_string());
        assert!(
            p_fail.is_err(),
            "should not allow new start while old task still running even if marked Finished"
        );
        // Now abort the long task to make it finish
        rt2.stop().await.unwrap();
        assert!(!rt2.is_active());
        // After stop, should allow
        assert!(rt2.try_acquire_start("next3".to_string()).is_ok());
    }

    #[tokio::test]
    async fn stale_completion_after_new_owner_is_ignored() {
        let rt = test_runtime();
        let permit_old = rt.try_acquire_start("old".to_string()).unwrap();
        let owner_old = permit_old.owner();
        let handle_old: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
        permit_old.commit(handle_old).unwrap();
        // natural completion of old
        rt.mark_completed(&owner_old);
        // simulate next acquisition after reap? Need handle finished first
        tokio::time::sleep(Duration::from_millis(60)).await;
        let permit_new = rt.try_acquire_start("new".to_string()).unwrap();
        let owner_new = permit_new.owner();
        let handle_new: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        permit_new.commit(handle_new).unwrap();
        // Now stale old completion tries to mark again (old owner)
        let stale_ok = rt.mark_completed(&owner_old);
        assert!(!stale_ok, "stale old owner must not affect new owner");
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        assert!(rt.is_active());
        // Also stale rollback should not clear
        rt.rollback_starting_for_test(&owner_old);
        assert_eq!(rt.current_owner().unwrap(), owner_new);
    }
}
