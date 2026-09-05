pub const DEFAULT_EXPIRE_MS: i32 = 5_000;

pub fn effective_expire_timeout(requested_timeout: i32, is_critical: bool) -> i32 {
    if is_critical || requested_timeout == 0 {
        0
    } else if requested_timeout < 0 {
        DEFAULT_EXPIRE_MS
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
}
