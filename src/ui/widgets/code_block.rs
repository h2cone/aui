use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb};

pub fn code_block(language: SharedString, code: SharedString) -> impl IntoElement {
    div()
        .rounded_lg()
        .bg(hsla(0.55, 0.3, 0.97, 0.4))
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x475569))
                .child(language),
        )
        .child(
            div()
                .text_sm()
                .font_family("Cascadia Mono")
                .text_color(rgb(0x0f172a))
                .child(code),
        )
}
