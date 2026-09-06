pub const USAGE: &str = "uso: nobody [center toggle | dismiss all | dnd on|off|toggle|status]";

pub enum Cli {
    Daemon,
    CenterToggle,
    DismissAll,
    DndOn,
    DndOff,
    DndToggle,
    DndStatus,
    Bad(String),
}

pub fn parse(args: &[String]) -> Cli {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Cli::Daemon,
        ["center", "toggle"] => Cli::CenterToggle,
        ["dismiss", "all"] => Cli::DismissAll,
        ["dnd", "on"] => Cli::DndOn,
        ["dnd", "off"] => Cli::DndOff,
        ["dnd", "toggle"] => Cli::DndToggle,
        ["dnd", "status"] => Cli::DndStatus,
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
        assert!(matches!(parse(&args(&["center", "toggle"])), Cli::CenterToggle));
        assert!(matches!(parse(&args(&["dismiss", "all"])), Cli::DismissAll));
        assert!(matches!(parse(&args(&["dnd", "on"])), Cli::DndOn));
        assert!(matches!(parse(&args(&["dnd", "off"])), Cli::DndOff));
        assert!(matches!(parse(&args(&["dnd", "toggle"])), Cli::DndToggle));
        assert!(matches!(parse(&args(&["dnd", "status"])), Cli::DndStatus));
    }

    #[test]
    fn rejects_unknown_or_partial_commands() {
        for words in [
            vec!["center"],
            vec!["center", "open"],
            vec!["dnd"],
            vec!["dnd", "oui"],
            vec!["foo"],
            vec!["center", "toggle", "extra"],
        ] {
            assert!(matches!(parse(&args(&words)), Cli::Bad(_)), "{words:?}");
        }
    }
}
