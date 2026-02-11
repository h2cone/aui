use gpui::{
    Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div, hsla,
    prelude::*, px,
};

use crate::app::AuiApp;
use crate::ui::theme;
use aui_ai::SessionStatus;

pub fn render_sidebar(view: &AuiApp, cx: &mut Context<AuiApp>) -> impl IntoElement {
    let active_id = view.active_session_id();
    let mut session_rows = Vec::new();

    for session in view.sessions() {
        let id = session.id;
        let is_active = active_id == Some(id);
        let dot_color = status_color(&session.status);
        let row_bg = if is_active {
            theme::accent_bg()
        } else {
            hsla(0.0, 0.0, 1.0, 0.0)
        };

        session_rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .rounded_md()
                .border_1()
                .border_color(theme::border())
                .bg(row_bg)
                .px(px(10.))
                .py(px(8.))
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
                        .child(div().w(px(10.)).h(px(10.)).rounded_full().bg(dot_color))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme::text())
                                        .child(session.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme::muted_text())
                                        .child(SharedString::from(session.provider.name.clone())),
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
                                .text_color(theme::subtle_text())
                                .child(session.status_label()),
                        )
                        .child(
                            div()
                                .px(px(8.))
                                .py(px(4.))
                                .rounded_md()
                                .border_1()
                                .border_color(theme::border())
                                .bg(hsla(0.0, 0.0, 1.0, 0.0))
                                .text_xs()
                                .text_color(theme::muted_text())
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, _, cx| {
                                        cx.stop_propagation();
                                        view.delete_session(id, cx);
                                    }),
                                )
                                .child("Del"),
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
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Sessions"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::subtle_text())
                        .child(format!("{}", view.sessions().len())),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.new_session(cx)),
                        )
                        .child("+ New"),
                )
                .child(
                    div()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(theme::muted_text())
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.cycle_new_session_provider(cx)),
                        )
                        .child(view.new_session_provider_label()),
                ),
        )
        .child(div().flex().flex_col().gap_2().children(session_rows))
}

fn status_color(status: &SessionStatus) -> gpui::Hsla {
    match status {
        SessionStatus::Idle => hsla(0.0, 0.0, 0.0, 0.18),
        SessionStatus::Thinking => theme::accent(),
        SessionStatus::Executing { .. } => hsla(0.12, 0.85, 0.56, 0.9),
        SessionStatus::WaitingInput { .. } => hsla(0.1, 0.9, 0.6, 0.9),
        SessionStatus::Error { .. } => theme::danger(),
    }
}
