//! UI — card de notificação isolado.

use gpui::{FontWeight, ParentElement, Styled, StyledImage, div, img, prelude::*, px};

use crate::domain::notice::Notice;
use crate::presentation::theme::{
    CHIP, MUTED, TEXT, TEXT_BADGE, TEXT_BODY, TEXT_LABEL, TEXT_TITLE, app_font, fade,
};

/// Inicial do badge (fallback quando não há ícone).
/// Retorna UM caractere display em maiúscula; "?" se vazio.
/// Nota: `char::to_uppercase` pode mapear para múltiplos chars ('ß' → "SS");
/// pegamos só o primeiro para caber no glifo de 40px.
pub fn badge_initial(app: &str) -> String {
    app.chars()
        .next()
        .and_then(|c| c.to_uppercase().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".into())
}

pub fn badge(icon: Option<&std::path::PathBuf>, app: &str) -> gpui::Div {
    let base = div()
        .size(px(40.))
        .rounded(px(12.))
        .overflow_hidden()
        .bg(fade(CHIP, 1.))
        .border_1()
        .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.12 })
        .flex()
        .items_center()
        .justify_center();

    if let Some(path) = icon {
        // TOCTOU: lookup em icons.rs valida is_file(), mas o arquivo pode sumir
        // antes do render. Sem fallback o GPUI renderiza chip vazio; com fallback
        // mostramos a inicial (mesmo conteúdo do ramo sem-ícone).
        let initial = badge_initial(app);
        base.child(
            img(path.clone()).size(px(40.)).object_fit(gpui::ObjectFit::Cover).with_fallback(
                move || {
                    div()
                        .size(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(TEXT_BADGE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fade(TEXT, 1.))
                        .child(initial.clone())
                        .into_any_element()
                },
            ),
        )
    } else {
        let initial = badge_initial(app);
        base.font(app_font())
            .text_size(px(TEXT_BADGE))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(fade(TEXT, 1.))
            .child(initial)
    }
}

pub fn card_content(notice: &Notice) -> gpui::Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(TEXT_LABEL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fade(MUTED, 0.9))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(notice.app.to_uppercase()),
        )
        .child(
            div()
                .text_size(px(TEXT_TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fade(TEXT, 1.))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(notice.summary.clone()),
        )
        .when(!notice.body.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(TEXT_BODY))
                    .text_color(fade(MUTED, 1.))
                    .overflow_hidden()
                    .child(notice.body.clone()),
            )
        })
}

pub fn a11y_label(notice: &Notice) -> String {
    if notice.body.is_empty() {
        format!(
            "{} de {}. Pressione Enter, Espaço ou Escape para dispensar",
            notice.summary, notice.app
        )
    } else {
        format!(
            "{} — {} de {}. Pressione Enter, Espaço ou Escape para dispensar",
            notice.summary, notice.body, notice.app
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_initial_is_single_display_char() {
        assert_eq!(badge_initial(""), "?");
        assert_eq!(badge_initial("firefox"), "F");
        assert_eq!(badge_initial("éclair"), "É");
        // 'ß'.to_uppercase() == "SS" — badge de 40px espera 1 glifo
        assert_eq!(badge_initial("ßeta"), "S");
    }
}
