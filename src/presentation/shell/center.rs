/// Texto do botão de Não Perturbe + nota de tela cheia.
pub fn dnd_button_label(manual: bool) -> &'static str {
    if manual { "Não Perturbe: on" } else { "Não Perturbe: off" }
}

pub fn fullscreen_note(manual: bool, auto: bool) -> Option<&'static str> {
    if !manual && auto { Some("Silêncio ativo por tela cheia") } else { None }
}

/// Idade legível a partir do relógio monotônico (ms desde o boot do daemon).
pub fn format_age(now_ms: u128, arrived_at_ms: u128) -> String {
    let elapsed_s = now_ms.saturating_sub(arrived_at_ms) / 1000;
    if elapsed_s < 5 {
        "agora".to_string()
    } else if elapsed_s < 60 {
        format!("há {elapsed_s}s")
    } else {
        format!("há {}min", elapsed_s / 60)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SearchKeyAction {
    InsertChar(String),
    Backspace,
    Paste,
    Close,
    FocusNext,
    FocusPrev,
    Ignore,
}

pub fn classify_search_key(
    key: &str,
    key_char: Option<&str>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    platform: bool,
    function: bool,
) -> SearchKeyAction {
    let clean = !ctrl && !platform && !function;
    match key {
        "escape" if clean && !alt => SearchKeyAction::Close,
        "tab" if clean && !alt => {
            if shift {
                SearchKeyAction::FocusPrev
            } else {
                SearchKeyAction::FocusNext
            }
        }
        "backspace" if clean && !alt => SearchKeyAction::Backspace,
        "v" if ctrl && !shift && !alt && !platform && !function => SearchKeyAction::Paste,
        _ if clean => match key_char {
            Some(text) if !text.is_empty() => SearchKeyAction::InsertChar(text.to_string()),
            _ => SearchKeyAction::Ignore,
        },
        _ => SearchKeyAction::Ignore,
    }
}

pub fn apply_search_edit(query: &mut String, action: &SearchKeyAction) -> bool {
    match action {
        SearchKeyAction::InsertChar(text) => {
            query.push_str(text);
            true
        }
        SearchKeyAction::Backspace => query.pop().is_some(),
        _ => false,
    }
}

pub fn dnd_state_changed(prev: (bool, bool), next: (bool, bool)) -> bool {
    prev != next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_boundaries() {
        assert_eq!(format_age(10_000, 10_000), "agora");
        assert_eq!(format_age(14_999, 10_000), "agora");
        assert_eq!(format_age(15_000, 10_000), "há 5s");
        assert_eq!(format_age(69_999, 10_000), "há 59s");
        assert_eq!(format_age(70_000, 10_000), "há 1min");
        assert_eq!(format_age(5_000, 10_000), "agora");
    }

    #[test]
    fn search_key_classification() {
        use SearchKeyAction::*;
        let plain = |key: &str, ch: Option<&str>| {
            classify_search_key(key, ch, false, false, false, false, false)
        };
        assert_eq!(
            classify_search_key("a", Some("A"), true, false, false, false, false),
            InsertChar("A".into())
        );
        assert_eq!(plain("a", Some("á")), InsertChar("á".into()));
        assert_eq!(
            classify_search_key("7", Some("{"), false, false, true, false, false),
            InsertChar("{".into())
        );
        assert_eq!(plain("space", Some(" ")), InsertChar(" ".into()));
        assert_eq!(plain("backspace", None), Backspace);
        assert_eq!(plain("escape", None), Close);
        assert_eq!(plain("tab", None), FocusNext);
        assert_eq!(classify_search_key("tab", None, true, false, false, false, false), FocusPrev);
        assert_eq!(classify_search_key("v", None, false, true, false, false, false), Paste);
        assert_eq!(classify_search_key("a", None, false, true, false, false, false), Ignore);
        assert_eq!(classify_search_key("tab", None, false, true, false, false, false), Ignore);
        assert_eq!(plain("enter", None), Ignore);
    }

    #[test]
    fn search_edit_applies_insert_and_backspace() {
        let mut q = String::new();
        assert!(apply_search_edit(&mut q, &SearchKeyAction::InsertChar("Á".into())));
        assert!(apply_search_edit(&mut q, &SearchKeyAction::Backspace));
        assert_eq!(q, "");
        assert!(!apply_search_edit(&mut q, &SearchKeyAction::Backspace));
        assert!(!apply_search_edit(&mut q, &SearchKeyAction::Ignore));
        assert!(!apply_search_edit(&mut q, &SearchKeyAction::Paste));
    }

    #[test]
    fn dnd_watch_fires_on_manual_change_under_fullscreen() {
        assert!(dnd_state_changed((false, true), (true, true)));
        assert!(dnd_state_changed((true, true), (false, true)));
        assert!(dnd_state_changed((false, false), (false, true)));
        assert!(!dnd_state_changed((false, true), (false, true)));
        assert!(!dnd_state_changed((true, false), (true, false)));
    }

    #[test]
    fn dnd_labels_and_fullscreen_note() {
        assert_eq!(dnd_button_label(true), "Não Perturbe: on");
        assert_eq!(dnd_button_label(false), "Não Perturbe: off");
        assert_eq!(fullscreen_note(false, true), Some("Silêncio ativo por tela cheia"));
        assert_eq!(fullscreen_note(true, true), None);
        assert_eq!(fullscreen_note(false, false), None);
    }
}
