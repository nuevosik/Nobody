use gpui::Rgba;

pub const CARD_R: f32 = 14.;
pub const CARD_H: f32 = 54.;
pub const CARD_GAP: f32 = 8.;
pub const MARGIN: f32 = 12.;
pub const POPUP_W: f32 = 320.;
pub const STACK_TOP: f32 = 46.;
pub const QUIET_BADGE: f32 = 28.;

pub const FONT: &str = "SF Pro Display";
pub const FONT_FALLBACK: &str = "Inter";
pub const TEXT_LABEL: f32 = 10.;
pub const TEXT_TITLE: f32 = 12.;
pub const TEXT_BODY: f32 = 11.;
pub const TEXT_BADGE: f32 = 12.;

pub const INK: u32 = 0x0a0a0a;
pub const TEXT: u32 = 0xf5f5f7;
pub const MUTED: u32 = 0x9a9aa2;
pub const ACCENT: u32 = 0x4da3ff;
pub const CHIP: u32 = 0x1e1e22;

pub fn fade(hex: u32, alpha: f32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.,
        g: ((hex >> 8) & 0xff) as f32 / 255.,
        b: (hex & 0xff) as f32 / 255.,
        a: alpha.clamp(0., 1.),
    }
}

pub fn app_font() -> gpui::Font {
    let mut f = gpui::font(FONT);
    f.fallbacks = Some(gpui::FontFallbacks::from_fonts(vec![FONT_FALLBACK.to_string()]));
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_font_wires_inter_fallback() {
        let f = app_font();
        assert_eq!(f.family.as_ref(), FONT);
        let fallbacks = f.fallbacks.expect("app_font deve definir fallbacks");
        assert!(
            fallbacks.fallback_list().iter().any(|s| s == FONT_FALLBACK),
            "fallbacks devem conter Inter"
        );
    }
}
