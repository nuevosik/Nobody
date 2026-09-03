//! Relógio monotônico para animações e timeouts.
//! Usa `Instant` para não quebrar se o relógio do sistema voltar.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();

fn start() -> Instant {
    *START.get_or_init(Instant::now)
}

/// Milissegundos desde o boot do processo (monotônico).
pub fn now_ms() -> u128 {
    start().elapsed().as_millis()
}

/// Elapsed desde `since` em ms (saturating).
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
        // `since` no futuro (ex: replaces com arrived_at adiantado) não pode
        // underflow: saturating_sub retorna 0. u128 nunca overflowa em ms
        // (~1e32 anos de uptime).
        assert_eq!(elapsed_ms(u128::MAX), 0);
        assert_eq!(elapsed_ms(now_ms() + 1_000_000), 0);
    }
}
