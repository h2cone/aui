use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    Context, Entity, FontWeight, Window, div, hsla, linear_color_stop, linear_gradient, prelude::*,
    px, rgb,
};

use crate::actions::Submit;
use crate::agent::bridge::BridgeClient;
use crate::agent::{AgentInfo, AgentKind, AgentStatus, Attachment, StreamEvent, UserMessage};
use crate::logger;
use crate::session::{SessionId, SessionManager, SessionRole, SessionStorage, StoredSession};
use crate::text_input::TextInput;
use crate::ui::{conversation, input_box, sidebar, status_bar};

pub struct AuiApp {
    pub text_input: Entity<TextInput>,
    sessions: SessionManager,
    bridge: BridgeClient,
    attachments: Vec<Attachment>,
    status_note: String,
    storage: SessionStorage,
    stream_targets: HashMap<SessionId, usize>,
}

impl AuiApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        logger::info("app init");
        let text_input = cx.new(|cx| TextInput::new(cx));
        let bridge = BridgeClient::new();
        let storage = SessionStorage::new();
        let mut sessions = SessionManager::new();
        logger::debug("restoring sessions");
        restore_sessions(&bridge, &storage, &mut sessions);
        if sessions.sessions().is_empty() {
            logger::debug("no sessions restored, seeding defaults");
            seed_sessions(&bridge, &mut sessions);
        }
        if sessions.active_id().is_none() {
            if let Some(first) = sessions.sessions().first() {
                sessions.set_active(first.id);
            }
        }
        logger::debug(&format!(
            "sessions ready count={} active={}",
            sessions.sessions().len(),
            sessions
                .active_id()
                .map(|id| id.value().to_string())
                .unwrap_or_else(|| "none".to_string())
        ));

        Self {
            text_input,
            sessions,
            bridge,
            attachments: Vec::new(),
            status_note: "Ready".to_string(),
            storage,
            stream_targets: HashMap::new(),
        }
    }

    pub fn sessions(&self) -> &[crate::session::Session] {
        self.sessions.sessions()
    }

    pub fn active_session_id(&self) -> Option<SessionId> {
        self.sessions.active_id()
    }

    pub fn select_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        logger::debug(&format!("session select id={}", id.value()));
        self.sessions.set_active(id);
        self.status_note = "Session focus updated".to_string();
        cx.notify();
    }

    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        let next = self.sessions.sessions().len() + 1;
        let title = format!("session-{}", next);
        let agent = select_agent(&self.bridge, AgentKind::Claude);
        let id = self.sessions.create_session(title, agent);
        logger::debug(&format!(
            "session created id={} agent={}",
            id.value(),
            self.sessions
                .session(id)
                .map(|session| session.agent.id.clone())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        self.sessions
            .append_message(id, SessionRole::Assistant, "New session ready.".to_string());
        self.status_note = "New session created".to_string();
        self.persist_session(id);
        cx.notify();
    }

    pub fn delete_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if !self.sessions.remove_session(id) {
            return;
        }
        self.stream_targets.remove(&id);
        if let Err(err) = self.storage.delete_session(id) {
            logger::warn(&format!(
                "session delete failed id={} error={}",
                id.value(),
                err
            ));
        }
        if self.sessions.sessions().is_empty() {
            self.status_note = "No sessions remaining".to_string();
        } else {
            self.status_note = "Session deleted".to_string();
        }
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
            logger::debug("submit ignored: empty input");
            cx.notify();
            return;
        };

        let Some(active_id) = self.sessions.active_id() else {
            self.status_note = "Create a session first".to_string();
            logger::debug("submit ignored: no active session");
            cx.notify();
            return;
        };

        let user_text = message.to_string();
        logger::debug(&format!(
            "submit message session={} len={} attachments={}",
            active_id.value(),
            user_text.len(),
            self.attachments.len()
        ));
        self.sessions
            .append_message(active_id, SessionRole::User, user_text.clone());

        let assistant_index =
            self.sessions
                .push_message(active_id, SessionRole::Assistant, String::new());
        if let Some(index) = assistant_index {
            self.stream_targets.insert(active_id, index);
        }

        self.sessions.set_status(active_id, AgentStatus::Thinking);
        self.status_note = "Delivering to agent".to_string();
        self.persist_session(active_id);
        cx.notify();

        let agent = self
            .sessions
            .session(active_id)
            .map(|session| session.agent.clone())
            .unwrap_or_else(|| select_agent(&self.bridge, AgentKind::Claude));
        logger::debug(&format!(
            "bridge send session={} agent={}",
            active_id.value(),
            agent.id.as_str()
        ));
        let stream = self.bridge.connect(&agent).send(UserMessage {
            text: user_text,
            attachments: self.attachments.clone(),
            context: None,
        });

        let handle = cx.entity().downgrade();
        let events = stream.events;
        gpui::App::spawn(cx, async move |cx| {
            let mut done = false;
            while !done {
                while let Ok(event) = events.try_recv() {
                    let is_terminal = matches!(event, StreamEvent::Done | StreamEvent::Error(_));
                    let _ = handle.update(cx, |view, cx| {
                        view.apply_stream_event(active_id, event);
                        cx.notify();
                    });
                    if is_terminal {
                        done = true;
                        break;
                    }
                }
                if done {
                    break;
                }
                gpui::Timer::after(Duration::from_millis(40)).await;
            }
        })
        .detach();
    }

    fn apply_stream_event(&mut self, id: SessionId, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta(delta) => {
                logger::debug(&format!(
                    "stream delta session={} len={}",
                    id.value(),
                    delta.len()
                ));
                if let Some(index) = self.stream_targets.get(&id).copied() {
                    if let Some(session) = self.sessions.session_mut(id) {
                        if let Some(message) = session.messages.get_mut(index) {
                            message.content.push_str(&delta);
                        }
                    }
                }
                self.persist_session(id);
            }
            StreamEvent::ToolStart { name, input } => {
                logger::debug(&format!(
                    "stream tool start session={} tool={} len={}",
                    id.value(),
                    name.as_str(),
                    input.len()
                ));
                self.sessions.push_message(
                    id,
                    SessionRole::Tool,
                    format!("Tool start: {name}\n{input}"),
                );
                self.sessions
                    .set_status(id, AgentStatus::Executing { tool: name });
                self.persist_session(id);
            }
            StreamEvent::ToolResult { name, output } => {
                logger::debug(&format!(
                    "stream tool result session={} tool={} len={}",
                    id.value(),
                    name.as_str(),
                    output.len()
                ));
                self.sessions.push_message(
                    id,
                    SessionRole::Tool,
                    format!("Tool result: {name}\n{output}"),
                );
                self.sessions.set_status(id, AgentStatus::Thinking);
                self.persist_session(id);
            }
            StreamEvent::TokenUsage { input, output } => {
                logger::debug(&format!(
                    "stream token usage session={} in={} out={}",
                    id.value(),
                    input,
                    output
                ));
                self.sessions.bump_usage(id, input, output, 0.0);
                self.persist_session(id);
            }
            StreamEvent::Done => {
                logger::debug(&format!("stream done session={}", id.value()));
                self.stream_targets.remove(&id);
                self.sessions.set_status(id, AgentStatus::Idle);
                self.status_note = "Idle".to_string();
                self.persist_session(id);
            }
            StreamEvent::Error(message) => {
                logger::warn(&format!(
                    "stream error session={} message={}",
                    id.value(),
                    message.as_str()
                ));
                self.stream_targets.remove(&id);
                self.sessions.set_status(
                    id,
                    AgentStatus::Error {
                        message: message.clone(),
                    },
                );
                self.status_note = format!("Error: {message}");
                self.persist_session(id);
            }
        }
    }

    fn persist_session(&self, id: SessionId) {
        if let Some(session) = self.sessions.session(id) {
            if let Err(err) = self.storage.save_session(session) {
                logger::warn(&format!(
                    "session save failed id={} error={}",
                    id.value(),
                    err
                ));
            }
        }
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
                                                .map(|session| session.agent.name.clone())
                                                .unwrap_or_else(|| "Select a session".to_string()),
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
                            .id("conversation-scroll")
                            .overflow_y_scroll()
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
    logger::debug("seeding default sessions");
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

fn restore_sessions(
    bridge: &BridgeClient,
    storage: &SessionStorage,
    sessions: &mut SessionManager,
) {
    let stored_sessions = match storage.load_sessions() {
        Ok(stored_sessions) => stored_sessions,
        Err(err) => {
            logger::warn(&format!("session load failed: {err}"));
            return;
        }
    };

    for stored in stored_sessions {
        logger::debug(&format!(
            "restoring session id={} title={} agent={} messages={}",
            stored.id.value(),
            stored.title.as_str(),
            stored.agent_id.as_str(),
            stored.messages.len()
        ));
        let agent = resolve_stored_agent(bridge, &stored);
        let session = crate::session::Session {
            id: stored.id,
            title: stored.title,
            agent,
            status: AgentStatus::Idle,
            stats: crate::session::SessionStats::new(),
            messages: stored.messages,
        };
        sessions.restore_session(session);
    }
}

fn select_agent(bridge: &BridgeClient, kind: AgentKind) -> AgentInfo {
    bridge
        .agents()
        .iter()
        .find(|agent| agent.kind == kind)
        .cloned()
        .unwrap_or_else(|| AgentInfo::new("claude-code", "Claude Code", AgentKind::Claude))
}

fn resolve_stored_agent(bridge: &BridgeClient, stored: &StoredSession) -> AgentInfo {
    if let Some(agent) = bridge.agent_by_id(&stored.agent_id) {
        return agent;
    }
    AgentInfo::new(
        stored.agent_id.clone(),
        stored.agent_name.clone(),
        AgentKind::Claude,
    )
}
