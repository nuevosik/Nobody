use std::time::Duration;

use gpui::accesskit::Role;
use gpui::{
    AppContext, Context, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px,
};

use crate::application::{clock, commands};
use crate::domain::queue::Queue;
use crate::presentation::theme::{
    ACCENT, CARD_GAP, CARD_H, CARD_R, CHIP, FONT, INK, MARGIN, POPUP_W, QUIET_BADGE, TEXT, fade,
};

use super::{anim, feed, geometry, popup};

pub(crate) const MAX_VISIBLE: usize = 5;

pub(crate) fn visible_count(n: usize) -> usize {
    n.min(MAX_VISIBLE)
}

pub struct NotificationStack {
    stack: feed::Stack,
    exiting: Vec<feed::Exiting>,
    queue: Queue,
    quiet: bool,
    expanded: Option<String>,
    last_window_h: Option<f32>,
    last_input_len: usize,
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
            expanded: None,
            last_window_h: None,
            last_input_len: usize::MAX,
        }
    }

    fn dismiss(&self, id: u32) {
        commands::request_dismissal(&self.queue, id);
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
                entering || exiting
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
                    let quiet = commands::quiet_mode(&stack.queue);
                    let changed =
                        feed::apply_snapshot(&mut stack.stack, &mut stack.exiting, snapshot);
                    let flipped = quiet != stack.quiet;
                    stack.quiet = quiet;
                    if changed || flipped {
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
        if self.quiet && !self.stack.notices.is_empty() {
            geometry::sync_window_geometry(
                window,
                &mut self.last_window_h,
                &mut self.last_input_len,
                &[],
                MARGIN + QUIET_BADGE + MARGIN,
            );
            return div().size_full().relative().child(
                div().absolute().top(px(MARGIN)).right(px(MARGIN)).child(popup::nobody_badge()),
            );
        }

        let n = visible_count(self.stack.notices.len());
        let card_h = CARD_H;

        let exiting_alive: Vec<&feed::Exiting> =
            self.exiting.iter().filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS).collect();
        let exiting_max_y = exiting_alive.iter().map(|e| e.y + card_h).fold(0., f32::max);
        let deck_list = geometry::decks(&self.stack.notices, self.expanded.as_deref());
        let shown = geometry::shown_decks(&deck_list, n);
        let (y_map, _, deck_h) = geometry::deck_layout(&self.stack.notices, &deck_list, &shown);
        let total_h_current = deck_h;
        let total_h = if exiting_alive.is_empty() {
            total_h_current
        } else {
            total_h_current.max(exiting_max_y + CARD_GAP)
        };
        let cards_y: Vec<f32> =
            shown.iter().flat_map(|s| s.indices.iter().map(|&i| y_map[i])).collect();

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

        let reduced = anim::prefers_reduced_motion();

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
                    let first_y = y_map[s.indices[0]];
                    let last_y = y_map[*s.indices.last().expect("deck mostrado não é vazio")];
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
                            if *hovered {
                                if stack.expanded.as_deref() != Some(expand_app.as_str()) {
                                    stack.expanded = Some(expand_app.clone());
                                    cx.notify();
                                }
                            } else if stack.expanded.as_deref() == Some(expand_app.as_str()) {
                                stack.expanded = None;
                                cx.notify();
                            }
                        }));
                    for (m, &idx) in s.indices.iter().enumerate() {
                        let slot = slot_of[k] + m;
                        let y = y_map[idx] - first_y;
                        let notice = &self.stack.notices[idx];
                        let hidden = if deck.collapsed && m == 0 { deck.hidden_count() } else { 0 };
                        let t = anim::enter_progress(notice.arrived_at_ms);
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
