use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Context, Entity, FontWeight, PathPromptOptions, ScrollHandle, SharedString, Window, div, hsla,
    linear_color_stop, linear_gradient, prelude::*, px, rgb,
};

use crate::actions::{AttachFiles, ClearAttachments, ExportSession, Submit};
use crate::agent::bridge::BridgeClient;
use crate::agent::{
    Attachment, ProviderEvent, ProviderInfo, ProviderKind, SessionStatus, UserMessage,
    WorkingContext,
};
use crate::config;
use crate::logger;
use crate::session::{
    Session, SessionId, SessionManager, SessionRole, SessionStorage, StoredDiffDecision,
    StoredSession,
};
use crate::text_input::TextInput;
use crate::ui::{conversation, input_box, sidebar};

pub struct AuiApp {
    pub text_input: Entity<TextInput>,
    sessions: SessionManager,
    bridge: BridgeClient,
    attachments: Vec<Attachment>,
    new_session_provider_id: Arc<str>,
    storage: SessionStorage,
    stream_targets: HashMap<SessionId, usize>,
    diff_decisions: HashMap<DiffKey, DiffDecision>,
    shell_collapsed: HashMap<ShellKey, bool>,
    conversation_scroll: ScrollHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DiffKey {
    session_id: SessionId,
    message_index: usize,
    block_index: usize,
}

impl DiffKey {
    pub const fn new(session_id: SessionId, message_index: usize, block_index: usize) -> Self {
        Self {
            session_id,
            message_index,
            block_index,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffDecision {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShellKey {
    session_id: SessionId,
    message_index: usize,
    block_index: usize,
}

impl ShellKey {
    pub const fn new(session_id: SessionId, message_index: usize, block_index: usize) -> Self {
        Self {
            session_id,
            message_index,
            block_index,
        }
    }
}

impl AuiApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        logger::info("app init");
        let text_input = cx.new(|cx| TextInput::new(cx));
        let bridge = BridgeClient::new();
        let config = config::Config::load();
        let storage = SessionStorage::new();
        let mut sessions = SessionManager::new();
        let conversation_scroll = ScrollHandle::new();
        logger::debug("restoring sessions");
        restore_sessions(&bridge, &storage, &mut sessions);
        if sessions.sessions().is_empty() {
            logger::debug("no sessions restored");
        }
        if sessions.active_id().is_none() {
            if let Some(first) = sessions.sessions().first() {
                sessions.set_active(first.id);
            }
        }
        if sessions.active_id().is_some() {
            conversation_scroll.scroll_to_bottom();
        }
        logger::debug(&format!(
            "sessions ready count={} active={}",
            sessions.sessions().len(),
            sessions
                .active_id()
                .map(|id| id.value().to_string())
                .unwrap_or_else(|| "none".to_string())
        ));

        let mut diff_decisions = HashMap::new();
        for session in sessions.sessions() {
            if let Ok(decisions) = storage.load_diff_decisions(session.id) {
                for decision in decisions {
                    let value = if decision.accepted {
                        DiffDecision::Accepted
                    } else {
                        DiffDecision::Rejected
                    };
                    diff_decisions.insert(
                        DiffKey::new(session.id, decision.message_index, decision.block_index),
                        value,
                    );
                }
            }
        }

        let new_session_provider_id = bridge
            .provider_by_id(config.default_provider_id.as_str())
            .map(|provider| provider.id)
            .unwrap_or_else(|| "claude-code".into());

        Self {
            text_input,
            sessions,
            bridge,
            attachments: Vec::new(),
            new_session_provider_id,
            storage,
            stream_targets: HashMap::new(),
            diff_decisions,
            shell_collapsed: HashMap::new(),
            conversation_scroll,
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
        self.conversation_scroll.scroll_to_bottom();
        cx.notify();
    }

    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        let next = self.sessions.sessions().len() + 1;
        let title = format!("session-{}", next);
        let provider = self
            .bridge
            .provider_by_id(self.new_session_provider_id.as_ref())
            .unwrap_or_else(|| select_provider(&self.bridge, ProviderKind::Anthropic));
        let id = self.sessions.create_session(title, provider);
        logger::debug(&format!(
            "session created id={} provider={}",
            id.value(),
            self.sessions
                .session(id)
                .map(|session| session.provider.id.as_ref())
                .unwrap_or("unknown")
        ));
        self.sessions
            .append_message(id, SessionRole::Assistant, "New session ready.".to_string());
        self.persist_session(id);
        self.conversation_scroll.scroll_to_bottom();
        cx.notify();
    }

    pub fn new_session_provider_label(&self) -> SharedString {
        self.bridge
            .provider_by_id(self.new_session_provider_id.as_ref())
            .map(|provider| SharedString::from(provider.name.clone()))
            .unwrap_or_else(|| SharedString::from("Anthropic"))
    }

    pub fn cycle_new_session_provider(&mut self, cx: &mut Context<Self>) {
        let providers = self.bridge.providers();
        if providers.is_empty() {
            return;
        }
        let current_ix = providers
            .iter()
            .position(|provider| provider.id.as_ref() == self.new_session_provider_id.as_ref())
            .unwrap_or(0);
        let next_ix = (current_ix + 1) % providers.len();
        self.new_session_provider_id = providers[next_ix].id.clone();
        cx.notify();
    }

    pub fn cycle_session_provider(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let providers = self.bridge.providers();
        let Some(session) = self.sessions.session_mut(id) else {
            return;
        };
        if providers.is_empty() {
            return;
        }
        let current_ix = providers
            .iter()
            .position(|provider| provider.id == session.provider.id)
            .unwrap_or(0);
        let next_ix = (current_ix + 1) % providers.len();
        session.provider = providers[next_ix].clone();
        self.persist_session(id);
        cx.notify();
    }

    pub fn delete_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if !self.sessions.remove_session(id) {
            return;
        }
        self.stream_targets.remove(&id);
        self.diff_decisions.retain(|key, _| key.session_id != id);
        self.shell_collapsed.retain(|key, _| key.session_id != id);
        if let Err(err) = self.storage.delete_session(id) {
            logger::warn(&format!(
                "session delete failed id={} error={}",
                id.value(),
                err
            ));
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
            logger::debug("submit ignored: empty input");
            cx.notify();
            return;
        };

        let Some(active_id) = self.sessions.active_id() else {
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
        self.conversation_scroll.scroll_to_bottom();

        self.sessions.set_status(active_id, SessionStatus::Thinking);
        self.persist_session(active_id);
        cx.notify();

        let provider = self
            .sessions
            .session(active_id)
            .map(|session| session.provider.clone())
            .unwrap_or_else(|| select_provider(&self.bridge, ProviderKind::Anthropic));
        logger::debug(&format!(
            "bridge send session={} provider={}",
            active_id.value(),
            provider.id.as_ref()
        ));
        let attachments = std::mem::take(&mut self.attachments);
        let stream = self.bridge.connect(&provider).send(UserMessage {
            text: user_text,
            attachments,
            context: Some(WorkingContext {
                cwd: std::env::current_dir().ok(),
            }),
        });

        let handle = cx.entity().downgrade();
        let events = stream.events;
        gpui::App::spawn(cx, async move |cx| {
            let mut done = false;
            while !done {
                while let Ok(event) = events.try_recv() {
                    let is_terminal =
                        matches!(event, ProviderEvent::Done | ProviderEvent::Error(_));
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

    fn attach_files(&mut self, _: &AttachFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.open_attachment_picker(window, cx);
    }

    fn clear_attachments_action(
        &mut self,
        _: &ClearAttachments,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_attachments(cx);
    }

    fn export_session(&mut self, _: &ExportSession, window: &mut Window, cx: &mut Context<Self>) {
        self.export_active_session(window, cx);
    }

    fn apply_stream_event(&mut self, id: SessionId, event: ProviderEvent) {
        match event {
            ProviderEvent::TextDelta(delta) => {
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
                self.conversation_scroll.scroll_to_bottom();
            }
            ProviderEvent::ToolStart { name, input } => {
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
                    .set_status(id, SessionStatus::Executing { tool: name });
                self.persist_session(id);
                self.conversation_scroll.scroll_to_bottom();
            }
            ProviderEvent::ToolResult { name, output } => {
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
                self.sessions.set_status(id, SessionStatus::Thinking);
                self.persist_session(id);
                self.conversation_scroll.scroll_to_bottom();
            }
            ProviderEvent::TokenUsage { input, output } => {
                logger::debug(&format!(
                    "stream token usage session={} in={} out={}",
                    id.value(),
                    input,
                    output
                ));
                let kind = self
                    .sessions
                    .session(id)
                    .map(|session| session.provider.kind)
                    .unwrap_or(ProviderKind::Anthropic);
                let cost = estimate_cost_usd(kind, input, output);
                self.sessions.bump_usage(id, input, output, cost);
                self.persist_session(id);
            }
            ProviderEvent::Done => {
                logger::debug(&format!("stream done session={}", id.value()));
                self.stream_targets.remove(&id);
                self.sessions.set_status(id, SessionStatus::Idle);
                self.persist_session(id);
                self.conversation_scroll.scroll_to_bottom();
            }
            ProviderEvent::Error(message) => {
                logger::warn(&format!(
                    "stream error session={} message={}",
                    id.value(),
                    message.as_str()
                ));
                self.stream_targets.remove(&id);
                let user_message = friendly_error_message(&message);
                self.sessions.set_status(
                    id,
                    SessionStatus::Error {
                        message: user_message,
                    },
                );
                self.persist_session(id);
                self.conversation_scroll.scroll_to_bottom();
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

    pub fn diff_decision(&self, key: DiffKey) -> Option<DiffDecision> {
        self.diff_decisions.get(&key).copied()
    }

    pub fn set_diff_decision(
        &mut self,
        key: DiffKey,
        decision: DiffDecision,
        cx: &mut Context<Self>,
    ) {
        self.diff_decisions.insert(key, decision);
        self.persist_diff_decisions(key.session_id);
        cx.notify();
    }

    fn persist_diff_decisions(&self, session_id: SessionId) {
        let mut decisions = Vec::new();
        for (key, decision) in self.diff_decisions.iter() {
            if key.session_id != session_id {
                continue;
            }
            decisions.push(StoredDiffDecision {
                message_index: key.message_index,
                block_index: key.block_index,
                accepted: matches!(decision, DiffDecision::Accepted),
            });
        }
        decisions.sort_by_key(|decision| (decision.message_index, decision.block_index));
        if let Err(err) = self.storage.save_diff_decisions(session_id, &decisions) {
            logger::warn(&format!(
                "diff decision save failed session={} error={}",
                session_id.value(),
                err
            ));
        }
    }

    pub fn shell_collapsed(&self, key: ShellKey) -> Option<bool> {
        self.shell_collapsed.get(&key).copied()
    }

    pub fn toggle_shell(&mut self, key: ShellKey, cx: &mut Context<Self>) {
        let next = !self.shell_collapsed.get(&key).copied().unwrap_or(false);
        self.shell_collapsed.insert(key, next);
        cx.notify();
    }

    pub fn add_attachments(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            if self
                .attachments
                .iter()
                .any(|existing| existing.path.as_ref() == Some(&path))
            {
                continue;
            }
            self.attachments.push(Attachment {
                name,
                path: Some(path),
            });
        }
        cx.notify();
    }

    pub fn clear_attachments(&mut self, cx: &mut Context<Self>) {
        if self.attachments.is_empty() {
            return;
        }
        self.attachments.clear();
        cx.notify();
    }

    pub fn open_attachment_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = cx.entity().downgrade();
        window.defer(cx, move |_, app| {
            let async_app = app.to_async();
            let rx = app.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some("Select attachments".into()),
            });

            let task = app.foreground_executor().spawn(async move {
                let result = rx.await;
                let mut cx = async_app.clone();
                match result {
                    Ok(Ok(Some(paths))) => {
                        let _ = handle.update(&mut cx, |view, cx| {
                            view.add_attachments(paths, cx);
                        });
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(_)) | Err(_) => {}
                }
            });
            task.detach();
        });
    }

    pub fn export_active_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.active().cloned() else {
            cx.notify();
            return;
        };
        let handle = cx.entity().downgrade();
        let suggested = format!("{}.md", sanitize_filename(session.title.as_ref()));
        let dir = config::data_dir();
        window.defer(cx, move |_, app| {
            let async_app = app.to_async();
            let rx = app.prompt_for_new_path(&dir, Some(&suggested));
            let task = app.foreground_executor().spawn(async move {
                let result = rx.await;
                let mut cx = async_app.clone();
                let path = match result {
                    Ok(Ok(Some(path))) => path,
                    Ok(Ok(None)) => return,
                    Ok(Err(_)) | Err(_) => return,
                };

                let markdown = export_session_markdown(&session);
                let write_result = write_text_file(&path, &markdown);
                if write_result.is_ok() {
                    let _ = handle.update(&mut cx, |_, cx| {
                        cx.notify();
                    });
                }
            });
            task.detach();
        });
    }
}

fn sanitize_filename(name: &str) -> String {
    let mut cleaned = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ' ') {
            cleaned.push(ch);
        } else {
            cleaned.push('-');
        }
    }
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        "session".to_string()
    } else {
        cleaned.replace(' ', "-")
    }
}

fn export_session_markdown(session: &Session) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(session.title.as_ref());
    out.push_str("\n\n");
    out.push_str("- Provider: ");
    out.push_str(session.provider.name.as_ref());
    out.push('\n');
    out.push_str("- Exported: ");
    out.push_str(&format_system_time(std::time::SystemTime::now()));
    out.push_str("\n\n---\n\n");

    for message in &session.messages {
        out.push_str("## ");
        out.push_str(match message.role {
            SessionRole::User => "User",
            SessionRole::Assistant => "Assistant",
            SessionRole::Tool => "Tool",
        });
        out.push('\n');
        out.push_str("_");
        out.push_str(&format_system_time(message.timestamp));
        out.push_str("_\n\n");
        out.push_str(&message.content);
        out.push_str("\n\n");
    }

    out
}

fn format_system_time(time: std::time::SystemTime) -> String {
    let seconds = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, contents).map_err(|err| err.to_string())
}

fn estimate_cost_usd(kind: ProviderKind, input_tokens: u32, output_tokens: u32) -> f32 {
    let (in_rate, out_rate) = cost_rates_for(kind);
    if in_rate <= 0.0 && out_rate <= 0.0 {
        return 0.0;
    }
    let input_cost = (input_tokens as f32 / 1_000_000.0) * in_rate;
    let output_cost = (output_tokens as f32 / 1_000_000.0) * out_rate;
    input_cost + output_cost
}

fn cost_rates_for(kind: ProviderKind) -> (f32, f32) {
    let (default_in, default_out) = match kind {
        ProviderKind::Anthropic => (0.0, 0.0),
        ProviderKind::OpenAI => (0.0, 0.0),
        ProviderKind::Google => (0.0, 0.0),
        ProviderKind::OpenCode => (0.0, 0.0),
    };

    let prefix = match kind {
        ProviderKind::Anthropic => "AUI_CLAUDE",
        ProviderKind::OpenAI => "AUI_CODEX",
        ProviderKind::Google => "AUI_GEMINI",
        ProviderKind::OpenCode => "AUI_OPENCODE",
    };

    let in_key = format!("{prefix}_COST_IN_PER_MILLION");
    let out_key = format!("{prefix}_COST_OUT_PER_MILLION");
    let global_in = "AUI_COST_IN_PER_MILLION";
    let global_out = "AUI_COST_OUT_PER_MILLION";

    (
        read_env_f32(&in_key)
            .or_else(|| read_env_f32(global_in))
            .unwrap_or(default_in),
        read_env_f32(&out_key)
            .or_else(|| read_env_f32(global_out))
            .unwrap_or(default_out),
    )
}

fn read_env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok()?.trim().parse::<f32>().ok()
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
        let panel_height = px((viewport_h - outer_padding_f32 * 2.0).max(0.0));

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
        let error_banner = active_session.and_then(|session| match &session.status {
            SessionStatus::Error { message } => Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(hsla(0.0, 0.7, 0.55, 0.25))
                    .bg(hsla(0.0, 0.75, 0.96, 0.8))
                    .px(px(14.))
                    .py(px(10.))
                    .text_sm()
                    .text_color(rgb(0x7a1f1f))
                    .child(message.clone())
                    .into_any_element(),
            ),
            _ => None,
        });

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
                    .h(panel_height)
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
            .child({
                let mut main_panel = div()
                    .w(main_width)
                    .h(panel_height)
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
                    .on_action(cx.listener(Self::attach_files))
                    .on_action(cx.listener(Self::export_session))
                    .on_action(cx.listener(Self::clear_attachments_action))
                    .child(
                        div().flex().items_center().justify_start().child(
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
                                                    format!(
                                                        "{} conversation",
                                                        session.title.as_ref()
                                                    )
                                                })
                                                .unwrap_or_else(|| "Conversation".to_string()),
                                        ),
                                )
                                .child(
                                    div().text_sm().text_color(rgb(0x5b6777)).child(
                                        active_session
                                            .map(|session| {
                                                SharedString::from(session.provider.name.clone())
                                            })
                                            .unwrap_or_else(|| {
                                                SharedString::from("Select a session")
                                            }),
                                    ),
                                ),
                        ),
                    );

                if let Some(banner) = error_banner {
                    main_panel = main_panel.child(banner);
                }

                main_panel
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .id("conversation-scroll")
                            .overflow_y_scroll()
                            .track_scroll(&self.conversation_scroll)
                            .child(conversation::render_conversation(self, active_session, cx)),
                    )
                    .child(input_box::render_input_box(&self.text_input, cx))
            })
    }
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
            "restoring session id={} title={} provider={} messages={}",
            stored.id.value(),
            stored.title.as_str(),
            stored.provider_id.as_str(),
            stored.messages.len()
        ));
        let provider = resolve_stored_provider(bridge, &stored);
        let session = crate::session::Session {
            id: stored.id,
            title: SharedString::from(stored.title),
            provider,
            status: SessionStatus::Idle,
            stats: crate::session::SessionStats::new(),
            messages: stored.messages,
        };
        sessions.restore_session(session);
    }
}

fn select_provider(bridge: &BridgeClient, kind: ProviderKind) -> ProviderInfo {
    bridge
        .providers()
        .iter()
        .find(|provider| provider.kind == kind)
        .cloned()
        .unwrap_or_else(|| ProviderInfo::new("claude-code", "Anthropic", ProviderKind::Anthropic))
}

fn resolve_stored_provider(bridge: &BridgeClient, stored: &StoredSession) -> ProviderInfo {
    if let Some(provider) = bridge.provider_by_id(&stored.provider_id) {
        return provider;
    }
    ProviderInfo::new(
        stored.provider_id.clone(),
        stored.provider_name.clone(),
        ProviderKind::Anthropic,
    )
}

fn friendly_error_message(raw: &str) -> SharedString {
    let message = raw.to_ascii_lowercase();
    if message.contains("missing") && message.contains("api_key") {
        return SharedString::from("Agent credentials are not configured.");
    }
    if message.contains("unauthorized") || message.contains("401") || message.contains("403") {
        return SharedString::from("Agent authentication failed.");
    }
    if message.contains("timeout") {
        return SharedString::from("Agent request timed out.");
    }
    if message.contains("http") {
        return SharedString::from("Agent request failed. Check your network or settings.");
    }
    SharedString::from("Agent error. Check logs for details.")
}
