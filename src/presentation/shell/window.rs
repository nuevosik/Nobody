use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

use gpui::accesskit::Role;
use gpui::{
    AppContext, Bounds, Context, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px, size,
};

use crate::application::config::{Anchor as ConfigAnchor, Config};
use crate::application::{clock, commands};
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;
use crate::presentation::theme::{
    ACCENT, CARD_GAP, CARD_H, CARD_R, CHIP, INK, MARGIN, MUTED, POPUP_W, QUIET_BADGE, TEXT,
    TEXT_BODY, TEXT_TITLE, app_font, fade,
};

use super::{anim, center, feed, geometry, popup};

pub(crate) const PANEL_H: f32 = 480.;

pub(crate) fn visible_count(n: usize, max_visible: usize) -> usize {
    n.min(max_visible)
}

pub struct NotificationStack {
    stack: feed::Stack,
    exiting: Vec<feed::Exiting>,
    queue: Queue,
    max_visible: usize,
    left: bool,
    quiet: bool,
    center_open: bool,
    center_query: String,
    center_marked: Option<Range<usize>>,
    search_focus: gpui::FocusHandle,
    popup_focus: gpui::FocusHandle,
    center_focus_armed: bool,
    dnd_manual: bool,
    dnd_auto: bool,
    expanded: Option<String>,
    last_window_h: Option<f32>,
    fullscreen_hidden: bool,
    last_age_tick_ms: u128,
    glide: GlideState,
}

pub(crate) struct Layout {
    pub deck_list: Vec<geometry::Deck>,
    pub shown: Vec<geometry::ShownDeck>,
    pub positions: Vec<f32>,
    pub input_regions: Vec<Bounds<gpui::Pixels>>,
    pub total_h: f32,
}

#[derive(Default)]
pub(crate) struct GlideState {
    smooth_y: HashMap<u32, f32>,
    target_y: HashMap<u32, f32>,
    shown_at: HashMap<u32, u128>,
    visible_y: HashMap<u32, f32>,
    last_expanded: Option<String>,
}

impl GlideState {
    pub(crate) fn shown_at(&self, id: u32) -> Option<u128> {
        self.shown_at.get(&id).copied()
    }

    pub(crate) fn is_entering(&self, now: u128) -> bool {
        self.shown_at.values().any(|&shown| now.saturating_sub(shown) < anim::ENTER_MS)
    }

    pub(crate) fn is_settling(&self) -> bool {
        self.target_y
            .iter()
            .any(|(id, target)| self.smooth_y.get(id).is_some_and(|s| (s - target).abs() >= 0.5))
    }

    pub(crate) fn advance(
        &mut self,
        notices: &[Notice],
        max_visible: usize,
        expanded: Option<&str>,
        exiting_alive: &[&feed::Exiting],
        reduced: bool,
        now: u128,
    ) -> Layout {
        let deck_list = geometry::decks(notices, expanded);
        let shown = geometry::shown_decks(&deck_list, visible_count(notices.len(), max_visible));
        let (y_map, _, _) = geometry::deck_layout(notices, &deck_list, &shown);

        let live: HashSet<u32> = notices.iter().map(|notice| notice.id).collect();
        self.smooth_y.retain(|id, _| live.contains(id));
        self.target_y.retain(|id, _| live.contains(id));
        self.shown_at.retain(|id, _| live.contains(id));

        let expanded_changed = self.last_expanded.as_deref() != expanded;
        for slot in &shown {
            let deck_top = y_map[deck_list[slot.deck].indices[0]];
            for &idx in &slot.indices {
                let id = notices[idx].id;
                self.target_y.insert(id, y_map[idx]);
                self.shown_at.entry(id).or_insert(now);
                let current = self.smooth_y.get(&id).copied().unwrap_or(deck_top);
                let next = if reduced || expanded_changed {
                    y_map[idx]
                } else {
                    step_toward(current, y_map[idx]).0
                };
                self.smooth_y.insert(id, next);
            }
        }
        self.last_expanded = expanded.map(str::to_owned);

        let mut total_h_current = 0f32;
        for slot in &shown {
            let deck = &deck_list[slot.deck];
            for &idx in &slot.indices {
                let id = notices[idx].id;
                let mut top = self.smooth_y.get(&id).copied().unwrap_or(y_map[idx]) + CARD_H;
                if deck.collapsed && idx == deck.indices[0] {
                    top += geometry::STACK_PEEK;
                }
                total_h_current = total_h_current.max(top);
            }
        }
        let exiting_max_y = exiting_alive.iter().map(|e| e.y + CARD_H).fold(0., f32::max);
        let total_h = if exiting_alive.is_empty() {
            total_h_current
        } else {
            total_h_current.max(exiting_max_y + CARD_GAP)
        };

        let positions: Vec<f32> = notices
            .iter()
            .enumerate()
            .map(|(i, notice)| self.smooth_y.get(&notice.id).copied().unwrap_or(y_map[i]))
            .collect();
        let input_regions = geometry::deck_input_regions(&deck_list, &shown, &positions);
        self.visible_y = shown
            .iter()
            .flat_map(|slot| slot.indices.iter())
            .map(|&idx| (notices[idx].id, positions[idx]))
            .collect();

        Layout { deck_list, shown, positions, input_regions, total_h }
    }
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
    pub fn new(cx: &mut Context<Self>, queue: Queue, max_visible: usize, left: bool) -> Self {
        spawn_anim_ticker(cx);
        spawn_feed_sync(queue.clone(), cx);

        Self {
            stack: feed::Stack::default(),
            exiting: Vec::new(),
            queue,
            max_visible,
            left,
            quiet: false,
            center_open: false,
            center_query: String::new(),
            center_marked: None,
            search_focus: cx.focus_handle().tab_stop(true).tab_index(3),
            popup_focus: cx.focus_handle().tab_stop(false).tab_index(0),
            center_focus_armed: false,
            dnd_manual: false,
            dnd_auto: false,
            expanded: None,
            last_window_h: None,
            fullscreen_hidden: false,
            last_age_tick_ms: 0,
            glide: GlideState::default(),
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
        let w = POPUP_W + MARGIN * 2. + CARD_R;
        if !self.center_focus_armed {
            self.center_focus_armed = true;
            window.focus(&self.search_focus, cx);
        }
        if self.last_window_h != Some(PANEL_H) {
            window.resize(Size::new(px(w), px(PANEL_H)));
            window.set_input_region(Some(&[Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(w), px(PANEL_H)),
            }]));
            self.last_window_h = Some(PANEL_H);
        }

        let entries = commands::history(&self.queue);
        let filtered = commands::filter_history(&entries, &self.center_query);
        let manual = commands::manual_quiet(&self.queue);
        let auto = commands::quiet_mode(&self.queue);
        let now = clock::now_ms();
        let query_text = self.center_query.clone();
        let show_placeholder = query_text.is_empty();
        let search_focused = self.search_focus.is_focused(window);
        let caret = || div().w(px(1.)).h(px(12.)).bg(fade(ACCENT, 1.));

        let root = div()
            .id("center")
            .tab_group()
            .size_full()
            .font(app_font())
            .text_size(px(11.))
            .relative()
            .p(px(MARGIN))
            .when(self.left, |el| el.pr(px(MARGIN + CARD_R)))
            .when(!self.left, |el| el.pl(px(MARGIN + CARD_R)))
            .child(
                gpui::canvas(|_, _, _| (), {
                    let left = self.left;
                    move |bounds, _, window, _| {
                        let origin = bounds.origin;
                        let mut path = gpui::PathBuilder::fill();
                        for cmd in panel_outline(
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                            left,
                        ) {
                            match cmd {
                                PanelCmd::Move(x, y) => path.move_to(origin + point(px(x), px(y))),
                                PanelCmd::Line(x, y) => path.line_to(origin + point(px(x), px(y))),
                                PanelCmd::Quad(to_x, to_y, ctrl_x, ctrl_y) => path.curve_to(
                                    origin + point(px(to_x), px(to_y)),
                                    origin + point(px(ctrl_x), px(ctrl_y)),
                                ),
                            }
                        }
                        path.close();
                        if let Ok(outline) = path.build() {
                            window.paint_path(outline, fade(INK, 0.92));
                        }
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .flex()
            .flex_col()
            .gap(px(8.))
            .on_key_down(cx.listener(|stack, event: &gpui::KeyDownEvent, window, cx| {
                let m = &event.keystroke.modifiers;
                let clean = !m.control && !m.alt && !m.platform && !m.function;
                match event.keystroke.key.as_str() {
                    "tab" if clean => {
                        if m.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }
                    "escape" if clean => {
                        stack.close_center();
                        cx.stop_propagation();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div().text_size(px(TEXT_TITLE)).text_color(fade(TEXT, 1.)).child(
                            center::history_header(filtered.len(), entries.len(), &query_text),
                        ),
                    )
                    .child(
                        div()
                            .id("center-close")
                            .tab_index(0)
                            .role(Role::Button)
                            .aria_label("Fechar central")
                            .aria_keyshortcuts("Escape")
                            .cursor_pointer()
                            .px(px(6.))
                            .py(px(0.))
                            .border_2()
                            .border_color(fade(INK, 0.))
                            .rounded(px(8.))
                            .bg(fade(CHIP, 1.))
                            .text_color(fade(TEXT, 0.9))
                            .focus_visible(|s| s.border_color(fade(ACCENT, 1.)))
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
                            .tab_index(1)
                            .role(Role::Button)
                            .aria_label("Alternar Não Perturbe manual")
                            .cursor_pointer()
                            .px(px(6.))
                            .py(px(0.))
                            .border_2()
                            .border_color(fade(INK, 0.))
                            .rounded(px(8.))
                            .bg(fade(CHIP, 1.))
                            .text_color(fade(TEXT, 0.9))
                            .focus_visible(|s| s.border_color(fade(ACCENT, 1.)))
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
                    .when(!entries.is_empty(), |el| {
                        el.child(
                            div()
                                .id("center-clear")
                                .tab_index(2)
                                .role(Role::Button)
                                .aria_label("Limpar histórico")
                                .cursor_pointer()
                                .px(px(6.))
                                .py(px(0.))
                                .border_2()
                                .border_color(fade(INK, 0.))
                                .rounded(px(8.))
                                .bg(fade(CHIP, 1.))
                                .text_color(fade(TEXT, 0.9))
                                .focus_visible(|s| s.border_color(fade(ACCENT, 1.)))
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
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            commands::clear_history(&stack.queue);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }
                                    },
                                )),
                        )
                    }),
            )
            .when(center::fullscreen_note(manual, auto).is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(fade(MUTED, 1.))
                        .child(center::fullscreen_note(manual, auto).unwrap_or("")),
                )
            })
            .child(ImeField {
                inner: div()
                    .id("center-search")
                    .track_focus(&self.search_focus)
                    .role(Role::SearchInput)
                    .aria_label("Buscar no histórico")
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if search_focused {
                        fade(ACCENT, 0.6)
                    } else {
                        gpui::Rgba { r: 1., g: 1., b: 1., a: 0.15 }
                    })
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .text_color(if show_placeholder { fade(MUTED, 1.) } else { fade(TEXT, 1.) })
                    .when(search_focused && show_placeholder, |el| el.child(caret()))
                    .child(if show_placeholder {
                        "Buscar por app, título ou corpo".to_string()
                    } else {
                        query_text.clone()
                    })
                    .when(search_focused && !show_placeholder, |el| el.child(caret()))
                    .on_key_down(cx.listener(
                        move |stack, event: &gpui::KeyDownEvent, window, cx| {
                            let m = &event.keystroke.modifiers;
                            let action = center::classify_search_key(
                                event.keystroke.key.as_str(),
                                event.keystroke.key_char.as_deref(),
                                m.shift,
                                m.control,
                                m.alt,
                                m.platform,
                                m.function,
                            );
                            match action {
                                center::SearchKeyAction::Close => {
                                    stack.close_center();
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                                center::SearchKeyAction::FocusNext => {
                                    window.focus_next(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                                center::SearchKeyAction::FocusPrev => {
                                    window.focus_prev(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                                center::SearchKeyAction::Paste => {
                                    let changed = if let Some(text) =
                                        cx.read_from_clipboard().and_then(|item| item.text())
                                    {
                                        let text = center::sanitize_ime_text(&text);
                                        let changed = center::append_query_with_marked(
                                            &mut stack.center_query,
                                            &mut stack.center_marked,
                                            &text,
                                        );
                                        if changed && !text.is_empty() {
                                            cx.stop_propagation();
                                        }
                                        changed
                                    } else {
                                        center::drop_marked(
                                            &mut stack.center_query,
                                            &mut stack.center_marked,
                                        )
                                    };
                                    if changed {
                                        cx.notify();
                                    }
                                }
                                center::SearchKeyAction::InsertChar(text) => {
                                    if center::append_query_with_marked(
                                        &mut stack.center_query,
                                        &mut stack.center_marked,
                                        &text,
                                    ) {
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }
                                center::SearchKeyAction::Backspace => {
                                    let changed = if center::drop_marked(
                                        &mut stack.center_query,
                                        &mut stack.center_marked,
                                    ) {
                                        true
                                    } else {
                                        center::apply_search_edit(&mut stack.center_query, &action)
                                    };
                                    if changed {
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }
                                center::SearchKeyAction::Ignore => {}
                            }
                        },
                    ))
                    .into_any_element(),
                stack: cx.entity(),
                focus: self.search_focus.clone(),
            });

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
                        .bg(fade(TEXT, 0.05))
                        .child(popup::badge(n.icon.as_ref(), &n.app, cx))
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
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(n.summary.clone()),
                                )
                                .when(!n.body.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(TEXT_BODY))
                                            .text_color(fade(MUTED, 1.))
                                            .overflow_hidden()
                                            .line_clamp(2)
                                            .child(n.body.clone()),
                                    )
                                }),
                        )
                })),
        )
    }
}

struct ImeField {
    inner: gpui::AnyElement,
    stack: gpui::Entity<NotificationStack>,
    focus: gpui::FocusHandle,
}

impl gpui::Element for ImeField {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, ()) {
        <gpui::AnyElement as gpui::Element>::request_layout(
            &mut self.inner,
            id,
            inspector_id,
            window,
            cx,
        )
    }

    fn prepaint(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<gpui::Pixels>,
        request_layout: &mut (),
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        <gpui::AnyElement as gpui::Element>::prepaint(
            &mut self.inner,
            id,
            inspector_id,
            bounds,
            request_layout,
            window,
            cx,
        );
    }

    fn paint(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<gpui::Pixels>,
        request_layout: &mut (),
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        <gpui::AnyElement as gpui::Element>::paint(
            &mut self.inner,
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
        window.handle_input(
            &self.focus,
            gpui::ElementInputHandler::new(bounds, self.stack.clone()),
            cx,
        );
    }
}

impl gpui::IntoElement for ImeField {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::EntityInputHandler for NotificationStack {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = center::utf8_range_for_utf16(&self.center_query, range);
        *adjusted_range = Some(center::utf16_range_for_utf8(&self.center_query, range.clone()));
        self.center_query.get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        let end = center::utf16_len(&self.center_query);
        Some(gpui::UTF16Selection { range: end..end, reversed: false })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.center_marked
            .clone()
            .map(|range| center::utf16_range_for_utf8(&self.center_query, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        center::ime_unmark(&mut self.center_marked);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        center::ime_replace(&mut self.center_query, &mut self.center_marked, range, text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        center::ime_mark(&mut self.center_query, &mut self.center_marked, range, new_text);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<gpui::Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(center::utf16_len(&self.center_query))
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(center::utf16_len(&self.center_query))
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        self.center_open
    }
}

fn spawn_anim_ticker(cx: &mut Context<NotificationStack>) {
    cx.spawn(async move |this, cx| {
        loop {
            let Ok(needs_anim) = this.update(cx, |stack, _| {
                stack.exiting.retain(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS);
                let entering = stack.glide.is_entering(clock::now_ms());
                let exiting = !stack.exiting.is_empty();
                let settling = stack.glide.is_settling();
                let now = clock::now_ms();
                let mut age_tick = false;
                if stack.center_open && now.saturating_sub(stack.last_age_tick_ms) >= 1000 {
                    stack.last_age_tick_ms = now;
                    age_tick = !commands::history(&stack.queue).is_empty();
                }
                entering || exiting || settling || age_tick
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
            let changed = queue.on_change();
            let snapshot = commands::snapshot(&queue);

            if this
                .update(cx, |stack, cx| {
                    let quiet = commands::effective_quiet(&stack.queue);
                    let changed = feed::sync_snapshot(
                        &mut stack.stack,
                        &mut stack.exiting,
                        &stack.glide.visible_y,
                        snapshot,
                        quiet,
                    );
                    let flipped = quiet != stack.quiet;
                    stack.quiet = quiet;
                    let manual = commands::manual_quiet(&stack.queue);
                    let auto = commands::quiet_mode(&stack.queue);
                    let dnd_changed = center::dnd_state_changed(
                        (stack.dnd_manual, stack.dnd_auto),
                        (manual, auto),
                    );
                    stack.dnd_manual = manual;
                    stack.dnd_auto = auto;
                    let center = commands::center_open(&stack.queue);
                    let center_flipped = center != stack.center_open;
                    stack.center_open = center;
                    if center_flipped {
                        stack.center_focus_armed = false;
                    }
                    if changed || flipped || center_flipped || dnd_changed {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }

            let fallback = cx.background_executor().timer(Duration::from_secs(2));
            let _ = futures_lite::future::or(changed, fallback).await;
        }
    })
    .detach();
}

impl gpui::Render for NotificationStack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let left = self.left;
        if commands::quiet_mode(&self.queue) {
            if !self.fullscreen_hidden {
                window.resize(Size::new(px(1.), px(1.)));
                window.set_input_region(Some(&[]));
                self.fullscreen_hidden = true;
            }
            return div().size_full().into_any_element();
        }
        if self.fullscreen_hidden {
            self.fullscreen_hidden = false;
            self.last_window_h = None;
        }
        if self.center_open {
            return self.render_center(window, cx).into_any_element();
        }
        if self.quiet {
            geometry::sync_window_geometry(
                window,
                &mut self.last_window_h,
                &[],
                MARGIN + QUIET_BADGE + MARGIN,
            );
            return div()
                .size_full()
                .relative()
                .when(!self.stack.notices.is_empty(), |el| {
                    el.child(
                        edge(div().absolute().top(px(MARGIN)), left, MARGIN)
                            .child(popup::nobody_badge()),
                    )
                })
                .into_any_element();
        }

        let card_h = CARD_H;
        let exiting_alive: Vec<&feed::Exiting> =
            self.exiting.iter().filter(|e| clock::elapsed_ms(e.start_ms) < anim::EXIT_MS).collect();
        let reduced = anim::prefers_reduced_motion();
        let layout = self.glide.advance(
            &self.stack.notices,
            self.max_visible,
            self.expanded.as_deref(),
            &exiting_alive,
            reduced,
            clock::now_ms(),
        );
        let deck_list = &layout.deck_list;
        let shown = &layout.shown;
        let positions = &layout.positions;
        let input_regions = &layout.input_regions;

        geometry::sync_window_geometry(
            window,
            &mut self.last_window_h,
            input_regions,
            layout.total_h,
        );

        if !shown.is_empty() && !self.popup_focus.contains_focused(window, cx) {
            window.focus(&self.popup_focus, cx);
        }

        let announcement = self
            .stack
            .notices
            .first()
            .map(|n| format!("{}: {} — {}", n.app, n.summary, n.body))
            .unwrap_or_default();

        div()
            .id("popup")
            .track_focus(&self.popup_focus)
            .tab_group()
            .tab_stop(false)
            .size_full()
            .relative()
            .font(app_font())
            .text_size(px(11.))
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = &event.keystroke.modifiers;
                let clean = !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && !modifiers.function;
                if clean && event.keystroke.key == "tab" {
                    if modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
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
                let mut tab_of = vec![0; self.stack.notices.len()];
                {
                    let mut slot = 0;
                    let mut tab = 0;
                    for (k, s) in shown.iter().enumerate() {
                        slot_of[k] = slot;
                        slot += s.indices.len();
                        for &idx in &s.indices {
                            tab_of[idx] = tab;
                            tab += 1 + self.stack.notices[idx].actions.len() / 2;
                        }
                    }
                }
                let mut els: Vec<gpui::Stateful<gpui::Div>> = Vec::new();
                for (k, s) in shown.iter().enumerate().rev() {
                    let deck = &deck_list[s.deck];
                    let first_y = f32::from(input_regions[k].origin.y);
                    let expand_app = deck.app.clone();
                    let mut container = edge(div().absolute().top(px(first_y)), left, MARGIN)
                        .id(format!("deck-{}", deck.app))
                        .w(px(POPUP_W))
                        .h(input_regions[k].size.height)
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
                        let y = positions[idx] - first_y;
                        let notice = &self.stack.notices[idx];
                        let hidden = if deck.collapsed && m == 0 { deck.hidden_count() } else { 0 };
                        let shown_since =
                            self.glide.shown_at(notice.id).unwrap_or(notice.arrived_at_ms);
                        let t = anim::enter_progress(shown_since);
                        let slide = if reduced { 0. } else { (1. - t) * (POPUP_W + MARGIN) };
                        if hidden > 0 {
                            container = container.child(ghost_card(y + 5., 8., slide, left));
                            if hidden > 1 {
                                container = container.child(ghost_card(y + 10., 16., slide, left));
                            }
                        }
                        let base_opacity =
                            if slot == 0 { 1. } else { (1. - slot as f32 * 0.07).clamp(0.8, 1.) };
                        let opacity = t * base_opacity;
                        let notice_id = notice.id;
                        let card_focus = window
                            .use_keyed_state(("card-focus", notice_id), cx, |_, cx| {
                                cx.focus_handle().tab_stop(true)
                            })
                            .read(cx)
                            .clone();
                        if deck.collapsed
                            && m == 0
                            && card_focus.is_focused(window)
                            && self.expanded.as_deref() != Some(deck.app.as_str())
                        {
                            self.expanded = Some(deck.app.clone());
                            cx.notify();
                        }
                        let mut card = edge(div().absolute().top(px(y)), left, -slide)
                            .id(("notif", notice_id))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .border_2()
                            .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.05 })
                            .bg(fade(INK, 0.78))
                            .shadow_lg()
                            .overflow_hidden()
                            .opacity(opacity)
                            .cursor_pointer()
                            .track_focus(&card_focus.tab_index(tab_of[idx] as isize))
                            .role(gpui::accesskit::Role::Button)
                            .aria_label(popup::a11y_label(notice))
                            .aria_keyshortcuts("Enter Space Escape")
                            .focus_visible(|s| s.border_color(fade(ACCENT, 1.)))
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
                            .child(popup::badge(notice.icon.as_ref(), &notice.app, cx))
                            .child(popup::card_content(notice));
                        let pairs = notice.actions.as_chunks::<2>().0;
                        if !pairs.is_empty() {
                            let action_id = notice_id;
                            let base_tab = tab_of[idx];
                            let scroll = window
                                .use_keyed_state(("action-scroll", action_id), cx, |_, _| {
                                    gpui::ScrollHandle::new()
                                })
                                .read(cx)
                                .clone();
                            card = card.child(
                                div()
                                    .id(format!("actions-{action_id}"))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .max_w(px(150.))
                                    .flex_shrink_0()
                                    .overflow_x_scroll()
                                    .track_scroll(&scroll)
                                    .children(pairs.iter().enumerate().map(|(i, pair)| {
                                        let key = pair[0].clone();
                                        let label = if key == "default" {
                                            "Abrir"
                                        } else if pair[1].is_empty() {
                                            "Ação"
                                        } else {
                                            &pair[1]
                                        }
                                        .to_string();
                                        let (focus, just_focused) = window
                                            .use_keyed_state(
                                                format!("action-focus-{action_id}-{key}"),
                                                cx,
                                                |_, cx| (cx.focus_handle().tab_stop(true), false),
                                            )
                                            .update(cx, |(focus, was_focused), _| {
                                                let focused = focus.is_focused(window);
                                                let just_focused = focused && !*was_focused;
                                                *was_focused = focused;
                                                (focus.clone(), just_focused)
                                            });
                                        if just_focused {
                                            scroll.scroll_to_item(i);
                                        }
                                        let key_click = key.clone();
                                        div()
                                            .id(format!("action-{action_id}-{key}"))
                                            .track_focus(
                                                &focus.tab_index((base_tab + 1 + i) as isize),
                                            )
                                            .role(Role::Button)
                                            .aria_label(label.clone())
                                            .aria_keyshortcuts("Enter Space")
                                            .cursor_pointer()
                                            .px(px(6.))
                                            .py(px(2.))
                                            .border_2()
                                            .border_color(fade(INK, 0.))
                                            .rounded(px(6.))
                                            .bg(fade(CHIP, 1.))
                                            .text_color(fade(TEXT, 0.95))
                                            .flex_shrink_0()
                                            .max_w(px(150.))
                                            .truncate()
                                            .focus_visible(|s| s.border_color(fade(ACCENT, 1.)))
                                            .child(label)
                                            .on_click(cx.listener(move |stack, _, _, cx| {
                                                cx.stop_propagation();
                                                commands::request_action(
                                                    &stack.queue,
                                                    action_id,
                                                    &key_click,
                                                );
                                                cx.notify();
                                            }))
                                            .on_key_down(cx.listener(
                                                move |stack, event: &gpui::KeyDownEvent, _, cx| {
                                                    if event.keystroke.modifiers.modified() {
                                                        return;
                                                    }
                                                    if matches!(
                                                        event.keystroke.key.as_str(),
                                                        "enter" | "space"
                                                    ) {
                                                        cx.stop_propagation();
                                                        commands::request_action(
                                                            &stack.queue,
                                                            action_id,
                                                            &key,
                                                        );
                                                        cx.notify();
                                                    }
                                                },
                                            ))
                                    })),
                            );
                        }
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
                        edge(div().absolute().top(px(y)), left, MARGIN - slide_out)
                            .id(("exiting", ex.notice.id))
                            .w(px(POPUP_W))
                            .min_h(px(card_h))
                            .rounded(px(CARD_R))
                            .border_2()
                            .border_color(gpui::Rgba { r: 1., g: 1., b: 1., a: 0.05 })
                            .bg(fade(INK, 0.78))
                            .shadow_lg()
                            .overflow_hidden()
                            .opacity(opacity)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .child(popup::badge(ex.notice.icon.as_ref(), &ex.notice.app, cx))
                            .child(popup::card_content(&ex.notice))
                    })
                    .collect::<Vec<_>>()
            })
            .into_any_element()
    }
}

fn edge(el: gpui::Div, left: bool, offset: f32) -> gpui::Div {
    if left { el.left(px(offset)) } else { el.right(px(offset)) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PanelCmd {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
}

fn panel_outline(w: f32, h: f32, left: bool) -> Vec<PanelCmd> {
    let x = |v: f32| if left { w - v } else { v };
    let r = CARD_R;
    vec![
        PanelCmd::Move(x(0.), 0.),
        PanelCmd::Line(x(w), 0.),
        PanelCmd::Line(x(w), h),
        PanelCmd::Line(x(r * 2.), h),
        PanelCmd::Quad(x(r), h - r, x(r), h),
        PanelCmd::Line(x(r), r),
        PanelCmd::Quad(x(0.), 0., x(r), 0.),
    ]
}

fn ghost_card(y: f32, inset: f32, slide: f32, left: bool) -> gpui::Div {
    edge(div().absolute().top(px(y)), left, inset - slide)
        .w(px(POPUP_W - inset * 2.))
        .h(px(CARD_H))
        .rounded(px(CARD_R))
        .bg(fade(INK, 0.35))
}

fn more_chip(hidden: usize) -> gpui::Div {
    div()
        .flex_shrink_0()
        .px(px(7.))
        .py(px(1.))
        .rounded(px(9.))
        .bg(fade(CHIP, 0.92))
        .text_size(px(10.))
        .text_color(fade(TEXT, 0.9))
        .child(format!("+{hidden}"))
}

pub fn open_window(cx: &mut gpui::App, queue: Queue, config: Config) -> anyhow::Result<()> {
    let anchor = match config.anchor {
        ConfigAnchor::TopRight => Anchor::TOP | Anchor::RIGHT,
        ConfigAnchor::TopLeft => Anchor::TOP | Anchor::LEFT,
    };
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
                anchor,
                exclusive_zone: Some(px(0.)),
                exclusive_edge: None,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            let left = config.anchor == ConfigAnchor::TopLeft;
            cx.new(|cx| NotificationStack::new(cx, queue, config.max_visible, left))
        },
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
        assert_eq!(visible_count(0, 5), 0);
        assert_eq!(visible_count(1, 5), 1);
        assert_eq!(visible_count(5, 5), 5);
        assert_eq!(visible_count(6, 5), 5);
        assert_eq!(visible_count(12, 5), 5);
    }

    #[test]
    fn visible_count_caps_full_queue() {
        assert_eq!(visible_count(KEEP, 5), 5);
        assert_eq!(visible_count(KEEP, 12), KEEP);
    }

    #[test]
    fn panel_outline_mirrors_the_flare_for_the_left_anchor() {
        let (w, h) = (POPUP_W + MARGIN * 2. + CARD_R, PANEL_H);
        let right = panel_outline(w, h, false);
        let left = panel_outline(w, h, true);

        let expected: Vec<PanelCmd> = right
            .iter()
            .map(|cmd| match *cmd {
                PanelCmd::Move(x, y) => PanelCmd::Move(w - x, y),
                PanelCmd::Line(x, y) => PanelCmd::Line(w - x, y),
                PanelCmd::Quad(to_x, to_y, ctrl_x, ctrl_y) => {
                    PanelCmd::Quad(w - to_x, to_y, w - ctrl_x, ctrl_y)
                }
            })
            .collect();
        assert_eq!(left, expected, "left anchor must be the x-mirror of the right one");
        assert_ne!(left, right, "left anchor must not reuse the right-anchored outline");

        assert_eq!(right[1], PanelCmd::Line(w, 0.), "right anchor is flush on the right");
        assert_eq!(left[1], PanelCmd::Line(0., 0.), "left anchor is flush on the left");
        assert_eq!(right[5], PanelCmd::Line(CARD_R, CARD_R));
        assert_eq!(left[5], PanelCmd::Line(w - CARD_R, CARD_R), "flare moves to the right");
    }

    use crate::presentation::theme::STACK_TOP;

    fn mk(id: u32, app: &str) -> Notice {
        Notice {
            id,
            app: app.into(),
            desktop_entry: None,
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
            stack_tag: None,
            progress: None,
        }
    }

    #[test]
    fn advance_glides_existing_cards_toward_their_new_slot() {
        let mut glide = GlideState::default();
        let both = vec![mk(1, "A"), mk(2, "B")];
        let first = glide.advance(&both, 5, None, &[], false, 1_000);
        let b_start = first.positions[1];
        assert!((b_start - (STACK_TOP + CARD_H + CARD_GAP)).abs() < 0.01);
        assert!(!glide.is_settling(), "a fresh layout is already at rest");

        let only_b = vec![mk(2, "B")];
        let second = glide.advance(&only_b, 5, None, &[], false, 1_016);
        let moved = second.positions[0];
        assert!(moved < b_start, "card must glide toward the freed slot: {moved} >= {b_start}");
        assert!(moved > STACK_TOP, "one frame must not snap to the target: {moved}");
        assert!(glide.is_settling());
    }

    #[test]
    fn advance_snaps_under_reduced_motion_and_on_expansion_change() {
        let both = vec![mk(1, "A"), mk(2, "B")];
        let only_b = vec![mk(2, "B")];

        let mut reduced = GlideState::default();
        reduced.advance(&both, 5, None, &[], false, 1_000);
        let snapped = reduced.advance(&only_b, 5, None, &[], true, 1_016);
        assert!((snapped.positions[0] - STACK_TOP).abs() < 0.01, "reduced motion jumps");
        assert!(!reduced.is_settling());

        let mut expanded = GlideState::default();
        expanded.advance(&both, 5, None, &[], false, 1_000);
        let jumped = expanded.advance(&only_b, 5, Some("B"), &[], false, 1_016);
        assert!((jumped.positions[0] - STACK_TOP).abs() < 0.01, "expansion change jumps");
        assert!(!expanded.is_settling());
    }

    #[test]
    fn advance_drops_animation_state_for_vanished_ids() {
        let mut glide = GlideState::default();
        glide.advance(&[mk(1, "A"), mk(2, "B")], 5, None, &[], false, 1_000);
        let second = glide.advance(&[mk(2, "B")], 5, None, &[], false, 1_016);
        assert_eq!(second.positions.len(), 1);
        assert!(glide.shown_at(1).is_none(), "a dismissed id must not keep glide state");
        assert!(glide.shown_at(2).is_some());
    }

    #[test]
    fn advance_reserves_room_for_cards_that_are_still_exiting() {
        let mut glide = GlideState::default();
        let notices = vec![mk(1, "A")];
        let plain = glide.advance(&notices, 5, None, &[], false, 1_000);
        assert_eq!(plain.total_h, STACK_TOP + CARD_H);

        let leaving = feed::Exiting { notice: mk(9, "X"), start_ms: 0, y: 500. };
        let alive: Vec<&feed::Exiting> = vec![&leaving];
        let with_exit = glide.advance(&notices, 5, None, &alive, false, 1_016);
        assert_eq!(with_exit.total_h, 500. + CARD_H + CARD_GAP);
    }

    #[test]
    fn advance_records_only_rendered_y_for_collapsed_decks() {
        let mut glide = GlideState::default();
        let notices = vec![mk(1, "A"), mk(2, "A"), mk(3, "B")];
        glide.advance(&notices, 5, None, &[], false, 1_000);
        assert_eq!(glide.visible_y.get(&1).copied(), Some(STACK_TOP));
        assert_eq!(
            glide.visible_y.get(&3).copied(),
            Some(STACK_TOP + CARD_H + geometry::STACK_PEEK + CARD_GAP)
        );
        assert!(!glide.visible_y.contains_key(&2), "id escondido no deck colapsado");

        let mut capped = GlideState::default();
        capped.advance(&notices, 1, None, &[], false, 1_000);
        assert!(!capped.visible_y.contains_key(&3), "sem slot acima do max_visible");
    }

    #[test]
    fn advance_adds_the_peek_for_a_collapsed_deck() {
        let mut single = GlideState::default();
        let one = single.advance(&[mk(1, "A")], 5, None, &[], false, 1_000);
        let mut collapsed = GlideState::default();
        let pair = collapsed.advance(&[mk(1, "A"), mk(2, "A")], 5, None, &[], false, 1_000);
        assert_eq!(one.total_h, STACK_TOP + CARD_H);
        assert_eq!(pair.total_h, STACK_TOP + CARD_H + geometry::STACK_PEEK);
    }

    #[test]
    fn is_entering_expires_after_the_enter_window() {
        let mut glide = GlideState::default();
        glide.advance(&[mk(1, "A")], 5, None, &[], false, 1_000);
        assert!(glide.is_entering(1_000));
        assert!(glide.is_entering(1_000 + anim::ENTER_MS - 1));
        assert!(!glide.is_entering(1_000 + anim::ENTER_MS));
    }

    #[test]
    fn animating_predicate_converges_so_the_ticker_can_stop() {
        let mut glide = GlideState::default();
        glide.advance(&[mk(1, "A"), mk(2, "B")], 5, None, &[], false, 1_000);
        let only_b = [mk(2, "B")];

        let mut now = 1_016;
        let mut frames = 0;
        while glide.is_entering(now) || glide.is_settling() {
            glide.advance(&only_b, 5, None, &[], false, now);
            now += 16;
            frames += 1;
            assert!(frames < 1_000, "ticker predicate never converges; would notify every 16ms");
        }
        assert!(frames > 0, "the layout change must take at least one frame");

        glide.advance(&only_b, 5, None, &[], false, now);
        assert!(!glide.is_entering(now + 16));
        assert!(!glide.is_settling());
    }
}
