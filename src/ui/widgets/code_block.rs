use gpui::{
    ClipboardItem, Context, CursorStyle, FontWeight, IntoElement, MouseButton, SharedString, div,
    hsla, prelude::*, px, rgb,
};

use crate::app::AuiApp;
use crate::ui::widgets::highlighted_code::HighlightedCode;

pub fn code_block(
    language: SharedString,
    code: SharedString,
    cx: &mut Context<AuiApp>,
) -> impl IntoElement {
    let label = language.clone();
    let code_copy = code.clone();
    div()
        .rounded_lg()
        .bg(hsla(0.55, 0.3, 0.97, 0.4))
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x475569))
                .child(label)
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.12))
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(rgb(0x475569))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    code_copy.to_string(),
                                ));
                            }),
                        )
                        .child("Copy"),
                ),
        )
        .child(
            div()
                .font_family("Cascadia Mono")
                .text_size(px(14.))
                .text_color(rgb(0x0f172a))
                .child(HighlightedCode {
                    text: code,
                    language,
                }),
        )
}
