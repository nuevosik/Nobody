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

/// Atualiza a busca a partir de uma tecla do GPUI.
/// Retorna true quando a consulta mudou.
pub fn apply_search_key(query: &mut String, key: &str) -> bool {
    match key {
        "backspace" => query.pop().is_some(),
        "space" => {
            query.push(' ');
            true
        }
        k if k.chars().count() == 1 => {
            query.push_str(k);
            true
        }
        _ => false,
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
    fn search_keys_type_erase_and_ignore_specials() {
        let mut q = String::new();
        assert!(apply_search_key(&mut q, "a"));
        assert!(apply_search_key(&mut q, "B"));
        assert!(apply_search_key(&mut q, "space"));
        assert_eq!(q, "aB ");
        assert!(apply_search_key(&mut q, "backspace"));
        assert_eq!(q, "aB");
        assert!(!apply_search_key(&mut q, "enter"));
        assert!(!apply_search_key(&mut q, "escape"));
        assert!(!apply_search_key(&mut q, "shift"));
        assert_eq!(q, "aB");
        assert!(!apply_search_key(&mut String::new(), "backspace"));
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
