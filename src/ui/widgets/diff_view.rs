use gpui::{FontWeight, Render, SharedString, Window, div, hsla, prelude::*, px, rgb};

pub struct DiffView {
    pub title: SharedString,
    pub diff: SharedString,
}

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .font_family("Cascadia Mono")
                    .text_color(rgb(0x0f172a))
                    .child(self.diff.clone()),
            )
    }
}
