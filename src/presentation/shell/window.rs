use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::accesskit::Role;
use gpui::{
    AppContext, Bounds, Context, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px, size,
};

use crate::application::{clock, commands};
use crate::domain::queue::Queue;
use crate::presentation::theme::{
    ACCENT, CARD_GAP, CARD_H, CARD_R, CHIP, FONT, INK, MARGIN, MUTED, POPUP_W, QUIET_BADGE, TEXT,
    TEXT_BODY, TEXT_TITLE, fade,
};

use super::{anim, center, feed, geometry, popup};

pub(crate) const MAX_VISIBLE: usize = 5;
pub(crate) const PANEL_H: f32 = 480.;

pub(crate) fn visible_count(n: usize) -> usize {
    n.min(MAX_VISIBLE)
}

pub struct NotificationStack {
    stack: feed::Stack,
    exiting: Vec<feed::Exiting>,
    queue: Queue,
    quiet: bool,
    center_open: bool,
    center_query: String,
    expanded: Option<String>,
    last_expanded: Option<String>,
    smooth_y: HashMap<u32, f32>,
    target_y: HashMap<u32, f32>,
    shown_at: HashMap<u32, u128>,
    last_window_h: Option<f32>,
    last_input_len: usize,
}

pub(crate) fn step_toward(current: f32, target: f32) -> (f32, bool) {
    let next = current + (target - current) * 0.3;
    if (target - next).abs() < 0.5 { (target, true) } else { (next, false) }
}

pub(crate) fn hover_expand(current: Option<&str>, app: &str, hovered: bool) -> Option<String> {
    if hovered {
        if current == Some(app) { current.map(str::to_owned) } else { Some(app.to_owned()) }
    } else if current == Some(app) {
        None
    } else {
        current.map(str::to_owned)
    }
}

impl NotificationStack {
    pub fn new(cx: &mut Context<Self>, queue: Queue) -> Self {
        spawn_anim_ticker(cx);
        spawn_feed_sync(queue.clone(), cx);

        Self {
            stack: feed::Stack::default(),
            exiting: Vec::new(),
            queue,
            quiet: false,
            center_open: false,
            center_query: String::new(),
            expanded: None,
            last_expanded: None,
            smooth_y: HashMap::new(),
            target_y: HashMap::new(),
            shown_at: HashMap::new(),
            last_window_h: None,
            last_input_len: usize::MAX,
        }
    }

    fn dismiss(&self, id: u32) {
        commands::request_dismissal(&self.queue, id);
    }

    fn close_center(&self) {
        commands::set_center_open(&self.queue, false);
    }

    fn render_center(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let w = POPUP_W + MARGIN * 2.;
        if self.last_window_h != Some(PANEL_H) {
            window.resize(Size::new(px(w), px(PANEL_H)));
            window.set_input_region(Some(&[Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(w), px(PANEL_H)),
            }]));
            self.last_window_h = Some(PANEL_H);
            self.last_input_len = usize::MAX;
        }

        let entries = commands::history(&self.queue);
        let filtered = commands::filter_history(&entries, &self.center_query);
        let manual = commands::manual_quiet(&self.queue);
        let auto = commands::quiet_mode(&self.queue);
        let now = clock::now_ms();
        let query_text = self.center_query.clone();
        let empty_query = query_text.trim().is_empty();

        let root = div()
            .id("center")
            .size_full()
            .font_family(FONT)
            .text_size(px(11.))
            .bg(fade(INK, 0.92))
            .rounded(px(CARD_R))
            .border_1()
            .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.1 })
            .p(px(MARGIN))
            .flex()
            .flex_col()
            .gap(px(8.))
            .on_key_down(cx.listener(|stack, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.modified() {
                    return;
                }
                if event.keystroke.key.as_str() == "escape" {
                    stack.close_center();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(TEXT_TITLE))
                            .text_color(fade(TEXT, 1.))
                            .child(format!("Central ({})", entries.len())),
                    )
                    .child(
                        div()
                            .id("center-close")
                            .focusable()
                            .role(Role::Button)
                            .aria_label("Fechar central")
                            .aria_keyshortcuts("Escape")
                            .cursor_pointer()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(8.))
                            .bg(fade(CHIP, 1.))
                            .text_color(fade(TEXT, 0.9))
                            .focus_visible(|s| s.border_2().border_color(fade(ACCENT, 1.)))
                            .child("Fechar")
                            .on_click(cx.listener(|stack, _, _, cx| {
                                stack.close_center();
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(
                                |stack, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.modifiers.modified() {
                                        return;
                                    }
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        stack.close_center();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("center-dnd")
                            .focusable()
                            .role(Role::Button)
                            .aria_label("Alternar Não Perturbe manual")
                            .cursor_pointer()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(8.))
                            .bg(fade(CHIP, 1.))
                            .text_color(fade(TEXT, 0.9))
                            .focus_visible(|s| s.border_2().border_color(fade(ACCENT, 1.)))
                            .child(center::dnd_button_label(manual))
                            .on_click(cx.listener(|stack, _, _, cx| {
                                commands::toggle_manual_quiet(&stack.queue);
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(
                                |stack, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.modifiers.modified() {
                                        return;
                                    }
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        commands::toggle_manual_quiet(&stack.queue);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            )),
                    )
                    .child(
                        div()
                            .id("center-clear")
                            .focusable()
                            .role(Role::Button)
                            .aria_label("Limpar histórico")
                            .cursor_pointer()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(8.))
                            .bg(fade(CHIP, 1.))
                            .text_color(fade(TEXT, 0.9))
                            .focus_visible(|s| s.border_2().border_color(fade(ACCENT, 1.)))
                            .child("Limpar histórico")
                            .on_click(cx.listener(|stack, _, _, cx| {
                                commands::clear_history(&stack.queue);
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(
                                |stack, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.modifiers.modified() {
                                        return;
                                    }
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        commands::clear_history(&stack.queue);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            )),
                    ),
            )
            .when(center::fullscreen_note(manual, auto).is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(fade(MUTED, 1.))
                        .child(center::fullscreen_note(manual, auto).unwrap_or("")),
                )
            })
            .child(
                div()
                    .id("center-search")
                    .focusable()
                    .role(Role::SearchInput)
                    .aria_label("Buscar no histórico")
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.15 })
                    .text_color(if empty_query { fade(MUTED, 1.) } else { fade(TEXT, 1.) })
                    .child(if empty_query {
                        "Buscar por app, título ou corpo".to_string()
                    } else {
                        query_text.clone()
                    })
                    .on_key_down(cx.listener(move |stack, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.modifiers.modified() {
                            return;
                        }
                        let key = event.keystroke.key.as_str();
                        if key == "escape" {
                            stack.close_center();
                            cx.stop_propagation();
                            cx.notify();
                        } else if center::apply_search_key(&mut stack.center_query, key) {
                            cx.stop_propagation();
                            cx.notify();
                        }
                    })),
            );

        if entries.is_empty() {
            return root.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(fade(MUTED, 1.))
                    .child("Nenhuma notificação nesta sessão"),
            );
        }
        if filtered.is_empty() {
            return root.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(fade(MUTED, 1.))
                    .child(format!("Nenhum resultado para “{query_text}”")),
            );
        }
        root.child(
            div()
                .id("center-list")
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(filtered.iter().map(|entry| {
                    let n = &entry.notice;
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(10.))
                        .bg(fade(INK, 0.78))
                        .child(popup::badge(n.icon.as_ref(), &n.app))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(1.))
                                .child(div().text_size(px(10.)).text_color(fade(MUTED, 1.)).child(
                                    format!(
                                        "{} • {}",
                                        n.app,
                                        center::format_age(now, n.arrived_at_ms)
                                    ),
                                ))
                                .child(
                                    div()
                                        .text_size(px(TEXT_TITLE))
                                        .text_color(fade(TEXT, 1.))
                                        .child(n.summary.clone()),
                                )
                                .when(!n.body.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(TEXT_BODY))
                                            .text_color(fade(MUTED, 1.))
                                            .child(n.body.clone()),
                                    )
                                }),
                        )
                })),
        )
    }
}

fn spawn_anim_ticker(cx: &mut Context<NotificationStack>) {
    cx.spawn(async move |this, cx| {
        loop {
            let Ok(needs_anim) = this.update(cx, |stack, _| {
                let entering = stack
                    .stack
                    .notices
                    .iter()
                    .any(|n| clock::elapsed_ms(n.arrived_at_ms) < anim::ENTER_MS);
                let exiting = !stack.exiting.is_empty();
                let settling = stack
                    .target_y
                    .iter()
                    .any(|(id, t)| stack.smooth_y.get(id).is_some_and(|s| (s - t).abs() >= 0.5));
                entering || exiting || settling
            }) else {
                break;
            };

            if needs_anim {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
                cx.background_executor().timer(Duration::from_millis(16)).await;
            } else {
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        }
    })
    .detach();
}

fn spawn_feed_sync(queue: Queue, cx: &mut Context<NotificationStack>) {
    cx.spawn(async move |this, cx| {
        loop {
            let snapshot = commands::snapshot(&queue);

            if this
                .update(cx, |stack, cx| {
                    let quiet = commands::effective_quiet(&stack.queue);
                    let changed =
                        feed::sync_snapshot(&mut stack.stack, &mut stack.exiting, snapshot, quiet);
                    let flipped = quiet != stack.quiet;
                    stack.quiet = quiet;
                    let center = commands::center_open(&stack.queue);
                    let center_flipped = center != stack.center_open;
                    stack.center_open = center;
                    if changed || flipped || center_flipped {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }

            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
    })
    .detach();
}

impl gpui::Render for NotificationStack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.center_open {
            return self.render_center(window, cx).into_any_element();
        }
        if self.quiet {
            geometry::sync_window_geometry(
                window,
                &mut self.last_window_h,
                &mut self.last_input_len,
                &[],
                MARGIN + QUIET_BADGE + MARGIN,
            );
            return div()
                .size_full()
                .relative()
                .when(!self.stack.notices.is_empty(), |el| {
                    el.child(
                        div()
                            .absolute()
                            .top(px(MARGIN))
                            .right(px(MARGIN))
                            .child(popup::nobody_badge()),
                    )
                })
                .into_any_element();
        }

        let n = visible_count(self.stack.notices.len());
        let card_h = CARD_H;

        let exiting_alive: Vec<&feed::Exiting> =
            self.exiting.iter().filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS).collect();
        let exiting_max_y = exiting_alive.iter().map(|e| e.y + card_h).fold(0., f32::max);
        let deck_list = geometry::decks(&self.stack.notices, self.expanded.as_deref());
        let shown = geometry::shown_decks(&deck_list, n);
        let (y_map, _, _) = geometry::deck_layout(&self.stack.notices, &deck_list, &shown);
        let reduced = anim::prefers_reduced_motion();
        let now = clock::now_ms();
        let live: HashSet<u32> = self.stack.notices.iter().map(|n| n.id).collect();
        self.smooth_y.retain(|id, _| live.contains(id));
        self.target_y.retain(|id, _| live.contains(id));
        self.shown_at.retain(|id, _| live.contains(id));
        for s in &shown {
            let deck_top = y_map[deck_list[s.deck].indices[0]];
            for &idx in &s.indices {
                let id = self.stack.notices[idx].id;
                self.target_y.insert(id, y_map[idx]);
                self.shown_at.entry(id).or_insert(now);
                let cur = self.smooth_y.get(&id).copied().unwrap_or(deck_top);
                let next = if reduced || self.last_expanded != self.expanded {
                    y_map[idx]
                } else {
                    step_toward(cur, y_map[idx]).0
                };
                self.smooth_y.insert(id, next);
            }
        }
        self.last_expanded = self.expanded.clone();
        let mut total_h_current = 0f32;
        for s in &shown {
            let deck = &deck_list[s.deck];
            for &idx in &s.indices {
                let id = self.stack.notices[idx].id;
                let mut top = self.smooth_y.get(&id).copied().unwrap_or(y_map[idx]) + CARD_H;
                if deck.collapsed && idx == deck.indices[0] {
                    top += geometry::STACK_PEEK;
                }
                total_h_current = total_h_current.max(top);
            }
        }
        let total_h = if exiting_alive.is_empty() {
            total_h_current
        } else {
            total_h_current.max(exiting_max_y + CARD_GAP)
        };
        let cards_y: Vec<f32> = shown
            .iter()
            .flat_map(|s| {
                s.indices.iter().map(|&i| {
                    let id = self.stack.notices[i].id;
                    self.smooth_y.get(&id).copied().unwrap_or(y_map[i])
                })
            })
            .collect();

        geometry::sync_window_geometry(
            window,
            &mut self.last_window_h,
            &mut self.last_input_len,
            &cards_y,
            total_h,
        );

        let announcement = self
            .stack
            .notices
            .first()
            .map(|n| format!("{}: {} — {}", n.app, n.summary, n.body))
            .unwrap_or_default();

        div()
            .size_full()
            .relative()
            .font_family(FONT)
            .text_size(px(11.))
            .child(
                div()
                    .id("a11y-live")
                    .role(Role::Status)
                    .aria_label(announcement.clone())
                    .absolute()
                    .left(px(-10000.))
                    .top(px(-10000.))
                    .size(px(1.))
                    .overflow_hidden(),
            )
            .children({
                let mut slot_of: Vec<usize> = vec![0; shown.len()];
                {
                    let mut slot = 0;
                    for (k, s) in shown.iter().enumerate() {
                        slot_of[k] = slot;
                        slot += s.indices.len();
                    }
                }
                let mut els: Vec<gpui::Stateful<gpui::Div>> = Vec::new();
                for (k, s) in shown.iter().enumerate().rev() {
                    let deck = &deck_list[s.deck];
                    let smooth_at = |idx: usize| {
                        let id = self.stack.notices[idx].id;
                        self.smooth_y.get(&id).copied().unwrap_or(y_map[idx])
                    };
                    let first_y = smooth_at(s.indices[0]);
                    let last_y = smooth_at(*s.indices.last().expect("deck mostrado não é vazio"));
                    let mut footprint = last_y - first_y + CARD_H;
                    if deck.collapsed {
                        footprint += geometry::STACK_PEEK;
                    }
                    let expand_app = deck.app.clone();
                    let mut container = div()
                        .id(format!("deck-{}", deck.app))
                        .absolute()
                        .top(px(first_y))
                        .right(px(MARGIN))
                        .w(px(POPUP_W))
                        .h(px(footprint))
                        .on_hover(cx.listener(move |stack, hovered: &bool, _, cx| {
                            let next =
                                hover_expand(stack.expanded.as_deref(), &expand_app, *hovered);
                            if next != stack.expanded {
                                stack.expanded = next;
                                cx.notify();
                            }
                        }));
                    for (m, &idx) in s.indices.iter().enumerate() {
                        let slot = slot_of[k] + m;
                        let y = smooth_at(idx) - first_y;
                        let notice = &self.stack.notices[idx];
                        let hidden = if deck.collapsed && m == 0 { deck.hidden_count() } else { 0 };
                        let shown_since =
                            self.shown_at.get(&notice.id).copied().unwrap_or(notice.arrived_at_ms);
                        let t = anim::enter_progress(shown_since);
                        let slide = if reduced { 0. } else { (1. - t) * (POPUP_W + MARGIN) };
                        if hidden > 0 {
                            container = container.child(ghost_card(y + 5., 8., slide));
                            if hidden > 1 {
                                container = container.child(ghost_card(y + 10., 16., slide));
                            }
                        }
                        let base_opacity =
                            if slot == 0 { 1. } else { (1. - slot as f32 * 0.14).clamp(0.55, 1.) };
                        let opacity = t * base_opacity;
                        let notice_id = notice.id;
                        let mut card = div()
                            .id(("notif", notice_id))
                            .absolute()
                            .top(px(y))
                            .right(px(-slide))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .border_1()
                            .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.1 })
                            .bg(fade(INK, 0.78))
                            .shadow_lg()
                            .overflow_hidden()
                            .opacity(opacity)
                            .cursor_pointer()
                            .focusable()
                            .role(gpui::accesskit::Role::Button)
                            .aria_label(popup::a11y_label(notice))
                            .aria_keyshortcuts("Enter Space Escape")
                            .focus_visible(|s| s.border_2().border_color(fade(ACCENT, 1.)))
                            .active(|s| s.opacity(0.96))
                            .on_click(cx.listener(move |stack, _, _, cx| {
                                stack.dismiss(notice_id);
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(
                                move |stack, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.modifiers.modified() {
                                        return;
                                    }
                                    if matches!(
                                        event.keystroke.key.as_str(),
                                        "enter" | "space" | "escape"
                                    ) {
                                        stack.dismiss(notice_id);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .child(popup::badge(notice.icon.as_ref(), &notice.app))
                            .child(popup::card_content(notice));
                        if hidden > 0 {
                            card = card.child(more_chip(hidden));
                        }
                        container = container.child(card);
                    }
                    els.push(container);
                }
                els
            })
            .children({
                self.exiting
                    .iter()
                    .filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS)
                    .map(|ex| {
                        let t = anim::exit_progress(ex.start_ms);
                        let y = ex.y;
                        let slide_out = if reduced { 0. } else { t * (POPUP_W + MARGIN) };
                        let opacity = 1. - t;
                        div()
                            .id(("exiting", ex.notice.id))
                            .absolute()
                            .top(px(y))
                            .right(px(MARGIN - slide_out))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .border_1()
                            .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.1 })
                            .bg(fade(INK, 0.78))
                            .shadow_lg()
                            .overflow_hidden()
                            .opacity(opacity)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .child(popup::badge(ex.notice.icon.as_ref(), &ex.notice.app))
                            .child(popup::card_content(&ex.notice))
                    })
                    .collect::<Vec<_>>()
            })
            .into_any_element()
    }
}

fn ghost_card(y: f32, inset: f32, slide: f32) -> gpui::Div {
    div()
        .absolute()
        .top(px(y))
        .right(px(inset - slide))
        .w(px(POPUP_W - inset * 2.))
        .h(px(CARD_H))
        .rounded(px(CARD_R))
        .bg(fade(INK, 0.35))
}

fn more_chip(hidden: usize) -> gpui::Div {
    div()
        .absolute()
        .bottom(px(6.))
        .right(px(8.))
        .px(px(7.))
        .py(px(1.))
        .rounded(px(9.))
        .bg(fade(CHIP, 0.92))
        .text_size(px(10.))
        .text_color(fade(TEXT, 0.9))
        .child(format!("+{hidden}"))
}

pub fn open_window(cx: &mut gpui::App, queue: Queue) -> anyhow::Result<()> {
    cx.open_window(
        WindowOptions {
            titlebar: None,
            app_id: Some("nobody".to_string()),
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds {
                origin: point(px(0.), px(0.)),
                size: Size::new(px(POPUP_W + MARGIN * 2.), px(400.)),
            })),
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "nobody".to_string(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP | Anchor::RIGHT,
                exclusive_zone: Some(px(-1.)),
                exclusive_edge: None,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| NotificationStack::new(cx, queue)),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::queue::KEEP;

    #[test]
    fn step_toward_glides_and_settles() {
        let (v, settled) = step_toward(0., 100.);
        assert!((v - 30.).abs() < 1e-4);
        assert!(!settled);
        let (v, settled) = step_toward(99.8, 100.);
        assert_eq!(v, 100.);
        assert!(settled);
        let (v, settled) = step_toward(50., 50.);
        assert_eq!(v, 50.);
        assert!(settled);
    }

    #[test]
    fn step_toward_moves_down_and_converges() {
        let (v, settled) = step_toward(100., 0.);
        assert!((v - 70.).abs() < 1e-4);
        assert!(!settled);
        let mut v = 0f32;
        for _ in 0..200 {
            let (next, _) = step_toward(v, 100.);
            v = next;
        }
        assert_eq!(v, 100.);
    }

    #[test]
    fn hover_expand_pins_state_machine() {
        assert_eq!(hover_expand(None, "A", true), Some("A".to_owned()));
        assert_eq!(hover_expand(Some("A"), "A", true), Some("A".to_owned()));
        assert_eq!(hover_expand(Some("B"), "A", true), Some("A".to_owned()));
        assert_eq!(hover_expand(Some("A"), "A", false), None);
        assert_eq!(hover_expand(Some("B"), "A", false), Some("B".to_owned()));
        assert_eq!(hover_expand(None, "A", false), None);
    }

    #[test]
    fn visible_count_pins_cap() {
        assert_eq!(MAX_VISIBLE, 5);
        assert_eq!(visible_count(0), 0);
        assert_eq!(visible_count(1), 1);
        assert_eq!(visible_count(5), 5);
        assert_eq!(visible_count(6), 5);
        assert_eq!(visible_count(12), 5);
    }

    #[test]
    fn visible_count_caps_full_queue() {
        assert_eq!(visible_count(KEEP), MAX_VISIBLE);
    }
}
