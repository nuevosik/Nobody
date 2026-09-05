use crate::domain::notice::Notice;

pub const HISTORY_KEEP: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    pub seq: u64,
    pub notice: Notice,
}
