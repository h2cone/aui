use gpui::{
    Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div, hsla,
    prelude::*, px, rgb,
};

use crate::app::{AuiApp, DiffDecision, DiffKey};
use crate::ui::theme;

pub fn diff_view(
    title: SharedString,
    diff: SharedString,
    decision: Option<DiffDecision>,
    key: DiffKey,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let status = match decision {
        Some(DiffDecision::Accepted) => ("Accepted", hsla(0.37, 0.6, 0.5, 0.25)),
        Some(DiffDecision::Rejected) => ("Rejected", hsla(0.0, 0.7, 0.55, 0.2)),
        None => ("Review", hsla(0.55, 0.4, 0.9, 0.2)),
    };
    let accept_style = button_style(decision == Some(DiffDecision::Accepted), true);
    let reject_style = button_style(decision == Some(DiffDecision::Rejected), false);
    let accept_key = key;
    let reject_key = key;
    let lines = render_diff_lines(diff.as_ref());

    div()
        .rounded_lg()
        .bg(theme::surface_3())
        .border_1()
        .border_color(theme::border())
        .px(px(16.))
        .py(px(12.))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
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
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::muted_text())
                                .child(title),
                        )
                        .child(
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded_full()
                                .bg(status.1)
                                .text_xs()
                                .text_color(theme::text())
                                .child(status.0),
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
                                .py(px(4.))
                                .rounded_full()
                                .border_1()
                                .border_color(accept_style.border)
                                .bg(accept_style.bg)
                                .text_xs()
                                .text_color(accept_style.text)
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, _, cx| {
                                        view.set_diff_decision(
                                            accept_key,
                                            DiffDecision::Accepted,
                                            cx,
                                        );
                                    }),
                                )
                                .child("Accept"),
                        )
                        .child(
                            div()
                                .px(px(10.))
                                .py(px(4.))
                                .rounded_full()
                                .border_1()
                                .border_color(reject_style.border)
                                .bg(reject_style.bg)
                                .text_xs()
                                .text_color(reject_style.text)
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, _, cx| {
                                        view.set_diff_decision(
                                            reject_key,
                                            DiffDecision::Rejected,
                                            cx,
                                        );
                                    }),
                                )
                                .child("Reject"),
                        ),
                ),
        )
        .child(div().flex().flex_col().gap_1().children(lines))
}

struct ButtonStyle {
    bg: gpui::Hsla,
    border: gpui::Hsla,
    text: gpui::Rgba,
}

fn button_style(active: bool, positive: bool) -> ButtonStyle {
    if active && positive {
        return ButtonStyle {
            bg: hsla(0.37, 0.6, 0.55, 0.25),
            border: hsla(0.37, 0.6, 0.55, 0.45),
            text: rgb(0x065f46),
        };
    }
    if active && !positive {
        return ButtonStyle {
            bg: hsla(0.0, 0.7, 0.55, 0.22),
            border: hsla(0.0, 0.7, 0.55, 0.45),
            text: rgb(0x7f1d1d),
        };
    }
    ButtonStyle {
        bg: hsla(0.0, 0.0, 1.0, 0.0),
        border: theme::border(),
        text: theme::muted_text(),
    }
}

fn render_diff_lines(diff: &str) -> Vec<gpui::AnyElement> {
    let mut lines = Vec::new();
    for line in diff.lines() {
        let style = diff_line_style(line);
        lines.push(
            div()
                .px(px(10.))
                .py(px(2.))
                .rounded_lg()
                .bg(style.bg)
                .text_sm()
                .font_family("Cascadia Mono")
                .text_color(style.text)
                .child(line.to_string())
                .into_any_element(),
        );
    }
    if lines.is_empty() {
        lines.push(
            div()
                .text_sm()
                .text_color(theme::muted_text())
                .child("No diff content.")
                .into_any_element(),
        );
    }
    lines
}

struct DiffLineStyle {
    bg: gpui::Hsla,
    text: gpui::Rgba,
}

fn diff_line_style(line: &str) -> DiffLineStyle {
    if line.starts_with("+++ ") || line.starts_with("--- ") || line.starts_with("diff ") {
        return DiffLineStyle {
            bg: hsla(0.0, 0.0, 1.0, 0.0),
            text: theme::subtle_text(),
        };
    }
    if line.starts_with("@@") {
        return DiffLineStyle {
            bg: hsla(0.55, 0.4, 0.9, 0.2),
            text: rgb(0x1d4ed8),
        };
    }
    if line.starts_with('+') {
        return DiffLineStyle {
            bg: hsla(0.37, 0.6, 0.55, 0.18),
            text: rgb(0x166534),
        };
    }
    if line.starts_with('-') {
        return DiffLineStyle {
            bg: hsla(0.0, 0.7, 0.55, 0.16),
            text: rgb(0x991b1b),
        };
    }
    DiffLineStyle {
        bg: hsla(0.0, 0.0, 1.0, 0.0),
        text: theme::text(),
    }
}
