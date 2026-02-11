use gpui::{
    ClipboardItem, Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div,
    hsla, prelude::*, px,
};

use crate::app::{AuiApp, ShellKey};
use crate::ui::theme;

pub fn shell_output(
    title: SharedString,
    output: SharedString,
    key: ShellKey,
    collapsed: bool,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let toggle_key = key;
    let label = if collapsed { "Show" } else { "Hide" };
    let copy_payload = output.clone();

    let body = if collapsed {
        div()
            .text_sm()
            .text_color(theme::muted_text())
            .child("Output hidden.")
            .into_any_element()
    } else {
        div()
            .font_family("Cascadia Mono")
            .text_size(px(13.))
            .text_color(theme::text())
            .child(output)
            .into_any_element()
    };

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
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted_text())
                        .child(title),
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
                                .border_color(theme::border())
                                .bg(hsla(0.0, 0.0, 1.0, 0.0))
                                .text_xs()
                                .text_color(theme::muted_text())
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_payload.to_string(),
                                        ));
                                    }),
                                )
                                .child("Copy"),
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
                                        view.toggle_shell(toggle_key, cx);
                                    }),
                                )
                                .child(label),
                        ),
                ),
        )
        .child(body)
}
