use gpui::{
    BoxShadow, Context, CursorStyle, FontWeight, IntoElement, MouseButton, div, hsla, prelude::*,
    px, rgb,
};

use crate::agent::SessionStatus;
use crate::app::AuiApp;

pub fn render_sidebar(view: &AuiApp, cx: &mut Context<AuiApp>) -> impl IntoElement {
    let active_id = view.active_session_id();
    let mut session_rows = Vec::new();

    for session in view.sessions() {
        let id = session.id;
        let is_active = active_id == Some(id);
        let dot_color = status_color(&session.status);
        let row_bg = if is_active {
            hsla(0.52, 0.32, 0.92, 0.35)
        } else {
            hsla(0.0, 0.0, 1.0, 0.0)
        };

        session_rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .rounded_lg()
                .border_1()
                .border_color(hsla(0.0, 0.0, 0.0, 0.06))
                .bg(row_bg)
                .px(px(12.))
                .py(px(10.))
                .cursor(CursorStyle::PointingHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _, _, cx| view.select_session(id, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(10.))
                                .h(px(10.))
                                .rounded_full()
                                .bg(dot_color)
                                .shadow(vec![BoxShadow {
                                    color: dot_color,
                                    offset: gpui::point(px(0.), px(0.)),
                                    blur_radius: px(12.),
                                    spread_radius: px(0.),
                                }]),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0x0b1220))
                                        .child(session.title.clone()),
                                )
                                .child(
                                    div()
                                        .px(px(8.))
                                        .py(px(2.))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                                        .text_xs()
                                        .text_color(rgb(0x5b6777))
                                        .cursor(CursorStyle::PointingHand)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _, _, cx| {
                                                cx.stop_propagation();
                                                view.cycle_session_provider(id, cx);
                                            }),
                                        )
                                        .child(session.provider.name.to_string()),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x475569))
                                .child(session.status_label()),
                        )
                        .child(
                            div()
                                .px(px(6.))
                                .py(px(2.))
                                .rounded_full()
                                .border_1()
                                .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                                .bg(hsla(0.0, 0.0, 1.0, 0.0))
                                .text_xs()
                                .text_color(rgb(0x64748b))
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, _, cx| {
                                        cx.stop_propagation();
                                        view.delete_session(id, cx);
                                    }),
                                )
                                .child("x"),
                        ),
                )
                .into_any_element(),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x0b1220))
                        .child("AUI Desktop"),
                )
                .child(div().text_xs().text_color(rgb(0x5b6777)).child("Sessions")),
        )
        .child(
            div()
                .px(px(12.))
                .py(px(6.))
                .rounded_full()
                .bg(hsla(0.48, 0.52, 0.9, 0.35))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x0f172a))
                .cursor(CursorStyle::PointingHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| view.new_session(cx)),
                )
                .child("+ New"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(rgb(0x5b6777))
                .child("New session provider:")
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.1))
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(rgb(0x0f172a))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.cycle_new_session_provider(cx)),
                        )
                        .child(view.new_session_provider_label()),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x5b6777))
                .child(format!("{} sessions", view.sessions().len())),
        )
        .child(div().flex().flex_col().gap_2().children(session_rows))
}

fn status_color(status: &SessionStatus) -> gpui::Hsla {
    match status {
        SessionStatus::Idle => hsla(0.0, 0.0, 0.6, 0.45),
        SessionStatus::Thinking => hsla(0.53, 0.65, 0.55, 0.9),
        SessionStatus::Executing { .. } => hsla(0.08, 0.75, 0.55, 0.9),
        SessionStatus::WaitingInput { .. } => hsla(0.12, 0.85, 0.52, 0.85),
        SessionStatus::Error { .. } => hsla(0.0, 0.7, 0.55, 0.9),
    }
}
