use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px};

use aui_agent_core::{MessageBlock, parse_blocks};

use crate::app::{AuiApp, DiffKey};
use crate::session::{Session, SessionRole};
use crate::ui::theme;
use crate::ui::widgets::code_block::code_block;
use crate::ui::widgets::diff_view::diff_view;
use crate::ui::widgets::markdown::markdown_block;
use crate::ui::widgets::shell_output::shell_output;

pub fn render_conversation(
    view: &AuiApp,
    session: Option<&Session>,
    cx: &mut gpui::Context<AuiApp>,
) -> Vec<gpui::AnyElement> {
    let mut message_items = Vec::new();

    if let Some(session) = session {
        for (message_index, message) in session.messages.iter().enumerate() {
            let (label, bubble_bg, text_color) = style_for_role(&message.role);
            let mut body_children = Vec::new();

            let blocks = parse_blocks(message.content.as_str());
            for (block_index, block) in blocks.into_iter().enumerate() {
                body_children.push(render_block(
                    view,
                    session.id,
                    message_index,
                    block_index,
                    block,
                    cx,
                ));
            }

            if body_children.is_empty() {
                body_children.push(
                    div()
                        .text_sm()
                        .text_color(theme::muted_text())
                        .child("...")
                        .into_any_element(),
                );
            }

            message_items.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_xl()
                    .bg(bubble_bg)
                    .border_1()
                    .border_color(theme::border())
                    .px(px(14.))
                    .py(px(10.))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(text_color)
                            .child(label),
                    )
                    .child(div().flex().flex_col().gap_2().children(body_children))
                    .into_any_element(),
            );
        }
    }

    if message_items.is_empty() {
        message_items.push(
            div()
                .rounded_xl()
                .bg(theme::surface_2())
                .border_1()
                .border_color(theme::border())
                .px(px(18.))
                .py(px(16.))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::muted_text())
                        .child("Start a session to see the conversation."),
                )
                .into_any_element(),
        );
    }

    message_items
}

fn render_block(
    view: &AuiApp,
    session_id: crate::session::SessionId,
    message_index: usize,
    block_index: usize,
    block: MessageBlock,
    cx: &mut gpui::Context<AuiApp>,
) -> gpui::AnyElement {
    match block {
        MessageBlock::Text(text) => markdown_block(SharedString::from(text)).into_any_element(),
        MessageBlock::Code { language, code } => {
            code_block(SharedString::from(language), SharedString::from(code), cx)
                .into_any_element()
        }
        MessageBlock::Diff { title, diff } => {
            let key = DiffKey::new(session_id, message_index, block_index);
            let decision = view.diff_decision(key);
            diff_view(
                SharedString::from(title),
                SharedString::from(diff),
                decision,
                key,
                cx,
            )
            .into_any_element()
        }
        MessageBlock::Shell { title, output } => {
            let key = crate::app::ShellKey::new(session_id, message_index, block_index);
            let collapsed = view.shell_collapsed(key).unwrap_or(false);
            shell_output(
                SharedString::from(title),
                SharedString::from(output),
                key,
                collapsed,
                cx,
            )
            .into_any_element()
        }
    }
}

fn style_for_role(role: &SessionRole) -> (&'static str, gpui::Hsla, gpui::Rgba) {
    match role {
        SessionRole::User => ("You", hsla(0.55, 0.75, 0.6, 0.12), theme::text()),
        SessionRole::Assistant => ("Assistant", hsla(0.0, 0.0, 1.0, 0.06), theme::text()),
        SessionRole::Tool => ("Tool", hsla(0.22, 0.6, 0.6, 0.1), theme::text()),
    }
}
