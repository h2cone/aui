use gpui::{
    AnyElement, FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb, white,
};

pub fn markdown_block(content: SharedString) -> impl IntoElement {
    let blocks = render_markdown_blocks(content.as_ref());
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
        .child(div().flex().flex_col().gap_2().children(blocks))
}

fn render_markdown_blocks(content: &str) -> Vec<AnyElement> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut list_items = Vec::new();
    let mut quote_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_list(&mut blocks, &mut list_items);
            flush_quote(&mut blocks, &mut quote_lines);
            continue;
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_list(&mut blocks, &mut list_items);
            flush_quote(&mut blocks, &mut quote_lines);
            blocks.push(render_heading(level, text));
            continue;
        }

        if let Some(item) = parse_list_item(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_quote(&mut blocks, &mut quote_lines);
            list_items.push(item);
            continue;
        }

        if let Some(quote) = parse_quote(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_list(&mut blocks, &mut list_items);
            quote_lines.push(quote);
            continue;
        }

        paragraph.push(trimmed.to_string());
    }

    flush_paragraph(&mut blocks, &mut paragraph);
    flush_list(&mut blocks, &mut list_items);
    flush_quote(&mut blocks, &mut quote_lines);

    if blocks.is_empty() {
        blocks.push(
            div()
                .text_sm()
                .text_color(rgb(0x64748b))
                .child("No markdown content.")
                .into_any_element(),
        );
    }

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

fn flush_paragraph(blocks: &mut Vec<AnyElement>, paragraph: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    let text = paragraph.join("\n");
    blocks.push(
        div()
            .text_sm()
            .text_color(rgb(0x0f172a))
            .child(text)
            .into_any_element(),
    );
    paragraph.clear();
}

fn flush_list(blocks: &mut Vec<AnyElement>, list_items: &mut Vec<String>) {
    if list_items.is_empty() {
        return;
    }
    let mut items = Vec::new();
    for item in list_items.iter() {
        items.push(
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(div().text_sm().text_color(rgb(0x94a3b8)).child("-"))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x0f172a))
                        .child(item.clone()),
                )
                .into_any_element(),
        );
    }
    blocks.push(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(items)
            .into_any_element(),
    );
    list_items.clear();
}

fn flush_quote(blocks: &mut Vec<AnyElement>, quote_lines: &mut Vec<String>) {
    if quote_lines.is_empty() {
        return;
    }
    let text = quote_lines.join("\n");
    blocks.push(
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(div().w(px(3.)).bg(hsla(0.52, 0.4, 0.6, 0.4)).rounded_full())
            .child(div().text_sm().text_color(rgb(0x475569)).child(text))
            .into_any_element(),
    );
    quote_lines.clear();
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
        .text_color(rgb(0x0b1220))
        .child(text)
        .into_any_element()
}
