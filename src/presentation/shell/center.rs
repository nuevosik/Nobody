use std::ops::Range;

pub fn dnd_button_label(manual: bool) -> &'static str {
    if manual { "Não Perturbe: on" } else { "Não Perturbe: off" }
}

pub fn fullscreen_note(manual: bool, auto: bool) -> Option<&'static str> {
    if !manual && auto { Some("Silêncio ativo por tela cheia") } else { None }
}

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

pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn utf8_range_for_utf16(text: &str, range: Range<usize>) -> Range<usize> {
    let total = utf16_len(text);
    let start16 = range.start.min(total);
    let end16 = range.end.min(total).max(start16);
    let mut start8 = 0;
    let mut end8 = 0;
    let mut count = 0;
    for (byte, ch) in text.char_indices().chain(std::iter::once((text.len(), '\0'))) {
        if count > end16 {
            break;
        }
        if count <= start16 {
            start8 = byte;
        }
        end8 = byte;
        count += ch.len_utf16();
    }
    start8..end8
}

pub fn utf16_range_for_utf8(text: &str, range: Range<usize>) -> Range<usize> {
    let start8 = floor_char_boundary(text, range.start);
    let end8 = floor_char_boundary(text, range.end).max(start8);
    let total = utf16_len(text);
    let mut start16 = total;
    let mut end16 = total;
    let mut count = 0;
    for (byte, ch) in text.char_indices() {
        if byte == start8 {
            start16 = count;
        }
        if byte == end8 {
            end16 = count;
            break;
        }
        count += ch.len_utf16();
    }
    start16.min(end16)..end16
}

pub fn sanitize_ime_text(text: &str) -> String {
    text.replace('\n', " ")
}

pub fn ime_replace(
    query: &mut String,
    marked: &mut Option<Range<usize>>,
    range_utf16: Option<Range<usize>>,
    text: &str,
) {
    let range = match range_utf16 {
        Some(range) => utf8_range_for_utf16(query, range),
        None => marked.clone().unwrap_or(query.len()..query.len()),
    };
    query.replace_range(range, &sanitize_ime_text(text));
    *marked = None;
}

pub fn ime_mark(
    query: &mut String,
    marked: &mut Option<Range<usize>>,
    range_utf16: Option<Range<usize>>,
    text: &str,
) {
    let range = match range_utf16 {
        Some(range) => utf8_range_for_utf16(query, range),
        None => marked.clone().unwrap_or(query.len()..query.len()),
    };
    let inserted = sanitize_ime_text(text);
    let start = range.start;
    query.replace_range(range, &inserted);
    *marked = Some(start..start + inserted.len());
}

pub fn ime_unmark(marked: &mut Option<Range<usize>>) {
    *marked = None;
}

pub fn drop_marked(query: &mut String, marked: &mut Option<Range<usize>>) -> bool {
    match marked.take() {
        Some(range) => {
            let end = floor_char_boundary(query, range.end);
            let start = floor_char_boundary(query, range.start).min(end);
            query.replace_range(start..end, "");
            true
        }
        None => false,
    }
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
    fn ime_utf16_ranges_round_trip() {
        let text = "aéb中";
        assert_eq!(utf16_len(text), 4);
        assert_eq!(utf8_range_for_utf16(text, 0..4), 0..text.len());
        assert_eq!(utf8_range_for_utf16(text, 1..2), 1..3);
        assert_eq!(utf8_range_for_utf16(text, 3..4), 4..text.len());
        assert_eq!(utf8_range_for_utf16(text, 4..4), text.len()..text.len());
        assert_eq!(utf8_range_for_utf16(text, 9..12), text.len()..text.len());
        assert_eq!(utf16_range_for_utf8(text, 1..3), 1..2);
        assert_eq!(utf16_range_for_utf8(text, 0..0), 0..0);
        assert_eq!(utf16_range_for_utf8(text, 7..7), 4..4);
        assert_eq!(utf8_range_for_utf16("", 0..0), 0..0);
    }

    #[test]
    fn ime_surrogate_boundaries_preserve_following_text() {
        let text = "a😀b";
        assert_eq!(utf8_range_for_utf16(text, 1..2), 1..1);
        assert_eq!(utf8_range_for_utf16(text, 2..2), 1..1);
        assert_eq!(utf8_range_for_utf16(text, 2..3), 1..5);
        assert_eq!(utf8_range_for_utf16(text, 1..3), 1..5);
        assert_eq!(utf8_range_for_utf16(text, 3..4), 5..6);
        let mut query = text.to_string();
        ime_replace(&mut query, &mut None, Some(1..2), "X");
        assert_eq!(query, "aX😀b");
        let mut query = text.to_string();
        ime_replace(&mut query, &mut None, Some(1..3), "X");
        assert_eq!(query, "aXb");
    }

    #[test]
    fn ime_replace_mark_lifecycle() {
        let mut query = String::from("ab");
        let mut marked = None;
        ime_mark(&mut query, &mut marked, None, "´");
        assert_eq!(query, "ab´");
        assert_eq!(marked, Some(2..4));
        ime_replace(&mut query, &mut marked, None, "á");
        assert_eq!(query, "abá");
        assert_eq!(marked, None);
        ime_replace(&mut query, &mut marked, Some(0..1), "中");
        assert_eq!(query, "中bá");
        ime_mark(&mut query, &mut marked, Some(0..1), "x");
        assert_eq!(query, "xbá");
        assert_eq!(marked, Some(0..1));
        assert_eq!(utf16_range_for_utf8(&query, marked.clone().unwrap()), 0..1);
        ime_unmark(&mut marked);
        assert_eq!(marked, None);
        assert_eq!(query, "xbá");
        assert!(!drop_marked(&mut query, &mut marked));
        marked = Some(0..1);
        assert!(drop_marked(&mut query, &mut marked));
        assert_eq!(query, "bá");
        assert_eq!(marked, None);
        marked = Some(0..99);
        assert!(drop_marked(&mut query, &mut marked));
        assert_eq!(query, "");
        let mut spaced = String::from("a\nb");
        ime_replace(&mut spaced, &mut None, None, "x\ny");
        assert_eq!(spaced, "a\nbx y");
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
