use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{FontWeight, ParentElement, Styled, StyledImage, div, img, prelude::*, px, relative};

use crate::domain::notice::Notice;
use crate::presentation::theme::{
    ACCENT, MUTED, TEXT, TEXT_BADGE, TEXT_BODY, TEXT_TITLE, app_font, fade,
};

pub fn badge_initial(app: &str) -> String {
    app.chars()
        .next()
        .and_then(|c| c.to_uppercase().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".into())
}

pub fn badge<T: 'static>(
    icon: Option<&std::path::PathBuf>,
    app: &str,
    cx: &gpui::Context<'_, T>,
) -> gpui::Div {
    let base = div()
        .size(px(32.))
        .rounded(px(8.))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center();

    if let Some(path) = icon {
        start_icon_load(path, cx);
        if let Some(rendered) = icon_image(path) {
            return base.child(
                img(rendered).size(px(32.)).rounded(px(8.)).object_fit(gpui::ObjectFit::Cover),
            );
        }
    }

    base.font(app_font())
        .text_size(px(TEXT_BADGE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(fade(MUTED, 1.))
        .child(badge_initial(app))
}

const BADGE_PX: u32 = 64;
const MAX_ICON_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ICON_DIMENSION: u32 = 2048;
const MAX_ICON_ALLOC: u64 = 24 * 1024 * 1024;
const MAX_RASTER_JOBS: usize = 4;
const RASTER_CACHE_LIMIT: usize = 256;

#[derive(Default)]
struct RasterCache {
    images: HashMap<PathBuf, Option<Arc<gpui::RenderImage>>>,
    order: std::collections::VecDeque<PathBuf>,
}

impl RasterCache {
    fn insert(&mut self, path: PathBuf, image: Option<Arc<gpui::RenderImage>>) {
        if !self.images.contains_key(&path) {
            if self.images.len() >= RASTER_CACHE_LIMIT
                && let Some(oldest) = self.order.pop_front()
            {
                self.images.remove(&oldest);
            }
            self.order.push_back(path.clone());
        }
        self.images.insert(path, image);
    }
}

static RASTER: LazyLock<Mutex<RasterCache>> = LazyLock::new(|| Mutex::new(RasterCache::default()));
static ACTIVE_RASTER_JOBS: AtomicUsize = AtomicUsize::new(0);

fn start_icon_load<T: 'static>(path: &Path, cx: &gpui::Context<'_, T>) {
    let path = path.to_path_buf();
    let mut cache = RASTER.lock().unwrap_or_else(|error| error.into_inner());
    if cache.images.contains_key(&path)
        || ACTIVE_RASTER_JOBS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_RASTER_JOBS).then_some(active + 1)
            })
            .is_err()
    {
        return;
    }
    cache.insert(path.clone(), None);
    drop(cache);

    let task = cx.background_executor().spawn(async move {
        let image = decode_badge_icon(&path);
        RASTER.lock().unwrap_or_else(|error| error.into_inner()).insert(path, image);
        ACTIVE_RASTER_JOBS.fetch_sub(1, Ordering::AcqRel);
    });
    cx.spawn(async move |this, cx| {
        task.await;
        _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

fn icon_image(path: &Path) -> Option<Arc<gpui::RenderImage>> {
    RASTER.lock().unwrap_or_else(|error| error.into_inner()).images.get(path).cloned().flatten()
}

fn decode_badge_icon(path: &Path) -> Option<Arc<gpui::RenderImage>> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return None;
    }
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ICON_BYTES {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ICON_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return None;
    }

    let mut reader =
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_ICON_DIMENSION);
    limits.max_image_height = Some(MAX_ICON_DIMENSION);
    limits.max_alloc = Some(MAX_ICON_ALLOC);
    reader.limits(limits);
    let small = reader
        .decode()
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

fn badge_image() -> Option<Arc<gpui::RenderImage>> {
    let rgba = image::load_from_memory(include_bytes!("../../../assets/nobody-badge.png"))
        .ok()?
        .into_rgba8();
    let small =
        image::imageops::resize(&rgba, BADGE_PX, BADGE_PX, image::imageops::FilterType::Lanczos3);
    bgra_render(small)
}

pub fn nobody_badge() -> gpui::Div {
    static BADGE: LazyLock<Option<Arc<gpui::RenderImage>>> = LazyLock::new(badge_image);
    div().when_some(BADGE.clone(), |el, badge| {
        el.child(img(badge).size(px(28.)).object_fit(gpui::ObjectFit::Contain))
    })
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
        .when_some(notice.progress, |el, progress| el.child(progress_bar(progress)))
}

fn progress_bar(progress: u8) -> gpui::Div {
    div()
        .h(px(3.))
        .w_full()
        .rounded(px(2.))
        .overflow_hidden()
        .bg(fade(MUTED, 0.28))
        .child(div().h_full().w(relative(progress as f32 / 100.)).bg(fade(ACCENT, 1.)))
}

pub fn a11y_label(notice: &Notice) -> String {
    let progress =
        notice.progress.map(|value| format!(". Progresso: {value}%")).unwrap_or_default();
    if notice.body.is_empty() {
        format!(
            "{} de {}{}. Pressione Enter, Espaço ou Escape para dispensar",
            notice.summary, notice.app, progress
        )
    } else {
        format!(
            "{} — {} de {}{}. Pressione Enter, Espaço ou Escape para dispensar",
            notice.summary, notice.body, notice.app, progress
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_cache_evicts_oldest_and_releases_image() {
        let mut cache = RasterCache::default();
        let rendered = bgra_render(image::RgbaImage::new(64, 64)).unwrap();
        let weak = Arc::downgrade(&rendered);
        let first = PathBuf::from("first.png");
        cache.insert(first.clone(), Some(rendered));
        for i in 0..RASTER_CACHE_LIMIT - 1 {
            cache.insert(PathBuf::from(format!("{i}.png")), None);
        }
        assert!(weak.upgrade().is_some());
        cache.insert(PathBuf::from("new.png"), None);
        assert_eq!(cache.images.len(), RASTER_CACHE_LIMIT);
        assert!(!cache.images.contains_key(&first));
        assert!(weak.upgrade().is_none());
        for _ in 0..10 {
            cache.insert(PathBuf::from("new.png"), None);
        }
        assert_eq!(cache.order.len(), RASTER_CACHE_LIMIT);
    }

    #[test]
    fn embedded_badge_decodes_at_the_display_size() {
        let badge = badge_image().expect("assets/nobody-badge.png must decode");
        let bytes = badge.as_bytes(0).expect("one frame");
        assert_eq!(bytes.len(), BADGE_PX as usize * BADGE_PX as usize * 4);
    }

    #[test]
    fn badge_icon_is_prerastered_at_display_size() {
        let dir = std::env::temp_dir().join(format!("nobody-badge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255])).save(&path).unwrap();
        assert!(icon_image(&path).is_none(), "render must not decode an uncached path");
        let a = decode_badge_icon(&path).expect("png decodifica");
        RASTER
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(path.clone(), Some(a.clone()));
        let bytes = a.as_bytes(0).expect("um frame");
        assert_eq!(bytes.len(), 64 * 64 * 4);
        assert_eq!(&bytes[..4], &[0, 0, 255, 255]);
        assert!(Arc::ptr_eq(&a, &icon_image(&path).expect("cache hit")));
        std::fs::remove_dir_all(&dir).ok();
        assert!(decode_badge_icon(Path::new("/nonexistent-xyz.png")).is_none());
    }

    #[test]
    fn badge_icon_decoder_enforces_source_bounds_and_rejects_svg() {
        let dir = std::env::temp_dir().join(format!("nobody-badge-limits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let too_wide = dir.join("wide.png");
        image::RgbaImage::new(MAX_ICON_DIMENSION + 1, 1).save(&too_wide).unwrap();
        assert!(decode_badge_icon(&too_wide).is_none());

        let too_large = dir.join("large.png");
        std::fs::File::create(&too_large).unwrap().set_len(MAX_ICON_BYTES + 1).unwrap();
        assert!(decode_badge_icon(&too_large).is_none());

        let svg = dir.join("icon.svg");
        std::fs::write(
            &svg,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20" fill="red"/></svg>"#,
        )
        .unwrap();
        assert!(decode_badge_icon(&svg).is_none());

        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn badge_icon_decoder_rejects_fifo_without_blocking() {
        let dir = std::env::temp_dir().join(format!("nobody-badge-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("icon.png");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if made {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(decode_badge_icon(&fifo));
            });
            assert!(
                receiver
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .expect("FIFO decode must return")
                    .is_none()
            );
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn badge_initial_is_single_display_char() {
        assert_eq!(badge_initial(""), "?");
        assert_eq!(badge_initial("firefox"), "F");
        assert_eq!(badge_initial("éclair"), "É");
        assert_eq!(badge_initial("ßeta"), "S");
    }

    #[test]
    fn badge_initial_passthrough_for_upper_digit() {
        assert_eq!(badge_initial("Firefox"), "F");
        assert_eq!(badge_initial("123app"), "1");
    }

    fn mk_notice(summary: &str, body: &str, app: &str) -> Notice {
        Notice {
            id: 1,
            app: app.into(),
            desktop_entry: None,
            summary: summary.into(),
            body: body.into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
            stack_tag: None,
            progress: None,
        }
    }

    #[test]
    fn a11y_label_without_body_omits_separator() {
        let n = mk_notice("Title", "", "Firefox");
        assert_eq!(
            a11y_label(&n),
            "Title de Firefox. Pressione Enter, Espaço ou Escape para dispensar"
        );
    }

    #[test]
    fn a11y_label_with_body_includes_summary_body_app() {
        let n = mk_notice("Title", "Hello", "Firefox");
        assert_eq!(
            a11y_label(&n),
            "Title — Hello de Firefox. Pressione Enter, Espaço ou Escape para dispensar"
        );
    }

    #[test]
    fn a11y_label_includes_progress_when_present() {
        let mut n = mk_notice("Title", "Hello", "Firefox");
        n.progress = Some(50);
        assert!(a11y_label(&n).contains("Progresso: 50%"));
    }
}
