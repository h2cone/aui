use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb};

pub fn diff_view(title: SharedString, diff: SharedString) -> impl IntoElement {
    div()
        .rounded_lg()
        .bg(hsla(0.0, 0.0, 1.0, 0.9))
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x475569))
                .child(title),
        )
        .child(
            div()
                .text_sm()
                .font_family("Cascadia Mono")
                .text_color(rgb(0x0f172a))
                .child(diff),
        )
}
