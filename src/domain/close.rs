use crate::domain::notice::Notice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    DismissedByUser = 2,
    ClosedByCall = 3,
    Undefined = 4,
}

impl CloseReason {
    pub const fn code(self) -> u32 {
        self as u32
    }

    pub(crate) const fn priority(self) -> u8 {
        match self {
            CloseReason::DismissedByUser | CloseReason::ClosedByCall => 3,
            CloseReason::Expired => 2,
            CloseReason::Undefined => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub id: u32,
    pub reason: CloseReason,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PushOutcome {
    pub id: u32,
    pub evicted: Vec<Notice>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_codes_are_stable() {
        assert_eq!(CloseReason::Expired.code(), 1);
        assert_eq!(CloseReason::DismissedByUser.code(), 2);
        assert_eq!(CloseReason::ClosedByCall.code(), 3);
        assert_eq!(CloseReason::Undefined.code(), 4);
    }

    #[test]
    fn user_and_call_outrank_expiry_outranks_undefined() {
        assert_eq!(CloseReason::DismissedByUser.priority(), 3);
        assert_eq!(CloseReason::ClosedByCall.priority(), 3);
        assert_eq!(CloseReason::Expired.priority(), 2);
        assert_eq!(CloseReason::Undefined.priority(), 1);
        assert!(CloseReason::DismissedByUser.priority() > CloseReason::Expired.priority());
        assert!(CloseReason::Expired.priority() > CloseReason::Undefined.priority());
    }

    #[test]
    fn push_outcome_carries_id_and_evicted() {
        let notice = Notice {
            id: 9,
            app: "A".into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        };
        let outcome = PushOutcome { id: 1, evicted: vec![notice.clone()] };
        assert_eq!(outcome.id, 1);
        assert_eq!(outcome.evicted, vec![notice]);
        assert_ne!(outcome, PushOutcome { id: 2, evicted: vec![] });
    }
}
