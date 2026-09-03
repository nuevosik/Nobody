//! Presentation — `NotificationStack` (LayerShell overlay).

use std::collections::HashMap;
use std::time::Duration;

use gpui::accesskit::Role;
use gpui::{
    AppContext, Bounds, Context, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px, size,
};
use zbus::fdo::RequestNameReply;

use crate::application::{clock, commands};
use crate::domain::notice::{Notice, Stack};
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::daemon::{self, NOTIFICATION_PATH, NotificationDaemon};
use crate::presentation::theme::{
    ACCENT, CARD_GAP, CARD_H, CARD_R, FONT, INK, MARGIN, POPUP_W, fade,
};

use super::anim;
use super::popup;

const DECK_GAP: f32 = 16.;
const MIN_HIT: f32 = 24.;

fn grouped_y(notices: &[Notice], idx: usize) -> f32 {
    grouped_y_map(notices).get(idx).copied().unwrap_or(MARGIN)
}

/// Calcula Y agrupado por app em O(n).
fn grouped_y_map(notices: &[Notice]) -> Vec<f32> {
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
    let mut y_cursor = MARGIN;
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
fn total_h_current_for(notices: &[Notice], n: usize) -> f32 {
    if n == 0 {
        return 0.;
    }
    let y_map = grouped_y_map(notices);
    y_map.iter().take(n).fold(MARGIN, |a, &y| a.max(y)) + CARD_H
}

struct Exiting {
    notice: Notice,
    start_ms: u128,
    y: f32,
}

pub struct NotificationStack {
    stack: Stack,
    exiting: Vec<Exiting>,
    queue: Queue,
    last_window_h: Option<f32>,
    last_input_len: usize,
}

impl NotificationStack {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let queue = Queue::new();

        spawn_anim_ticker(cx);
        spawn_dbus(queue.clone(), cx);

        Self {
            stack: Stack::default(),
            exiting: Vec::new(),
            queue,
            last_window_h: None,
            last_input_len: usize::MAX,
        }
    }

    fn dismiss(&self, id: u32) {
        commands::request_dismissal(&self.queue, id);
    }

    fn sync_window_geometry(&mut self, window: &mut Window, total_h: f32, n: usize) {
        let should_resize = self.last_window_h.is_none_or(|h| (h - total_h).abs() > 0.5);
        if should_resize {
            let h = if total_h > 0. { total_h } else { 1. };
            window.resize(Size::new(px(POPUP_W + MARGIN * 2.), px(h)));
            self.last_window_h = Some(total_h);
        }

        let should_recalc_input = self.last_input_len != n || should_resize;
        if should_recalc_input {
            let y_map = grouped_y_map(&self.stack.notices);
            // Pré-computa último índice de cada app para hit-region correta
            let mut last_idx_per_app: HashMap<String, usize> = HashMap::new();
            for (k, n) in self.stack.notices.iter().enumerate() {
                last_idx_per_app.insert(n.app.clone(), k);
            }
            let cards: Vec<Bounds<gpui::Pixels>> = (0..n)
                .map(|i| {
                    let y = y_map.get(i).copied().unwrap_or(MARGIN);
                    let is_last_in_group =
                        last_idx_per_app.get(&self.stack.notices[i].app).copied() == Some(i);
                    let h = if is_last_in_group { CARD_H } else { MIN_HIT };
                    Bounds {
                        origin: point(px(0.), px(y)),
                        size: size(px(POPUP_W + MARGIN * 2.), px(h)),
                    }
                })
                .collect();
            window.set_input_region(Some(&cards));
            self.last_input_len = n;
        }
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
                // entidade foi dropada (janela fechada) → encerra ticker
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

fn spawn_dbus(queue: Queue, cx: &mut Context<NotificationStack>) {
    cx.spawn(async move |this, cx| {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "nobody: não foi possível conectar ao session bus ({e}). Verifique DBUS_SESSION_BUS_ADDRESS."
                );
                return;
            }
        };

        let daemon = NotificationDaemon {
            queue: queue.clone(),
        };
        if let Err(e) = conn
            .object_server()
            .at(NOTIFICATION_PATH, daemon)
            .await
        {
            eprintln!("nobody: register interface: {e}");
            return;
        }

        let name = "org.freedesktop.Notifications";
        match conn
            .request_name_with_flags(name, zbus::fdo::RequestNameFlags::DoNotQueue.into())
            .await
        {
            Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {}
            Ok(_) => {
                eprintln!(
                    "nobody: outro daemon ocupa {name}. Pare o mako: systemctl --user stop mako && pkill mako"
                );
                return;
            }
            Err(e) => {
                eprintln!("nobody: request_name {name}: {e}");
                return;
            }
        }

        loop {
            flush_lifecycle_events(&conn, &queue).await;
            let snapshot = commands::snapshot(&queue);

            if this
                .update(cx, |stack, cx| {
                let now_ms = clock::now_ms();
                let mut removed: Vec<(u32, f32)> = Vec::new();
                for (idx, old) in stack.stack.notices.iter().enumerate() {
                    if !snapshot.iter().any(|n| n.id == old.id) {
                        removed.push((old.id, grouped_y(&stack.stack.notices, idx)));
                    }
                }
                for (id, y) in &removed {
                    if let Some(old) = stack.stack.notices.iter().find(|n| n.id == *id).cloned()
                        && !stack.exiting.iter().any(|e| e.notice.id == *id)
                    {
                        stack.exiting.push(Exiting {
                            notice: old,
                            start_ms: now_ms,
                            y: *y,
                        });
                    }
                }
                stack.exiting.retain(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS);

                if stack.stack.notices != snapshot || !removed.is_empty() {
                    stack.stack = Stack { notices: snapshot };
                    cx.notify();
                } else if !stack.exiting.is_empty() {
                    cx.notify();
                }
            })
            .is_err()
            {
                break;
            }

            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }
    })
    .detach();
}

async fn flush_lifecycle_events(connection: &zbus::Connection, queue: &Queue) {
    let interface = match connection
        .object_server()
        .interface::<_, NotificationDaemon>(NOTIFICATION_PATH)
        .await
    {
        Ok(interface) => interface,
        Err(error) => {
            eprintln!("nobody: não foi possível obter a interface de notificações: {error}");
            return;
        }
    };

    for notice in commands::expire(queue, clock::now_ms()) {
        if let Err(error) = daemon::emit_notification_closed(
            interface.signal_emitter(),
            notice.id,
            crate::domain::queue::CloseReason::Expired,
        )
        .await
        {
            eprintln!("nobody: falha ao sinalizar expiração de {}: {error}", notice.id);
        }
    }

    for request in queue.drain_close_requests() {
        if queue.remove(request.id).is_none() {
            continue;
        }
        if let Err(error) =
            daemon::emit_notification_closed(interface.signal_emitter(), request.id, request.reason)
                .await
        {
            eprintln!("nobody: falha ao sinalizar fechamento de {}: {error}", request.id);
        }
    }
}

impl gpui::Render for NotificationStack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let n = self.stack.notices.len().min(5);
        let card_h = CARD_H;

        // Filtra expirados já passados do EXIT_MS sem mutar durante render
        let exiting_alive: Vec<&Exiting> =
            self.exiting.iter().filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS).collect();
        let exiting_max_y = exiting_alive.iter().map(|e| e.y + card_h).fold(0., f32::max);
        let total_h_current = total_h_current_for(&self.stack.notices, n);
        let total_h = if exiting_alive.is_empty() {
            total_h_current
        } else {
            total_h_current.max(exiting_max_y + CARD_GAP)
        };

        self.sync_window_geometry(window, total_h, n);

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
                let y_map = grouped_y_map(&self.stack.notices);
                self.stack
                    .notices
                    .iter()
                    .take(5)
                    .enumerate()
                    .rev()
                    .map(move |(i, notice)| {
                        let y_target = y_map[i];
                        let t = anim::enter_progress(notice.arrived_at_ms);
                        let y = if reduced { y_target } else { y_target - (1. - t) * 28. };
                        let base_opacity =
                            if i == 0 { 1. } else { (1. - i as f32 * 0.14).clamp(0.55, 1.) };
                        let opacity = t * base_opacity;
                        let notice_id = notice.id;
                        div()
                            .id(("notif", notice_id))
                            .absolute()
                            .top(px(y))
                            .right(px(MARGIN))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .bg(fade(INK, 0.97))
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
                            .gap(px(12.))
                            .px(px(16.))
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
                        let y = if reduced { ex.y } else { ex.y - t * 12. };
                        let opacity = 1. - t;
                        div()
                            .id(("exiting", ex.notice.id))
                            .absolute()
                            .top(px(y))
                            .right(px(MARGIN))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .bg(fade(INK, 0.97))
                            .shadow_lg()
                            .overflow_hidden()
                            .opacity(opacity)
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(16.))
                            .child(popup::badge(ex.notice.icon.as_ref(), &ex.notice.app))
                            .child(popup::card_content(&ex.notice))
                    })
                    .collect::<Vec<_>>()
            })
    }
}

/// Cria a janela LayerShell. Retorna erro em vez de panic.
pub fn open_window(cx: &mut gpui::App) -> anyhow::Result<()> {
    cx.open_window(
        WindowOptions {
            titlebar: None,
            app_id: Some("nobody".to_string()),
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
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
        |_, cx| cx.new(NotificationStack::new),
    )?;
    Ok(())
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
        // A,B,A intercalados: y_map = [MARGIN, 120, MARGIN+DECK_GAP].
        // total_h correto = max(y[0..3]) + CARD_H = 120 + 76 = 196,
        // não y[2] + CARD_H = 28 + 76 = 104 (janela cliparia o card B).
        let notices = vec![mk(1, "A"), mk(2, "B"), mk(3, "A")];
        let n = notices.len();
        let y_map = grouped_y_map(&notices);
        assert_eq!(y_map.len(), 3);
        let expected = y_map.iter().take(n).fold(MARGIN, |a, &y| a.max(y)) + CARD_H;
        assert!((expected - 196.).abs() < 0.01, "expected 196, got {expected}");
        let buggy = y_map[n - 1] + CARD_H;
        assert!(
            (buggy - 104.).abs() < 0.01,
            "precondição do bug: y[n-1]+CARD_H deve ser 104, foi {buggy}"
        );
        let got = total_h_current_for(&notices, n);
        assert!(
            (got - expected).abs() < 0.01,
            "total_h_current deve ser max(y)+CARD_H={expected}, foi {got}"
        );
    }
}
