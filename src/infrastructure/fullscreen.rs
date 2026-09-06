use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const TTL: std::time::Duration = std::time::Duration::from_secs(1);

static CACHE: OnceLock<Mutex<(Option<bool>, Instant)>> = OnceLock::new();

pub fn parse_quiet(out: &str) -> bool {
    // hyprctl batch emits consecutive JSON documents, not a JSON array.
    let mut documents = serde_json::Deserializer::from_str(out).into_iter::<serde_json::Value>();
    let (Some(Ok(workspace)), Some(Ok(clients))) = (documents.next(), documents.next()) else {
        return false;
    };
    let (Some(id), Some(clients)) = (workspace["id"].as_i64(), clients.as_array()) else {
        return false;
    };
    clients.iter().any(|client| {
        client["workspace"]["id"].as_i64() == Some(id)
            && client["mapped"].as_bool() == Some(true)
            && client["hidden"].as_bool() == Some(false)
            && matches!(client["fullscreen"].as_u64(), Some(2 | 3))
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
        Command::new("hyprctl").args(["-j", "--batch", "activeworkspace; clients"]),
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
        let output = workspace_output(2, 5);
        assert!(query_command(
            Command::new("sh").args(["-c", "printf '%s' \"$1\"", "sh", &output]),
            std::time::Duration::from_secs(1),
        ));
        assert!(!query_command(
            Command::new("sh").args(["-c", "printf '%s' \"$1\"; exit 1", "sh", &output]),
            std::time::Duration::from_secs(1),
        ));
    }

    fn workspace_output(fullscreen: u8, workspace: i64) -> String {
        format!(
            r#"{{"id":5,"hasfullscreen":true}}
            [{{"workspace":{{"id":{workspace}}},"mapped":true,"hidden":false,
               "fullscreen":{fullscreen},"fullscreenClient":2}},
             {{"workspace":{{"id":5}},"mapped":true,"hidden":false,
               "fullscreen":0,"focusHistoryID":0}}]"#
        )
    }

    #[test]
    fn only_fullscreen_on_active_workspace_triggers_quiet_even_without_focus() {
        for mode in [0, 1, 2, 3, 20] {
            assert_eq!(parse_quiet(&workspace_output(mode, 5)), matches!(mode, 2 | 3));
            assert!(!parse_quiet(&workspace_output(mode, 6)));
        }
        let output = workspace_output(2, 5);
        assert!(!parse_quiet(&output.replace("\"mapped\":true", "\"mapped\":false")));
        assert!(!parse_quiet(&output.replace("\"hidden\":false", "\"hidden\":true")));
    }

    #[test]
    fn ignores_missing_or_malformed_workspace_and_clients() {
        assert!(!parse_quiet(""));
        assert!(!parse_quiet(r#"{"id":5}"#));
        assert!(!parse_quiet(r#"{"id":5} []"#));
        assert!(!parse_quiet(r#"{"id":5} ["#));
        assert!(!parse_quiet(r#"{} [{"fullscreen":2}]"#));
        assert!(!parse_quiet(r#"{"id":5} {}"#));
    }
}
