//! Presentation/shell — layout do deck agrupado por app.

use std::collections::HashMap;

use gpui::{Bounds, Size, Window, point, px, size};

use crate::domain::notice::Notice;
use crate::presentation::theme::{CARD_GAP, CARD_H, MARGIN, POPUP_W, STACK_TOP};

const DECK_GAP: f32 = 16.;
const MIN_HIT: f32 = 24.;

pub(crate) fn grouped_y(notices: &[Notice], idx: usize) -> f32 {
    grouped_y_map(notices).get(idx).copied().unwrap_or(STACK_TOP)
}

/// Calcula Y agrupado por app em O(n).
pub fn grouped_y_map(notices: &[Notice]) -> Vec<f32> {
    if notices.is_empty() {
        return Vec::new();
    }
    let mut app_order: Vec<String> = Vec::new();
    let mut app_to_indices: HashMap<String, Vec<usize>> = HashMap::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for (i, n) in notices.iter().enumerate() {
        if seen.insert(n.app.clone(), ()).is_none() {
            app_order.push(n.app.clone());
        }
        app_to_indices.entry(n.app.clone()).or_default().push(i);
    }
    let mut y_map = vec![0.; notices.len()];
    let mut y_cursor = STACK_TOP;
    for app in app_order {
        if let Some(indices) = app_to_indices.get(&app) {
            for (k, &orig_idx) in indices.iter().enumerate() {
                y_map[orig_idx] = y_cursor + k as f32 * DECK_GAP;
            }
            let deck_h = CARD_H + (indices.len() as f32 - 1.) * DECK_GAP;
            y_cursor += deck_h + CARD_GAP;
        }
    }
    y_map
}

/// Altura total ocupada pelos `n` primeiros cards visíveis.
///
/// Extraída de `render()` para permitir teste unitário puro.
pub fn total_h_current_for(notices: &[Notice], n: usize) -> f32 {
    if n == 0 {
        return 0.;
    }
    let y_map = grouped_y_map(notices);
    y_map.iter().take(n).fold(STACK_TOP, |a, &y| a.max(y)) + CARD_H
}

/// Redimensiona a janela e recalcula a `input_region` só quando necessário.
pub fn sync_window_geometry(
    window: &mut Window,
    last_h: &mut Option<f32>,
    last_n: &mut usize,
    notices: &[Notice],
    total_h: f32,
    n: usize,
) {
    let should_resize = last_h.is_none_or(|h| (h - total_h).abs() > 0.5);
    if should_resize {
        let h = if total_h > 0. { total_h } else { 1. };
        window.resize(Size::new(px(POPUP_W + MARGIN * 2.), px(h)));
        *last_h = Some(total_h);
    }

    let should_recalc_input = *last_n != n || should_resize;
    if should_recalc_input {
        let y_map = grouped_y_map(notices);
        // Pré-computa último índice de cada app para hit-region correta
        let mut last_idx_per_app: HashMap<String, usize> = HashMap::new();
        for (k, n) in notices.iter().enumerate() {
            last_idx_per_app.insert(n.app.clone(), k);
        }
        let cards: Vec<Bounds<gpui::Pixels>> = (0..n)
            .map(|i| {
                let y = y_map.get(i).copied().unwrap_or(STACK_TOP);
                let is_last_in_group = last_idx_per_app.get(&notices[i].app).copied() == Some(i);
                let h = if is_last_in_group { CARD_H } else { MIN_HIT };
                Bounds {
                    origin: point(px(0.), px(y)),
                    size: size(px(POPUP_W + MARGIN * 2.), px(h)),
                }
            })
            .collect();
        window.set_input_region(Some(&cards));
        *last_n = n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(id: u32, app: &str) -> Notice {
        Notice {
            id,
            app: app.into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        }
    }

    #[test]
    fn interleaved_apps_total_h_uses_max_y_not_last_index() {
        // A,B,A intercalados: y_map = [STACK_TOP, 154, STACK_TOP+DECK_GAP].
        // total_h correto = max(y[0..3]) + CARD_H = 154 + 76 = 230,
        // não y[2] + CARD_H = 62 + 76 = 138 (janela cliparia o card B).
        let notices = vec![mk(1, "A"), mk(2, "B"), mk(3, "A")];
        let n = notices.len();
        let y_map = grouped_y_map(&notices);
        assert_eq!(y_map.len(), 3);
        let expected = y_map.iter().take(n).fold(STACK_TOP, |a, &y| a.max(y)) + CARD_H;
        assert!((expected - 230.).abs() < 0.01, "expected 230, got {expected}");
        let buggy = y_map[n - 1] + CARD_H;
        assert!(
            (buggy - 138.).abs() < 0.01,
            "precondição do bug: y[n-1]+CARD_H deve ser 138, foi {buggy}"
        );
        let got = total_h_current_for(&notices, n);
        assert!(
            (got - expected).abs() < 0.01,
            "total_h_current deve ser max(y)+CARD_H={expected}, foi {got}"
        );
    }
}
