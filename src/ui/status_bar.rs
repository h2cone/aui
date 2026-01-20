use gpui::{FontWeight, IntoElement, SharedString, div, hsla, prelude::*, px, rgb, white};

use crate::agent::AgentStatus;
use crate::session::SessionStats;

pub fn render_status_bar(
    status: &AgentStatus,
    stats: &SessionStats,
    status_note: SharedString,
) -> impl IntoElement {
    let (label, detail, color) = status_label(status);
    let elapsed = stats.elapsed().as_secs();
    let usage = format!("Tokens: {} in / {} out", stats.tokens_in, stats.tokens_out);
    let cost = format!("Cost: ${:.2}", stats.cost_usd);
    let duration = format!("Session: {}s", elapsed);

    div()
        .flex()
        .items_center()
        .justify_between()
        .rounded_xl()
        .bg(white())
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
        .px(px(16.))
        .py(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(color))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x0f172a))
                        .child(format!("{}{}", label, detail)),
                )
                .child(div().text_xs().text_color(rgb(0x64748b)).child(status_note)),
        )
        .child(
            div()
                .flex()
                .gap_3()
                .text_xs()
                .text_color(rgb(0x475569))
                .child(usage)
                .child(cost)
                .child(duration),
        )
}

fn status_label(status: &AgentStatus) -> (&'static str, String, gpui::Hsla) {
    match status {
        AgentStatus::Idle => ("Idle", String::new(), hsla(0.0, 0.0, 0.6, 0.45)),
        AgentStatus::Thinking => ("Thinking", "...".to_string(), hsla(0.53, 0.65, 0.55, 0.9)),
        AgentStatus::Executing { tool } => (
            "Executing",
            format!(": {tool}"),
            hsla(0.08, 0.75, 0.55, 0.9),
        ),
        AgentStatus::WaitingInput { prompt } => (
            "Waiting",
            format!(": {prompt}"),
            hsla(0.12, 0.85, 0.52, 0.85),
        ),
        AgentStatus::Error { message } => {
            ("Error", format!(": {message}"), hsla(0.0, 0.7, 0.55, 0.9))
        }
    }
}
