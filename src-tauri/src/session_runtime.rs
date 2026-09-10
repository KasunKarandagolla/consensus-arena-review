use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Notify, OwnedMutexGuard};
use tokio::task::JoinHandle;

#[cfg(test)]
pub mod test_hooks {
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;
    static HOOK: Mutex<Option<(Arc<Notify>, Arc<Notify>)>> = Mutex::new(None);
    pub fn set_hook(observed: Arc<Notify>, proceed: Arc<Notify>) {
        *HOOK.lock().unwrap() = Some((observed, proceed));
    }
    pub fn clear_hook() {
        *HOOK.lock().unwrap() = None;
    }
    pub(crate) fn take_hook() -> Option<(Arc<Notify>, Arc<Notify>)> {
        HOOK.lock().unwrap().take()
    }
}

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

    fn is_active_raw(&self) -> bool {
        matches!(
            self,
            RuntimePhase::Starting(_)
                | RuntimePhase::Running(_)
                | RuntimePhase::Paused(_)
                | RuntimePhase::Resuming(_)
                | RuntimePhase::Stopping(_)
                | RuntimePhase::Finished(_)
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

struct PreHandoff {
    owner: SessionOwner,
    notify: Arc<Notify>,
}

struct Inner {
    phase: RuntimePhase,
    handle: Option<JoinHandle<()>>,
    pre_handoff: Option<PreHandoff>,
}

/// SessionRuntime is the single concurrency authority for orchestration tasks.
/// Synchronization uses `std::sync::Mutex` for short ownership metadata and an
/// atomic counter for monotonic generation. Never hold the mutex across `.await`.
pub struct SessionRuntime {
    inner: Mutex<Inner>,
    generation: AtomicU64,
    stop_gate: Arc<tokio::sync::Mutex<()>>,
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
                pre_handoff: None,
            }),
            generation: AtomicU64::new(0),
            stop_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// True if an execution owner still occupies the runtime or its task has not yet been proven terminated.
    /// Lazily reaps `Finished` only if handle is actually `is_finished()`.
    pub fn is_active(&self) -> bool {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        self.try_reap_finished_locked(&mut guard);
        !matches!(guard.phase, RuntimePhase::Idle)
    }

    /// Current owner if any (including Finished that hasn't been reaped). Reaps if finished handle is done.
    pub fn current_owner(&self) -> Option<SessionOwner> {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        self.try_reap_finished_locked(&mut guard);
        guard.phase.owner().cloned()
    }

    /// True if the runtime is active and the session_id matches the live owner.
    pub fn is_active_session(&self, session_id: &str) -> bool {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        self.try_reap_finished_locked(&mut guard);
        if matches!(guard.phase, RuntimePhase::Idle) {
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

    fn try_reap_finished_locked(&self, inner: &mut Inner) {
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
        self.try_reap_finished_locked(&mut inner);

        match &inner.phase {
            RuntimePhase::Idle => {}
            _ => {
                return Err(
                    "A session is already active. Use Stop to end it before starting a new one."
                        .to_string(),
                );
            }
        }
        // Also ensure no pre_handoff still active (should be None when Idle, but check)
        if inner.pre_handoff.is_some() {
            return Err(
                "A session is already active. Use Stop to end it before starting a new one."
                    .to_string(),
            );
        }

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let owner = SessionOwner {
            session_id,
            run_generation: generation,
        };
        let notify = Arc::new(Notify::new());
        inner.pre_handoff = Some(PreHandoff {
            owner: owner.clone(),
            notify: notify.clone(),
        });
        inner.phase = if is_start {
            RuntimePhase::Starting(owner.clone())
        } else {
            RuntimePhase::Resuming(owner.clone())
        };
        // handle remains None until handoff
        Ok(SessionStartPermit {
            runtime: Arc::clone(self),
            owner,
            notify,
            committed: false,
        })
    }

    // ── Completion / stale protection ───────────────────────────────────

    /// Mark the given owner terminal (natural completion). Owner-checked including generation.
    /// Only accepts Running; after activation gate, Starting/Resuming should not be completable.
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
            RuntimePhase::Running(_) => {
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

    /// Atomic resume of a Paused session. Under one lock, checks Paused(owner) with matching session_id and transitions to Running.
    pub fn resume_paused_session(&self, target_session_id: &str) -> Result<SessionOwner, String> {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        self.try_reap_finished_locked(&mut inner);
        match &inner.phase {
            RuntimePhase::Paused(owner) if owner.session_id == target_session_id => {
                let owner_clone = owner.clone();
                inner.phase = RuntimePhase::Running(owner_clone.clone());
                Ok(owner_clone)
            }
            RuntimePhase::Paused(owner) => Err(format!(
                "Cannot resume session {} while session {} is paused — stop the current session first",
                target_session_id, owner.session_id
            )),
            _ => {
                if let Some(cur) = inner.phase.owner() {
                    if cur.session_id == target_session_id {
                        return Err(format!(
                            "Session {} is not paused (current phase {})",
                            target_session_id,
                            inner.phase.name()
                        ));
                    } else {
                        return Err(format!(
                            "Cannot resume session {} while session {} is still active — stop the current session first",
                            target_session_id, cur.session_id
                        ));
                    }
                }
                Err("No paused session to resume".to_string())
            }
        }
    }

    // ── Stop semantics ──────────────────────────────────────────────────

    /// Stop the current session task, aborting and awaiting termination.
    /// Legacy helper that stops whatever owner is current (used by simple cases).
    /// For correction, prefer `stop_owner`.
    pub async fn stop(self: &Arc<Self>) -> Result<(), String> {
        let owner_opt = self.current_owner();
        if let Some(owner) = owner_opt {
            let guard_opt = self.stop_owner(&owner).await?;
            if let Some(guard) = guard_opt {
                guard.finish();
            }
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Stop the exact expected owner. Serialized via `stop_gate`. Returns `Some(guard)` if owner was live and is now Stopping with task proven dead but admission still reserved. Caller must perform final cleanup then `guard.finish()` to release to Idle. Returns `None` if expected is stale (no-op).
    pub async fn stop_owner(
        self: &Arc<Self>,
        expected: &SessionOwner,
    ) -> Result<Option<SessionStopGuard>, String> {
        // Acquire async stop serialization gate
        let gate_guard = self.stop_gate.clone().lock_owned().await;

        // Re-check exact owner under lock, and transition to Stopping if needed, then wait for pre_handoff
        {
            let mut inner = match self.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            self.try_reap_finished_locked(&mut inner);
            let cur_owned = inner.phase.owner().cloned();
            if cur_owned.as_ref() != Some(expected) {
                // Stale or Idle
                return Ok(None);
            }
            // If already Stopping with same owner, keep it (duplicate Stop)
            // Otherwise transition to Stopping
            if !matches!(&inner.phase, RuntimePhase::Stopping(o) if o == expected) {
                // Only transition if phase is Starting/Running/Paused/Resuming/Finished with same owner
                let can_stop = matches!(&inner.phase, RuntimePhase::Starting(o) if o == expected)
                    || matches!(&inner.phase, RuntimePhase::Resuming(o) if o == expected)
                    || matches!(&inner.phase, RuntimePhase::Running(o) if o == expected)
                    || matches!(&inner.phase, RuntimePhase::Paused(o) if o == expected)
                    || matches!(&inner.phase, RuntimePhase::Finished(o) if o == expected);
                if can_stop {
                    inner.phase = RuntimePhase::Stopping(expected.clone());
                } else {
                    return Ok(None);
                }
            }
        }

        // Wait for pre_handoff to resolve if it is for expected owner
        loop {
            let notify_opt = {
                let inner = match self.inner.lock() {
                    Ok(g) => g,
                    Err(poison) => poison.into_inner(),
                };
                if let Some(ph) = &inner.pre_handoff {
                    if &ph.owner == expected {
                        Some(ph.notify.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(notify) = notify_opt {
                #[cfg(test)]
                {
                    if let Some((observed, proceed)) = test_hooks::take_hook() {
                        observed.notify_one();
                        proceed.notified().await;
                    }
                }
                notify.notified().await;
            } else {
                break;
            }
        }

        // After pre_handoff cleared, take handle and abort/await outside lock
        let handle_opt = {
            let mut inner = match self.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            // Re-check still Stopping with same owner
            if !matches!(&inner.phase, RuntimePhase::Stopping(o) if o == expected) {
                return Ok(None);
            }
            inner.handle.take()
        };

        if let Some(handle) = handle_opt {
            handle.abort();
            let _ = handle.await;
        }
        // At this point, owned task is proven dead, but runtime remains Stopping(expected) and gate held.
        // Return guard that will set Idle after caller cleanup.
        Ok(Some(SessionStopGuard {
            runtime: Arc::clone(self),
            owner: expected.clone(),
            _gate_guard: gate_guard,
        }))
    }

    /// Synchronous variant for tests where no handle exists (Starting without task).
    #[cfg(test)]
    pub fn rollback_starting_for_test(&self, owner: &SessionOwner) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        match &inner.phase {
            RuntimePhase::Starting(o) if o == owner => {
                inner.phase = RuntimePhase::Idle;
                inner.pre_handoff = None;
            }
            RuntimePhase::Resuming(o) if o == owner => {
                inner.phase = RuntimePhase::Idle;
                inner.pre_handoff = None;
            }
            _ => {}
        }
    }
}

/// Guard that keeps runtime in Stopping(expected) and holds stop_gate until `finish()` is called.
/// While guard is alive, new Start/Resume remains rejected and duplicate Stop(A) cannot release admission.
pub struct SessionStopGuard {
    runtime: Arc<SessionRuntime>,
    owner: SessionOwner,
    _gate_guard: OwnedMutexGuard<()>,
}

impl SessionStopGuard {
    pub fn owner(&self) -> &SessionOwner {
        &self.owner
    }

    /// Transition Stopping(expected) -> Idle, owner-checked, and release gate.
    pub fn finish(self) {
        let mut inner = match self.runtime.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        if let RuntimePhase::Stopping(o) = &inner.phase {
            if o == &self.owner {
                inner.phase = RuntimePhase::Idle;
                inner.handle = None;
                // pre_handoff should already be None
            }
        }
        // gate_guard dropped here, releasing serialization
    }
}

impl Drop for SessionStopGuard {
    fn drop(&mut self) {
        // If finish() was not called explicitly, ensure we still set Idle.
        // This is defensive; normal path calls finish().
        let mut inner = match self.runtime.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        if let RuntimePhase::Stopping(o) = &inner.phase {
            if o == &self.owner {
                inner.phase = RuntimePhase::Idle;
                inner.handle = None;
            }
        }
    }
}

/// Guard/permit for a newly acquired execution ownership. Must be handed off
/// to the runtime via `commit` after the task is spawned. If dropped without
/// commit, admission rolls back automatically with owner check.
pub struct SessionStartPermit {
    runtime: Arc<SessionRuntime>,
    owner: SessionOwner,
    notify: Arc<Notify>,
    committed: bool,
}

impl SessionStartPermit {
    pub fn owner(&self) -> SessionOwner {
        self.owner.clone()
    }

    /// Commit the permit with a JoinHandle and activation sender.
    /// On success, sends activation and returns Ok. On Stopping, stores handle for Stop and returns Err without activation.
    pub fn commit(
        mut self,
        handle: JoinHandle<()>,
        activate: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(), String> {
        let res = {
            let mut inner = match self.runtime.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            let still_valid = matches!(&inner.phase, RuntimePhase::Starting(o) if o == &self.owner)
                || matches!(&inner.phase, RuntimePhase::Resuming(o) if o == &self.owner);
            let is_stopping = matches!(&inner.phase, RuntimePhase::Stopping(o) if o == &self.owner);
            if still_valid {
                inner.phase = RuntimePhase::Running(self.owner.clone());
                inner.handle = Some(handle);
                // Clear pre_handoff and capture notify to wake Stop waiter
                let notify = if let Some(ph) = inner.pre_handoff.take() {
                    // Should be same owner
                    Some(ph.notify.clone())
                } else {
                    None
                };
                drop(inner);
                if let Some(n) = notify {
                    n.notify_one();
                }
                // Send activation
                let _ = activate.send(());
                Ok(())
            } else if is_stopping {
                // Store handle for Stop to abort, clear pre_handoff, notify, do not activate
                inner.handle = Some(handle);
                let notify = if let Some(ph) = inner.pre_handoff.take() {
                    Some(ph.notify.clone())
                } else {
                    None
                };
                drop(inner);
                if let Some(n) = notify {
                    n.notify_one();
                }
                // Do not send activation; task will remain pending until abort
                Err("Session start cancelled — stop requested before task became live".to_string())
            } else {
                drop(inner);
                handle.abort();
                // Actually still abort, but we already aborted handle
                Err("Session start cancelled — stop requested before task became live".to_string())
            }
        };
        self.committed = true;
        res
    }

    /// Lightweight check before major side effects; returns Err if Stop has already taken ownership.
    pub fn ensure_admitted(&self) -> Result<(), String> {
        let inner = match self.runtime.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        if matches!(&inner.phase, RuntimePhase::Stopping(o) if o == &self.owner) {
            return Err("Session cancelled — stop requested".to_string());
        }
        if matches!(&inner.phase, RuntimePhase::Starting(o) if o == &self.owner)
            || matches!(&inner.phase, RuntimePhase::Resuming(o) if o == &self.owner)
        {
            Ok(())
        } else {
            Err("Session no longer admitted".to_string())
        }
    }
}

impl Drop for SessionStartPermit {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let notify_opt = {
            let mut inner = match self.runtime.inner.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            if let Some(ph) = &inner.pre_handoff {
                if ph.owner == self.owner {
                    let n = ph.notify.clone();
                    // Only rollback to Idle if not Stopping
                    let is_stopping =
                        matches!(&inner.phase, RuntimePhase::Stopping(o) if o == &self.owner);
                    if !is_stopping {
                        let is_starting =
                            matches!(&inner.phase, RuntimePhase::Starting(o) if o == &self.owner);
                        let is_resuming =
                            matches!(&inner.phase, RuntimePhase::Resuming(o) if o == &self.owner);
                        if is_starting || is_resuming {
                            inner.phase = RuntimePhase::Idle;
                        }
                    }
                    inner.pre_handoff = None;
                    Some(n)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(n) = notify_opt {
            n.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;
    use tokio::sync::{Barrier, Notify};
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
        drop(permit_old);
        let permit_new = rt.try_acquire_start("new".to_string()).unwrap();
        let owner_new = permit_new.owner();
        assert_ne!(owner_old.run_generation, owner_new.run_generation);
        rt.rollback_starting_for_test(&owner_old);
        assert!(rt.is_active());
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        let stale_permit = SessionStartPermit {
            runtime: Arc::clone(&rt),
            owner: owner_old.clone(),
            notify: Arc::new(Notify::new()),
            committed: false,
        };
        drop(stale_permit);
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        assert!(rt.is_active());
    }

    #[test]
    fn t5_natural_completion_only_exact_owner() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-x".to_string()).unwrap();
        let owner = permit.owner();
        {
            let mut inner = rt.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.phase = RuntimePhase::Running(owner.clone());
        }
        let ok = rt.mark_completed(&owner);
        assert!(ok, "exact owner should mark completed");
        // After completion, should be Finished until reaped; is_active still true until handle finished
        // With no handle, Finished is reapable to Idle, so is_active false
        assert!(!rt.is_active());
        // current_owner after reap should be None, so we check before reap by inspecting inner directly
        {
            let inner = rt.inner.lock().unwrap_or_else(|e| e.into_inner());
            // After is_active, it reaped to Idle, so no owner
            assert!(inner.phase == RuntimePhase::Idle);
        }

        // Stale owner (different generation) should fail
        let rt2 = test_runtime();
        let p2 = rt2.try_acquire_start("sess-y".to_string()).unwrap();
        let owner2 = p2.owner();
        {
            let mut inner = rt2.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.phase = RuntimePhase::Running(owner2.clone());
        }
        let stale = SessionOwner {
            session_id: owner2.session_id.clone(),
            run_generation: owner2.run_generation + 100,
        };
        let ok2 = rt2.mark_completed(&stale);
        assert!(!ok2, "stale generation must not mark completed");
        assert_eq!(rt2.current_owner().unwrap(), owner2);
    }

    #[tokio::test]
    async fn t6_stop_retains_admission_until_task_dead() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess".to_string()).unwrap();
        let owner = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        permit.commit(handle, activate_tx).expect("commit ok");
        assert!(rt.is_active());
        assert_eq!(rt.current_owner().unwrap(), owner);
        assert!(rt.try_acquire_start("other".to_string()).is_err());
        rt.stop().await.unwrap();
        assert!(!rt.is_active());
        assert!(rt.try_acquire_start("new2".to_string()).is_ok());
    }

    #[tokio::test]
    async fn t7_stop_during_starting_prevents_handoff_resurrection() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner = permit.owner();
        assert!(matches!(
            rt.inner.lock().unwrap_or_else(|e| e.into_inner()).phase,
            RuntimePhase::Starting(_)
        ));
        // Spawn Stop concurrently while permit is still alive — Stop must wait for pre_handoff
        let rt_clone = Arc::clone(&rt);
        let owner_clone = owner.clone();
        let stop_handle = tokio::spawn(async move {
            let guard = rt_clone.stop_owner(&owner_clone).await.unwrap();
            assert!(
                guard.is_some(),
                "Stop should obtain guard for Starting owner"
            );
            // While Stopping, new Start must fail
            assert!(rt_clone.try_acquire_start("other".to_string()).is_err());
            guard.unwrap().finish();
        });
        // Give Stop a moment to enter waiting
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Attempt handoff while Stop is waiting — must be rejected and not activate
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            if activate_rx.await.is_err() {
                return;
            }
            panic!("handoff-rejected task should not execute");
        });
        let res = permit.commit(handle, activate_tx);
        assert!(res.is_err(), "handoff after stop must be rejected");
        stop_handle.await.unwrap();
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Idle");
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
    }

    #[tokio::test]
    async fn t8_wrong_session_in_process_resume_rejected() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-A".to_string()).unwrap();
        let owner_a = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
        });
        permit.commit(handle, activate_tx).unwrap();
        // Mark running and then paused for resume test
        assert!(rt.is_active());
        // Simulate pause
        assert!(rt.mark_paused(&owner_a));
        // Try to resume wrong session
        let res = rt.resume_paused_session("sess-B");
        assert!(res.is_err(), "wrong session resume must be rejected");
        assert_eq!(rt.current_owner().unwrap().session_id, "sess-A");
        // Correct resume
        let res2 = rt
            .resume_paused_session("sess-A")
            .expect("correct resume should succeed");
        assert_eq!(res2, owner_a);
        assert!(matches!(
            rt.inner.lock().unwrap_or_else(|e| e.into_inner()).phase,
            RuntimePhase::Running(_)
        ));
    }

    #[tokio::test]
    async fn t9_duplicate_resume_cannot_spawn_two_tasks() {
        let rt = test_runtime();
        let p1 = rt
            .try_acquire_resume("sess-R".to_string())
            .expect("first resume acquire ok");
        assert!(rt.is_active());
        let p2 = rt.try_acquire_resume("sess-R".to_string());
        assert!(p2.is_err(), "duplicate resume must be rejected");
        let p3 = rt.try_acquire_start("sess-other".to_string());
        assert!(p3.is_err(), "start while resuming must be rejected");
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
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
        permit.commit(handle, activate_tx).unwrap();
        assert!(rt.is_active());
        tokio::time::sleep(Duration::from_millis(30)).await;
        let completed = rt.mark_completed(&owner);
        assert!(completed);
        // After completion, still Finished with handle finished, so is_active will reap to Idle on next check
        // But before reap, phase is Finished; after is_active, it becomes Idle
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Idle");
        let p2 = rt
            .try_acquire_start("next".to_string())
            .expect("should reap finished and allow new start");
        assert_eq!(p2.owner().session_id, "next");
        assert!(p2.owner().run_generation > owner.run_generation);

        let rt2 = test_runtime();
        let permit2 = rt2.try_acquire_start("sess2".to_string()).unwrap();
        let owner2 = permit2.owner();
        let (activate_tx2, activate_rx2) = tokio::sync::oneshot::channel::<()>();
        let long_handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx2.await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        permit2.commit(long_handle, activate_tx2).unwrap();
        let completed2 = rt2.mark_completed(&owner2);
        assert!(completed2);
        // Even though marked Finished, handle not finished (still sleeping 500ms), so try_reap will keep it Finished and is_active true
        assert!(
            rt2.is_active(),
            "Finished with unfinished handle must still be active"
        );
        let p_fail = rt2.try_acquire_start("next2".to_string());
        assert!(
            p_fail.is_err(),
            "should not allow new start while old task still running even if marked Finished"
        );
        rt2.stop().await.unwrap();
        assert!(!rt2.is_active());
        assert!(rt2.try_acquire_start("next3".to_string()).is_ok());
    }

    #[tokio::test]
    async fn stale_completion_after_new_owner_is_ignored() {
        let rt = test_runtime();
        let permit_old = rt.try_acquire_start("old".to_string()).unwrap();
        let owner_old = permit_old.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle_old: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
        permit_old.commit(handle_old, activate_tx).unwrap();
        rt.mark_completed(&owner_old);
        tokio::time::sleep(Duration::from_millis(60)).await;
        // After handle finished, is_active will reap to Idle, so we can acquire new
        assert!(!rt.is_active());
        let permit_new = rt.try_acquire_start("new".to_string()).unwrap();
        let owner_new = permit_new.owner();
        let (activate_tx2, activate_rx2) = tokio::sync::oneshot::channel::<()>();
        let handle_new: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx2.await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        permit_new.commit(handle_new, activate_tx2).unwrap();
        let stale_ok = rt.mark_completed(&owner_old);
        assert!(!stale_ok, "stale old owner must not affect new owner");
        assert_eq!(rt.current_owner().unwrap(), owner_new);
        assert!(rt.is_active());
        rt.rollback_starting_for_test(&owner_old);
        assert_eq!(rt.current_owner().unwrap(), owner_new);
    }

    // ── C1–C10 correction tests ────────────────────────────────────────────

    #[tokio::test]
    async fn c1_pre_handoff_lifetime_barrier() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        // Do not commit yet; permit is alive. Start a Stop that should wait for permit to resolve.
        let rt_clone = Arc::clone(&rt);
        let owner_clone = owner_a.clone();
        let stop_handle = tokio::spawn(async move {
            // This should wait until permit is dropped
            let guard = rt_clone.stop_owner(&owner_clone).await.unwrap();
            assert!(guard.is_some(), "Stop should obtain guard for owner A");
            // At this point, task is dead (no handle) but runtime still Stopping(A)
            assert!(
                rt_clone.is_active(),
                "while StopGuard alive, runtime must still be active"
            );
            assert_eq!(rt_clone.phase_name(), "Stopping");
            // Try to acquire B should fail while guard alive
            assert!(
                rt_clone.try_acquire_start("sess-b".to_string()).is_err(),
                "B must not be admitted while StopGuard holds Stopping(A)"
            );
            guard.unwrap().finish();
            assert!(!rt_clone.is_active());
        });
        // Give stop a moment to start and block on pre_handoff
        tokio::time::sleep(Duration::from_millis(30)).await;
        // While permit alive and Stop waiting, new Start must fail
        assert!(
            rt.try_acquire_start("sess-b".to_string()).is_err(),
            "Stop cannot release admission while pre-handoff permit alive"
        );
        // Now drop permit, waking Stop
        drop(permit);
        stop_handle.await.unwrap();
        // After guard finished, B can be admitted
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
    }

    #[tokio::test]
    async fn c2_stop_during_pre_handoff_and_handoff() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        let rt_clone = Arc::clone(&rt);
        let owner_clone = owner_a.clone();
        let stop_handle = tokio::spawn(async move {
            let guard = rt_clone.stop_owner(&owner_clone).await.unwrap().unwrap();
            // Stop has waited for pre_handoff and now owns Stopping. At this point, permit still alive but Stop is waiting.
            // The permit will attempt handoff; handoff under Stopping should store handle for Stop.
            // We need to ensure Stop obtains the handle and terminates it before guard finish.
            // This test will be driven by the main task's handoff attempt.
            // Keep guard alive until main signals
            tokio::time::sleep(Duration::from_millis(50)).await;
            // While guard held, try_acquire B must fail
            assert!(rt_clone.try_acquire_start("sess-b".to_string()).is_err());
            guard.finish();
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Attempt handoff while Stop is waiting (Stopping)
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            // This task is gated and should never run because handoff will be rejected
            if activate_rx.await.is_err() {
                return;
            }
            panic!("gated task should not execute");
        });
        let res = permit.commit(handle, activate_tx);
        assert!(res.is_err(), "handoff under Stopping must be rejected");
        // Stop should have been woken and now holds the handle; it will abort and await it
        stop_handle.await.unwrap();
        assert!(!rt.is_active());
        // Now B can be admitted
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
    }

    #[tokio::test]
    async fn c3_task_cannot_execute_before_handoff() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess".to_string()).unwrap();
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            if activate_rx.await.is_err() {
                return;
            }
            executed_clone.store(true, Ordering::SeqCst);
        });
        // Before handoff, task should not have executed
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !executed.load(Ordering::SeqCst),
            "task must not run before handoff"
        );
        permit.commit(handle, activate_tx).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            executed.load(Ordering::SeqCst),
            "task must run after activation"
        );
        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn c4_concurrent_stop() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            if activate_rx.await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        permit.commit(handle, activate_tx).unwrap();
        // Two concurrent Stop(A) — only one should obtain guard, the other should be None after first finishes
        let rt1 = Arc::clone(&rt);
        let rt2 = Arc::clone(&rt);
        let owner1 = owner_a.clone();
        let owner2 = owner_a.clone();
        let barrier = Arc::new(Barrier::new(2));
        let b1 = barrier.clone();
        let b2 = barrier.clone();
        let (tx1, rx1) = tokio::sync::oneshot::channel::<bool>();
        let (tx2, rx2) = tokio::sync::oneshot::channel::<bool>();
        let stop1 = tokio::spawn(async move {
            b1.wait().await;
            let guard = rt1.stop_owner(&owner1).await.unwrap();
            let had = guard.is_some();
            if let Some(g) = guard {
                tokio::time::sleep(Duration::from_millis(50)).await;
                assert!(rt1.try_acquire_start("sess-b".to_string()).is_err());
                g.finish();
            }
            let _ = tx1.send(had);
        });
        let stop2 = tokio::spawn(async move {
            b2.wait().await;
            let guard = rt2.stop_owner(&owner2).await.unwrap();
            let had = guard.is_some();
            if let Some(g) = guard {
                g.finish();
            }
            let _ = tx2.send(had);
        });
        stop1.await.unwrap();
        stop2.await.unwrap();
        let had1 = rx1.await.unwrap();
        let had2 = rx2.await.unwrap();
        assert_eq!(
            (had1 as u8 + had2 as u8),
            1,
            "exactly one concurrent Stop should obtain guard"
        );
        assert!(!rt.is_active());
        // After both Stops, B can be admitted
        let p = rt.try_acquire_start("sess-b".to_string()).unwrap();
        let owner_b = p.owner();
        let (activate_tx_b, activate_rx_b) = tokio::sync::oneshot::channel::<()>();
        let handle_b: JoinHandle<()> = tokio::spawn(async move {
            if activate_rx_b.await.is_err() {
                return;
            }
        });
        p.commit(handle_b, activate_tx_b).unwrap();
        // Delayed second Stop(A) must not affect B
        let delayed = rt.stop_owner(&owner_a).await.unwrap();
        assert!(delayed.is_none(), "stale Stop(A) must not stop B");
        assert_eq!(rt.current_owner().unwrap(), owner_b);
        assert!(rt.is_active());
        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn c5_stopguard_prevents_post_stop_cleanup_race() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
        });
        permit.commit(handle, activate_tx).unwrap();
        let guard = rt.stop_owner(&owner_a).await.unwrap().unwrap();
        // While guard held (task dead but Stopping), new Start must fail
        assert!(rt.try_acquire_start("sess-b".to_string()).is_err());
        // Simulate abort cleanup that would previously have set Idle before cleanup
        // Now we are still Stopping, so is_active true
        assert!(rt.is_active());
        guard.finish();
        assert!(!rt.is_active());
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
    }

    #[tokio::test]
    async fn c6_finished_but_handle_running_is_active() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess".to_string()).unwrap();
        let owner = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
            tokio::time::sleep(Duration::from_millis(300)).await;
        });
        permit.commit(handle, activate_tx).unwrap();
        // Mark completed while handle still running
        assert!(rt.mark_completed(&owner));
        // Now phase is Finished with unfinished handle, should still be active
        assert!(
            rt.is_active(),
            "Finished with unfinished handle must be active"
        );
        assert_eq!(rt.phase_name(), "Finished");
        // New Start must be blocked
        assert!(rt.try_acquire_start("next".to_string()).is_err());
        // After handle finishes and is_active reaps, it becomes Idle
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert!(!rt.is_active());
        assert_eq!(rt.phase_name(), "Idle");
        assert!(rt.try_acquire_start("next2".to_string()).is_ok());
    }

    #[tokio::test]
    async fn c7_resume_requires_paused() {
        let rt = test_runtime();
        // Start and make Running
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
        });
        permit.commit(handle, activate_tx).unwrap();
        // Running -> resume should fail
        assert!(rt.resume_paused_session("sess-a").is_err());
        // Starting -> resume should fail (create a new runtime for Starting)
        let rt2 = test_runtime();
        let _p_start = rt2.try_acquire_start("sess-b".to_string()).unwrap();
        assert!(rt2.resume_paused_session("sess-b").is_err());
        // Stopping -> resume should fail
        let permit3 = test_runtime();
        let p3 = permit3.try_acquire_start("sess-c".to_string()).unwrap();
        let owner_c = p3.owner();
        let (activate_tx3, activate_rx3) = tokio::sync::oneshot::channel::<()>();
        let handle3: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx3.await;
        });
        p3.commit(handle3, activate_tx3).unwrap();
        // Move to Stopping via stop_owner but don't finish
        let guard = permit3.stop_owner(&owner_c).await.unwrap().unwrap();
        assert!(permit3.resume_paused_session("sess-c").is_err());
        guard.finish();
        // Paused -> resume should succeed exactly once
        let rt4 = test_runtime();
        let permit4 = rt4.try_acquire_start("sess-d".to_string()).unwrap();
        let owner_d = permit4.owner();
        let (activate_tx4, activate_rx4) = tokio::sync::oneshot::channel::<()>();
        let handle4: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx4.await;
        });
        permit4.commit(handle4, activate_tx4).unwrap();
        assert!(rt4.mark_paused(&owner_d));
        let resumed = rt4
            .resume_paused_session("sess-d")
            .expect("Paused -> Running should succeed");
        assert_eq!(resumed, owner_d);
        // Second resume should fail (already Running)
        assert!(rt4.resume_paused_session("sess-d").is_err());
        // Wrong session
        assert!(rt4.resume_paused_session("other").is_err());
        rt4.stop().await.unwrap();
    }

    #[tokio::test]
    async fn c8_failed_handoff_is_error() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        // Start Stop before handoff
        let rt_clone = Arc::clone(&rt);
        let owner_clone = owner_a.clone();
        let stop_handle = tokio::spawn(async move {
            let guard = rt_clone.stop_owner(&owner_clone).await.unwrap().unwrap();
            // Hold guard
            tokio::time::sleep(Duration::from_millis(30)).await;
            guard.finish();
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
        });
        let res = permit.commit(handle, activate_tx);
        assert!(res.is_err(), "failed handoff must return Err");
        stop_handle.await.unwrap();
    }

    #[tokio::test]
    async fn c9_delayed_duplicate_stop_cannot_stop_newer_owner() {
        let rt = test_runtime();
        let permit_a = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit_a.owner();
        let (activate_tx_a, activate_rx_a) = tokio::sync::oneshot::channel::<()>();
        let handle_a: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx_a.await;
        });
        permit_a.commit(handle_a, activate_tx_a).unwrap();
        // Stop A and finish
        let guard_a = rt.stop_owner(&owner_a).await.unwrap().unwrap();
        guard_a.finish();
        assert!(!rt.is_active());
        // Admit B
        let permit_b = rt.try_acquire_start("sess-b".to_string()).unwrap();
        let owner_b = permit_b.owner();
        let (activate_tx_b, activate_rx_b) = tokio::sync::oneshot::channel::<()>();
        let handle_b: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx_b.await;
        });
        permit_b.commit(handle_b, activate_tx_b).unwrap();
        // Delayed Stop(A) must be no-op
        let delayed = rt.stop_owner(&owner_a).await.unwrap();
        assert!(delayed.is_none(), "stale Stop(A) must not affect B");
        assert_eq!(rt.current_owner().unwrap(), owner_b);
        assert!(rt.is_active());
        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn c10_stale_pause_run_cannot_mutate_newer_owner() {
        let rt = test_runtime();
        let permit_a = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit_a.owner();
        let (activate_tx_a, activate_rx_a) = tokio::sync::oneshot::channel::<()>();
        let handle_a: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx_a.await;
        });
        permit_a.commit(handle_a, activate_tx_a).unwrap();
        // Mark paused for A
        assert!(rt.mark_paused(&owner_a));
        // Stop A and admit B
        let guard_a = rt.stop_owner(&owner_a).await.unwrap().unwrap();
        guard_a.finish();
        let permit_b = rt.try_acquire_start("sess-b".to_string()).unwrap();
        let owner_b = permit_b.owner();
        let (activate_tx_b, activate_rx_b) = tokio::sync::oneshot::channel::<()>();
        let handle_b: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx_b.await;
        });
        permit_b.commit(handle_b, activate_tx_b).unwrap();
        // Stale transitions with owner A must not affect B
        assert!(
            !rt.mark_paused(&owner_a),
            "stale mark_paused(A) must not affect B"
        );
        assert!(!rt.mark_running(&owner_a));
        assert!(!rt.mark_completed(&owner_a));
        assert_eq!(rt.current_owner().unwrap(), owner_b);
        assert!(matches!(
            rt.inner.lock().unwrap_or_else(|e| e.into_inner()).phase,
            RuntimePhase::Running(_)
        ));
        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn c11_missed_wakeup_before_wait() {
        // Deterministically force the notification-before-wait registration race.
        // A is Starting with live permit, Stop has observed pre_handoff but not yet begun waiting,
        // then permit resolves and signals, only then Stop proceeds to await.
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner_a = permit.owner();
        let observed = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        test_hooks::set_hook(observed.clone(), proceed.clone());
        let rt_clone = Arc::clone(&rt);
        let owner_clone = owner_a.clone();
        let stop_handle = tokio::spawn(async move {
            // This will enter Stopping(A) and then hit the test hook before notified().await
            let guard = rt_clone.stop_owner(&owner_clone).await.unwrap();
            assert!(
                guard.is_some(),
                "Stop should obtain guard for Starting owner"
            );
            assert_eq!(rt_clone.phase_name(), "Stopping");
            // While Stopping, try_acquire must fail
            assert!(rt_clone.try_acquire_start("sess-b".to_string()).is_err());
            guard.unwrap().finish();
            assert_eq!(rt_clone.phase_name(), "Idle");
        });
        // Wait for Stop to observe pre_handoff and pause before notified().await
        observed.notified().await;
        // Now Stop is paused before waiting, drop permit which does notify_one with no waiter (stored permit)
        drop(permit);
        // Allow Stop to proceed to notified().await, which should consume the stored permit and not hang
        proceed.notify_one();
        // Stop should complete without deadlock
        tokio::time::timeout(Duration::from_secs(2), stop_handle)
            .await
            .unwrap()
            .unwrap();
        assert!(!rt.is_active());
        assert!(rt.try_acquire_start("sess-b".to_string()).is_ok());
        test_hooks::clear_hook();
        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn paused_owner_cannot_be_marked_completed_and_can_resume() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner = permit.owner();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = activate_rx.await;
        });
        permit.commit(handle, activate_tx).unwrap();

        assert!(rt.mark_paused(&owner));
        assert!(!rt.mark_completed(&owner));
        assert_eq!(
            rt.inner.lock().unwrap_or_else(|e| e.into_inner()).phase,
            RuntimePhase::Paused(owner.clone())
        );
        assert_eq!(rt.resume_paused_session("sess-a").unwrap(), owner);

        rt.stop().await.unwrap();
    }

    #[tokio::test]
    async fn rejected_handoff_never_enters_gated_task_body() {
        let rt = test_runtime();
        let permit = rt.try_acquire_start("sess-a".to_string()).unwrap();
        let owner = permit.owner();
        let observed = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        test_hooks::set_hook(observed.clone(), proceed.clone());

        let rt_for_stop = Arc::clone(&rt);
        let owner_for_stop = owner.clone();
        let stop_handle = tokio::spawn(async move {
            let guard = rt_for_stop
                .stop_owner(&owner_for_stop)
                .await
                .unwrap()
                .unwrap();
            guard.finish();
        });
        observed.notified().await;

        let entered = Arc::new(AtomicBool::new(false));
        let entered_for_task = Arc::clone(&entered);
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel::<()>();
        let (activate_tx, activate_rx) = tokio::sync::oneshot::channel::<()>();
        let handle: JoinHandle<()> = tokio::spawn(async move {
            if activate_rx.await.is_err() {
                let _ = closed_tx.send(());
                return;
            }
            entered_for_task.store(true, Ordering::SeqCst);
        });

        assert!(permit.commit(handle, activate_tx).is_err());
        closed_rx.await.unwrap();
        assert!(!entered.load(Ordering::SeqCst));

        proceed.notify_one();
        stop_handle.await.unwrap();
        test_hooks::clear_hook();
    }
}
