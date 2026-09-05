use std::collections::HashMap;

use gpui::{Bounds, Size, Window, point, px, size};

use crate::domain::notice::Notice;
use crate::presentation::theme::{CARD_GAP, CARD_H, MARGIN, POPUP_W, STACK_TOP};

const STRIDE: f32 = CARD_H + CARD_GAP;

pub(crate) fn grouped_y(notices: &[Notice], idx: usize) -> f32 {
    grouped_y_map(notices).get(idx).copied().unwrap_or(STACK_TOP)
}

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
                y_map[orig_idx] = y_cursor + k as f32 * STRIDE;
            }
            let deck_h = indices.len() as f32 * STRIDE - CARD_GAP;
            y_cursor += deck_h + CARD_GAP;
        }
    }
    y_map
}

pub fn total_h_current_for(notices: &[Notice], n: usize) -> f32 {
    if n == 0 {
        return 0.;
    }
    let y_map = grouped_y_map(notices);
    y_map.iter().take(n).fold(STACK_TOP, |a, &y| a.max(y)) + CARD_H
}

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
        let cards: Vec<Bounds<gpui::Pixels>> = (0..n)
            .map(|i| {
                let y = y_map.get(i).copied().unwrap_or(STACK_TOP);
                Bounds {
                    origin: point(px(0.), px(y)),
                    size: size(px(POPUP_W + MARGIN * 2.), px(CARD_H)),
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
        let notices = vec![mk(1, "A"), mk(2, "B"), mk(3, "A")];
        let n = notices.len();
        let y_map = grouped_y_map(&notices);
        assert_eq!(y_map.len(), 3);
        assert!((y_map[0] - 46.).abs() < 0.01);
        assert!((y_map[1] - 170.).abs() < 0.01);
        assert!((y_map[2] - 108.).abs() < 0.01);
        let got = total_h_current_for(&notices, n);
        assert!((got - 224.).abs() < 0.01, "total_h deve ser 224, foi {got}");
    }

    #[test]
    fn same_app_cards_do_not_overlap() {
        let notices = vec![mk(1, "A"), mk(2, "A"), mk(3, "A")];
        let y_map = grouped_y_map(&notices);
        assert!((y_map[1] - y_map[0] - 62.).abs() < 0.01);
        assert!((y_map[2] - y_map[1] - 62.).abs() < 0.01);
    }
}
