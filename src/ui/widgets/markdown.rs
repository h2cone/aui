use gpui::{FontWeight, Render, SharedString, Window, div, hsla, prelude::*, px, rgb, white};

pub struct MarkdownBlock {
    pub content: SharedString,
}

impl Render for MarkdownBlock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x0f172a))
                    .child(self.content.clone()),
            )
    }
}
