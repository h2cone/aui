use gpui::{
    Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div, hsla,
    prelude::*, px,
};

use crate::app::{AuiApp, SettingsSection};
use crate::session::Session;
use crate::ui::text;
use crate::ui::theme;
use aui_ai::ProviderKind;

pub fn render_settings_panel(
    view: &AuiApp,
    active_session: Option<&Session>,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let section = view.settings_section();

    div()
        .w_full()
        .h_full()
        .rounded_xl()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::surface())
        .p(px(16.))
        .flex()
        .flex_col()
        .gap_3()
        .child(render_header(section, cx))
        .child(div().h(px(1.)).bg(theme::border()))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .gap_3()
                .child(render_nav(view, cx))
                .child(render_content(view, active_session, section, cx)),
        )
}

fn render_header(section: SettingsSection, cx: &mut Context<AuiApp>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::subtle_text())
                        .child(format!("{}", section.title())),
                ),
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
        )
}

fn render_nav(view: &AuiApp, cx: &mut Context<AuiApp>) -> impl IntoElement {
    let active = view.settings_section();

    div()
        .w(px(180.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_3())
        .p(px(8.))
        .flex()
        .flex_col()
        .gap_1()
        .children([
            nav_item(
                active,
                SettingsSection::General,
                "General",
                "Defaults and app behavior",
                cx,
            ),
            nav_item(
                active,
                SettingsSection::Models,
                "Models",
                "Provider and model selection",
                cx,
            ),
            nav_item(
                active,
                SettingsSection::Keybindings,
                "Keybindings",
                "Keyboard shortcuts",
                cx,
            ),
        ])
}

fn nav_item(
    active: SettingsSection,
    section: SettingsSection,
    label: &'static str,
    subtitle: &'static str,
    cx: &mut Context<AuiApp>,
) -> gpui::AnyElement {
    let is_active = active == section;
    div()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .rounded_md()
        .border_1()
        .border_color(if is_active {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if is_active {
            theme::accent_bg()
        } else {
            hsla(0.0, 0.0, 1.0, 0.0)
        })
        .px(px(10.))
        .py(px(8.))
        .cursor(CursorStyle::PointingHand)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| view.set_settings_section(section, cx)),
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
                        .text_color(theme::text())
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::subtle_text())
                        .child(subtitle),
                ),
        )
        .into_any_element()
}

fn render_content(
    view: &AuiApp,
    active_session: Option<&Session>,
    section: SettingsSection,
    cx: &mut Context<AuiApp>,
) -> gpui::AnyElement {
    match section {
        SettingsSection::General => render_general_section(view, active_session).into_any_element(),
        SettingsSection::Models => {
            render_models_section(view, active_session, cx).into_any_element()
        }
        SettingsSection::Keybindings => render_keybindings_section().into_any_element(),
    }
}

fn render_general_section(view: &AuiApp, active_session: Option<&Session>) -> impl IntoElement {
    let active_title = active_session
        .map(|session| session.title.as_ref().to_string())
        .unwrap_or_else(|| "No active session".to_string());
    let active_provider = active_session
        .map(|session| session.provider.name.as_ref().to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let default_provider = view.new_session_provider_label().to_string();

    div()
        .flex_1()
        .min_h(px(0.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_2())
        .p(px(12.))
        .flex()
        .flex_col()
        .gap_3()
        .child(info_card("Active Session", &active_title))
        .child(info_card("Active Provider", &active_provider))
        .child(info_card("Default New Session Provider", &default_provider))
        .child(
            div()
                .text_xs()
                .text_color(theme::subtle_text())
                .child("Tip: open Settings with Cmd/Ctrl+, and close with Esc."),
        )
}

fn info_card(label: &'static str, value: &str) -> gpui::AnyElement {
    div()
        .rounded_md()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_3())
        .p(px(10.))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::subtle_text())
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme::text())
                .child(value.to_string()),
        )
        .into_any_element()
}

fn render_models_section(
    view: &AuiApp,
    active_session: Option<&Session>,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
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

    let Some(session) = active_session else {
        return div()
            .flex_1()
            .min_h(px(0.))
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface_2())
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
    let list_limit = 12usize;
    let show_toggle = options.len() > list_limit;

    let refresh_label = if refreshing {
        SharedString::from("Refreshing...")
    } else {
        SharedString::from("Refresh")
    };

    let meta = updated_at
        .map(|ts| format!("Updated {}", format_age(ts)))
        .unwrap_or_else(|| "Using built-in list".to_string());

    let refresh_button = {
        let base = chip(refresh_label)
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

    let model_rows = render_model_rows(
        session_id,
        session.model.as_ref(),
        kind,
        &options,
        show_all,
        list_limit,
        cx,
    );

    div()
        .flex_1()
        .min_h(px(0.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_2())
        .p(px(12.))
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_3())
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::subtle_text())
                                .child("Provider"),
                        )
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
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::subtle_text())
                                .child("Model"),
                        )
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
                }),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .child(
                    div()
                        .h_full()
                        .id("settings-model-list")
                        .on_scroll_wheel(cx.listener(|_, _: &gpui::ScrollWheelEvent, _, cx| {
                            cx.stop_propagation();
                        }))
                        .overflow_y_scroll()
                        .p(px(10.))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(model_rows),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::subtle_text())
                        .child("Tip: Press Esc or click outside to close."),
                )
                .child({
                    if show_toggle {
                        let label = if show_all { "Show top 12" } else { "Show all" };
                        chip(SharedString::from(label))
                            .cursor(CursorStyle::PointingHand)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, _, _, cx| {
                                    view.toggle_settings_show_all_models(cx)
                                }),
                            )
                            .into_any_element()
                    } else {
                        div().h(px(0.)).into_any_element()
                    }
                }),
        )
}

fn render_keybindings_section() -> impl IntoElement {
    div()
        .flex_1()
        .min_h(px(0.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_2())
        .p(px(12.))
        .flex()
        .flex_col()
        .gap_2()
        .children([
            keybinding_row("Open Settings", "Cmd/Ctrl + ,"),
            keybinding_row("Close Settings", "Esc"),
            keybinding_row("Clear Attachments", "Shift + Esc"),
            keybinding_row("Submit Message", "Enter / Cmd/Ctrl + Enter"),
            keybinding_row("Attach Files", "Cmd/Ctrl + O"),
            keybinding_row("Export Session", "Cmd/Ctrl + S"),
        ])
}

fn keybinding_row(action: &'static str, shortcut: &'static str) -> gpui::AnyElement {
    div()
        .rounded_md()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_3())
        .px(px(10.))
        .py(px(8.))
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_sm().text_color(theme::text()).child(action))
        .child(
            div()
                .px(px(8.))
                .py(px(3.))
                .rounded_full()
                .border_1()
                .border_color(theme::border())
                .text_xs()
                .text_color(theme::muted_text())
                .child(shortcut),
        )
        .into_any_element()
}

fn render_model_rows(
    session_id: crate::session::SessionId,
    current_model: &str,
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
        let label = text::ellipsize(model.as_ref(), 72);
        let model_for_click = model.clone();
        let is_current = model.as_ref() == current_model;

        out.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .rounded_md()
                .border_1()
                .border_color(if is_current {
                    theme::accent()
                } else {
                    theme::border()
                })
                .bg(if is_current {
                    theme::accent_bg()
                } else {
                    theme::surface_3().into()
                })
                .px(px(10.))
                .py(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme::text())
                                .font_family("Cascadia Code")
                                .child(label),
                        )
                        .child({
                            if is_current {
                                div()
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded_full()
                                    .bg(theme::accent_bg())
                                    .text_xs()
                                    .text_color(theme::muted_text())
                                    .child("Current")
                                    .into_any_element()
                            } else {
                                div().h(px(0.)).into_any_element()
                            }
                        }),
                )
                .child({
                    if is_current {
                        div()
                            .px(px(10.))
                            .py(px(4.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::border())
                            .bg(hsla(0.0, 0.0, 1.0, 0.0))
                            .text_xs()
                            .text_color(theme::subtle_text())
                            .child("Using")
                            .into_any_element()
                    } else {
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
                                    view.set_session_model(
                                        session_id,
                                        model_for_click.as_ref(),
                                        cx,
                                    );
                                }),
                            )
                            .child("Use")
                            .into_any_element()
                    }
                })
                .into_any_element(),
        );
    }

    if out.is_empty() {
        out.push(
            div()
                .rounded_md()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_3())
                .p(px(12.))
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
