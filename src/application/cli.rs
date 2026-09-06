pub const USAGE: &str = "uso: nobody [center open|close|toggle | dismiss all | dnd on|off|toggle|status | list --json | history --json]";

pub enum Cli {
    Daemon,
    CenterOpen,
    CenterClose,
    CenterToggle,
    DismissAll,
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
        ["dnd", "on"] => Cli::DndOn,
        ["dnd", "off"] => Cli::DndOff,
        ["dnd", "toggle"] => Cli::DndToggle,
        ["dnd", "status"] => Cli::DndStatus,
        ["list", "--json"] => Cli::ListJson,
        ["history", "--json"] => Cli::HistoryJson,
        _ => Cli::Bad(USAGE.to_string()),
    }
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
        ] {
            assert!(matches!(parse(&args(&words)), Cli::Bad(_)), "{words:?}");
        }
    }
}
