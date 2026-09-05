use crate::domain::close::CloseReason;
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

pub fn quiet_mode(queue: &Queue) -> bool {
    queue.is_quiet()
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
    fn quiet_mode_passthrough() {
        let queue = Queue::new();
        assert!(!quiet_mode(&queue));
        queue.set_quiet(true);
        assert!(quiet_mode(&queue));
        queue.set_quiet(false);
        assert!(!quiet_mode(&queue));
    }
}
