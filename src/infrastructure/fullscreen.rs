use std::ffi::OsStr;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const QUERY_TIMEOUT: Duration = Duration::from_millis(500);
const SOCKET_POLL: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(120);
const RESYNC: Duration = Duration::from_secs(30);
const RETRY: Duration = Duration::from_secs(1);

static QUIET: AtomicBool = AtomicBool::new(false);
static CHANGES: OnceLock<event_listener::Event> = OnceLock::new();
static WATCHER: OnceLock<()> = OnceLock::new();

fn changes() -> &'static event_listener::Event {
    CHANGES.get_or_init(event_listener::Event::new)
}

fn publish(quiet: bool) {
    if QUIET.swap(quiet, Ordering::Relaxed) != quiet {
        changes().notify(usize::MAX);
    }
}

pub fn changed() -> event_listener::EventListener {
    changes().listen()
}

pub fn quiet_mode() -> bool {
    WATCHER.get_or_init(|| {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            std::thread::spawn(watch);
        }
    });
    QUIET.load(Ordering::Relaxed)
}

fn watch() {
    loop {
        let socket = socket_path(
            std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
            std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").as_deref(),
        )
        .and_then(|path| UnixStream::connect(path).ok());
        match socket {
            Some(stream) => read_events(stream),
            None => publish(query()),
        }
        std::thread::sleep(RETRY);
    }
}

fn read_events(stream: UnixStream) {
    if stream.set_read_timeout(Some(SOCKET_POLL)).is_err() {
        return;
    }
    publish(query());
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    let mut dirty = false;
    let mut last_event = Instant::now();
    let mut last_query = Instant::now();
    loop {
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) => {
                if line.ends_with(b"\n") {
                    if relevant(&String::from_utf8_lossy(&line)) {
                        dirty = true;
                        last_event = Instant::now();
                    }
                    line.clear();
                }
            }
            Err(error) if is_timeout(&error) => {
                if should_query(dirty, last_event.elapsed(), last_query.elapsed()) {
                    publish(query());
                    last_query = Instant::now();
                    dirty = false;
                }
            }
            Err(_) => return,
        }
    }
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn should_query(dirty: bool, since_last_event: Duration, since_last_query: Duration) -> bool {
    (dirty && since_last_event >= DEBOUNCE) || since_last_query >= RESYNC
}

fn relevant(line: &str) -> bool {
    let event = line.split(">>").next().unwrap_or("");
    !matches!(
        event,
        "windowtitle"
            | "activewindow"
            | "activewindowv2"
            | "submap"
            | "screencast"
            | "urgent"
            | "activelayout"
            | "configreloaded"
            | "openlayer"
            | "closelayer"
            | "lockgroups"
            | "pin"
    )
}

fn socket_path(xdg_runtime_dir: Option<&OsStr>, signature: Option<&OsStr>) -> Option<PathBuf> {
    let dir = xdg_runtime_dir.filter(|value| !value.is_empty())?;
    let signature = signature.filter(|value| !value.is_empty())?;
    Some(PathBuf::from(dir).join("hypr").join(signature).join(".socket2.sock"))
}

pub fn parse_quiet(out: &str) -> bool {
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

pub(crate) fn command_output(
    command: &mut Command,
    timeout: std::time::Duration,
) -> Option<Vec<u8>> {
    let Ok(mut child) = command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return None;
    };
    let stdout = child.stdout.take().expect("stdout is piped");
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
    let bytes = reader.join().ok().and_then(Result::ok)?;
    success.then_some(bytes)
}

fn query_command(command: &mut Command, timeout: std::time::Duration) -> bool {
    command_output(command, timeout)
        .is_some_and(|bytes| parse_quiet(&String::from_utf8_lossy(&bytes)))
}

fn query() -> bool {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return false;
    }
    query_command(
        Command::new("hyprctl").args(["-j", "--batch", "activeworkspace; clients"]),
        QUERY_TIMEOUT,
    )
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

    #[test]
    fn only_relevant_events_invalidate_the_fullscreen_state() {
        for line in [
            "workspace>>2",
            "focusedmon>>DP-1,1",
            "openwindow>>abc,1,kitty",
            "closewindow>>abc",
            "movewindow>>abc,2",
            "fullscreen>>1",
            "minimize>>abc,1",
            "changefloatingmode>>abc,1",
            "monitorremoved>>DP-1",
        ] {
            assert!(relevant(line), "{line} deveria invalidar o estado");
        }
        for line in [
            "windowtitle>>abc,title",
            "activewindowv2>>abc",
            "submap>>",
            "screencast>>1,1",
            "urgent>>abc",
            "activelayout>>kbd,us",
            "configreloaded>>",
            "openlayer>>ns",
            "closelayer>>ns",
            "lockgroups>>abc",
            "pin>>abc,1",
        ] {
            assert!(!relevant(line), "{line} não deveria invalidar o estado");
        }
    }

    #[test]
    fn debounce_and_resync_decide_queries() {
        let just_now = Duration::ZERO;
        let fresh = Duration::from_millis(1);
        assert!(!should_query(false, just_now, fresh));
        assert!(!should_query(true, just_now, fresh));
        assert!(should_query(true, DEBOUNCE, fresh));
        assert!(should_query(false, just_now, RESYNC));
        assert!(should_query(true, just_now, RESYNC));
    }

    #[test]
    fn socket_path_requires_both_environment_values() {
        let dir = OsStr::new("/run/user/1000");
        let signature = OsStr::new("hypr-abc");
        assert_eq!(
            socket_path(Some(dir), Some(signature)),
            Some(PathBuf::from("/run/user/1000/hypr/hypr-abc/.socket2.sock"))
        );
        assert_eq!(socket_path(None, Some(signature)), None);
        assert_eq!(socket_path(Some(dir), None), None);
        assert_eq!(socket_path(Some(OsStr::new("")), Some(signature)), None);
    }
}
