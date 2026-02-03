use gpui::{Context, CursorStyle, IntoElement, MouseButton, Window, div, hsla, prelude::*, px};

use crate::app::AuiApp;
use crate::providers::SessionStatus;
use crate::ui::{conversation, input_box, scrollbar, settings_panel, sidebar, theme, top_bar};

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

    let conversation_scroll = view.conversation_scroll_handle();
    let conversation_scroll_thumb = scrollbar::compute_thumb(
        conversation_scroll.bounds().size.height.to_f64() as f32,
        conversation_scroll.max_offset().height.to_f64() as f32,
        conversation_scroll.offset().y.to_f64() as f32,
    );
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
                        .min_h(px(0.))
                        .min_w(px(0.))
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
                                .min_h(px(0.))
                                .min_w(px(0.))
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
                                        .min_h(px(0.))
                                        .min_w(px(0.))
                                        .relative()
                                        .child(
                                            div()
                                                .id("conversation-scroll")
                                                .debug_selector(|| {
                                                    "conversation-scroll".to_string()
                                                })
                                                .size_full()
                                                .overflow_y_scroll()
                                                .overflow_x_hidden()
                                                .track_scroll(view.conversation_scroll_handle())
                                                .p(px(metrics.content_padding))
                                                .pr(px(metrics.content_padding + 16.0))
                                                .flex()
                                                .flex_col()
                                                .gap_3()
                                                .children(conversation::render_conversation(
                                                    view,
                                                    active_session,
                                                    cx,
                                                )),
                                        )
                                        .child(render_conversation_scrollbar(
                                            conversation_scroll_thumb,
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

fn render_conversation_scrollbar(
    thumb: scrollbar::ScrollbarThumb,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let track_bg = theme::surface_2();
    let track_border = theme::border_strong();
    let thumb_bg = hsla(0.0, 0.0, 0.0, 0.28);
    let thumb_bg_hover = hsla(0.0, 0.0, 0.0, 0.40);

    div()
        .id("conversation-scrollbar")
        .debug_selector(|| "conversation-scrollbar".to_string())
        .absolute()
        .top(px(0.))
        .right(px(0.))
        .bottom(px(0.))
        .w(px(12.))
        .flex_shrink_0()
        .min_h(px(0.))
        .p(px(2.))
        .cursor(CursorStyle::PointingHand)
        .on_scroll_wheel(cx.listener(AuiApp::conversation_scrollbar_scroll_wheel))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(AuiApp::conversation_scrollbar_mouse_down),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(AuiApp::conversation_scrollbar_mouse_up),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(AuiApp::conversation_scrollbar_mouse_up),
        )
        .on_mouse_move(cx.listener(AuiApp::conversation_scrollbar_mouse_move))
        .child(
            div()
                .size_full()
                .rounded_md()
                .bg(track_bg)
                .border_1()
                .border_color(track_border)
                .child(if thumb.show_thumb {
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(div().h(px(thumb.thumb_top_px)))
                        .child(
                            div()
                                .h(px(thumb.thumb_height_px))
                                .rounded_md()
                                .bg(thumb_bg)
                                .hover(|style| style.bg(thumb_bg_hover)),
                        )
                        .child(div().flex_1())
                        .into_any_element()
                } else {
                    div().h(px(0.)).into_any_element()
                }),
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
