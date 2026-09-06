use crate::domain::close::CloseReason;
use crate::domain::history::HistoryEntry;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;

pub fn snapshot(queue: &Queue) -> Vec<Notice> {
    queue.snapshot()
}

pub fn expire(queue: &Queue, now_ms: u128) -> Vec<Notice> {
    queue.remove_expired_at(now_ms)
}

pub fn request_dismissal(queue: &Queue, id: u32) {
    queue.request_close(id, CloseReason::DismissedByUser);
}

pub fn dismiss_all(queue: &Queue) {
    for notice in snapshot(queue) {
        request_dismissal(queue, notice.id);
    }
}

pub fn quiet_mode(queue: &Queue) -> bool {
    queue.is_quiet()
}

pub fn history(queue: &Queue) -> Vec<HistoryEntry> {
    queue.history_snapshot()
}

pub fn clear_history(queue: &Queue) {
    queue.clear_history();
}

/// Busca local por aplicativo, título ou corpo, sem distinguir maiúsculas.
/// Consulta vazia retorna tudo.
pub fn filter_history(entries: &[HistoryEntry], query: &str) -> Vec<HistoryEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|e| {
            e.notice.app.to_lowercase().contains(&q)
                || e.notice.summary.to_lowercase().contains(&q)
                || e.notice.body.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

pub fn manual_quiet(queue: &Queue) -> bool {
    queue.is_manual_quiet()
}

pub fn set_manual_quiet(queue: &Queue, quiet: bool) {
    queue.set_manual_quiet(quiet);
}

pub fn toggle_manual_quiet(queue: &Queue) -> bool {
    queue.toggle_manual_quiet()
}

pub fn effective_quiet(queue: &Queue) -> bool {
    queue.is_effective_quiet()
}

pub fn center_open(queue: &Queue) -> bool {
    queue.is_center_open()
}

pub fn set_center_open(queue: &Queue, open: bool) {
    queue.set_center_open(open);
}

pub fn toggle_center(queue: &Queue) -> bool {
    queue.toggle_center_open()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::close::CloseRequest;

    fn mk(app: &str, expire_ms: i32, arrived_at_ms: u128) -> Notice {
        Notice {
            id: 0,
            app: app.into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms,
            arrived_at_ms,
        }
    }

    #[test]
    fn expire_removes_only_expired() {
        let queue = Queue::new();
        queue.push_with_outcome(0, mk("Expired", 10, 100));
        queue.push_with_outcome(0, mk("Active", 0, 100));

        let removed = expire(&queue, 110);

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].app, "Expired");
        assert_eq!(snapshot(&queue).len(), 1);
        assert_eq!(snapshot(&queue)[0].app, "Active");
    }

    #[test]
    fn request_dismissal_enqueues_dismissed_by_user() {
        let queue = Queue::new();
        request_dismissal(&queue, 7);
        assert_eq!(
            queue.drain_close_requests(),
            vec![CloseRequest { id: 7, reason: CloseReason::DismissedByUser }]
        );
    }

    #[test]
    fn dismiss_all_requests_active_notifications_and_preserves_history() {
        let queue = Queue::new();
        queue.push_with_outcome(0, mk("A", 0, 1));
        queue.push_with_outcome(0, mk("B", 0, 2));
        let active_ids = snapshot(&queue).into_iter().map(|n| n.id).collect::<Vec<_>>();
        let history_before = history(&queue);

        dismiss_all(&queue);

        assert_eq!(
            queue.drain_close_requests(),
            active_ids
                .into_iter()
                .map(|id| CloseRequest { id, reason: CloseReason::DismissedByUser })
                .collect::<Vec<_>>()
        );
        assert_eq!(snapshot(&queue).len(), 2);
        assert_eq!(history(&queue), history_before);

        let empty = Queue::new();
        dismiss_all(&empty);
        assert!(empty.drain_close_requests().is_empty());
    }

    #[test]
    fn quiet_mode_passthrough() {
        let queue = Queue::new();
        assert!(!quiet_mode(&queue));
        queue.set_quiet(true);
        assert!(quiet_mode(&queue));
        queue.set_quiet(false);
        assert!(!quiet_mode(&queue));
    }

    #[test]
    fn effective_quiet_is_manual_or_auto() {
        let queue = Queue::new();
        for (manual, auto) in [(false, false), (true, false), (false, true), (true, true)] {
            set_manual_quiet(&queue, manual);
            queue.set_quiet(auto);
            assert_eq!(manual_quiet(&queue), manual);
            assert_eq!(effective_quiet(&queue), manual || auto);
        }
        set_manual_quiet(&queue, false);
        assert!(toggle_manual_quiet(&queue));
        assert!(!toggle_manual_quiet(&queue));
    }

    #[test]
    fn center_toggle_passthrough() {
        let queue = Queue::new();
        assert!(!center_open(&queue));
        assert!(toggle_center(&queue));
        set_center_open(&queue, false);
        assert!(!center_open(&queue));
    }

    fn history_entry(app: &str, summary: &str, body: &str) -> HistoryEntry {
        use crate::domain::notice::Notice;
        HistoryEntry {
            seq: 0,
            notice: Notice {
                id: 1,
                app: app.into(),
                summary: summary.into(),
                body: body.into(),
                icon: None,
                actions: vec![],
                expire_ms: 0,
                arrived_at_ms: 0,
            },
        }
    }

    #[test]
    fn filter_finds_all_three_fields_case_insensitive() {
        let entries = vec![
            history_entry("Firefox", "t", "b"),
            history_entry("a", "Atualização pronta", "b"),
            history_entry("a", "t", "CORPO com Token"),
        ];
        assert_eq!(filter_history(&entries, "fire").len(), 1);
        assert_eq!(filter_history(&entries, "FIREFOX").len(), 1);
        assert_eq!(filter_history(&entries, "atualização").len(), 1);
        assert_eq!(filter_history(&entries, "token").len(), 1);
        assert!(filter_history(&entries, "ausente").is_empty());
    }

    #[test]
    fn filter_empty_query_returns_everything() {
        let entries = vec![history_entry("A", "t", "b"), history_entry("B", "t", "b")];
        assert_eq!(filter_history(&entries, "").len(), 2);
        assert_eq!(filter_history(&entries, "   ").len(), 2);
    }
}
