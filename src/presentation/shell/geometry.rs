use std::collections::HashMap;

use gpui::{Bounds, Size, Window, point, px, size};

use crate::domain::notice::Notice;
use crate::presentation::theme::{CARD_GAP, CARD_H, MARGIN, POPUP_W, STACK_TOP};

const STRIDE: f32 = CARD_H + CARD_GAP;

/// Extra footprint of a collapsed multi-notice deck (room for the ghost peek).
pub const STACK_PEEK: f32 = 10.;

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

/// One app's notices in first-seen order. A multi-notice deck is `collapsed`
/// unless its app is currently expanded (hover).
pub struct Deck {
    pub app: String,
    pub indices: Vec<usize>,
    pub collapsed: bool,
}

impl Deck {
    pub fn hidden_count(&self) -> usize {
        if self.collapsed { self.indices.len().saturating_sub(1) } else { 0 }
    }
}

pub fn decks(notices: &[Notice], expanded: Option<&str>) -> Vec<Deck> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, n) in notices.iter().enumerate() {
        map.entry(n.app.clone()).or_insert_with(|| {
            order.push(n.app.clone());
            Vec::new()
        });
        map.get_mut(&n.app).expect("app recém-inserido").push(i);
    }
    order
        .into_iter()
        .map(|app| {
            let indices = map.remove(&app).unwrap_or_default();
            let collapsed = indices.len() > 1 && expanded != Some(app.as_str());
            Deck { app, indices, collapsed }
        })
        .collect()
}

/// Collapsed decks show only their most recent notice (`indices[0]`); the rest
/// sit behind it. Returns y per notice index, visibility per index, and total
/// window height for the `shown` decks only.
pub fn deck_layout(
    notices: &[Notice],
    decks: &[Deck],
    shown: &[ShownDeck],
) -> (Vec<f32>, Vec<bool>, f32) {
    let mut y_map = vec![STACK_TOP; notices.len()];
    let mut visible = vec![false; notices.len()];
    let mut in_shown = vec![false; decks.len()];
    for s in shown {
        in_shown[s.deck] = true;
    }
    let mut cursor = STACK_TOP;
    let mut total_h: f32 = 0.;
    for (d, deck) in decks.iter().enumerate() {
        if deck.indices.is_empty() {
            continue;
        }
        if deck.collapsed {
            let top = deck.indices[0];
            y_map[top] = cursor;
            if in_shown[d] {
                visible[top] = true;
                total_h = total_h.max(cursor + CARD_H + STACK_PEEK);
            }
            cursor += CARD_H + STACK_PEEK + CARD_GAP;
        } else {
            for (k, &idx) in deck.indices.iter().enumerate() {
                y_map[idx] = cursor + k as f32 * STRIDE;
            }
            if in_shown[d] {
                for s in shown.iter().filter(|s| s.deck == d) {
                    for &idx in &s.indices {
                        visible[idx] = true;
                        total_h = total_h.max(y_map[idx] + CARD_H);
                    }
                }
            }
            cursor += deck.indices.len() as f32 * STRIDE;
        }
    }
    if shown.is_empty() {
        total_h = 0.;
    }
    (y_map, visible, total_h)
}

/// A deck clipped to the slot budget: collapsed/single decks cost 1 slot,
/// expanded decks cost up to their member count (partial when it doesn't fit).
pub struct ShownDeck {
    pub deck: usize,
    pub indices: Vec<usize>,
}

pub fn shown_decks(decks: &[Deck], max_slots: usize) -> Vec<ShownDeck> {
    let mut out = Vec::new();
    let mut used = 0;
    for (d, deck) in decks.iter().enumerate() {
        if used >= max_slots || deck.indices.is_empty() {
            continue;
        }
        if deck.collapsed {
            out.push(ShownDeck { deck: d, indices: vec![deck.indices[0]] });
            used += 1;
        } else {
            let room = max_slots - used;
            let take = deck.indices.len().min(room);
            out.push(ShownDeck { deck: d, indices: deck.indices[..take].to_vec() });
            used += take;
        }
    }
    out
}

pub fn sync_window_geometry(
    window: &mut Window,
    last_h: &mut Option<f32>,
    last_n: &mut usize,
    cards_y: &[f32],
    total_h: f32,
) {
    let n = cards_y.len();
    let should_resize = last_h.is_none_or(|h| (h - total_h).abs() > 0.5);
    if should_resize {
        let h = if total_h > 0. { total_h } else { 1. };
        window.resize(Size::new(px(POPUP_W + MARGIN * 2.), px(h)));
        *last_h = Some(total_h);
    }

    let should_recalc_input = *last_n != n || should_resize;
    if should_recalc_input {
        let cards: Vec<Bounds<gpui::Pixels>> = cards_y
            .iter()
            .map(|&y| Bounds {
                origin: point(px(0.), px(y)),
                size: size(px(POPUP_W + MARGIN * 2.), px(CARD_H)),
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

    #[test]
    fn total_h_zero_visible_is_zero() {
        let notices = vec![mk(1, "A")];
        assert_eq!(total_h_current_for(&notices, 0), 0.);
        assert_eq!(total_h_current_for(&[], 0), 0.);
    }

    #[test]
    fn empty_map_and_out_of_range_fallback() {
        let empty: Vec<Notice> = vec![];
        assert!(grouped_y_map(&empty).is_empty());
        assert!((grouped_y(&empty, 0) - STACK_TOP).abs() < 0.01);
        let notices = vec![mk(1, "A")];
        assert!((grouped_y(&notices, 99) - STACK_TOP).abs() < 0.01);
    }

    #[test]
    fn decks_do_not_overlap_across_apps() {
        let notices = vec![mk(1, "A"), mk(2, "B"), mk(3, "A"), mk(4, "B")];
        let mut ys = grouped_y_map(&notices);
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for w in ys.windows(2) {
            assert!(w[1] - w[0] >= CARD_H - 0.01, "cards sobrepostos: {ys:?}");
        }
    }

    #[test]
    fn decks_collapse_multi_app_groups_until_hovered() {
        let notices = vec![mk(1, "A"), mk(2, "A"), mk(3, "B")];
        let ds = decks(&notices, None);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].app, "A");
        assert_eq!(ds[0].indices, vec![0, 1]);
        assert!(ds[0].collapsed);
        assert_eq!(ds[0].hidden_count(), 1);
        assert!(!ds[1].collapsed);
        assert_eq!(ds[1].hidden_count(), 0);

        let ds = decks(&notices, Some("A"));
        assert!(!ds[0].collapsed);
        assert_eq!(ds[0].hidden_count(), 0);
    }

    #[test]
    fn deck_layout_hides_stacked_members_behind_the_top() {
        let notices = vec![mk(1, "A"), mk(2, "A"), mk(3, "B")];
        let ds = decks(&notices, None);
        let shown = shown_decks(&ds, 5);
        let (y, vis, total) = deck_layout(&notices, &ds, &shown);
        assert!((y[0] - STACK_TOP).abs() < 0.01);
        assert!((y[1] - STACK_TOP).abs() < 0.01);
        assert!(vis[0] && !vis[1] && vis[2]);
        assert!((y[2] - (STACK_TOP + CARD_H + STACK_PEEK + CARD_GAP)).abs() < 0.01);
        assert!((total - (y[2] + CARD_H)).abs() < 0.01);
    }

    #[test]
    fn deck_layout_expanded_lists_every_member() {
        let notices = vec![mk(1, "A"), mk(2, "A")];
        let ds = decks(&notices, Some("A"));
        let shown = shown_decks(&ds, 5);
        let (y, vis, total) = deck_layout(&notices, &ds, &shown);
        assert!((y[0] - STACK_TOP).abs() < 0.01);
        assert!((y[1] - STACK_TOP - (CARD_H + CARD_GAP)).abs() < 0.01);
        assert!(vis[0] && vis[1]);
        assert!((total - (y[1] + CARD_H)).abs() < 0.01);
    }

    #[test]
    fn shown_decks_respects_slot_budget() {
        let notices = vec![mk(1, "A"), mk(2, "B"), mk(3, "C")];
        let ds = decks(&notices, None);
        let shown = shown_decks(&ds, 2);
        assert_eq!(shown.len(), 2);

        let notices = vec![mk(1, "A"), mk(2, "A"), mk(3, "A")];
        let ds = decks(&notices, Some("A"));
        let shown = shown_decks(&ds, 2);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].indices, vec![0, 1]);

        let ds = decks(&notices, None);
        let (_, _, total) = deck_layout(&notices, &ds, &[]);
        assert_eq!(total, 0.);
    }
}
