use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{FontWeight, ParentElement, Styled, StyledImage, div, img, prelude::*, px};

use crate::domain::notice::Notice;
use crate::presentation::theme::{MUTED, TEXT, TEXT_BADGE, TEXT_BODY, TEXT_TITLE, app_font, fade};

pub fn badge_initial(app: &str) -> String {
    app.chars()
        .next()
        .and_then(|c| c.to_uppercase().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".into())
}

pub fn badge(icon: Option<&std::path::PathBuf>, app: &str) -> gpui::Div {
    let base = div()
        .size(px(32.))
        .rounded(px(8.))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center();

    if let Some(path) = icon {
        if let Some(rendered) = icon_image(path) {
            return base.child(img(rendered).size(px(32.)).object_fit(gpui::ObjectFit::Cover));
        }
        let initial = badge_initial(app);
        base.child(
            img(path.clone()).size(px(32.)).object_fit(gpui::ObjectFit::Cover).with_fallback(
                move || {
                    div()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(TEXT_BADGE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fade(MUTED, 1.))
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
            .text_color(fade(MUTED, 1.))
            .child(initial)
    }
}

const BADGE_PX: u32 = 64;

static RASTER: LazyLock<Mutex<HashMap<PathBuf, Option<Arc<gpui::RenderImage>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn icon_image(path: &Path) -> Option<Arc<gpui::RenderImage>> {
    if let Some(hit) = RASTER.lock().unwrap_or_else(|e| e.into_inner()).get(path).cloned() {
        return hit;
    }
    let found = decode_badge_icon(path);
    RASTER.lock().unwrap_or_else(|e| e.into_inner()).insert(path.to_path_buf(), found.clone());
    found
}

fn decode_badge_icon(path: &Path) -> Option<Arc<gpui::RenderImage>> {
    let small = image::open(path)
        .ok()?
        .resize(BADGE_PX, BADGE_PX, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    bgra_render(small)
}

fn bgra_render(small: image::RgbaImage) -> Option<Arc<gpui::RenderImage>> {
    let (w, h) = (small.width(), small.height());
    let mut buf = small.into_raw();
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let frame = image::Frame::new(image::RgbaImage::from_raw(w, h, buf)?);
    Some(Arc::new(gpui::RenderImage::new(smallvec::SmallVec::from_elem(frame, 1))))
}

pub fn nobody_badge() -> gpui::Div {
    static BADGE: LazyLock<Arc<gpui::RenderImage>> = LazyLock::new(|| {
        let rgba = image::load_from_memory(include_bytes!("../../../assets/nobody-badge.png"))
            .expect("assets/nobody-badge.png é PNG válido")
            .into_rgba8();
        let small = image::imageops::resize(
            &rgba,
            BADGE_PX,
            BADGE_PX,
            image::imageops::FilterType::Lanczos3,
        );
        bgra_render(small).expect("resize gera dimensões válidas")
    });
    div().child(img(BADGE.clone()).size(px(28.)).object_fit(gpui::ObjectFit::Contain))
}

pub fn card_content(notice: &Notice) -> gpui::Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(1.))
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
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
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
    fn badge_icon_is_prerastered_at_display_size() {
        let dir = std::env::temp_dir().join(format!("nobody-badge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255])).save(&path).unwrap();
        let a = icon_image(&path).expect("png decodifica");
        let bytes = a.as_bytes(0).expect("um frame");
        assert_eq!(bytes.len(), 64 * 64 * 4);
        assert_eq!(&bytes[..4], &[0, 0, 255, 255]);
        assert!(Arc::ptr_eq(&a, &icon_image(&path).expect("cache hit")));
        std::fs::remove_dir_all(&dir).ok();
        assert!(decode_badge_icon(Path::new("/nonexistent-xyz.png")).is_none());
    }

    #[test]
    fn badge_initial_is_single_display_char() {
        assert_eq!(badge_initial(""), "?");
        assert_eq!(badge_initial("firefox"), "F");
        assert_eq!(badge_initial("éclair"), "É");
        assert_eq!(badge_initial("ßeta"), "S");
    }
}
