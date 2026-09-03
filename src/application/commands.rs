//! Application — comandos finos sobre o domain.
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
