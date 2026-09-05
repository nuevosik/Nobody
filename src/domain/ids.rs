use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::domain::notice::Notice;
use crate::domain::queue::KEEP;

pub(crate) fn next_available_id(next_id: &AtomicU32, notices: &VecDeque<Notice>) -> u32 {
    let mut attempts = 0;
    loop {
        let id = next_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.checked_add(1).unwrap_or(1))
            })
            .expect("the ID generator always produces a value");
        if !notices.iter().any(|notice| notice.id == id) {
            return id;
        }
        attempts += 1;
        if attempts > KEEP * 4 {
            for cand in 1..=u32::MAX {
                if !notices.iter().any(|n| n.id == cand) {
                    let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |_| {
                        Some(cand.checked_add(1).unwrap_or(1))
                    });
                    return cand;
                }
                if cand == u32::MAX {
                    break;
                }
            }
            return id;
        }
    }
}

pub(crate) fn reserve_id(next_id: &AtomicU32, id: u32) {
    if id == u32::MAX {
        let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current == u32::MAX).then_some(1)
        });
        return;
    }
    let next = id + 1;
    let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        (current <= id).then_some(next)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_id_never_rewinds_past_max() {
        use std::sync::atomic::Ordering;
        let next_id = AtomicU32::new(3);
        let before = next_id.load(Ordering::SeqCst);
        reserve_id(&next_id, u32::MAX);
        let after = next_id.load(Ordering::SeqCst);
        assert_eq!(after, before, "reserve(u32::MAX) não pode rebobinar {before} -> {after}");
    }
}
