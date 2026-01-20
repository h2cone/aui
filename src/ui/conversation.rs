use gpui::{
    BoxShadow, FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb, white,
};

use crate::session::{Session, SessionRole};
use crate::ui::widgets::code_block::code_block;
use crate::ui::widgets::diff_view::diff_view;
use crate::ui::widgets::markdown::markdown_block;

pub fn render_conversation(session: Option<&Session>) -> impl IntoElement {
    let mut message_items = Vec::new();

    if let Some(session) = session {
        for message in &session.messages {
            let (label, bubble_bg, text_color) = style_for_role(&message.role);
            let blocks = parse_blocks(&message.content);
            let mut body_children = Vec::new();

            for block in blocks {
                body_children.push(render_block(block));
            }

            if body_children.is_empty() {
                body_children.push(
                    div()
                        .text_sm()
                        .text_color(rgb(0x5b6777))
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
                    .child(div().flex().flex_col().gap_2().children(body_children))
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

fn render_block(block: MessageBlock) -> gpui::AnyElement {
    match block {
        MessageBlock::Text(text) => markdown_block(SharedString::from(text)).into_any_element(),
        MessageBlock::Code { language, code } => {
            code_block(SharedString::from(language), SharedString::from(code)).into_any_element()
        }
        MessageBlock::Diff { title, diff } => {
            diff_view(SharedString::from(title), SharedString::from(diff)).into_any_element()
        }
    }
}

fn parse_blocks(content: &str) -> Vec<MessageBlock> {
    let mut blocks = Vec::new();
    let mut text_buffer = String::new();
    let mut code_buffer = String::new();
    let mut code_lang = String::new();
    let mut in_code = false;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if in_code {
                flush_code_block(&mut blocks, &mut code_lang, &mut code_buffer);
                in_code = false;
            } else {
                flush_text_block(&mut blocks, &mut text_buffer);
                code_lang = rest.trim().to_string();
                in_code = true;
            }
            continue;
        }

        if in_code {
            code_buffer.push_str(line);
            code_buffer.push('\n');
        } else {
            text_buffer.push_str(line);
            text_buffer.push('\n');
        }
    }

    if in_code {
        flush_code_block(&mut blocks, &mut code_lang, &mut code_buffer);
    }
    flush_text_block(&mut blocks, &mut text_buffer);

    blocks
}

fn flush_text_block(blocks: &mut Vec<MessageBlock>, buffer: &mut String) {
    let trimmed = buffer.trim();
    if !trimmed.is_empty() {
        blocks.push(MessageBlock::Text(trimmed.to_string()));
    }
    buffer.clear();
}

fn flush_code_block(blocks: &mut Vec<MessageBlock>, language: &mut String, buffer: &mut String) {
    let trimmed = buffer.trim_end();
    if trimmed.is_empty() {
        buffer.clear();
        language.clear();
        return;
    }
    let lang = if language.is_empty() {
        "code".to_string()
    } else {
        language.clone()
    };
    if lang == "diff" {
        blocks.push(MessageBlock::Diff {
            title: "Diff".to_string(),
            diff: trimmed.to_string(),
        });
    } else {
        blocks.push(MessageBlock::Code {
            language: lang,
            code: trimmed.to_string(),
        });
    }
    buffer.clear();
    language.clear();
}

fn style_for_role(role: &SessionRole) -> (&'static str, gpui::Hsla, gpui::Rgba) {
    match role {
        SessionRole::User => ("You", hsla(0.55, 0.4, 0.95, 0.32), rgb(0x0b1220)),
        SessionRole::Assistant => ("Agent", hsla(0.14, 0.6, 0.92, 0.32), rgb(0x0f172a)),
        SessionRole::Tool => ("Tool", hsla(0.0, 0.0, 0.9, 0.28), rgb(0x1f2937)),
    }
}

#[derive(Debug)]
enum MessageBlock {
    Text(String),
    Code { language: String, code: String },
    Diff { title: String, diff: String },
}
