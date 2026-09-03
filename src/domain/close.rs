//! Domain — motivos de fechamento e resultado de push.
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
