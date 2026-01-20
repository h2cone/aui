use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb, white};

pub fn markdown_block(content: SharedString) -> impl IntoElement {
    div()
        .rounded_lg()
        .bg(white())
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.06))
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x64748b))
                .child("Markdown"),
        )
        .child(div().text_sm().text_color(rgb(0x0f172a)).child(content))
}
