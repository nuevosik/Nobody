use nobody::domain::notice::Notice;
use nobody::domain::queue::{KEEP, Queue};

fn mk(app: &str) -> Notice {
    Notice {
        id: 0,
        app: app.into(),
        summary: "s".into(),
        body: "".into(),
        icon: None,
        actions: vec![],
        expire_ms: 0,
        arrived_at_ms: 0,
    }
}

#[test]
fn caps_at_keep_and_reports_evicted() {
    let q = Queue::new();
    for _ in 0..KEEP {
        q.push_with_outcome(0, mk("A"));
    }
    let out = q.push_with_outcome(0, mk("B"));
    assert_eq!(q.snapshot().len(), KEEP);
    assert_eq!(out.evicted.len(), 1);
}

#[test]
fn ghost_replaces_does_not_squat() {
    let q = Queue::new();
    let ghost = q.push_with_outcome(999_999, mk("Ghost")).id;
    assert_ne!(ghost, 999_999);
}
