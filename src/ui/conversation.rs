use gpui::{
    BoxShadow, FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb, white,
};

use crate::session::{Session, SessionRole};

pub fn render_conversation(session: Option<&Session>) -> impl IntoElement {
    let mut message_items = Vec::new();

    if let Some(session) = session {
        for message in &session.messages {
            let (label, bubble_bg, text_color) = style_for_role(&message.role);
            message_items.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_xl()
                    .bg(bubble_bg)
                    .px(px(16.))
                    .py(px(12.))
                    .shadow(vec![BoxShadow {
                        color: hsla(0.0, 0.0, 0.0, 0.08),
                        offset: gpui::point(px(0.), px(10.)),
                        blur_radius: px(24.),
                        spread_radius: px(-12.),
                    }])
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(text_color)
                            .child(label),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x0f172a))
                            .child(SharedString::from(message.content.clone())),
                    )
                    .into_any_element(),
            );
        }
    }

    if message_items.is_empty() {
        message_items.push(
            div()
                .rounded_xl()
                .bg(white())
                .border_1()
                .border_color(hsla(0.0, 0.0, 0.0, 0.06))
                .px(px(18.))
                .py(px(16.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x5b6777))
                        .child("Start a session to see the conversation."),
                )
                .into_any_element(),
        );
    }

    div().flex().flex_col().gap_3().children(message_items)
}

fn style_for_role(role: &SessionRole) -> (&'static str, gpui::Hsla, gpui::Rgba) {
    match role {
        SessionRole::User => ("You", hsla(0.55, 0.4, 0.95, 0.32), rgb(0x0b1220)),
        SessionRole::Assistant => ("Agent", hsla(0.14, 0.6, 0.92, 0.32), rgb(0x0f172a)),
        SessionRole::Tool => ("Tool", hsla(0.0, 0.0, 0.9, 0.28), rgb(0x1f2937)),
    }
}
