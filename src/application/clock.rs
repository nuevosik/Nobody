use std::sync::OnceLock;
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();

fn start() -> Instant {
    *START.get_or_init(Instant::now)
}

pub fn now_ms() -> u128 {
    start().elapsed().as_millis()
}

pub fn elapsed_ms(since: u128) -> u128 {
    now_ms().saturating_sub(since)
}

#[allow(dead_code)]
pub fn sleep_ms(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_increases() {
        let a = now_ms();
        std::thread::sleep(Duration::from_millis(5));
        let b = now_ms();
        assert!(b >= a);
        assert!(elapsed_ms(a) >= 5);
    }

    #[test]
    fn future_since_saturates_to_zero() {
        assert_eq!(elapsed_ms(u128::MAX), 0);
        assert_eq!(elapsed_ms(now_ms() + 1_000_000), 0);
    }
}
