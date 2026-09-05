use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const TTL: std::time::Duration = std::time::Duration::from_secs(1);

static CACHE: OnceLock<Mutex<(Option<bool>, Instant)>> = OnceLock::new();

pub fn parse_quiet(out: &str) -> bool {
    out.lines().any(|line| {
        let mut parts = line.splitn(2, ':');
        parts.next().is_some_and(|k| k.trim() == "fullscreen")
            && parts
                .next()
                .is_some_and(|v| v.split_whitespace().next().is_some_and(|n| n == "2" || n == "3"))
    })
}

fn query() -> bool {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return false;
    }
    Command::new("hyprctl")
        .args(["activewindow"])
        .output()
        .is_ok_and(|o| o.status.success() && parse_quiet(&String::from_utf8_lossy(&o.stdout)))
}

pub fn quiet_mode() -> bool {
    let cache = CACHE.get_or_init(|| Mutex::new((None, Instant::now())));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if guard.0.is_none_or(|_| guard.1.elapsed() >= TTL) {
        guard.0 = Some(query());
        guard.1 = Instant::now();
    }
    guard.0.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_values_trigger_quiet() {
        assert!(parse_quiet("fullscreen: 2\n"));
        assert!(parse_quiet("  fullscreen: 3  \n"));
        assert!(!parse_quiet("fullscreen: 0\n"));
        assert!(!parse_quiet("fullscreen: 1\n"));
    }

    #[test]
    fn ignores_fullscreen_client_and_garbage() {
        let out = "fullscreenClient: 2\nfullscreen: 0\n";
        assert!(!parse_quiet(out));
        assert!(!parse_quiet(""));
        assert!(!parse_quiet("fullscreen:\n"));
        assert!(!parse_quiet("fullscreen: 20\n"));
    }
}
