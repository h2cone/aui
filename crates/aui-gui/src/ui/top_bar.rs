use gpui::{
    Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div, hsla,
    prelude::*, px,
};

use crate::app::AuiApp;
use crate::session::Session;
use crate::ui::text;
use crate::ui::theme;

pub fn render_top_bar(
    view: &AuiApp,
    active_session: Option<&Session>,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let active_id = active_session.map(|session| session.id);
    let title = active_session
        .and_then(|session| session.title.as_ref().strip_prefix("session-"))
        .map(|rest| format!("session-{rest}"))
        .unwrap_or_else(|| "No session".to_string());

    let provider_label = active_session
        .map(|session| SharedString::from(session.provider.name.clone()))
        .unwrap_or_else(|| SharedString::from("Provider"));
    let model_label = active_session
        .map(|session| SharedString::from(text::ellipsize(session.model.as_ref(), 42)))
        .unwrap_or_else(|| SharedString::from("Model"));
    let model_refreshing = active_session
        .map(|session| view.model_refreshing(session.provider.kind))
        .unwrap_or(false);
    let refresh_label = if model_refreshing {
        SharedString::from("Refreshing...")
    } else {
        SharedString::from("Refresh")
    };

    let settings_bg = if view.settings_open() {
        theme::accent_bg()
    } else {
        hsla(0.0, 0.0, 1.0, 0.0)
    };

    let chip = |label: SharedString| {
        div()
            .px(px(10.))
            .py(px(4.))
            .rounded_full()
            .border_1()
            .border_color(theme::border())
            .bg(hsla(0.0, 0.0, 1.0, 0.0))
            .text_xs()
            .text_color(theme::muted_text())
            .child(label)
    };

    let button = |label: &'static str| {
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
            .child(label)
    };

    div()
        .h_full()
        .w_full()
        .px(px(12.))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("aui"),
                )
                .child(div().text_xs().text_color(theme::subtle_text()).child("·"))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::muted_text())
                        .child(SharedString::from(text::ellipsize(&title, 42))),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child({
                    if let Some(id) = active_id {
                        chip(provider_label.clone())
                            .cursor(CursorStyle::PointingHand)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _, _, cx| {
                                    view.cycle_session_provider(id, cx)
                                }),
                            )
                            .into_any_element()
                    } else {
                        chip(provider_label.clone()).into_any_element()
                    }
                })
                .child({
                    if let Some(id) = active_id {
                        chip(model_label.clone())
                            .cursor(CursorStyle::PointingHand)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _, _, cx| view.cycle_session_model(id, cx)),
                            )
                            .into_any_element()
                    } else {
                        chip(model_label.clone()).into_any_element()
                    }
                })
                .child({
                    let style = chip(refresh_label.clone());
                    if model_refreshing {
                        style
                            .bg(theme::accent_bg())
                            .text_color(theme::text())
                            .into_any_element()
                    } else if active_id.is_some() {
                        style
                            .cursor(CursorStyle::PointingHand)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, _, _, cx| view.refresh_active_model_catalog(cx)),
                            )
                            .into_any_element()
                    } else {
                        style.text_color(theme::subtle_text()).into_any_element()
                    }
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(button("+ New").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| view.new_session(cx)),
                ))
                .child(button("Attach").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, window, cx| view.open_attachment_picker(window, cx)),
                ))
                .child({
                    let style = button("Export");
                    if active_id.is_some() {
                        style.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.export_active_session(window, cx)
                            }),
                        )
                    } else {
                        style.text_color(theme::subtle_text())
                    }
                })
                .child(
                    div()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .bg(settings_bg)
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.open_settings(cx)),
                        )
                        .child("Settings"),
                ),
        )
}
