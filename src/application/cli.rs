pub const USAGE: &str = "uso: nobody [center open|close|toggle | dismiss all|ID|app NOME | dnd on|off|toggle|status | list --json | history --json]";

pub enum Cli {
    Daemon,
    CenterOpen,
    CenterClose,
    CenterToggle,
    DismissAll,
    Dismiss(u32),
    DismissApp(String),
    DndOn,
    DndOff,
    DndToggle,
    DndStatus,
    ListJson,
    HistoryJson,
    Bad(String),
}

pub fn parse(args: &[String]) -> Cli {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Cli::Daemon,
        ["center", "open"] => Cli::CenterOpen,
        ["center", "close"] => Cli::CenterClose,
        ["center", "toggle"] => Cli::CenterToggle,
        ["dismiss", "all"] => Cli::DismissAll,
        ["dismiss", id] => parse_id(id).map_or_else(|| Cli::Bad(USAGE.to_string()), Cli::Dismiss),
        ["dismiss", "app", app] if !app.trim().is_empty() => Cli::DismissApp((*app).to_string()),
        ["dnd", "on"] => Cli::DndOn,
        ["dnd", "off"] => Cli::DndOff,
        ["dnd", "toggle"] => Cli::DndToggle,
        ["dnd", "status"] => Cli::DndStatus,
        ["list", "--json"] => Cli::ListJson,
        ["history", "--json"] => Cli::HistoryJson,
        _ => Cli::Bad(USAGE.to_string()),
    }
}

fn parse_id(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
        .filter(|id| *id != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_every_supported_command() {
        assert!(matches!(parse(&args(&[])), Cli::Daemon));
        assert!(matches!(parse(&args(&["center", "open"])), Cli::CenterOpen));
        assert!(matches!(parse(&args(&["center", "close"])), Cli::CenterClose));
        assert!(matches!(parse(&args(&["center", "toggle"])), Cli::CenterToggle));
        assert!(matches!(parse(&args(&["dismiss", "all"])), Cli::DismissAll));
        assert!(matches!(parse(&args(&["dismiss", "1"])), Cli::Dismiss(1)));
        assert!(matches!(parse(&args(&["dismiss", "0001"])), Cli::Dismiss(1)));
        assert!(matches!(parse(&args(&["dismiss", "4294967295"])), Cli::Dismiss(u32::MAX)));
        assert!(matches!(parse(&args(&["dismiss", "42"])), Cli::Dismiss(42)));
        assert!(
            matches!(parse(&args(&["dismiss", "app", "My App"])), Cli::DismissApp(app) if app == "My App")
        );
        assert!(matches!(parse(&args(&["dnd", "on"])), Cli::DndOn));
        assert!(matches!(parse(&args(&["dnd", "off"])), Cli::DndOff));
        assert!(matches!(parse(&args(&["dnd", "toggle"])), Cli::DndToggle));
        assert!(matches!(parse(&args(&["dnd", "status"])), Cli::DndStatus));
        assert!(matches!(parse(&args(&["list", "--json"])), Cli::ListJson));
        assert!(matches!(parse(&args(&["history", "--json"])), Cli::HistoryJson));
    }

    #[test]
    fn rejects_unknown_or_partial_commands() {
        for words in [
            vec!["center"],
            vec!["center", "invalid"],
            vec!["center", "open", "extra"],
            vec!["center", "close", "extra"],
            vec!["dnd"],
            vec!["dnd", "oui"],
            vec!["foo"],
            vec!["center", "toggle", "extra"],
            vec!["list"],
            vec!["list", "extra"],
            vec!["list", "--json", "extra"],
            vec!["history"],
            vec!["history", "extra"],
            vec!["history", "--json", "extra"],
            vec!["dismiss"],
            vec!["dismiss", ""],
            vec!["dismiss", " "],
            vec!["dismiss", "0"],
            vec!["dismiss", "000"],
            vec!["dismiss", "letters"],
            vec!["dismiss", "+1"],
            vec!["dismiss", "-1"],
            vec!["dismiss", "١"],
            vec!["dismiss", "4294967296"],
            vec!["dismiss", "42", "extra"],
            vec!["dismiss", "app"],
            vec!["dismiss", "app", "   "],
            vec!["dismiss", "app", "Spotify", "extra"],
        ] {
            assert!(matches!(parse(&args(&words)), Cli::Bad(_)), "{words:?}");
        }
    }
}
