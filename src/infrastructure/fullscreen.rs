use std::io::Read;
use std::process::{Command, Stdio};
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

fn query_command(command: &mut Command, timeout: std::time::Duration) -> bool {
    let Ok(mut child) = command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    let stdout = child.stdout.take().expect("stdout is piped");
    // Drain concurrently so a full pipe cannot prevent the child from exiting.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(64 * 1024).read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    reader
        .join()
        .ok()
        .and_then(Result::ok)
        .is_some_and(|bytes| success && parse_quiet(&String::from_utf8_lossy(&bytes)))
}

fn query() -> bool {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return false;
    }
    query_command(
        Command::new("hyprctl").arg("activewindow"),
        std::time::Duration::from_millis(500),
    )
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
    fn command_timeout_kills_and_reaps_child() {
        let start = Instant::now();
        assert!(!query_command(
            Command::new("sleep").arg("10"),
            std::time::Duration::from_millis(30),
        ));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn command_output_requires_successful_exit() {
        assert!(query_command(
            Command::new("sh").args(["-c", "printf 'fullscreen: 2\n'"]),
            std::time::Duration::from_secs(1),
        ));
        assert!(!query_command(
            Command::new("sh").args(["-c", "printf 'fullscreen: 2\n'; exit 1"]),
            std::time::Duration::from_secs(1),
        ));
    }

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
