use gpui::{FontWeight, Render, SharedString, Window, div, hsla, prelude::*, px, rgb};

pub struct CodeBlock {
    pub language: SharedString,
    pub code: SharedString,
}

impl Render for CodeBlock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(self.language.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .font_family("Cascadia Mono")
                    .text_color(rgb(0x0f172a))
                    .child(self.code.clone()),
            )
    }
}
