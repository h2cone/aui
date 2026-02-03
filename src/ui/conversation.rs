use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px};

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

            let blocks = parse_blocks(&message.content);
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
        std::mem::take(language)
    };
    if lang == "diff" {
        blocks.push(MessageBlock::Diff {
            title: "Diff".to_string(),
            diff: trimmed.to_string(),
        });
    } else if is_shell_language(&lang) {
        blocks.push(MessageBlock::Shell {
            title: "Shell".to_string(),
            output: trimmed.to_string(),
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
        SessionRole::User => ("You", hsla(0.55, 0.75, 0.6, 0.12), theme::text()),
        SessionRole::Assistant => ("Assistant", hsla(0.0, 0.0, 1.0, 0.06), theme::text()),
        SessionRole::Tool => ("Tool", hsla(0.22, 0.6, 0.6, 0.1), theme::text()),
    }
}

#[derive(Debug)]
enum MessageBlock {
    Text(String),
    Code { language: String, code: String },
    Diff { title: String, diff: String },
    Shell { title: String, output: String },
}

fn is_shell_language(language: &str) -> bool {
    matches!(
        language.to_ascii_lowercase().as_str(),
        "sh" | "bash" | "zsh" | "shell" | "console"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_blocks_handles_code_diff_and_shell() {
        let content =
            "hello\n```rs\nlet x = 1;\n```\nmore\n```diff\n+add\n```\n```sh\necho hi\n```\n";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 5);

        match &blocks[0] {
            MessageBlock::Text(text) => assert_eq!(text, "hello"),
            _ => panic!("expected text block"),
        }
        match &blocks[1] {
            MessageBlock::Code { language, code } => {
                assert_eq!(language, "rs");
                assert_eq!(code, "let x = 1;");
            }
            _ => panic!("expected code block"),
        }
        match &blocks[2] {
            MessageBlock::Text(text) => assert_eq!(text, "more"),
            _ => panic!("expected text block"),
        }
        match &blocks[3] {
            MessageBlock::Diff { title, diff } => {
                assert_eq!(title, "Diff");
                assert_eq!(diff, "+add");
            }
            _ => panic!("expected diff block"),
        }
        match &blocks[4] {
            MessageBlock::Shell { title, output } => {
                assert_eq!(title, "Shell");
                assert_eq!(output, "echo hi");
            }
            _ => panic!("expected shell block"),
        }
    }

    #[test]
    fn parse_blocks_skips_empty_code_and_resets_language() {
        let content = "```rs\n\n```\n```py\nprint(1)\n```\n``` \nvalue\n```\n";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 2);

        match &blocks[0] {
            MessageBlock::Code { language, code } => {
                assert_eq!(language, "py");
                assert_eq!(code, "print(1)");
            }
            _ => panic!("expected code block"),
        }
        match &blocks[1] {
            MessageBlock::Code { language, code } => {
                assert_eq!(language, "code");
                assert_eq!(code, "value");
            }
            _ => panic!("expected code block"),
        }
    }
}
