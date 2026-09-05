use crate::application::clock;
use crate::domain::notice::Notice;

use super::{anim, geometry};

#[derive(Clone, Default)]
pub struct Stack {
    pub notices: Vec<Notice>,
}

pub struct Exiting {
    pub notice: Notice,
    pub start_ms: u128,
    pub y: f32,
}

pub fn apply_snapshot(
    stack: &mut Stack,
    exiting: &mut Vec<Exiting>,
    snapshot: Vec<Notice>,
) -> bool {
    let now_ms = clock::now_ms();
    let mut removed: Vec<(u32, f32)> = Vec::new();
    for (idx, old) in stack.notices.iter().enumerate() {
        if !snapshot.iter().any(|n| n.id == old.id) {
            removed.push((old.id, geometry::grouped_y(&stack.notices, idx)));
        }
    }
    for (id, y) in &removed {
        if let Some(old) = stack.notices.iter().find(|n| n.id == *id).cloned()
            && !exiting.iter().any(|e| e.notice.id == *id)
        {
            exiting.push(Exiting { notice: old, start_ms: now_ms, y: *y });
        }
    }
    exiting.retain(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS);

    if stack.notices != snapshot || !removed.is_empty() {
        stack.notices = snapshot;
        true
    } else {
        !exiting.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::notice::Notice;
    use crate::presentation::theme::STACK_TOP;

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

    #[test]
    fn identical_snapshot_reports_no_change() {
        let mut stack = Stack { notices: vec![mk(1, "A")] };
        let mut exiting = Vec::new();
        assert!(!apply_snapshot(&mut stack, &mut exiting, vec![mk(1, "A")]));
        assert!(exiting.is_empty());
    }

    #[test]
    fn added_notice_reports_change_by_id() {
        let mut stack = Stack { notices: vec![mk(1, "A")] };
        let mut exiting = Vec::new();
        assert!(apply_snapshot(&mut stack, &mut exiting, vec![mk(1, "A"), mk(2, "B")]));
        assert_eq!(stack.notices.len(), 2);
        assert!(exiting.is_empty());
    }

    #[test]
    fn removed_notice_moves_to_exiting_once() {
        let mut stack = Stack { notices: vec![mk(1, "A"), mk(2, "B")] };
        let mut exiting = Vec::new();
        assert!(apply_snapshot(&mut stack, &mut exiting, vec![mk(2, "B")]));
        assert_eq!(exiting.len(), 1);
        assert_eq!(exiting[0].notice.id, 1);
        assert!((exiting[0].y - STACK_TOP).abs() < 0.01);
        assert!(apply_snapshot(&mut stack, &mut exiting, vec![mk(2, "B")]));
        assert_eq!(exiting.len(), 1, "exiting não deve duplicar");
    }

    #[test]
    fn same_id_different_body_is_change_without_exiting() {
        let mut stack = Stack { notices: vec![mk(1, "A")] };
        let mut exiting = Vec::new();
        let mut updated = mk(1, "A");
        updated.body = "new".into();
        assert!(apply_snapshot(&mut stack, &mut exiting, vec![updated]));
        assert!(exiting.is_empty());
    }
}
