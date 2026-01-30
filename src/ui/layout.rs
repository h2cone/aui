use gpui::{Context, IntoElement, Window, div, prelude::*, px};

use crate::app::AuiApp;
use crate::providers::SessionStatus;
use crate::ui::{conversation, input_box, settings_panel, sidebar, theme, top_bar};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMetrics {
    pub top_bar_height: f32,
    pub settings_height: f32,
    pub sidebar_width: f32,
    pub content_padding: f32,
}

impl LayoutMetrics {
    pub fn for_viewport(width_px: f32, height_px: f32) -> Self {
        let _ = height_px;
        Self {
            top_bar_height: 44.0,
            settings_height: 280.0,
            sidebar_width: (width_px * 0.26).clamp(220.0, 300.0),
            content_padding: (width_px.min(height_px) * 0.02).clamp(10.0, 16.0),
        }
    }
}

pub fn render_app(
    view: &mut AuiApp,
    window: &mut Window,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let viewport = window.viewport_size();
    let viewport_w = viewport.width.to_f64() as f32;
    let viewport_h = viewport.height.to_f64() as f32;
    let metrics = LayoutMetrics::for_viewport(viewport_w, viewport_h);

    let active_session = view.active_session();
    let settings_open = view.settings_open();
    let error_banner = active_session.and_then(|session| match &session.status {
        SessionStatus::Error { message } => Some(
            div()
                .rounded_md()
                .border_1()
                .border_color(theme::danger_bg())
                .bg(theme::danger_bg())
                .px(px(12.))
                .py(px(10.))
                .text_sm()
                .text_color(theme::text())
                .child(message.clone())
                .into_any_element(),
        ),
        _ => None,
    });

    div()
        .size_full()
        .font_family("Segoe UI")
        .bg(theme::app_bg())
        .on_action(cx.listener(AuiApp::submit))
        .on_action(cx.listener(AuiApp::attach_files))
        .on_action(cx.listener(AuiApp::export_session))
        .on_action(cx.listener(AuiApp::clear_attachments_action))
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(theme::surface())
                .border_1()
                .border_color(theme::border())
                .child(
                    div()
                        .h(px(metrics.top_bar_height))
                        .child(top_bar::render_top_bar(view, active_session, cx)),
                )
                .child(div().h(px(1.)).bg(theme::border()))
                .child({
                    if settings_open {
                        div()
                            .h(px(metrics.settings_height))
                            .id("settings-scroll")
                            .overflow_y_scroll()
                            .child(settings_panel::render_settings_panel(
                                view,
                                active_session,
                                cx,
                            ))
                            .into_any_element()
                    } else {
                        div().h(px(0.)).into_any_element()
                    }
                })
                .child({
                    if settings_open {
                        div().h(px(1.)).bg(theme::border()).into_any_element()
                    } else {
                        div().h(px(0.)).into_any_element()
                    }
                })
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .child(
                            div()
                                .w(px(metrics.sidebar_width))
                                .bg(theme::surface_2())
                                .p(px(metrics.content_padding))
                                .child(sidebar::render_sidebar(view, cx)),
                        )
                        .child(div().w(px(1.)).bg(theme::border()))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .bg(theme::surface())
                                .child({
                                    if let Some(banner) = error_banner {
                                        div()
                                            .p(px(metrics.content_padding))
                                            .child(banner)
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.)).into_any_element()
                                    }
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .id("conversation-scroll")
                                        .overflow_y_scroll()
                                        .track_scroll(view.conversation_scroll_handle())
                                        .p(px(metrics.content_padding))
                                        .child(conversation::render_conversation(
                                            view,
                                            active_session,
                                            cx,
                                        )),
                                )
                                .child(div().h(px(1.)).bg(theme::border()))
                                .child(
                                    div()
                                        .bg(theme::surface_2())
                                        .p(px(metrics.content_padding))
                                        .child(input_box::render_input_box(&view.text_input, cx)),
                                ),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_metrics_clamps_sidebar_width() {
        let narrow = LayoutMetrics::for_viewport(600.0, 700.0);
        assert!(narrow.sidebar_width >= 220.0);

        let wide = LayoutMetrics::for_viewport(2400.0, 900.0);
        assert!(wide.sidebar_width <= 300.0);
    }
}
