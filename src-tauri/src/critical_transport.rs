use crate::pipeline_ids::OperationId;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub const MAX_RESIDENT_OPERATIONS: usize = 2;
/// Maximum declared response chunks for one operation's chunked response.
/// `ResponseStart.chunk_count` admission is validated against this bound in
/// `browser_backend.rs`. A complete legal response additionally needs
/// Start/End/Done plus a small finite number of operation control events,
/// so the total per-operation mailbox budget is this plus headroom below.
pub const MAX_RESPONSE_CHUNKS: usize = 2_100;
/// Small finite headroom for non-chunk operation control events that share
/// the per-operation mailbox with a chunked response (ResponseStart,
/// ResponseEnd, Done, plus a bounded number of submit-report/control
/// signals). Kept small so the total stays within the 2 MiB / low-memory
/// budget; the 2 MiB payload bound is unchanged.
pub const MAX_OPERATION_CONTROL_EVENT_HEADROOM: usize = 16;
/// Total cumulative event budget for one resident operation: response chunks
/// plus the bound control/headroom events. Enforced as a cumulative
/// operation-lifetime total (not merely current queue depth), so a fast
/// consumer cannot evade the cap by `recv`ing between sends.
pub const MAX_OPERATION_CRITICAL_EVENTS: usize =
    MAX_RESPONSE_CHUNKS + MAX_OPERATION_CONTROL_EVENT_HEADROOM;
/// Semantic model-response content bound. The declared `ResponseStart` byte
/// length is admitted against this exactly (2 MiB unchanged).
pub const MAX_OPERATION_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
/// Separate finite allowance for bounded transport/control metadata
/// (ResponseStart/End/Done checksums, submit reports, protocol faults) so a
/// maximum legal response is never invalidated merely because the envelope
/// consumes a few bytes. Finite and deliberately small relative to content.
pub const MAX_OPERATION_CONTROL_PAYLOAD_HEADROOM: usize = 128 * 1024;
/// Cumulative per-operation transport payload budget applied by `dispatch`.
/// Response content stays capped at `MAX_OPERATION_PAYLOAD_BYTES`; this wider
/// ceiling absorbs bounded control metadata without weakening the 2 MiB limit.
pub const MAX_OPERATION_TOTAL_TRANSPORT_BYTES: usize =
    MAX_OPERATION_PAYLOAD_BYTES.saturating_add(MAX_OPERATION_CONTROL_PAYLOAD_HEADROOM);
/// Generic pre-ingress bound for any single critical event (whole responses,
/// manual responses, controls). Browser response chunks use the tighter
/// `MAX_RESPONSE_CHUNK_BYTES` rule instead.
pub const MAX_CRITICAL_EVENT_BYTES: usize = 64 * 1024;
/// Protocol-specific decoded byte bound for ONE browser response chunk. The
/// generator chunks at 1000 Unicode code points (at most ~4 KiB of UTF-8), so
/// this is the tight bound appropriate to that protocol even though some
/// bounded controls still use the generic 64 KiB ceiling.
pub const MAX_RESPONSE_CHUNK_BYTES: usize = 8 * 1024;
/// Global controls/fault/wake margin not charged to one operation.
pub const CRITICAL_INGRESS_SYSTEM_HEADROOM: usize = 32;
/// Fixed worst-case reservation. A conforming set of all admitted resident
/// operations can enqueue its complete event budget even if the bridge is
/// temporarily not draining. Derived from the admission contract — never a
/// magic literal.
pub const CRITICAL_INGRESS_CAPACITY: usize = MAX_RESIDENT_OPERATIONS
    .saturating_mul(MAX_OPERATION_CRITICAL_EVENTS)
    .saturating_add(CRITICAL_INGRESS_SYSTEM_HEADROOM);

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
    /// Monotonic operation-lifetime accounting. Never decremented by `recv()`.
    /// Admission budgets run against these totals, so consuming events from the
    /// queue cannot mask a budget overflow.
    accepted_events_total: usize,
    accepted_payload_bytes_total: usize,
    failed: Option<CriticalTransportError>,
    closed: bool,
    notify: Arc<Notify>,
    registration_epoch: u64,
}

struct HubInner<T> {
    operations: HashMap<OperationId, MailboxState<T>>,
    max_events: usize,
    max_payload_bytes: usize,
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
        Self::with_budgets(
            MAX_OPERATION_CRITICAL_EVENTS,
            MAX_OPERATION_TOTAL_TRANSPORT_BYTES,
        )
    }

    fn with_budgets(max_events: usize, max_payload_bytes: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HubInner {
                operations: HashMap::new(),
                max_events,
                max_payload_bytes,
            })),
        }
    }

    /// Test-only constructor with deliberately tiny budgets so cumulative
    /// event/payload accounting is exercisable without multi-MiB allocations
    /// or thousands of synthetic events.
    #[cfg(test)]
    pub fn new_for_budget_test(max_events: usize, max_payload_bytes: usize) -> Self {
        Self::with_budgets(max_events, max_payload_bytes)
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
            accepted_events_total: 0,
            accepted_payload_bytes_total: 0,
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
        let max_events = inner.max_events;
        let max_payload_bytes = inner.max_payload_bytes;
        let Some(state) = inner.operations.get_mut(&operation_id) else {
            // stale/unknown operation — ignore (old events)
            return;
        };
        if state.failed.is_some() || state.closed {
            return;
        }
        // Cumulative operation-lifetime event budget. Deliberately monotonic:
        // a fast consumer must not be able to evade the cap by recv'ing between
        // sends, so the accounted total is never decremented on recv.
        let next_events = match state.accepted_events_total.checked_add(1) {
            Some(next) => next,
            None => {
                state.failed = Some(CriticalTransportError::EventBudgetExceeded);
                state.notify.notify_one();
                return;
            }
        };
        if next_events > max_events {
            state.failed = Some(CriticalTransportError::EventBudgetExceeded);
            state.notify.notify_one();
            return;
        }
        // Cumulative payload budget with the same monotonic semantics. The
        // ceiling is the 2 MiB response content bound plus a separate bounded
        // control allowance, so Start/End/Done metadata can never invalidate a
        // maximum legal response.
        let next_payload = match state.accepted_payload_bytes_total.checked_add(payload_cost) {
            Some(next) => next,
            None => {
                state.failed = Some(CriticalTransportError::PayloadBudgetExceeded);
                state.notify.notify_one();
                return;
            }
        };
        if next_payload > max_payload_bytes {
            state.failed = Some(CriticalTransportError::PayloadBudgetExceeded);
            state.notify.notify_one();
            return;
        }
        // Only after BOTH cumulative checks pass do we enqueue.
        state.accepted_events_total = next_events;
        state.accepted_payload_bytes_total = next_payload;
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
        // Cumulative transport budget: response content (2 MiB) plus a separate
        // bounded control allowance; here exercised with a tiny test budget.
        // Two dispatches whose cumulative cost exceeds the ceiling must
        // sticky-fail the operation.
        let hub: CriticalEventHub<String> = CriticalEventHub::new_for_budget_test(100, 10);
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        hub.dispatch(op.clone(), "aaaaaa".to_string(), 6);
        // cumulative 6 + 6 > 10 base budget
        hub.dispatch(op.clone(), "bbbbbb".to_string(), 6);
        let res = inbox.recv().await;
        assert_eq!(
            res.unwrap_err(),
            CriticalTransportError::PayloadBudgetExceeded
        );
    }

    #[tokio::test]
    async fn ct_cumulative_event_does_not_reset_after_recv() {
        // Recv drains the queue but must never reset the operation-lifetime
        // event accounting: the cap cannot be evaded by fast consumption.
        let hub: CriticalEventHub<u32> = CriticalEventHub::new_for_budget_test(3, 1_000_000);
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        for i in 0..3 {
            hub.dispatch(op.clone(), i, 1);
            assert_eq!(inbox.recv().await.unwrap(), i);
        }
        // Cumulative total is already 3; the next event must fail even though
        // the queue is empty right now.
        hub.dispatch(op.clone(), 99, 1);
        let res = inbox.recv().await;
        assert_eq!(
            res.unwrap_err(),
            CriticalTransportError::EventBudgetExceeded
        );
        hub.retire_exact(&op);
    }

    #[tokio::test]
    async fn ct_cumulative_payload_does_not_reset_after_recv() {
        // Same monotonic semantics for the payload budget: draining between
        // sends must not allow the cumulative total to reset.
        let hub: CriticalEventHub<String> = CriticalEventHub::new_for_budget_test(100, 10);
        let op = crate::pipeline_ids::OperationId::new();
        let mut inbox = hub.register(op.clone(), 0, true).unwrap();
        hub.dispatch(op.clone(), "aaaaaa".to_string(), 6);
        assert_eq!(inbox.recv().await.unwrap(), "aaaaaa");
        hub.dispatch(op.clone(), "bbbbbb".to_string(), 6);
        // Queue is empty again but cumulative total (12) still exceeds budget (10).
        let res = inbox.recv().await;
        assert_eq!(
            res.unwrap_err(),
            CriticalTransportError::PayloadBudgetExceeded
        );
        hub.retire_exact(&op);
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
        assert_eq!(MAX_CRITICAL_EVENT_BYTES, 64 * 1024);
        assert_eq!(MAX_RESPONSE_CHUNK_BYTES, 8 * 1024);
        assert_eq!(MAX_OPERATION_PAYLOAD_BYTES, 2 * 1024 * 1024);
        assert_eq!(
            MAX_OPERATION_CRITICAL_EVENTS,
            MAX_RESPONSE_CHUNKS + MAX_OPERATION_CONTROL_EVENT_HEADROOM
        );
        assert_eq!(
            MAX_OPERATION_TOTAL_TRANSPORT_BYTES,
            MAX_OPERATION_PAYLOAD_BYTES + MAX_OPERATION_CONTROL_PAYLOAD_HEADROOM
        );
        // Formal worst-case ingress payload bound:
        // queue (derived capacity slots) * 64 KiB per event = ~267 MiB (string
        // ops only; response chunks use the 8 KiB decoded bound)
        // hub per-operation payload (2 MiB content + 128 KiB control, x2 ops)
        // total bounded well within the 2 GB target
        let worst_queue = CRITICAL_INGRESS_CAPACITY * MAX_CRITICAL_EVENT_BYTES;
        let worst_hub = MAX_RESIDENT_OPERATIONS * MAX_OPERATION_TOTAL_TRANSPORT_BYTES;
        let total = worst_queue + worst_hub;
        assert_eq!(worst_queue, 4_264 * 64 * 1024);
        assert!(total < 300 * 1024 * 1024);
        assert!(total < 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn critical_ingress_capacity_covers_all_resident_operation_budgets() {
        // The derived ingress capacity must be at least the sum of every
        // admitted resident operation's cumulative budget plus finite system
        // headroom, so a conforming set of operations can enqueue its complete
        // event budget while the bridge is temporarily not draining.
        assert_eq!(
            CRITICAL_INGRESS_CAPACITY,
            MAX_RESIDENT_OPERATIONS * MAX_OPERATION_CRITICAL_EVENTS
                + CRITICAL_INGRESS_SYSTEM_HEADROOM
        );
        for residents in 1..=MAX_RESIDENT_OPERATIONS {
            assert!(
                CRITICAL_INGRESS_CAPACITY
                    >= residents * MAX_OPERATION_CRITICAL_EVENTS + CRITICAL_INGRESS_SYSTEM_HEADROOM
            );
        }
        // A single event beyond a full conforming operation budget is NOT
        // conforming: admitted budget excludes the overflow margin entirely.
        assert!(
            (CRITICAL_INGRESS_SYSTEM_HEADROOM) < MAX_OPERATION_CRITICAL_EVENTS,
            "system headroom must stay strictly below the per-operation budget"
        );
    }
}
