use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub use crate::domain::close::{CloseReason, CloseRequest, PushOutcome};
use crate::domain::ids::{next_available_id, reserve_id};
use crate::domain::notice::Notice;

pub const KEEP: usize = 12;

#[derive(Clone)]
pub struct Queue {
    inner: Arc<Mutex<VecDeque<Notice>>>,
    close_requests: Arc<Mutex<VecDeque<CloseRequest>>>,
    next_id: Arc<AtomicU32>,
    quiet: Arc<AtomicBool>,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            close_requests: Arc::new(Mutex::new(VecDeque::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            quiet: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_quiet(&self, quiet: bool) {
        self.quiet.store(quiet, Ordering::Relaxed);
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet.load(Ordering::Relaxed)
    }

    pub fn push_with_outcome(&self, replaces: u32, mut notice: Notice) -> PushOutcome {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if replaces != 0 {
            if let Some(pos) = inner.iter().position(|n| n.id == replaces) {
                reserve_id(&self.next_id, replaces);
                inner.remove(pos);
                notice.id = replaces;
                inner.push_front(notice);
                return PushOutcome { id: replaces, evicted: Vec::new() };
            }
            reserve_id(&self.next_id, replaces);
        }

        let id = next_available_id(&self.next_id, &inner);
        notice.id = id;
        inner.push_front(notice);
        let mut evicted = Vec::new();
        while inner.len() > KEEP {
            if let Some(notice) = inner.pop_back() {
                evicted.push(notice);
            }
        }
        PushOutcome { id, evicted }
    }

    pub fn remove(&self, id: u32) -> Option<Notice> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let idx = inner.iter().position(|n| n.id == id)?;
        inner.remove(idx)
    }

    pub fn snapshot(&self) -> Vec<Notice> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    pub fn remove_expired_at(&self, now_ms: u128) -> Vec<Notice> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut expired = Vec::new();
        inner.retain(|notice| {
            if notice.is_expired_at(now_ms) {
                expired.push(notice.clone());
                false
            } else {
                true
            }
        });
        expired
    }

    pub fn request_close(&self, id: u32, reason: CloseReason) {
        if id == 0 {
            return;
        }

        let mut requests = self.close_requests.lock().unwrap_or_else(|e| e.into_inner());
        if requests.len() >= KEEP * 2 {
            requests.pop_front();
        }
        if let Some(existing) = requests.iter_mut().find(|r| r.id == id) {
            if existing.reason != reason && reason.priority() > existing.reason.priority() {
                existing.reason = reason;
            }
            return;
        }
        requests.push_back(CloseRequest { id, reason });
    }

    pub fn drain_close_requests(&self) -> Vec<CloseRequest> {
        self.close_requests.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::notice::Notice;

    fn mk(id: u32, app: &str) -> Notice {
        Notice {
            id,
            app: app.into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        }
    }

    fn push(q: &Queue, replaces: u32, notice: Notice) -> u32 {
        q.push_with_outcome(replaces, notice).id
    }

    #[test]
    fn caps_at_keep() {
        let q = Queue::new();
        for _ in 0..KEEP + 5 {
            push(&q, 0, mk(0, "A"));
        }
        assert_eq!(q.len(), KEEP);
    }

    #[test]
    fn reports_evicted_notifications() {
        let q = Queue::new();
        for _ in 0..KEEP {
            push(&q, 0, mk(0, "A"));
        }

        let outcome = q.push_with_outcome(0, mk(0, "B"));

        assert_eq!(outcome.evicted.len(), 1);
        assert_eq!(outcome.evicted[0].app, "A");
    }

    #[test]
    fn replaces_in_place() {
        let q = Queue::new();
        let id = push(&q, 0, mk(0, "A"));
        let id2 = push(&q, id, mk(0, "B"));
        assert_eq!(id, id2);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn replacement_keeps_the_requested_id_when_the_original_is_gone() {
        let q = Queue::new();
        let id = push(&q, 42, mk(0, "A"));

        assert_ne!(id, 42);
        assert_eq!(q.len(), 1);
        let id2 = push(&q, 0, mk(0, "B"));
        assert_ne!(id, id2);
    }

    #[test]
    fn removes_expired_notifications() {
        let q = Queue::new();
        let mut expired = mk(0, "Expired");
        expired.expire_ms = 10;
        expired.arrived_at_ms = 100;
        push(&q, 0, expired);
        push(&q, 0, mk(0, "Active"));

        let removed = q.remove_expired_at(110);

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].app, "Expired");
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn queues_each_close_request_once() {
        let q = Queue::new();
        q.request_close(7, CloseReason::DismissedByUser);
        q.request_close(7, CloseReason::ClosedByCall);

        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 7, reason: CloseReason::DismissedByUser }]
        );
    }

    #[test]
    fn close_request_ignores_id_zero() {
        let q = Queue::new();
        q.request_close(0, CloseReason::DismissedByUser);
        assert!(q.drain_close_requests().is_empty());
    }

    #[test]
    fn close_request_upgrades_to_more_specific_reason() {
        let q = Queue::new();
        q.request_close(1, CloseReason::Expired);
        q.request_close(1, CloseReason::DismissedByUser);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 1, reason: CloseReason::DismissedByUser }]
        );

        let q = Queue::new();
        q.request_close(2, CloseReason::Undefined);
        q.request_close(2, CloseReason::ClosedByCall);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 2, reason: CloseReason::ClosedByCall }]
        );
    }

    #[test]
    fn close_request_never_downgrades_reason() {
        let q = Queue::new();
        q.request_close(1, CloseReason::DismissedByUser);
        q.request_close(1, CloseReason::Expired);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 1, reason: CloseReason::DismissedByUser }]
        );

        let q = Queue::new();
        q.request_close(2, CloseReason::ClosedByCall);
        q.request_close(2, CloseReason::Undefined);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 2, reason: CloseReason::ClosedByCall }]
        );

        let q = Queue::new();
        q.request_close(3, CloseReason::Expired);
        q.request_close(3, CloseReason::Undefined);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 3, reason: CloseReason::Expired }]
        );
    }

    #[test]
    fn ids_stay_unique_when_replaces_is_missing() {
        let q = Queue::new();
        let id1 = push(&q, 0, mk(0, "A"));
        let ghost = push(&q, 999_999, mk(0, "Ghost"));
        assert_ne!(ghost, 999_999);
        assert_ne!(ghost, id1);
        let mut seen = std::collections::HashSet::new();
        for n in q.snapshot() {
            assert!(seen.insert(n.id), "ID duplicado no snapshot: {}", n.id);
        }
        let id3 = push(&q, 0, mk(0, "C"));
        assert!(!seen.contains(&id3), "novo ID reutilizou ID vivo: {id3}");
    }
}
