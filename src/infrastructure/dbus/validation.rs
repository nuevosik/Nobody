//! Infrastructure/dbus — limites anti-DoS e validação.
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

pub const MAX_SUMMARY_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 500;
pub const MAX_ACTIONS: usize = 20;
pub const MAX_ACTION_LEN: usize = 64;
pub const MAX_HINTS: usize = 64;
pub const MAX_ICON_LEN: usize = 512;

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

pub(crate) fn is_critical(hints: &HashMap<String, OwnedValue>) -> bool {
    if let Some(v) = hints.get("urgency") {
        if let Ok(cloned) = v.try_clone()
            && let Ok(b) = u8::try_from(cloned)
        {
            return b >= 2;
        }
        if let Ok(cloned) = v.try_clone()
            && let Ok(n) = i32::try_from(cloned)
        {
            return n >= 2;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_critical_urgency() {
        let hints = HashMap::from([("urgency".into(), OwnedValue::from(2_u8))]);

        assert!(is_critical(&hints));
    }

    #[test]
    fn truncate_limits() {
        assert_eq!(truncate("abcdef", 3), "abc");
    }
}
