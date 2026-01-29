use gpui::{AnyElement, FontWeight, IntoElement, SharedString, div, prelude::*, px};

use crate::ui::theme;

pub fn markdown_block(content: SharedString) -> impl IntoElement {
    let blocks = render_markdown_blocks(content.as_ref());
    div()
        .rounded_lg()
        .bg(theme::surface_3())
        .border_1()
        .border_color(theme::border())
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::subtle_text())
                .child("Markdown"),
        )
        .child(div().flex().flex_col().gap_2().children(blocks))
}

fn render_markdown_blocks(content: &str) -> Vec<AnyElement> {
    let blocks = parse_markdown_blocks(content);
    let mut rendered = Vec::new();
    for block in blocks {
        match block {
            MarkdownBlock::Heading { level, text } => rendered.push(render_heading(level, text)),
            MarkdownBlock::Paragraph(text) => rendered.push(render_paragraph(text)),
            MarkdownBlock::List(items) => rendered.push(render_list(items)),
            MarkdownBlock::Quote(text) => rendered.push(render_quote(text)),
        }
    }

    if rendered.is_empty() {
        rendered.push(
            div()
                .text_sm()
                .text_color(theme::muted_text())
                .child("No markdown content.")
                .into_any_element(),
        );
    }

    rendered
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MarkdownBlock {
    Heading { level: usize, text: String },
    Paragraph(String),
    List(Vec<String>),
    Quote(String),
}

fn parse_markdown_blocks(content: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut list_items = Vec::new();
    let mut quote_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            flush_paragraph_block(&mut blocks, &mut paragraph);
            flush_list_block(&mut blocks, &mut list_items);
            flush_quote_block(&mut blocks, &mut quote_lines);
            continue;
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            flush_paragraph_block(&mut blocks, &mut paragraph);
            flush_list_block(&mut blocks, &mut list_items);
            flush_quote_block(&mut blocks, &mut quote_lines);
            blocks.push(MarkdownBlock::Heading { level, text });
            continue;
        }

        if let Some(item) = parse_list_item(trimmed) {
            flush_paragraph_block(&mut blocks, &mut paragraph);
            flush_quote_block(&mut blocks, &mut quote_lines);
            list_items.push(item);
            continue;
        }

        if let Some(quote) = parse_quote(trimmed) {
            flush_paragraph_block(&mut blocks, &mut paragraph);
            flush_list_block(&mut blocks, &mut list_items);
            quote_lines.push(quote);
            continue;
        }

        paragraph.push(trimmed.to_string());
    }

    flush_paragraph_block(&mut blocks, &mut paragraph);
    flush_list_block(&mut blocks, &mut list_items);
    flush_quote_block(&mut blocks, &mut quote_lines);

    blocks
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let mut chars = line.chars();
    let mut level = 0;
    while let Some('#') = chars.next() {
        level += 1;
    }
    if level == 0 || level > 3 {
        return None;
    }
    let text = line[level..].trim();
    if text.is_empty() {
        None
    } else {
        Some((level, text.to_string()))
    }
}

fn parse_list_item(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("- ") {
        return Some(rest.trim().to_string());
    }
    if let Some(rest) = line.strip_prefix("* ") {
        return Some(rest.trim().to_string());
    }
    None
}

fn parse_quote(line: &str) -> Option<String> {
    line.strip_prefix("> ").map(|rest| rest.trim().to_string())
}

fn flush_paragraph_block(blocks: &mut Vec<MarkdownBlock>, paragraph: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    let text = paragraph.join("\n");
    blocks.push(MarkdownBlock::Paragraph(text));
    paragraph.clear();
}

fn flush_list_block(blocks: &mut Vec<MarkdownBlock>, list_items: &mut Vec<String>) {
    if list_items.is_empty() {
        return;
    }
    blocks.push(MarkdownBlock::List(std::mem::take(list_items)));
}

fn flush_quote_block(blocks: &mut Vec<MarkdownBlock>, quote_lines: &mut Vec<String>) {
    if quote_lines.is_empty() {
        return;
    }
    let text = quote_lines.join("\n");
    blocks.push(MarkdownBlock::Quote(text));
    quote_lines.clear();
}

fn render_paragraph(text: String) -> AnyElement {
    div()
        .text_sm()
        .text_color(theme::text())
        .child(text)
        .into_any_element()
}

fn render_list(items: Vec<String>) -> AnyElement {
    let mut rendered_items = Vec::new();
    for item in items {
        rendered_items.push(
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(div().text_sm().text_color(theme::subtle_text()).child("-"))
                .child(div().text_sm().text_color(theme::text()).child(item))
                .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(rendered_items)
        .into_any_element()
}

fn render_quote(text: String) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap_2()
        .child(div().w(px(3.)).bg(theme::accent()).rounded_full())
        .child(div().text_sm().text_color(theme::muted_text()).child(text))
        .into_any_element()
}

fn render_heading(level: usize, text: String) -> AnyElement {
    let size = match level {
        1 => 20.0,
        2 => 18.0,
        _ => 16.0,
    };
    div()
        .text_size(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text())
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_blocks(blocks: &[MarkdownBlock]) -> String {
        let mut out = String::new();
        for block in blocks {
            match block {
                MarkdownBlock::Heading { level, text } => {
                    out.push_str(&format!("H{level}:{text}\n"));
                }
                MarkdownBlock::Paragraph(text) => {
                    out.push_str("P:");
                    out.push_str(text);
                    out.push('\n');
                }
                MarkdownBlock::List(items) => {
                    out.push_str("L:\n");
                    for item in items {
                        out.push_str("- ");
                        out.push_str(item);
                        out.push('\n');
                    }
                }
                MarkdownBlock::Quote(text) => {
                    out.push_str("Q:");
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
        out.trim_end().to_string()
    }

    #[test]
    fn markdown_blocks_snapshot() {
        let input = "# Title\n\nHello\nworld\n- one\n- two\n> note\n";
        let blocks = parse_markdown_blocks(input);
        let snapshot = snapshot_blocks(&blocks);
        let expected = "H1:Title\nP:Hello\nworld\nL:\n- one\n- two\nQ:note";
        assert_eq!(snapshot, expected);
    }

    #[test]
    fn markdown_blocks_snapshot_multiple_quotes() {
        let input = "> first\n> second\n\nplain";
        let blocks = parse_markdown_blocks(input);
        let snapshot = snapshot_blocks(&blocks);
        let expected = "Q:first\nsecond\nP:plain";
        assert_eq!(snapshot, expected);
    }
}
