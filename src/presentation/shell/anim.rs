use crate::application::clock;

pub const ENTER_MS: u128 = 380;
pub const EXIT_MS: u128 = 260;

pub fn ease_out_cubic(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

pub fn enter_progress(arrived_at_ms: u128) -> f32 {
    let elapsed = clock::elapsed_ms(arrived_at_ms) as f32 / ENTER_MS as f32;
    ease_out_cubic(elapsed.clamp(0., 1.))
}

pub fn exit_progress(start_ms: u128) -> f32 {
    let elapsed = clock::elapsed_ms(start_ms) as f32 / EXIT_MS as f32;
    ease_out_cubic(elapsed.clamp(0., 1.))
}

fn is_truthy(v: &str) -> bool {
    v == "1" || v.to_lowercase() == "true"
}

pub fn prefers_reduced_motion() -> bool {
    std::env::var("PREFERS_REDUCED_MOTION").is_ok_and(|v| is_truthy(&v))
        || std::env::var("REDUCED_MOTION").is_ok_and(|v| is_truthy(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, val) };
            Self { key, prev }
        }

        fn remove(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn reduced_motion_empty_or_zero_does_not_activate() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _p = EnvGuard::remove("PREFERS_REDUCED_MOTION");
        let _r = EnvGuard::set("REDUCED_MOTION", "");
        assert!(!prefers_reduced_motion(), "REDUCED_MOTION=\"\" não deveria ativar reduced motion");
        let _r0 = EnvGuard::set("REDUCED_MOTION", "0");
        assert!(
            !prefers_reduced_motion(),
            "REDUCED_MOTION=\"0\" não deveria ativar reduced motion"
        );
    }

    #[test]
    fn reduced_motion_truthy_values_activate() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _p = EnvGuard::remove("PREFERS_REDUCED_MOTION");
        for v in ["1", "true", "TRUE", "True"] {
            let _r = EnvGuard::set("REDUCED_MOTION", v);
            assert!(prefers_reduced_motion(), "REDUCED_MOTION={v:?} deveria ativar");
        }
    }

    #[test]
    fn reduced_motion_unset_is_false() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _p = EnvGuard::remove("PREFERS_REDUCED_MOTION");
        let _r = EnvGuard::remove("REDUCED_MOTION");
        assert!(!prefers_reduced_motion());
    }

    #[test]
    fn prefers_primary_var_1_disables() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _r = EnvGuard::remove("REDUCED_MOTION");
        let _p = EnvGuard::set("PREFERS_REDUCED_MOTION", "1");
        assert!(prefers_reduced_motion());
    }

    #[test]
    fn ease_out_cubic_boundaries() {
        assert!((ease_out_cubic(0.)).abs() < 1e-6);
        assert!((ease_out_cubic(1.) - 1.).abs() < 1e-6);
        let m = ease_out_cubic(0.5);
        assert!(m > 0.5 && m < 1., "ease-out deve acelerar no início");
    }

    #[test]
    fn future_arrival_has_zero_progress() {
        assert_eq!(enter_progress(u128::MAX), 0.);
        assert_eq!(exit_progress(u128::MAX), 0.);
    }
}
