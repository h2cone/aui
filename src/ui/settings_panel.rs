use gpui::{
    Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div, hsla,
    prelude::*, px,
};

use crate::app::AuiApp;
use crate::providers::ProviderKind;
use crate::session::Session;
use crate::ui::text;
use crate::ui::theme;

pub fn render_settings_panel(
    view: &AuiApp,
    active_session: Option<&Session>,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let Some(session) = active_session else {
        return div()
            .w_full()
            .h_full()
            .p(px(12.))
            .text_sm()
            .text_color(theme::muted_text())
            .child("Select a session to configure provider and model.");
    };

    let session_id = session.id;
    let kind = session.provider.kind;
    let refreshing = view.model_refreshing(kind);
    let updated_at = view.model_updated_at(kind);
    let error = view.model_refresh_error(kind);
    let options = crate::app::model_options_for(view.model_catalog(), kind);
    let show_all = view.settings_show_all_models();
    let list_limit = 10usize;
    let show_toggle = options.len() > list_limit;

    let refresh_label = if refreshing {
        "Refreshing…".to_string()
    } else {
        "↻ Refresh".to_string()
    };

    let meta = updated_at
        .map(|ts| format!("Updated {}", format_age(ts)))
        .unwrap_or_else(|| "Using built-in list".to_string());

    let chip = |label: SharedString| {
        div()
            .px(px(10.))
            .py(px(4.))
            .rounded_full()
            .border_1()
            .border_color(theme::border())
            .bg(hsla(0.0, 0.0, 1.0, 0.0))
            .text_xs()
            .text_color(theme::text())
            .child(label)
    };

    let subheader = |label: &'static str| {
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::subtle_text())
            .child(label)
    };

    let refresh_button = {
        let base = chip(SharedString::from(refresh_label))
            .cursor(CursorStyle::PointingHand)
            .text_color(if refreshing {
                theme::subtle_text()
            } else {
                theme::text()
            });
        if refreshing {
            base
        } else {
            base.on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.refresh_active_model_catalog(cx)),
            )
        }
    };

    let model_rows = render_model_rows(session_id, kind, &options, show_all, list_limit, cx);

    div()
        .w_full()
        .h_full()
        .p(px(12.))
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
                        .child("Settings"),
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted_text())
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.close_settings(cx)),
                        )
                        .child("Close"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(subheader("Provider"))
                .child(
                    chip(SharedString::from(session.provider.name.clone()))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _, _, cx| {
                                view.cycle_session_provider(session_id, cx)
                            }),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(subheader("Model"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_xs().text_color(theme::muted_text()).child(meta))
                        .child(refresh_button),
                ),
        )
        .child({
            if let Some(err) = error {
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::danger_bg())
                    .bg(theme::danger_bg())
                    .p(px(10.))
                    .text_xs()
                    .text_color(theme::muted_text())
                    .child(err)
                    .into_any_element()
            } else {
                div().h(px(0.)).into_any_element()
            }
        })
        .child(div().flex().flex_col().gap_2().children(model_rows))
        .child({
            if show_toggle {
                let label = if show_all { "Show top 10" } else { "Show all" };
                chip(SharedString::from(label))
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| view.toggle_settings_show_all_models(cx)),
                    )
                    .into_any_element()
            } else {
                div().h(px(0.)).into_any_element()
            }
        })
}

fn render_model_rows(
    session_id: crate::session::SessionId,
    kind: ProviderKind,
    options: &[SharedString],
    show_all: bool,
    list_limit: usize,
    cx: &mut Context<AuiApp>,
) -> Vec<gpui::AnyElement> {
    let mut out = Vec::new();
    let limit = if show_all {
        options.len()
    } else {
        options.len().min(list_limit)
    };
    for model in options.iter().take(limit) {
        let label = text::ellipsize(model.as_ref(), 56);
        let model_for_click = model.clone();
        out.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .rounded_md()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_2())
                .px(px(10.))
                .py(px(8.))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text())
                        .font_family("Cascadia Code")
                        .child(label),
                )
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(theme::border())
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(theme::muted_text())
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _, _, cx| {
                                view.set_session_model(session_id, model_for_click.as_ref(), cx);
                            }),
                        )
                        .child("Use"),
                )
                .into_any_element(),
        );
    }
    if out.is_empty() {
        out.push(
            div()
                .text_sm()
                .text_color(theme::muted_text())
                .child(format!("No models configured for {}.", kind.key()))
                .into_any_element(),
        );
    }
    out
}

fn format_age(epoch_secs: u64) -> String {
    let now = crate::model_catalog::now_epoch_secs();
    let delta = now.saturating_sub(epoch_secs);
    let minutes = delta / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    format!("{days}d ago")
}
