use gpui::{Context, Entity, IntoElement, div, prelude::*};

use crate::app::AuiApp;
use crate::text_input::TextInput;
use crate::ui::theme;

pub fn render_input_box(
    text_input: &Entity<TextInput>,
    _cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(text_input.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_xs()
                .text_color(theme::subtle_text())
                .child("Shift+Enter for newline")
                .child("Cmd/Ctrl+Enter to send"),
        )
}
