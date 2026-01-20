use gpui::{
    Entity, FontWeight, IntoElement, SharedString, div, hsla, pattern_slash, prelude::*, px, rgb,
    white,
};

use crate::agent::Attachment;
use crate::text_input::TextInput;

pub fn render_input_box(
    text_input: &Entity<TextInput>,
    attachments: &[Attachment],
) -> impl IntoElement {
    let mut attachment_items = Vec::new();
    for attachment in attachments {
        attachment_items.push(
            div()
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .bg(hsla(0.48, 0.3, 0.92, 0.3))
                .text_xs()
                .text_color(rgb(0x0f172a))
                .child(SharedString::from(attachment.name.clone()))
                .into_any_element(),
        );
    }

    let attachment_row = if attachment_items.is_empty() {
        div()
            .text_xs()
            .text_color(rgb(0x64748b))
            .child("Drag files here or click to attach")
            .into_any_element()
    } else {
        div()
            .flex()
            .gap_2()
            .children(attachment_items)
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(12.))
                .py(px(8.))
                .rounded_lg()
                .bg(white())
                .border_1()
                .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x0f172a))
                        .child("Attachments"),
                )
                .child(attachment_row),
        )
        .child(
            div()
                .bg(pattern_slash(hsla(0.52, 0.4, 0.5, 0.12), 6.0, 6.0))
                .rounded_xl()
                .p(px(2.))
                .child(text_input.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_xs()
                .text_color(rgb(0x5b6777))
                .child("Shift+Enter for newline")
                .child("Cmd/Ctrl+Enter to send"),
        )
}
