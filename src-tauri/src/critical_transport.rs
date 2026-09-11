use crate::pipeline_ids::OperationId;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub const MAX_RESIDENT_OPERATIONS: usize = 2;
pub const MAX_OPERATION_CRITICAL_EVENTS: usize = 2_100;
pub const MAX_OPERATION_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CRITICAL_EVENT_BYTES: usize = 64 * 1024;
pub const CRITICAL_INGRESS_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CriticalTransportError {
    Closed,
    IngressUnavailable,
    IngressOverflow,
    EventBudgetExceeded,
    PayloadBudgetExceeded,
    Protocol(String),
}

impl std::fmt::Display for CriticalTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "operation closed"),
            Self::IngressUnavailable => write!(f, "critical ingress unavailable"),
            Self::IngressOverflow => write!(f, "critical ingress overflow"),
            Self::EventBudgetExceeded => write!(f, "event budget exceeded"),
            Self::PayloadBudgetExceeded => write!(f, "payload budget exceeded"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}
impl std::error::Error for CriticalTransportError {}

struct MailboxState<T> {
    queue: VecDeque<(T, usize)>,
    queued_events: usize,
    queued_payload_bytes: usize,
    failed: Option<CriticalTransportError>,
    closed: bool,
    notify: Arc<Notify>,
    registration_epoch: u64,
}

struct HubInner<T> {
    operations: HashMap<OperationId, MailboxState<T>>,
}

pub struct CriticalEventHub<T> {
    inner: Arc<Mutex<HubInner<T>>>,
}

impl<T> Clone for CriticalEventHub<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct OperationInbox<T> {
    operation_id: OperationId,
    hub: CriticalEventHub<T>,
    notify: Arc<Notify>,
}

impl<T> CriticalEventHub<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HubInner {
                operations: HashMap::new(),
            })),
        }
    }

    /// Register a new operation mailbox. Synchronous, short lock.
    /// `current_ingress_epoch` and `ingress_alive` are from BrowserEventIngress atomics.
    pub fn register(
        &self,
        operation_id: OperationId,
        registration_epoch: u64,
        ingress_alive: bool,
    ) -> Result<OperationInbox<T>, CriticalTransportError> {
        if !ingress_alive {
            return Err(CriticalTransportError::IngressUnavailable);
        }
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if inner.operations.contains_key(&operation_id) {
            return Err(CriticalTransportError::Protocol(
                "duplicate operation id".to_string(),
            ));
        }
        if inner.operations.len() >= MAX_RESIDENT_OPERATIONS {
            return Err(CriticalTransportError::Protocol(format!(
                "too many resident operations: {}",
                inner.operations.len()
            )));
        }
        let notify = Arc::new(Notify::new());
        let state = MailboxState {
            queue: VecDeque::new(),
            queued_events: 0,
            queued_payload_bytes: 0,
            failed: None,
            closed: false,
            notify: notify.clone(),
            registration_epoch,
        };
        inner.operations.insert(operation_id.clone(), state);
        Ok(OperationInbox {
            operation_id,
            hub: self.clone(),
            notify,
        })
    }

    /// Dispatch an event to its operation mailbox. Must be called from critical bridge thread only.
    /// `payload_cost` is already computed via `critical_payload_cost`.
    pub fn dispatch(&self, operation_id: OperationId, event: T, payload_cost: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(state) = inner.operations.get_mut(&operation_id) else {
            // stale/unknown operation — ignore (old events)
            return;
        };
        if state.failed.is_some() || state.closed {
            return;
        }
        if state.queued_events + 1 > MAX_OPERATION_CRITICAL_EVENTS {
            state.failed = Some(CriticalTransportError::EventBudgetExceeded);
            state.notify.notify_one();
            return;
        }
        if state.queued_payload_bytes + payload_cost > MAX_OPERATION_PAYLOAD_BYTES {
            state.failed = Some(CriticalTransportError::PayloadBudgetExceeded);
            state.notify.notify_one();
            return;
        }
        state.queue.push_back((event, payload_cost));
        state.queued_events += 1;
        state.queued_payload_bytes += payload_cost;
        state.notify.notify_one();
    }

    pub fn fail_operation(&self, operation_id: &OperationId, error: CriticalTransportError) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(state) = inner.operations.get_mut(operation_id) {
            if state.failed.is_none() {
                state.failed = Some(error);
                state.notify.notify_one();
            }
        }
    }

    /// Targeted protocol failure for a single operation. Preserves
    /// first-terminal-failure semantics: if the mailbox already holds a
    /// terminal error it is left unchanged. Returns true if the operation
    /// existed and was newly failed, false for stale/retired ids.
    pub fn fail_exact(&self, operation_id: &OperationId, error: CriticalTransportError) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(state) = inner.operations.get_mut(operation_id) else {
            return false;
        };
        if state.failed.is_none() {
            state.failed = Some(error);
            state.notify.notify_one();
            true
        } else {
            false
        }
    }

    /// Fail all operations registered before `failed_epoch` (i.e., overflow happened after they were registered).
    /// For determinism, we fail every operation whose registration_epoch < failed_epoch,
    /// or if failed_epoch is just increment after overflow, fail all currently registered.
    /// Simpler: fail all registered operations whose registration_epoch < current_epoch.
    pub fn fail_registered_before_epoch(&self, current_epoch: u64, error: CriticalTransportError) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        for state in inner.operations.values_mut() {
            if state.registration_epoch < current_epoch && state.failed.is_none() {
                state.failed = Some(error.clone());
                state.notify.notify_one();
            }
        }
    }

    pub fn fail_all(&self, error: CriticalTransportError) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        for state in inner.operations.values_mut() {
            if state.failed.is_none() {
                state.failed = Some(error.clone());
                state.notify.notify_one();
            }
        }
    }

    pub fn close_exact(&self, operation_id: &OperationId) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(state) = inner.operations.get_mut(operation_id) {
            state.closed = true;
            state.notify.notify_one();
        }
        // We do not remove immediately; let inbox be dropped or explicitly remove on close.
        // To bound memory, remove if closed and empty and not failed? For now keep until recv sees closed and then caller drops.
        // Actually we can remove after closed is observed? Simpler: remove if closed and queue empty.
        // But to avoid stale id reuse, keep until explicit remove or fail? Let's remove lazily on recv closed path.
    }

    fn remove_if_closed_and_empty(&self, operation_id: &OperationId) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(state) = inner.operations.get(operation_id) {
            if state.closed && state.queue.is_empty() {
                inner.operations.remove(operation_id);
            }
        }
    }

    /// Exact retirement: remove the mailbox for `operation_id` if present,
    /// wake any waiting `recv`, and free the resident slot. Stale or
    /// already-retired IDs are ignored and never affect another operation.
    pub fn retire_exact(&self, operation_id: &OperationId) {
        let notify_opt = {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            inner
                .operations
                .remove(operation_id)
                .map(|state| state.notify)
        };
        if let Some(notify) = notify_opt {
            notify.notify_one();
        }
    }
}

impl<T> Default for CriticalEventHub<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> OperationInbox<T> {
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    pub async fn recv(&mut self) -> Result<T, CriticalTransportError> {
        loop {
            let notify_clone: Arc<Notify>;
            {
                let mut inner = self.hub.inner.lock().unwrap_or_else(|p| p.into_inner());
                let Some(state) = inner.operations.get_mut(&self.operation_id) else {
                    return Err(CriticalTransportError::Closed);
                };
                if let Some(err) = state.failed.clone() {
                    return Err(err);
                }
                if let Some((event, cost)) = state.queue.pop_front() {
                    state.queued_events = state.queued_events.saturating_sub(1);
                    state.queued_payload_bytes = state.queued_payload_bytes.saturating_sub(cost);
                    return Ok(event);
                }
                if state.closed {
                    return Err(CriticalTransportError::Closed);
                }
                notify_clone = state.notify.clone();
            }
            notify_clone.notified().await;
        }
    }

    pub fn close(self) {
        self.hub.close_exact(&self.operation_id);
        self.hub.remove_if_closed_and_empty(&self.operation_id);
    }
}

impl<T> Drop for OperationInbox<T> {
    fn drop(&mut self) {
        self.hub.retire_exact(&self.operation_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_ids::{BrowserSurface, OperationContext};
    use crate::session_runtime::SessionOwner;

    fn owner(id: &str, generation: u64) -> SessionOwner {
        SessionOwner {
            session_id: id.to_string(),
            run_generation: generation,
        }
    }

    #[tokio::test]
    async fn ct2_exact_inbox_routing() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op1 = crate::pipeline_ids::OperationId::new();
        let op2 = crate::pipeline_ids::OperationId::new();
        let mut inbox1 = hub.register(op1.clone(), 0, true).unwrap();
        let mut inbox2 = hub.register(op2.clone(), 0, true).unwrap();
        hub.dispatch(op1.clone(), "hello1".to_string(), 6);
        hub.dispatch(op2.clone(), "hello2".to_string(), 6);
        // Should route exactly
        let v1 = inbox1.recv().await.unwrap();
        let v2 = inbox2.recv().await.unwrap();
        assert_eq!(v1, "hello1");
        assert_eq!(v2, "hello2");
    }

    #[tokio::test]
    async fn ct3_late_closed_operation_rejected() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        hub.dispatch(op.clone(), "first".to_string(), 5);
        let v = inbox.recv().await.unwrap();
        assert_eq!(v, "first");
        // close
        hub.close_exact(&op);
        // dispatch after close should be ignored
        hub.dispatch(op.clone(), "after_close".to_string(), 11);
        // recv should get Closed
        let res = inbox.recv().await;
        assert_eq!(res.unwrap_err(), CriticalTransportError::Closed);
        // further dispatch to closed id is ignored, not delivered to new op with same id (duplicate not allowed)
        // try to register same id again should fail (duplicate protocol)
        let res2 = hub.register(op.clone(), 1, true);
        // Since we didn't remove closed entry, duplicate should error
        assert!(res2.is_err());
    }

    #[test]
    fn ct_resident_bound() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op1 = crate::pipeline_ids::OperationId::new();
        let op2 = crate::pipeline_ids::OperationId::new();
        let op3 = crate::pipeline_ids::OperationId::new();
        let _i1 = hub.register(op1, 0, true).unwrap();
        let _i2 = hub.register(op2, 0, true).unwrap();
        let res = hub.register(op3, 0, true);
        assert!(res.is_err(), "third resident should be rejected");
    }

    #[tokio::test]
    async fn ct4_event_budget_overflow() {
        let hub: CriticalEventHub<u32> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        for i in 0..MAX_OPERATION_CRITICAL_EVENTS {
            hub.dispatch(op.clone(), i as u32, 1);
        }
        // Next should overflow and set sticky failure
        hub.dispatch(op.clone(), 9999, 1);
        // Drain events first? Our implementation sets failed but still has queued events.
        // recv should eventually return failure after draining? In our current impl failed is sticky and outranks queued success.
        // So next recv should return EventBudgetExceeded even though queue has items.
        let res = inbox.recv().await;
        assert_eq!(
            res.unwrap_err(),
            CriticalTransportError::EventBudgetExceeded
        );
    }

    #[tokio::test]
    async fn ct7_payload_budget_overflow() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        let big = "a".repeat(MAX_OPERATION_PAYLOAD_BYTES - 100);
        hub.dispatch(op.clone(), big.clone(), big.len());
        // Next small should exceed
        hub.dispatch(op.clone(), "x".repeat(200), 200);
        let res = inbox.recv().await;
        assert_eq!(
            res.unwrap_err(),
            CriticalTransportError::PayloadBudgetExceeded
        );
    }

    #[tokio::test]
    async fn ct8_notify_before_recv() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        // Dispatch before recv starts waiting — notify permit must remain
        hub.dispatch(op.clone(), "early".to_string(), 5);
        // Now recv should return immediately without waiting
        let v = tokio::time::timeout(std::time::Duration::from_millis(100), inbox.recv())
            .await
            .expect("should not timeout")
            .unwrap();
        assert_eq!(v, "early");
    }

    #[tokio::test]
    async fn ct9_notify_after_recv() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        let handle = tokio::spawn(async move { inbox.recv().await.unwrap() });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        hub.dispatch(op.clone(), "late".to_string(), 4);
        let v = tokio::time::timeout(std::time::Duration::from_millis(500), handle)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v, "late");
    }

    #[tokio::test]
    async fn ct_fail_all_wakes() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op1 = crate::pipeline_ids::OperationId::new();
        let op2 = crate::pipeline_ids::OperationId::new();
        let mut inbox1 = hub.register(op1.clone(), 0, true).unwrap();
        let mut inbox2 = hub.register(op2.clone(), 0, true).unwrap();
        let h1 = tokio::spawn(async move { inbox1.recv().await.unwrap_err() });
        let h2 = tokio::spawn(async move { inbox2.recv().await.unwrap_err() });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        hub.fail_all(CriticalTransportError::IngressUnavailable);
        let e1 = tokio::time::timeout(std::time::Duration::from_millis(500), h1)
            .await
            .unwrap()
            .unwrap();
        let e2 = tokio::time::timeout(std::time::Duration::from_millis(500), h2)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(e1, CriticalTransportError::IngressUnavailable);
        assert_eq!(e2, CriticalTransportError::IngressUnavailable);
    }

    #[tokio::test]
    async fn ct_fail_registered_before_epoch() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        // simulate epoch bump from 0 to 1
        hub.fail_registered_before_epoch(1, CriticalTransportError::IngressOverflow);
        let res = inbox.recv().await;
        assert_eq!(res.unwrap_err(), CriticalTransportError::IngressOverflow);
    }

    #[test]
    fn ingress_unavailable_if_not_alive() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let res = hub.register(op, 0, false);
        assert!(matches!(
            res,
            Err(CriticalTransportError::IngressUnavailable)
        ));
    }

    #[tokio::test]
    async fn ct_retire_sequential_reuse_beyond_limit() {
        // FIX A: sequential normal use with retire must not leak resident slots.
        // With MAX_RESIDENT_OPERATIONS=2, 12 sequential operations must all succeed.
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        for i in 0..12u32 {
            let op = crate::pipeline_ids::OperationId::new();
            let mut inbox = hub.register(op.clone(), i as u64, true).unwrap();
            hub.dispatch(op.clone(), format!("msg-{i}"), 5);
            let v = inbox.recv().await.unwrap();
            assert_eq!(v, format!("msg-{i}"));
            // Normal finish retires exact mailbox (simulating BrowserState::finish_active_operation)
            hub.retire_exact(&op);
            // Drop also retires (idempotent) — explicitly drop to exercise Drop
            drop(inbox);
            // Verify slot freed: we can register next immediately without hitting limit
        }
        // Final check: after 12 retires, we can still register 2 concurrent
        let op_a = crate::pipeline_ids::OperationId::new();
        let op_b = crate::pipeline_ids::OperationId::new();
        let _a = hub.register(op_a, 100, true).unwrap();
        let _b = hub.register(op_b, 100, true).unwrap();
        let op_c = crate::pipeline_ids::OperationId::new();
        assert!(
            hub.register(op_c, 100, true).is_err(),
            "third concurrent should still be rejected"
        );
    }

    #[tokio::test]
    async fn ct_hundred_sequential_operations() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        for i in 0..100u32 {
            let op = crate::pipeline_ids::OperationId::new();
            let mut inbox = hub.register(op.clone(), i as u64, true).unwrap();
            hub.dispatch(op.clone(), format!("seq-{i}"), 6);
            let v = tokio::time::timeout(std::time::Duration::from_millis(100), inbox.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(v, format!("seq-{i}"));
            hub.retire_exact(&op);
            drop(inbox);
        }
    }

    #[tokio::test]
    async fn ct_inbox_drop_without_explicit_close_frees_slot() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op1 = crate::pipeline_ids::OperationId::new();
        let op2 = crate::pipeline_ids::OperationId::new();
        let op3 = crate::pipeline_ids::OperationId::new();
        let inbox1 = hub.register(op1.clone(), 0, true).unwrap();
        let _inbox2 = hub.register(op2.clone(), 0, true).unwrap();
        // inbox1 dropped without explicit retire/close — Drop must retire
        drop(inbox1);
        // Give Drop a moment (sync retire, no await needed)
        tokio::task::yield_now().await;
        let res = hub.register(op3.clone(), 0, true);
        assert!(res.is_ok(), "slot should be freed by Drop");
        // cleanup
        hub.retire_exact(&op2);
        hub.retire_exact(&op3);
    }

    #[tokio::test]
    async fn ct_reset_while_recv_waits_wakes_with_closed() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        let mut recv_handle = tokio::spawn(async move { inbox.recv().await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Simulate reset_for_session: retire exact wakes waiter
        hub.retire_exact(&op);
        let res = tokio::time::timeout(std::time::Duration::from_millis(500), recv_handle)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(res.unwrap_err(), CriticalTransportError::Closed);
    }

    #[tokio::test]
    async fn ct_late_event_after_retirement_ignored() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        hub.dispatch(op.clone(), "first".to_string(), 5);
        assert_eq!(inbox.recv().await.unwrap(), "first");
        hub.retire_exact(&op);
        // Late dispatch for retired id must be ignored, not panic, and not affect new op
        hub.dispatch(op.clone(), "late".to_string(), 4);
        // New operation with same ID after retirement should be registerable (IDs are UUIDs, but we test same ID reuse is allowed after retire)
        let mut inbox2 = hub.register(op.clone(), 1, true).unwrap();
        hub.dispatch(op.clone(), "new".to_string(), 3);
        assert_eq!(inbox2.recv().await.unwrap(), "new");
        hub.retire_exact(&op);
    }

    #[tokio::test]
    async fn ct_stale_id_cannot_retire_other() {
        let hub: CriticalEventHub<String> = CriticalEventHub::new();
        let op1 = crate::pipeline_ids::OperationId::new();
        let op2 = crate::pipeline_ids::OperationId::new();
        let mut inbox1 = hub.register(op1.clone(), 0, true).unwrap();
        let mut inbox2 = hub.register(op2.clone(), 0, true).unwrap();
        // Retire stale op1 should not affect op2
        let stale = crate::pipeline_ids::OperationId::new();
        hub.retire_exact(&stale);
        // op2 should still be alive
        hub.dispatch(op2.clone(), "alive".to_string(), 5);
        assert_eq!(inbox2.recv().await.unwrap(), "alive");
        // Retire op1, op2 still alive
        hub.retire_exact(&op1);
        hub.dispatch(op2.clone(), "still".to_string(), 5);
        assert_eq!(inbox2.recv().await.unwrap(), "still");
        // op1's inbox should now get Closed on next recv
        let res = inbox1.recv().await;
        assert_eq!(res.unwrap_err(), CriticalTransportError::Closed);
        hub.retire_exact(&op2);
    }

    #[test]
    fn ct_capacity_finite_bound() {
        assert_eq!(CRITICAL_INGRESS_CAPACITY, 256);
        assert_eq!(MAX_CRITICAL_EVENT_BYTES, 64 * 1024);
        // Formal worst-case ingress payload bound:
        // queue (256 slots) * 64 KiB per event = 16 MiB
        // hub per-operation payload (2 MiB * 2 ops) = 4 MiB
        // total bounded < 20 MiB + overhead, well within 2 GB target
        let worst_queue = CRITICAL_INGRESS_CAPACITY * MAX_CRITICAL_EVENT_BYTES;
        let worst_hub = MAX_RESIDENT_OPERATIONS * MAX_OPERATION_PAYLOAD_BYTES;
        let total = worst_queue + worst_hub;
        assert_eq!(worst_queue, 16 * 1024 * 1024);
        assert_eq!(worst_hub, 4 * 1024 * 1024);
        assert_eq!(total, 20 * 1024 * 1024);
        assert!(total < 2 * 1024 * 1024 * 1024);
    }
}
