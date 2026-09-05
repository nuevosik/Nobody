use std::time::Duration;

use gpui::accesskit::Role;
use gpui::{
    AppContext, Context, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px,
};

use crate::application::{clock, commands};
use crate::domain::queue::Queue;
use crate::presentation::theme::{
    ACCENT, CARD_GAP, CARD_H, CARD_R, FONT, INK, MARGIN, POPUP_W, QUIET_BADGE, fade,
};

use super::{anim, feed, geometry, popup};

pub struct NotificationStack {
    stack: feed::Stack,
    exiting: Vec<feed::Exiting>,
    queue: Queue,
    quiet: bool,
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
                0,
            );
            return div().size_full().relative().child(
                div().absolute().top(px(MARGIN)).right(px(MARGIN)).child(popup::nobody_badge()),
            );
        }

        let n = self.stack.notices.len().min(5);
        let card_h = CARD_H;

        let exiting_alive: Vec<&feed::Exiting> =
            self.exiting.iter().filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS).collect();
        let exiting_max_y = exiting_alive.iter().map(|e| e.y + card_h).fold(0., f32::max);
        let total_h_current = geometry::total_h_current_for(&self.stack.notices, n);
        let total_h = if exiting_alive.is_empty() {
            total_h_current
        } else {
            total_h_current.max(exiting_max_y + CARD_GAP)
        };

        geometry::sync_window_geometry(
            window,
            &mut self.last_window_h,
            &mut self.last_input_len,
            &self.stack.notices,
            total_h,
            n,
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
                let y_map = geometry::grouped_y_map(&self.stack.notices);
                self.stack
                    .notices
                    .iter()
                    .take(5)
                    .enumerate()
                    .rev()
                    .map(move |(i, notice)| {
                        let y_target = y_map[i];
                        let t = anim::enter_progress(notice.arrived_at_ms);
                        let slide = if reduced { 0. } else { (1. - t) * (POPUP_W + MARGIN) };
                        let base_opacity =
                            if i == 0 { 1. } else { (1. - i as f32 * 0.14).clamp(0.55, 1.) };
                        let opacity = t * base_opacity;
                        let notice_id = notice.id;
                        div()
                            .id(("notif", notice_id))
                            .absolute()
                            .top(px(y_target))
                            .right(px(MARGIN - slide))
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
                            .child(popup::card_content(notice))
                    })
                    .collect::<Vec<_>>()
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
