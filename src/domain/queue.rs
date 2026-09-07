use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use crate::domain::action::ActionRequest;
pub use crate::domain::close::{CloseReason, CloseRequest, PushOutcome};
use crate::domain::history::{HISTORY_KEEP, HistoryEntry};
use crate::domain::ids::{next_available_id, reserve_id};
use crate::domain::notice::{Notice, is_valid_stack_tag};

pub const KEEP: usize = 12;

#[derive(Clone)]
pub struct Queue {
    inner: Arc<Mutex<VecDeque<Notice>>>,
    close_requests: Arc<Mutex<VecDeque<CloseRequest>>>,
    action_requests: Arc<Mutex<VecDeque<ActionRequest>>>,
    next_id: Arc<AtomicU32>,
    quiet: Arc<AtomicBool>,
    history: Arc<Mutex<VecDeque<HistoryEntry>>>,
    next_seq: Arc<AtomicU64>,
    manual_quiet: Arc<AtomicBool>,
    center_open: Arc<AtomicBool>,
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
            action_requests: Arc::new(Mutex::new(VecDeque::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            quiet: Arc::new(AtomicBool::new(false)),
            history: Arc::new(Mutex::new(VecDeque::new())),
            next_seq: Arc::new(AtomicU64::new(1)),
            manual_quiet: Arc::new(AtomicBool::new(false)),
            center_open: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_quiet(&self, quiet: bool) {
        self.quiet.store(quiet, Ordering::Relaxed);
        if quiet {
            self.set_center_open(false);
        }
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet.load(Ordering::Relaxed)
    }

    pub fn set_manual_quiet(&self, quiet: bool) {
        self.manual_quiet.store(quiet, Ordering::Relaxed);
    }

    pub fn toggle_manual_quiet(&self) -> bool {
        self.manual_quiet.fetch_xor(true, Ordering::Relaxed) ^ true
    }

    pub fn is_manual_quiet(&self) -> bool {
        self.manual_quiet.load(Ordering::Relaxed)
    }

    pub fn is_effective_quiet(&self) -> bool {
        self.is_manual_quiet() || self.is_quiet()
    }

    pub fn set_center_open(&self, open: bool) {
        self.center_open.store(open && !self.is_quiet(), Ordering::Relaxed);
    }

    pub fn toggle_center_open(&self) -> bool {
        if self.is_quiet() {
            self.set_center_open(false);
            return false;
        }
        self.center_open.fetch_xor(true, Ordering::Relaxed) ^ true
    }

    pub fn is_center_open(&self) -> bool {
        self.center_open.load(Ordering::Relaxed)
    }

    pub fn history_snapshot(&self) -> Vec<HistoryEntry> {
        self.history.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    pub fn clear_history(&self) {
        self.history.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn record_history(&self, replaces: u32, notice: Notice) {
        let mut history = self.history.lock().unwrap_or_else(|e| e.into_inner());
        if replaces != 0
            && let Some(pos) = history.iter().position(|e| e.notice.id == replaces)
        {
            let mut entry = history.remove(pos).expect("posição recém-encontrada");
            entry.notice = notice;
            history.push_front(entry);
            return;
        }
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        history.push_front(HistoryEntry { seq, notice });
        while history.len() > HISTORY_KEEP {
            history.pop_back();
        }
    }

    pub fn push_with_outcome(&self, replaces: u32, mut notice: Notice) -> PushOutcome {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if replaces != 0 {
            if let Some(pos) = inner.iter().position(|n| n.id == replaces) {
                reserve_id(&self.next_id, replaces);
                inner.remove(pos);
                notice.id = replaces;
                inner.push_front(notice.clone());
                self.record_history(replaces, notice);
                return PushOutcome { id: replaces, evicted: Vec::new() };
            }
            reserve_id(&self.next_id, replaces);
        } else if let Some(tag) = notice.stack_tag.as_deref()
            && is_valid_stack_tag(tag)
            && let Some(pos) = inner.iter().position(|current| {
                current.app == notice.app && current.stack_tag.as_deref() == Some(tag)
            })
        {
            let id = inner[pos].id;
            inner.remove(pos);
            notice.id = id;
            inner.push_front(notice.clone());
            self.record_history(id, notice);
            return PushOutcome { id, evicted: Vec::new() };
        }

        let id = next_available_id(&self.next_id, &inner);
        notice.id = id;
        inner.push_front(notice.clone());
        let mut evicted = Vec::new();
        while inner.len() > KEEP {
            if let Some(notice) = inner.pop_back() {
                evicted.push(notice);
            }
        }
        self.record_history(0, notice);
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
        if let Some(existing) = requests.iter_mut().find(|r| r.id == id) {
            if existing.reason != reason && reason.priority() > existing.reason.priority() {
                existing.reason = reason;
            }
            return;
        }
        if requests.len() >= KEEP * 2 {
            requests.pop_front();
        }
        requests.push_back(CloseRequest { id, reason });
    }

    pub fn drain_close_requests(&self) -> Vec<CloseRequest> {
        self.close_requests.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }

    pub fn request_action(&self, id: u32, key: &str) {
        if id == 0 || key.is_empty() {
            return;
        }
        if !self
            .snapshot()
            .iter()
            .any(|notice| notice.id == id && (key != "default" || notice.has_default_action()))
            || self.has_pending_close(id)
        {
            return;
        }
        let mut requests = self.action_requests.lock().unwrap_or_else(|e| e.into_inner());
        if requests.iter().any(|request| request.id == id && request.key == key) {
            return;
        }
        if requests.len() >= KEEP * 2 {
            requests.pop_front();
        }
        requests.push_back(ActionRequest { id, key: key.to_string() });
    }

    pub fn drain_action_requests(&self) -> Vec<ActionRequest> {
        self.action_requests.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }

    pub fn has_pending_close(&self, id: u32) -> bool {
        self.close_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|request| request.id == id)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

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
            stack_tag: None,
            progress: None,
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
    fn tag_replaces_active_notice_and_updates_one_history_entry() {
        let q = Queue::new();
        let mut first = mk(0, "Mixer");
        first.summary = "old".into();
        first.expire_ms = 1_000;
        first.arrived_at_ms = 10;
        first.stack_tag = Some("volume".into());
        let id = push(&q, 0, first);

        let mut updated = mk(0, "Mixer");
        updated.summary = "new".into();
        updated.expire_ms = 2_000;
        updated.arrived_at_ms = 20;
        updated.stack_tag = Some("volume".into());
        assert_eq!(push(&q, 0, updated), id);

        let active = q.snapshot();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id);
        assert_eq!(active[0].summary, "new");
        assert_eq!(active[0].expire_ms, 2_000);
        assert_eq!(active[0].arrived_at_ms, 20);
        assert_eq!(q.history_snapshot().len(), 1);
        assert_eq!(q.history_snapshot()[0].notice.summary, "new");
    }

    #[test]
    fn tags_match_app_and_distinguish_other_tags_or_missing_tags() {
        let q = Queue::new();
        let mut first = mk(0, "Mixer");
        first.stack_tag = Some("volume".into());
        let first_id = push(&q, 0, first);

        let mut other_app = mk(0, "Player");
        other_app.stack_tag = Some("volume".into());
        let other_app_id = push(&q, 0, other_app);

        let mut other_tag = mk(0, "Mixer");
        other_tag.stack_tag = Some("brightness".into());
        let other_tag_id = push(&q, 0, other_tag);

        let no_tag_id = push(&q, 0, mk(0, "Mixer"));
        let another_no_tag_id = push(&q, 0, mk(0, "Mixer"));

        assert_eq!(q.snapshot().len(), 5);
        assert_eq!(
            [first_id, other_app_id, other_tag_id, no_tag_id, another_no_tag_id]
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            5
        );
    }

    #[test]
    fn explicit_replacement_wins_and_tag_uses_new_value_and_most_recent_notice() {
        let q = Queue::new();
        let mut initial = mk(0, "Mixer");
        initial.stack_tag = Some("old".into());
        let id = push(&q, 0, initial);

        let mut explicit = mk(0, "Mixer");
        explicit.summary = "explicit".into();
        explicit.stack_tag = Some("new".into());
        assert_eq!(push(&q, id, explicit), id);

        let mut old_tag = mk(0, "Mixer");
        old_tag.stack_tag = Some("old".into());
        let old_id = push(&q, 0, old_tag);
        assert_ne!(old_id, id);

        let mut explicit_missing = mk(0, "Mixer");
        explicit_missing.summary = "explicit missing".into();
        explicit_missing.stack_tag = Some("new".into());
        let duplicate_id = push(&q, 999_999, explicit_missing);
        assert_ne!(duplicate_id, id);
        assert_ne!(duplicate_id, 999_999);

        let mut by_tag = mk(0, "Mixer");
        by_tag.summary = "by tag".into();
        by_tag.stack_tag = Some("new".into());
        assert_eq!(push(&q, 0, by_tag), duplicate_id);

        assert_eq!(q.snapshot().len(), 3);
        assert_eq!(q.snapshot()[0].id, duplicate_id);
        assert_eq!(q.snapshot()[0].summary, "by tag");
        assert_eq!(q.history_snapshot().len(), 3);

        assert_eq!(push(&q, duplicate_id, mk(0, "Mixer")), duplicate_id);
        let mut after_removed_tag = mk(0, "Mixer");
        after_removed_tag.stack_tag = Some("new".into());
        let after_removed_tag_id = push(&q, 0, after_removed_tag);
        assert_ne!(after_removed_tag_id, duplicate_id);
    }

    #[test]
    fn action_requests_require_current_default_and_deduplicate_before_flush() {
        let q = Queue::new();
        let mut notice = mk(0, "App");
        notice.actions = vec!["default".into(), "Abrir".into()];
        let id = push(&q, 0, notice);

        q.request_action(id, "default");
        q.request_action(id, "default");
        assert_eq!(q.drain_action_requests(), vec![ActionRequest { id, key: "default".into() }]);

        q.request_close(id, CloseReason::DismissedByUser);
        q.request_action(id, "default");
        assert!(q.drain_action_requests().is_empty());
    }

    #[test]
    fn replacement_removing_default_invalidates_a_pending_action() {
        let q = Queue::new();
        let mut notice = mk(0, "App");
        notice.actions = vec!["default".into(), "Abrir".into()];
        let id = push(&q, 0, notice);
        q.request_action(id, "default");
        assert_eq!(q.drain_action_requests().len(), 1);

        let replacement = mk(0, "App");
        assert_eq!(push(&q, id, replacement), id);
        let active = q.snapshot();
        assert!(!active[0].has_default_action());
    }

    #[test]
    fn expired_or_removed_tag_notice_is_not_resurrected() {
        let q = Queue::new();
        let mut expired = mk(0, "Mixer");
        expired.stack_tag = Some("volume".into());
        expired.expire_ms = 10;
        expired.arrived_at_ms = 100;
        let old_id = push(&q, 0, expired);
        assert_eq!(q.remove_expired_at(110).len(), 1);

        let mut replacement = mk(0, "Mixer");
        replacement.stack_tag = Some("volume".into());
        let new_id = push(&q, 0, replacement);
        assert_ne!(new_id, old_id);
        assert_eq!(q.history_snapshot().len(), 2);
        assert_eq!(q.history_snapshot()[1].notice.id, old_id);

        q.remove(new_id);
        let mut after_remove = mk(0, "Mixer");
        after_remove.stack_tag = Some("volume".into());
        let final_id = push(&q, 0, after_remove);
        assert_ne!(final_id, new_id);
        assert_eq!(q.history_snapshot().len(), 3);
    }

    #[test]
    fn concurrent_same_tag_insertions_leave_one_active_and_historical_notice() {
        let q = Queue::new();
        let workers = 16;
        let barrier = Arc::new(Barrier::new(workers));
        let handles = (0..workers)
            .map(|i| {
                let q = q.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    let mut notice = mk(0, "Mixer");
                    notice.summary = format!("summary-{i}");
                    notice.stack_tag = Some("volume".into());
                    barrier.wait();
                    q.push_with_outcome(0, notice)
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("tag insertion thread");
        }

        assert_eq!(q.snapshot().len(), 1);
        assert_eq!(q.history_snapshot().len(), 1);
        assert!(q.snapshot()[0].summary.starts_with("summary-"));
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
    fn close_request_dedupe_does_not_evict_when_full() {
        let q = Queue::new();
        for id in 1..=(KEEP * 2) as u32 {
            let reason = if id == 1 { CloseReason::DismissedByUser } else { CloseReason::Expired };
            q.request_close(id, reason);
        }
        q.request_close(12, CloseReason::Expired);
        q.request_close(1, CloseReason::Expired);

        let mut expected = Vec::new();
        expected.push(CloseRequest { id: 1, reason: CloseReason::DismissedByUser });
        for id in 2..=(KEEP * 2) as u32 {
            expected.push(CloseRequest { id, reason: CloseReason::Expired });
        }
        assert_eq!(q.drain_close_requests(), expected);

        let q = Queue::new();
        for id in 1..=(KEEP * 2) as u32 {
            q.request_close(id, CloseReason::Expired);
        }
        q.request_close(999, CloseReason::Expired);

        let mut expected = Vec::new();
        for id in 2..=(KEEP * 2) as u32 {
            expected.push(CloseRequest { id, reason: CloseReason::Expired });
        }
        expected.push(CloseRequest { id: 999, reason: CloseReason::Expired });
        assert_eq!(q.drain_close_requests(), expected);
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

    #[test]
    fn history_caps_at_100_dropping_oldest_only() {
        use crate::domain::history::HISTORY_KEEP;
        let q = Queue::new();
        for i in 0..(HISTORY_KEEP + 1) {
            let mut n = mk(0, "A");
            n.summary = format!("s{i}");
            push(&q, 0, n);
        }
        let h = q.history_snapshot();
        assert_eq!(h.len(), HISTORY_KEEP);
        assert_eq!(h[0].notice.summary, format!("s{HISTORY_KEEP}"));
        assert_eq!(h[HISTORY_KEEP - 1].notice.summary, "s1");
    }

    #[test]
    fn history_replace_updates_moves_to_front_without_dup() {
        let q = Queue::new();
        let id = push(&q, 0, mk(0, "A"));
        push(&q, 0, mk(0, "B"));
        let mut updated = mk(0, "A2");
        updated.body = "novo".into();
        let id2 = push(&q, id, updated);
        assert_eq!(id, id2);
        let h = q.history_snapshot();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].notice.id, id);
        assert_eq!(h[0].notice.body, "novo");
        let seqs: std::collections::HashSet<u64> = h.iter().map(|e| e.seq).collect();
        assert_eq!(seqs.len(), 2, "seq própria não pode duplicar");
    }

    #[test]
    fn history_replace_after_close_creates_new_entry() {
        let q = Queue::new();
        let mut first = mk(0, "A");
        first.summary = "primeira".into();
        let id = push(&q, 0, first);
        q.remove(id);

        let mut ghost = mk(0, "Ghost");
        ghost.summary = "fantasma".into();
        let ghost_id = push(&q, id, ghost);
        assert_ne!(ghost_id, id);

        let h = q.history_snapshot();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].notice.summary, "fantasma");
        assert_eq!(h[1].notice.summary, "primeira");
        assert_eq!(h[1].notice.id, id);
    }

    #[test]
    fn history_ghost_replace_creates_new_entry() {
        let q = Queue::new();
        push(&q, 0, mk(0, "A"));
        push(&q, 999_999, mk(0, "Ghost"));
        assert_eq!(q.history_snapshot().len(), 2);
    }

    #[test]
    fn history_survives_expiry_and_removal() {
        let q = Queue::new();
        let mut expired = mk(0, "Expired");
        expired.expire_ms = 10;
        expired.arrived_at_ms = 100;
        let id = push(&q, 0, expired);
        q.remove_expired_at(1_000);
        assert!(q.remove(id).is_none());
        assert_eq!(q.history_snapshot().len(), 1);
        let live = push(&q, 0, mk(0, "Live"));
        q.remove(live);
        assert_eq!(q.history_snapshot().len(), 2);
    }

    #[test]
    fn clear_history_preserves_active_queue() {
        let q = Queue::new();
        push(&q, 0, mk(0, "A"));
        push(&q, 0, mk(0, "B"));
        q.clear_history();
        assert!(q.history_snapshot().is_empty());
        assert_eq!(q.snapshot().len(), 2);
        assert!(q.drain_close_requests().is_empty());
    }

    #[test]
    fn effective_quiet_covers_all_four_combos() {
        let q = Queue::new();
        for (manual, auto) in [(false, false), (true, false), (false, true), (true, true)] {
            q.set_manual_quiet(manual);
            q.set_quiet(auto);
            assert_eq!(q.is_effective_quiet(), manual || auto, "manual={manual} auto={auto}");
        }
        q.set_manual_quiet(true);
        q.set_quiet(false);
        assert!(q.is_manual_quiet());
    }

    #[test]
    fn center_toggle_roundtrips() {
        let q = Queue::new();
        assert!(!q.is_center_open());
        assert!(q.toggle_center_open());
        assert!(!q.toggle_center_open());
    }

    #[test]
    fn fullscreen_closes_center_and_blocks_reopening() {
        let q = Queue::new();
        assert!(q.toggle_center_open());
        q.set_quiet(true);
        assert!(!q.is_center_open());
        q.set_center_open(true);
        assert!(!q.is_center_open());
        assert!(!q.toggle_center_open());
        assert!(!q.is_center_open());
        q.set_quiet(false);
        assert!(!q.is_center_open());
        assert!(q.toggle_center_open());
        q.set_manual_quiet(true);
        assert!(q.is_center_open());
        assert!(!q.toggle_center_open());
        assert!(q.toggle_center_open());
    }
}
