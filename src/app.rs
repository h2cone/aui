use std::time::Duration;

use gpui::{
    Context, Entity, FontWeight, Window, div, hsla, linear_color_stop, linear_gradient, prelude::*,
    px, rgb,
};

use crate::actions::Submit;
use crate::agent::bridge::BridgeClient;
use crate::agent::{AgentInfo, AgentKind, AgentStatus, Attachment};
use crate::session::{SessionId, SessionManager, SessionRole};
use crate::text_input::TextInput;
use crate::ui::{conversation, input_box, sidebar, status_bar};

pub struct AuiApp {
    pub text_input: Entity<TextInput>,
    sessions: SessionManager,
    bridge: BridgeClient,
    attachments: Vec<Attachment>,
    status_note: String,
}

impl AuiApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text_input = cx.new(|cx| TextInput::new(cx));
        let bridge = BridgeClient::new();
        let mut sessions = SessionManager::new();
        seed_sessions(&bridge, &mut sessions);

        Self {
            text_input,
            sessions,
            bridge,
            attachments: Vec::new(),
            status_note: "Ready".to_string(),
        }
    }

    pub fn sessions(&self) -> &[crate::session::Session] {
        self.sessions.sessions()
    }

    pub fn active_session_id(&self) -> Option<SessionId> {
        self.sessions.active_id()
    }

    pub fn select_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.sessions.set_active(id);
        self.status_note = "Session focus updated".to_string();
        cx.notify();
    }

    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        let next = self.sessions.sessions().len() + 1;
        let title = format!("session-{}", next);
        let agent = select_agent(&self.bridge, AgentKind::Claude);
        let id = self.sessions.create_session(title, agent);
        self.sessions
            .append_message(id, SessionRole::Assistant, "New session ready.".to_string());
        self.status_note = "New session created".to_string();
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let message = self.text_input.update(cx, |input, cx| {
            let message = input.take_submission();
            if message.is_some() {
                cx.notify();
            }
            message
        });

        let Some(message) = message else {
            self.status_note = "Type a message before sending".to_string();
            cx.notify();
            return;
        };

        let Some(active_id) = self.sessions.active_id() else {
            self.status_note = "Create a session first".to_string();
            cx.notify();
            return;
        };

        self.sessions
            .append_message(active_id, SessionRole::User, message.to_string());
        self.sessions.set_status(active_id, AgentStatus::Thinking);
        self.status_note = "Delivering to agent".to_string();
        cx.notify();

        let handle = cx.entity().downgrade();
        gpui::App::spawn(cx, async move |cx| {
            gpui::Timer::after(Duration::from_millis(520)).await;
            let _ = handle.update(cx, |view, cx| {
                view.sessions.append_message(
                    active_id,
                    SessionRole::Assistant,
                    "Bridge not connected yet. Configure the agent bridge to enable replies."
                        .to_string(),
                );
                view.sessions.set_status(active_id, AgentStatus::Idle);
                view.status_note = "Idle".to_string();
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for AuiApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let viewport_w = viewport.width.to_f64() as f32;
        let viewport_h = viewport.height.to_f64() as f32;
        let outer_padding_f32 = (viewport_w.min(viewport_h) * 0.04).clamp(14.0, 30.0);
        let outer_padding = px(outer_padding_f32);
        let sidebar_width_f32 = (viewport_w * 0.24).clamp(220.0, 280.0);
        let sidebar_width = px(sidebar_width_f32);
        let main_width =
            px((viewport_w - sidebar_width_f32 - outer_padding_f32 * 2.0).clamp(520.0, 980.0));
        let conversation_height = px((viewport_h - 340.0).clamp(220.0, 520.0));

        let background = linear_gradient(
            135.0,
            linear_color_stop(rgb(0xfff5ea), 0.0),
            linear_color_stop(rgb(0xe7f4f1), 1.0),
        );

        let panel_bg = linear_gradient(
            180.0,
            linear_color_stop(rgb(0xffffff), 0.0),
            linear_color_stop(rgb(0xf5f8ff), 1.0),
        );

        let active_session = self.sessions.active();
        let status = active_session
            .map(|session| &session.status)
            .unwrap_or(&AgentStatus::Idle);
        let fallback_stats = crate::session::SessionStats::new();
        let stats = active_session
            .map(|session| &session.stats)
            .unwrap_or(&fallback_stats);

        div()
            .size_full()
            .font_family("Bahnschrift")
            .bg(background)
            .flex()
            .items_start()
            .justify_start()
            .p(outer_padding)
            .child(
                div()
                    .w(sidebar_width)
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded_xl()
                    .p(px(18.))
                    .bg(panel_bg)
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.06))
                    .shadow(vec![gpui::BoxShadow {
                        color: hsla(0.0, 0.0, 0.0, 0.12),
                        offset: gpui::point(px(0.), px(14.)),
                        blur_radius: px(32.),
                        spread_radius: px(-16.),
                    }])
                    .child(sidebar::render_sidebar(self, cx)),
            )
            .child(
                div()
                    .w(main_width)
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded_xl()
                    .p(px(22.))
                    .bg(panel_bg)
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.06))
                    .shadow(vec![gpui::BoxShadow {
                        color: hsla(0.0, 0.0, 0.0, 0.12),
                        offset: gpui::point(px(0.), px(16.)),
                        blur_radius: px(36.),
                        spread_radius: px(-18.),
                    }])
                    .on_action(cx.listener(Self::submit))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x0b1220))
                                            .child(
                                                active_session
                                                    .map(|session| {
                                                        format!("{} conversation", session.title)
                                                    })
                                                    .unwrap_or_else(|| "Conversation".to_string()),
                                            ),
                                    )
                                    .child(
                                        div().text_sm().text_color(rgb(0x5b6777)).child(
                                            active_session
                                                .map(|session| session.agent.name)
                                                .unwrap_or("Select a session"),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(12.))
                                    .py(px(6.))
                                    .rounded_full()
                                    .bg(hsla(0.5, 0.4, 0.92, 0.45))
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x0f172a))
                                    .child("Unified desktop"),
                            ),
                    )
                    .child(
                        div()
                            .h(conversation_height)
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(conversation::render_conversation(active_session)),
                    )
                    .child(status_bar::render_status_bar(
                        status,
                        stats,
                        self.status_note.clone().into(),
                    ))
                    .child(input_box::render_input_box(
                        &self.text_input,
                        &self.attachments,
                    )),
            )
    }
}

fn seed_sessions(bridge: &BridgeClient, sessions: &mut SessionManager) {
    let claude = select_agent(bridge, AgentKind::Claude);
    let codex = select_agent(bridge, AgentKind::Codex);
    let gemini = select_agent(bridge, AgentKind::Gemini);

    let first = sessions.create_session("proj-a", claude);
    sessions.append_message(
        first,
        SessionRole::Assistant,
        "Welcome to AUI. Ask anything about your project.".to_string(),
    );

    let second = sessions.create_session("debug", codex);
    sessions.append_message(
        second,
        SessionRole::Assistant,
        "Provide logs or stack traces to start debugging.".to_string(),
    );

    let third = sessions.create_session("research", gemini);
    sessions.append_message(
        third,
        SessionRole::Assistant,
        "Summarize requirements or explore design options here.".to_string(),
    );

    sessions.set_active(first);
}

fn select_agent(bridge: &BridgeClient, kind: AgentKind) -> AgentInfo {
    bridge
        .agents()
        .iter()
        .find(|agent| agent.kind == kind)
        .cloned()
        .unwrap_or_else(|| AgentInfo::new("claude-code", "Claude Code", AgentKind::Claude))
}
