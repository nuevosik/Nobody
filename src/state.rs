//! Domain models — dados puros, sem dependência de UI ou D-Bus.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<PathBuf>,
    pub actions: Vec<String>,
    pub expire_ms: i32,
    pub arrived_at_ms: u128,
}

impl Notice {
    pub fn is_expired_at(&self, now_ms: u128) -> bool {
        self.expire_ms > 0 && now_ms.saturating_sub(self.arrived_at_ms) >= self.expire_ms as u128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(expire_ms: i32, arrived_at_ms: u128) -> Notice {
        Notice {
            id: 1,
            app: "A".into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms,
            arrived_at_ms,
        }
    }

    #[test]
    fn zero_or_negative_timeout_never_expires() {
        let n = notice(0, 100);
        assert!(!n.is_expired_at(100));
        assert!(!n.is_expired_at(u128::MAX));
        let neg = notice(-1, 100);
        assert!(!neg.is_expired_at(u128::MAX));
    }

    #[test]
    fn expires_at_exact_boundary() {
        let n = notice(10, 100);
        assert!(!n.is_expired_at(109));
        assert!(n.is_expired_at(110));
        assert!(n.is_expired_at(111));
    }

    #[test]
    fn future_arrival_is_not_expired() {
        let n = notice(10, 1_000);
        assert!(!n.is_expired_at(500));
    }
}

#[derive(Clone, Default)]
pub struct Stack {
    /// Mais recente primeiro.
    pub notices: Vec<Notice>,
}
