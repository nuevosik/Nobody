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
