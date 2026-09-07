use crate::application::config::DEFAULT_TIMEOUT_MS;

pub const DEFAULT_EXPIRE_MS: i32 = DEFAULT_TIMEOUT_MS;

pub fn effective_expire_timeout(requested_timeout: i32, is_critical: bool) -> i32 {
    effective_expire_timeout_with_default(requested_timeout, is_critical, DEFAULT_EXPIRE_MS)
}

pub fn effective_expire_timeout_with_default(
    requested_timeout: i32,
    is_critical: bool,
    default_timeout_ms: i32,
) -> i32 {
    if is_critical || requested_timeout == 0 {
        0
    } else if requested_timeout < 0 {
        default_timeout_ms
    } else {
        requested_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_the_server_default_when_timeout_is_unspecified() {
        assert_eq!(effective_expire_timeout(-1, false), DEFAULT_EXPIRE_MS);
    }
    #[test]
    fn zero_and_critical_notifications_do_not_expire() {
        assert_eq!(effective_expire_timeout(0, false), 0);
        assert_eq!(effective_expire_timeout(-1, true), 0);
        assert_eq!(effective_expire_timeout(500, true), 0);
    }
    #[test]
    fn preserves_an_explicit_timeout_for_normal_notifications() {
        assert_eq!(effective_expire_timeout(500, false), 500);
    }
    #[test]
    fn other_negative_timeouts_fall_back_to_default() {
        assert_eq!(effective_expire_timeout(-999, false), DEFAULT_EXPIRE_MS);
        assert_eq!(effective_expire_timeout(-2, false), DEFAULT_EXPIRE_MS);
        assert_eq!(effective_expire_timeout(i32::MIN, false), DEFAULT_EXPIRE_MS);
    }
    #[test]
    fn critical_overrides_any_negative_timeout() {
        assert_eq!(effective_expire_timeout(-999, true), 0);
    }

    #[test]
    fn configured_default_applies_only_to_negative_requests() {
        assert_eq!(effective_expire_timeout_with_default(-1, false, 123), 123);
        assert_eq!(effective_expire_timeout_with_default(0, false, 123), 0);
        assert_eq!(effective_expire_timeout_with_default(456, false, 123), 456);
        assert_eq!(effective_expire_timeout_with_default(-1, true, 123), 0);
    }
}
