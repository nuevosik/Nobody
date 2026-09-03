//! Tema — espelha `theme.rs` da rot. Tokens visuais centralizados.

use gpui::Rgba;

// Layout
pub const CARD_R: f32 = 28.;
pub const CARD_H: f32 = 76.;
pub const CARD_GAP: f32 = 16.;
pub const MARGIN: f32 = 12.;
pub const POPUP_W: f32 = 380.;

// Tipografia — "SF Pro Display" não existe no Arch Linux; o GPUI resolve
// via `resolve_font()` para a pilha interna (Noto Sans, …) quando a família
// falta. `app_font()` prefere Inter (instalado; `fc-match Inter` confirma)
// antes desse fallback genérico.
pub const FONT: &str = "SF Pro Display";
pub const FONT_FALLBACK: &str = "Inter";
pub const TEXT_LABEL: f32 = 10.;
pub const TEXT_TITLE: f32 = 13.;
pub const TEXT_BODY: f32 = 12.;
pub const TEXT_BADGE: f32 = 14.;

// Cores
pub const INK: u32 = 0x0a0a0a;
pub const TEXT: u32 = 0xf5f5f7;
pub const MUTED: u32 = 0x9a9aa2;
pub const ACCENT: u32 = 0x4da3ff;
pub const CHIP: u32 = 0x1e1e22;

/// Converte 0xRRGGBB + alpha para `Rgba`.
pub fn fade(hex: u32, alpha: f32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.,
        g: ((hex >> 8) & 0xff) as f32 / 255.,
        b: (hex & 0xff) as f32 / 255.,
        a: alpha.clamp(0., 1.),
    }
}

/// Fonte do app com fallback explícito: família principal + Inter.
/// Nota: `src/ui/stack.rs` (fora do escopo deste fix) ainda usa
/// `.font_family(FONT)` na raiz; migrar para `.font(app_font())` lá
/// estende o fallback para toda a árvore.
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
