use std::path::PathBuf;

pub const MAX_STACK_TAG_LEN: usize = 128;

pub fn is_valid_stack_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().count() <= MAX_STACK_TAG_LEN
}

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
    pub stack_tag: Option<String>,
    pub progress: Option<u8>,
}

impl Notice {
    pub fn has_action(&self, key: &str) -> bool {
        self.actions.as_chunks::<2>().0.iter().any(|pair| pair[0] == key)
    }

    pub fn has_default_action(&self) -> bool {
        self.has_action("default")
    }

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
            stack_tag: None,
            progress: None,
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

    #[test]
    fn action_keys_require_a_complete_pair() {
        let mut n = notice(0, 0);
        n.actions = vec![
            "default".into(),
            "Abrir".into(),
            "settings".into(),
            "Configurar".into(),
            "orphan".into(),
        ];
        assert!(n.has_default_action());
        assert!(n.has_action("settings"));
        assert!(!n.has_action("orphan"));
        assert!(!n.has_action("missing"));
    }

    #[test]
    fn stack_tag_validation_counts_unicode_characters_without_trimming() {
        assert!(!is_valid_stack_tag(""));
        assert!(is_valid_stack_tag("volume"));
        assert!(is_valid_stack_tag(" "));
        assert!(is_valid_stack_tag(&"🦀".repeat(MAX_STACK_TAG_LEN)));
        assert!(!is_valid_stack_tag(&"🦀".repeat(MAX_STACK_TAG_LEN + 1)));
        assert!(is_valid_stack_tag(" tag "));
    }
}
