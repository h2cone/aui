use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Context, CursorStyle, Entity, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathPromptOptions, ScrollHandle, ScrollWheelEvent, SharedString, Window, div,
    hsla, linear_color_stop, linear_gradient, point, prelude::*, px, rgb,
};

use aui_agent_core::{
    Command as CoreCommand, CoreRuntime, InMemoryCatalog, InMemoryStore,
    ProviderInfo as CoreProviderInfo, ProviderPort, ProviderResponseStream,
    SessionId as CoreSessionId, SessionStatus as CoreSessionStatus,
};
use aui_ai::ProviderGateway;
use aui_ai::{
    Attachment, ConversationMessage, ProviderEvent, ProviderInfo, ProviderKind, SessionStatus,
    UserMessage, WorkingContext,
};

use crate::actions::{AttachFiles, ClearAttachments, ExportSession, Submit};
use crate::config;
use crate::logger;
use crate::model_catalog::{ModelCatalog, fetch_models, now_epoch_secs, should_refresh};
use crate::session::{
    Session, SessionContent, SessionId, SessionManager, SessionRole, SessionStorage,
    StoredDiffDecision, StoredSession,
};
use crate::text_input::TextInput;
use crate::ui;

pub struct AuiApp {
    pub text_input: Entity<TextInput>,
    pub model_input: Entity<TextInput>,
    sessions: SessionManager,
    core_runtime: CoreRuntime,
    gateway: ProviderGateway,
    attachments: Vec<Attachment>,
    new_session_provider_id: Arc<str>,
    storage: SessionStorage,
    stream_targets: HashMap<SessionId, usize>,
    diff_decisions: HashMap<DiffKey, DiffDecision>,
    shell_collapsed: HashMap<ShellKey, bool>,
    conversation_scroll: ScrollHandle,
    conversation_scrollbar_drag: Option<ConversationScrollbarDrag>,
    model_catalog: ModelCatalog,
    model_refreshing: HashSet<ProviderKind>,
    model_refresh_errors: HashMap<ProviderKind, SharedString>,
    model_refresh_user_requested: HashSet<ProviderKind>,
    settings_open: bool,
    settings_show_all_models: bool,
}

#[derive(Clone, Copy, Debug)]
struct ConversationScrollbarDrag {
    grab_offset_in_thumb_px: f32,
    thumb_height_px: f32,
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

fn session_to_provider_history(session: &Session) -> Vec<ConversationMessage> {
    session
        .messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                SessionRole::User => aui_ai::ConversationRole::User,
                SessionRole::Assistant => aui_ai::ConversationRole::Assistant,
                SessionRole::Tool => aui_ai::ConversationRole::Assistant,
            };
            let content = msg.content.as_str();
            if content.trim().is_empty() {
                return None;
            }
            Some(ConversationMessage {
                role,
                content: SharedString::from(content.to_string()),
            })
        })
        .collect()
}

fn map_provider_kind_to_core(kind: ProviderKind) -> aui_agent_core::ProviderKind {
    match kind {
        ProviderKind::Anthropic => aui_agent_core::ProviderKind::Anthropic,
        ProviderKind::OpenAI => aui_agent_core::ProviderKind::OpenAI,
        ProviderKind::Gemini => aui_agent_core::ProviderKind::Gemini,
    }
}

fn map_provider_kind_from_core(kind: aui_agent_core::ProviderKind) -> ProviderKind {
    match kind {
        aui_agent_core::ProviderKind::Anthropic => ProviderKind::Anthropic,
        aui_agent_core::ProviderKind::OpenAI => ProviderKind::OpenAI,
        aui_agent_core::ProviderKind::Gemini => ProviderKind::Gemini,
    }
}

fn map_provider_to_core(provider: &ProviderInfo) -> CoreProviderInfo {
    CoreProviderInfo::new(
        provider.id.as_ref().to_string(),
        provider.name.as_ref().to_string(),
        map_provider_kind_to_core(provider.kind),
    )
}

fn map_provider_from_core(provider: &CoreProviderInfo) -> ProviderInfo {
    ProviderInfo::new(
        provider.id.clone(),
        provider.name.clone(),
        map_provider_kind_from_core(provider.kind),
    )
}

fn map_provider_event_to_core(event: ProviderEvent) -> aui_agent_core::ProviderEvent {
    match event {
        ProviderEvent::TextDelta(delta) => aui_agent_core::ProviderEvent::TextDelta(delta),
        ProviderEvent::ToolStart { name, input } => {
            aui_agent_core::ProviderEvent::ToolStart { name, input }
        }
        ProviderEvent::ToolResult { name, output } => {
            aui_agent_core::ProviderEvent::ToolResult { name, output }
        }
        ProviderEvent::TokenUsage { input, output } => {
            aui_agent_core::ProviderEvent::TokenUsage { input, output }
        }
        ProviderEvent::Done => aui_agent_core::ProviderEvent::Done,
        ProviderEvent::Error(message) => aui_agent_core::ProviderEvent::Error(message),
    }
}

fn map_provider_event_to_core_ref(event: &ProviderEvent) -> aui_agent_core::ProviderEvent {
    match event {
        ProviderEvent::TextDelta(delta) => aui_agent_core::ProviderEvent::TextDelta(delta.clone()),
        ProviderEvent::ToolStart { name, input } => aui_agent_core::ProviderEvent::ToolStart {
            name: name.clone(),
            input: input.clone(),
        },
        ProviderEvent::ToolResult { name, output } => aui_agent_core::ProviderEvent::ToolResult {
            name: name.clone(),
            output: output.clone(),
        },
        ProviderEvent::TokenUsage { input, output } => aui_agent_core::ProviderEvent::TokenUsage {
            input: *input,
            output: *output,
        },
        ProviderEvent::Done => aui_agent_core::ProviderEvent::Done,
        ProviderEvent::Error(message) => aui_agent_core::ProviderEvent::Error(message.clone()),
    }
}

fn map_provider_request_to_user_message(request: aui_agent_core::ProviderRequest) -> UserMessage {
    UserMessage {
        history: request
            .history
            .into_iter()
            .map(|msg| ConversationMessage {
                role: match msg.role {
                    aui_agent_core::ConversationRole::System => aui_ai::ConversationRole::System,
                    aui_agent_core::ConversationRole::User => aui_ai::ConversationRole::User,
                    aui_agent_core::ConversationRole::Assistant => {
                        aui_ai::ConversationRole::Assistant
                    }
                },
                content: SharedString::from(msg.content),
            })
            .collect(),
        text: SharedString::from(request.text),
        attachments: request
            .attachments
            .into_iter()
            .map(|attachment| Attachment {
                name: attachment.name,
                path: attachment.path,
            })
            .collect(),
        context: request
            .context
            .map(|context| WorkingContext { cwd: context.cwd }),
        model: SharedString::from(request.model),
    }
}

#[derive(Clone)]
struct GuiProviderPort {
    gateway: ProviderGateway,
}

impl GuiProviderPort {
    fn new(gateway: ProviderGateway) -> Self {
        Self { gateway }
    }
}

impl ProviderPort for GuiProviderPort {
    fn providers(&self) -> Vec<CoreProviderInfo> {
        self.gateway
            .providers()
            .iter()
            .map(map_provider_to_core)
            .collect()
    }

    fn send(
        &self,
        provider: &CoreProviderInfo,
        request: aui_agent_core::ProviderRequest,
    ) -> ProviderResponseStream {
        let native_provider = map_provider_from_core(provider);
        let user_message = map_provider_request_to_user_message(request);
        let stream = self.gateway.connect(&native_provider).send(user_message);

        let mut events = Vec::new();
        while let Ok(event) = stream.events.recv() {
            let core_event = map_provider_event_to_core(event);
            let terminal = matches!(
                core_event,
                aui_agent_core::ProviderEvent::Done | aui_agent_core::ProviderEvent::Error(_)
            );
            events.push(core_event);
            if terminal {
                break;
            }
        }

        ProviderResponseStream { events }
    }
}

impl AuiApp {
    fn sync_sessions_from_core(&mut self, preserve_streaming: bool) {
        let mut rebuilt = SessionManager::new();
        for core in self.core_runtime.state().sessions() {
            let provider = map_provider_from_core(&core.provider);

            let mut messages = Vec::with_capacity(core.messages.len());
            for message in &core.messages {
                let mut content = message.content.clone();
                if preserve_streaming
                    && core.status == CoreSessionStatus::Thinking
                    && matches!(message.role, aui_agent_core::SessionRole::Assistant)
                {
                    content = content.clone();
                }
                messages.push(crate::session::SessionMessage {
                    role: match message.role {
                        aui_agent_core::SessionRole::User => SessionRole::User,
                        aui_agent_core::SessionRole::Assistant => SessionRole::Assistant,
                        aui_agent_core::SessionRole::Tool => SessionRole::Tool,
                    },
                    content: if preserve_streaming
                        && core.status == CoreSessionStatus::Thinking
                        && matches!(message.role, aui_agent_core::SessionRole::Assistant)
                    {
                        SessionContent::streaming(content)
                    } else {
                        SessionContent::text(content)
                    },
                    timestamp: message.timestamp,
                });
            }

            rebuilt.restore_session(Session {
                id: SessionId::new(core.id.value()),
                title: SharedString::from(core.title.clone()),
                provider,
                model: SharedString::from(core.model.clone()),
                status: match &core.status {
                    CoreSessionStatus::Idle => SessionStatus::Idle,
                    CoreSessionStatus::Thinking => SessionStatus::Thinking,
                    CoreSessionStatus::Executing { tool } => {
                        SessionStatus::Executing { tool: tool.clone() }
                    }
                    CoreSessionStatus::WaitingInput { prompt } => SessionStatus::WaitingInput {
                        prompt: SharedString::from(prompt.clone()),
                    },
                    CoreSessionStatus::Error { message } => SessionStatus::Error {
                        message: SharedString::from(message.clone()),
                    },
                },
                stats: crate::session::SessionStats {
                    tokens_in: core.stats.tokens_in,
                    tokens_out: core.stats.tokens_out,
                    cost_usd: core.stats.cost_usd,
                    started_at: core.stats.started_at,
                },
                messages,
            });
        }

        if let Some(active_id) = self.core_runtime.state().active_id() {
            rebuilt.set_active(SessionId::new(active_id.value()));
        }

        self.sessions = rebuilt;
        self.stream_targets.clear();
        if preserve_streaming {
            for session in self.sessions.sessions() {
                if session.status != SessionStatus::Thinking {
                    continue;
                }
                if let Some(index) = session
                    .messages
                    .iter()
                    .rposition(|msg| matches!(msg.content, SessionContent::Streaming(_)))
                {
                    self.stream_targets.insert(session.id, index);
                }
            }
        }
    }

    fn dispatch_core_and_sync(&mut self, command: CoreCommand, preserve_streaming: bool) {
        if let Err(err) = self.core_runtime.dispatch(command) {
            logger::warn(&format!("core runtime dispatch failed: {err}"));
        }
        self.sync_sessions_from_core(preserve_streaming);
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        logger::info("app init");
        let text_input = cx.new(|cx| TextInput::new(cx));
        let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model name…"));
        let gateway = ProviderGateway::new();
        let config = config::Config::load();
        let storage = SessionStorage::new();
        let model_catalog = ModelCatalog::load();
        Self::new_with(
            cx,
            text_input,
            model_input,
            gateway,
            config,
            storage,
            model_catalog,
            true,
        )
    }

    fn new_with(
        cx: &mut Context<Self>,
        text_input: Entity<TextInput>,
        model_input: Entity<TextInput>,
        gateway: ProviderGateway,
        config: config::Config,
        storage: SessionStorage,
        model_catalog: ModelCatalog,
        start_model_refresh: bool,
    ) -> Self {
        let core_provider = GuiProviderPort::new(gateway.clone());
        let core_store = InMemoryStore::default();
        let core_catalog = InMemoryCatalog::default();
        let core_runtime = CoreRuntime::new(
            Box::new(core_provider),
            Box::new(core_store),
            Box::new(core_catalog),
        );

        let mut sessions = SessionManager::new();
        let conversation_scroll = ScrollHandle::new();
        logger::debug("restoring sessions");
        restore_sessions(&gateway, &storage, &mut sessions, &model_catalog);
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

        let new_session_provider_id = gateway
            .provider_by_id(config.default_provider_id.as_str())
            .map(|provider| provider.id)
            .unwrap_or_else(|| "anthropic".into());

        let mut app = Self {
            text_input,
            model_input,
            sessions,
            core_runtime,
            gateway,
            attachments: Vec::new(),
            new_session_provider_id,
            storage,
            stream_targets: HashMap::new(),
            diff_decisions,
            shell_collapsed: HashMap::new(),
            conversation_scroll,
            conversation_scrollbar_drag: None,
            model_catalog,
            model_refreshing: HashSet::new(),
            model_refresh_errors: HashMap::new(),
            model_refresh_user_requested: HashSet::new(),
            settings_open: false,
            settings_show_all_models: false,
        };

        let restored_core_sessions: Vec<aui_agent_core::Session> = app
            .sessions
            .sessions()
            .iter()
            .map(|session| aui_agent_core::Session {
                id: CoreSessionId::new(session.id.value()),
                title: session.title.as_ref().to_string(),
                provider: map_provider_to_core(&session.provider),
                model: session.model.as_ref().to_string(),
                status: match &session.status {
                    SessionStatus::Idle => CoreSessionStatus::Idle,
                    SessionStatus::Thinking => CoreSessionStatus::Thinking,
                    SessionStatus::Executing { tool } => {
                        CoreSessionStatus::Executing { tool: tool.clone() }
                    }
                    SessionStatus::WaitingInput { prompt } => CoreSessionStatus::WaitingInput {
                        prompt: prompt.as_ref().to_string(),
                    },
                    SessionStatus::Error { message } => CoreSessionStatus::Error {
                        message: message.as_ref().to_string(),
                    },
                },
                stats: aui_agent_core::SessionStats {
                    tokens_in: session.stats.tokens_in,
                    tokens_out: session.stats.tokens_out,
                    cost_usd: session.stats.cost_usd,
                    started_at: session.stats.started_at,
                },
                messages: session
                    .messages
                    .iter()
                    .map(|message| aui_agent_core::SessionMessage {
                        role: match message.role {
                            SessionRole::User => aui_agent_core::SessionRole::User,
                            SessionRole::Assistant => aui_agent_core::SessionRole::Assistant,
                            SessionRole::Tool => aui_agent_core::SessionRole::Tool,
                        },
                        content: message.content.as_str().to_string(),
                        timestamp: message.timestamp,
                    })
                    .collect(),
            })
            .collect();
        let active_core_id = app
            .sessions
            .active_id()
            .map(|id| CoreSessionId::new(id.value()));
        app.dispatch_core_and_sync(
            CoreCommand::RestoreSessions {
                sessions: restored_core_sessions,
                active_id: active_core_id,
            },
            false,
        );

        if start_model_refresh {
            app.start_model_catalog_refresh(cx);
        }

        app
    }

    pub fn sessions(&self) -> &[crate::session::Session] {
        self.sessions.sessions()
    }

    pub fn active_session_id(&self) -> Option<SessionId> {
        self.sessions.active_id()
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.active()
    }

    pub fn conversation_scroll_handle(&self) -> &ScrollHandle {
        &self.conversation_scroll
    }

    pub fn conversation_scrollbar_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_scroll.max_offset().height.to_f64() as f32 <= 0.5 {
            return;
        }

        let delta = event.delta.pixel_delta(window.line_height());
        let current = self.conversation_scroll.offset();
        self.conversation_scroll
            .set_offset(point(current.x, current.y + delta.y));
        cx.notify();
    }

    pub fn conversation_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.conversation_scroll.bounds();
        let viewport_height_px = bounds.size.height.to_f64() as f32;
        let scroll_max_height_px = self.conversation_scroll.max_offset().height.to_f64() as f32;
        if viewport_height_px <= 0.0 || scroll_max_height_px <= 0.5 {
            return;
        }

        let offset_y_px = self.conversation_scroll.offset().y.to_f64() as f32;
        let thumb = crate::ui::scrollbar::compute_thumb(
            viewport_height_px,
            scroll_max_height_px,
            offset_y_px,
        );
        if !thumb.show_thumb {
            return;
        }

        let mouse_y = event.position.y.to_f64() as f32;
        let bounds_top = bounds.top().to_f64() as f32;
        let y_in_bounds = (mouse_y - bounds_top).clamp(0.0, viewport_height_px);

        let in_thumb = y_in_bounds >= thumb.thumb_top_px
            && y_in_bounds <= thumb.thumb_top_px + thumb.thumb_height_px;
        let grab_offset_in_thumb_px = if in_thumb {
            y_in_bounds - thumb.thumb_top_px
        } else {
            thumb.thumb_height_px * 0.5
        };

        let desired_thumb_top = y_in_bounds - grab_offset_in_thumb_px;
        let new_offset_y_px = crate::ui::scrollbar::offset_for_thumb_top(
            viewport_height_px,
            scroll_max_height_px,
            thumb.thumb_height_px,
            desired_thumb_top,
        );
        let current = self.conversation_scroll.offset();
        self.conversation_scroll
            .set_offset(point(current.x, px(new_offset_y_px)));

        self.conversation_scrollbar_drag = Some(ConversationScrollbarDrag {
            grab_offset_in_thumb_px,
            thumb_height_px: thumb.thumb_height_px,
        });
        cx.notify();
    }

    pub fn conversation_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.conversation_scrollbar_drag else {
            return;
        };

        if !event.dragging() {
            self.conversation_scrollbar_drag = None;
            cx.notify();
            return;
        }

        let bounds = self.conversation_scroll.bounds();
        let viewport_height_px = bounds.size.height.to_f64() as f32;
        let scroll_max_height_px = self.conversation_scroll.max_offset().height.to_f64() as f32;
        if viewport_height_px <= 0.0 || scroll_max_height_px <= 0.5 {
            return;
        }

        let mouse_y = event.position.y.to_f64() as f32;
        let bounds_top = bounds.top().to_f64() as f32;
        let y_in_bounds = (mouse_y - bounds_top).clamp(0.0, viewport_height_px);
        let desired_thumb_top = y_in_bounds - drag.grab_offset_in_thumb_px;

        let new_offset_y_px = crate::ui::scrollbar::offset_for_thumb_top(
            viewport_height_px,
            scroll_max_height_px,
            drag.thumb_height_px,
            desired_thumb_top,
        );
        let current = self.conversation_scroll.offset();
        self.conversation_scroll
            .set_offset(point(current.x, px(new_offset_y_px)));
        cx.notify();
    }

    pub fn conversation_scrollbar_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_scrollbar_drag.take().is_some() {
            cx.notify();
        }
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    pub fn settings_show_all_models(&self) -> bool {
        self.settings_show_all_models
    }

    pub fn model_catalog(&self) -> &ModelCatalog {
        &self.model_catalog
    }

    pub fn model_refreshing(&self, kind: ProviderKind) -> bool {
        self.model_refreshing.contains(&kind)
    }

    pub fn model_updated_at(&self, kind: ProviderKind) -> Option<u64> {
        self.model_catalog.updated_at(kind)
    }

    pub fn model_refresh_error(&self, kind: ProviderKind) -> Option<SharedString> {
        self.model_refresh_errors.get(&kind).cloned()
    }

    pub fn select_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        logger::debug(&format!("session select id={}", id.value()));
        self.dispatch_core_and_sync(
            CoreCommand::SelectSession {
                id: CoreSessionId::new(id.value()),
            },
            false,
        );
        self.conversation_scroll.scroll_to_bottom();
        if let Some(session) = self.sessions.session(id) {
            let kind = session.provider.kind;
            if should_refresh(self.model_catalog.updated_at(kind)) {
                self.refresh_model_catalog(kind, false, cx);
            }
        }
        cx.notify();
    }

    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        let next = self.sessions.sessions().len() + 1;
        let title = format!("session-{}", next);
        let provider = self
            .gateway
            .provider_by_id(self.new_session_provider_id.as_ref())
            .unwrap_or_else(|| select_provider(&self.gateway, ProviderKind::Anthropic));
        let model = default_model_for(&self.model_catalog, provider.kind);
        self.dispatch_core_and_sync(
            CoreCommand::CreateSession {
                title,
                provider: map_provider_to_core(&provider),
                model: model.as_ref().to_string(),
            },
            false,
        );
        let Some(id) = self.sessions.active_id() else {
            cx.notify();
            return;
        };
        if let Some(session) = self.sessions.session(id) {
            let kind = session.provider.kind;
            if should_refresh(self.model_catalog.updated_at(kind)) {
                self.refresh_model_catalog(kind, false, cx);
            }
        }
        logger::debug(&format!(
            "session created id={} provider={}",
            id.value(),
            self.sessions
                .session(id)
                .map(|session| session.provider.id.as_ref())
                .unwrap_or("unknown")
        ));
        self.dispatch_core_and_sync(
            CoreCommand::BeginUserMessage {
                text: "New session ready.".to_string(),
            },
            false,
        );
        self.persist_sessions_immediate([id]);
        self.conversation_scroll.scroll_to_bottom();
        cx.notify();
    }

    pub fn new_session_provider_label(&self) -> SharedString {
        self.gateway
            .provider_by_id(self.new_session_provider_id.as_ref())
            .map(|provider| SharedString::from(provider.name.clone()))
            .unwrap_or_else(|| SharedString::from("Anthropic"))
    }

    pub fn cycle_new_session_provider(&mut self, cx: &mut Context<Self>) {
        let providers = self.gateway.providers();
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
        let providers = self.gateway.providers();
        let Some(session) = self.sessions.session(id) else {
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
        let next_provider = providers[next_ix].clone();
        let next_model = default_model_for(&self.model_catalog, next_provider.kind);
        self.dispatch_core_and_sync(
            CoreCommand::SetSessionProvider {
                id: CoreSessionId::new(id.value()),
                provider: map_provider_to_core(&next_provider),
            },
            false,
        );
        self.dispatch_core_and_sync(
            CoreCommand::SetSessionModel {
                id: CoreSessionId::new(id.value()),
                model: next_model.as_ref().to_string(),
            },
            false,
        );

        let kind = next_provider.kind;
        if should_refresh(self.model_catalog.updated_at(kind)) {
            self.refresh_model_catalog(kind, false, cx);
        }
        self.persist_sessions_immediate([id]);
        cx.notify();
    }

    pub fn cycle_session_model(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.session(id) else {
            return;
        };
        let next_model = next_model_for(
            &self.model_catalog,
            session.provider.kind,
            session.model.as_ref(),
        );
        self.dispatch_core_and_sync(
            CoreCommand::SetSessionModel {
                id: CoreSessionId::new(id.value()),
                model: next_model.as_ref().to_string(),
            },
            false,
        );
        self.persist_sessions_immediate([id]);
        cx.notify();
    }

    pub fn apply_model_input(&mut self, cx: &mut Context<Self>) {
        let Some(active_id) = self.sessions.active_id() else {
            return;
        };
        let value = self
            .model_input
            .update(cx, |input, _cx| input.take_submission());
        let Some(value) = value else {
            return;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        self.set_session_model(active_id, trimmed, cx);
    }

    pub fn refresh_active_model_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.active() else {
            return;
        };
        self.refresh_model_catalog(session.provider.kind, true, cx);
    }

    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        if !self.settings_open {
            return;
        }
        self.settings_open = false;
        cx.notify();
    }

    pub fn toggle_settings_show_all_models(&mut self, cx: &mut Context<Self>) {
        self.settings_show_all_models = !self.settings_show_all_models;
        cx.notify();
    }

    pub fn set_session_model(&mut self, id: SessionId, model: &str, cx: &mut Context<Self>) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.sessions.session(id).is_none() {
            return;
        }
        self.dispatch_core_and_sync(
            CoreCommand::SetSessionModel {
                id: CoreSessionId::new(id.value()),
                model: trimmed.to_string(),
            },
            false,
        );
        self.persist_sessions_immediate([id]);
        cx.notify();
    }

    fn refresh_model_catalog(
        &mut self,
        kind: ProviderKind,
        user_requested: bool,
        cx: &mut Context<Self>,
    ) {
        if self.model_refreshing.contains(&kind) {
            return;
        }
        self.model_refreshing.insert(kind);
        self.model_refresh_errors.remove(&kind);
        if user_requested {
            self.model_refresh_user_requested.insert(kind);
        }
        cx.notify();

        let handle = cx.entity().downgrade();
        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<String>, String>>();
        std::thread::spawn(move || {
            let result = fetch_models(kind);
            let _ = tx.send(result);
        });

        gpui::App::spawn(cx, async move |cx| {
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        let _ = handle.update(cx, |view, cx| match result {
                            Ok(models) => view.apply_model_catalog_update(kind, models, cx),
                            Err(err) => view.note_model_catalog_error(kind, &err, cx),
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        gpui::Timer::after(Duration::from_millis(50)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = handle.update(cx, |view, cx| {
                            view.note_model_catalog_error(kind, "refresh cancelled", cx);
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn start_model_catalog_refresh(&mut self, cx: &mut Context<Self>) {
        let providers = [
            ProviderKind::Anthropic,
            ProviderKind::OpenAI,
            ProviderKind::Gemini,
        ];

        for kind in providers {
            if !should_refresh(self.model_catalog.updated_at(kind)) {
                continue;
            }
            self.refresh_model_catalog(kind, false, cx);
        }
    }

    fn apply_model_catalog_update(
        &mut self,
        kind: ProviderKind,
        models: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.model_refreshing.remove(&kind);
        self.model_refresh_errors.remove(&kind);
        self.model_refresh_user_requested.remove(&kind);
        if models.is_empty() {
            return;
        }
        let now = now_epoch_secs();
        self.model_catalog.set_models(kind, models, now);
        self.model_catalog.save();
        cx.notify();
    }

    fn note_model_catalog_error(
        &mut self,
        kind: ProviderKind,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        self.model_refreshing.remove(&kind);
        if self.model_refresh_user_requested.contains(&kind) {
            let friendly = friendly_model_refresh_error(kind, error);
            self.model_refresh_errors
                .insert(kind, SharedString::from(friendly));
        }
        logger::debug(&format!(
            "model catalog refresh failed kind={} error={error}",
            kind.label()
        ));
        cx.notify();
    }

    pub fn delete_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if self.sessions.session(id).is_none() {
            return;
        }
        self.dispatch_core_and_sync(
            CoreCommand::DeleteSession {
                id: CoreSessionId::new(id.value()),
            },
            false,
        );
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

    pub(crate) fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_input.read(cx).focus_handle.is_focused(window) {
            self.apply_model_input(cx);
            return;
        }
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

        let user_text_len = message.as_ref().len();
        logger::debug(&format!(
            "submit message session={} len={} attachments={}",
            active_id.value(),
            user_text_len,
            self.attachments.len()
        ));

        self.dispatch_core_and_sync(
            CoreCommand::StartUserTurn {
                text: message.to_string(),
            },
            true,
        );
        self.conversation_scroll.scroll_to_bottom();
        self.persist_sessions_immediate([active_id]);
        cx.notify();

        let history: Vec<ConversationMessage> = self
            .sessions
            .session(active_id)
            .map(session_to_provider_history)
            .unwrap_or_default();
        let (provider, model) = self
            .sessions
            .session(active_id)
            .map(|session| (session.provider.clone(), session.model.clone()))
            .unwrap_or_else(|| {
                let provider = select_provider(&self.gateway, ProviderKind::Anthropic);
                let model = default_model_for(&self.model_catalog, provider.kind);
                (provider, model)
            });
        let attachments = std::mem::take(&mut self.attachments);
        logger::debug(&format!(
            "provider send session={} provider={}",
            active_id.value(),
            provider.id.as_ref()
        ));
        let stream = self.gateway.connect(&provider).send(UserMessage {
            history,
            text: message,
            attachments,
            context: Some(WorkingContext {
                cwd: std::env::current_dir().ok(),
            }),
            model,
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
                        let mapped_event = map_provider_event_to_core_ref(&event);
                        view.dispatch_core_and_sync(
                            CoreCommand::ReceiveProviderEvent {
                                session_id: CoreSessionId::new(active_id.value()),
                                event: mapped_event,
                            },
                            true,
                        );
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

    pub(crate) fn attach_files(
        &mut self,
        _: &AttachFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_attachment_picker(window, cx);
    }

    pub(crate) fn clear_attachments_action(
        &mut self,
        _: &ClearAttachments,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_attachments(cx);
    }

    pub(crate) fn export_session(
        &mut self,
        _: &ExportSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_active_session(window, cx);
    }

    fn persist_sessions_immediate(&mut self, ids: impl IntoIterator<Item = SessionId>) {
        let mut sessions = Vec::new();
        for id in ids {
            if let Some(session) = self.sessions.session(id).cloned() {
                sessions.push(session);
            }
        }
        if sessions.is_empty() {
            return;
        }

        let storage = self.storage.clone();
        std::thread::spawn(move || {
            for session in sessions {
                if let Err(err) = storage.save_session(&session) {
                    logger::warn(&format!(
                        "session save failed id={} error={}",
                        session.id.value(),
                        err
                    ));
                }
            }
        });
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
        out.push_str(message.content.as_str());
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

pub(crate) fn model_options_for(catalog: &ModelCatalog, kind: ProviderKind) -> Vec<SharedString> {
    let curated = match kind {
        ProviderKind::Anthropic => vec![
            "claude-sonnet-4-5-20250929",
            "claude-opus-4-5-20251101",
            "claude-haiku-4-5-20251001",
            "claude-sonnet-4-5",
            "claude-opus-4-5",
            "claude-haiku-4-5",
            "claude-opus-4",
            "claude-sonnet-4",
            "claude-haiku-4",
            "claude-sonnet-4-20250514",
            "claude-opus-4-20250514",
            "claude-3-7-sonnet-20250219",
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
            "claude-3-opus-20240229",
        ],
        ProviderKind::OpenAI => vec!["gpt-4.1", "gpt-4.1-mini", "gpt-4o", "gpt-4o-mini"],
        ProviderKind::Gemini => vec![
            "gemini-3-pro-preview",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
        ],
    };

    let mut options: Vec<SharedString> = curated.into_iter().map(SharedString::from).collect();

    for item in catalog.models_for(kind) {
        if !options
            .iter()
            .any(|option| option.as_ref() == item.as_ref())
        {
            options.push(item.clone());
        }
    }

    if kind == ProviderKind::Anthropic {
        if let Ok(value) = env::var("ANTHROPIC_MODEL") {
            let trimmed = value.trim();
            if !trimmed.is_empty() && !options.iter().any(|option| option.as_ref() == trimmed) {
                options.insert(0, SharedString::from(trimmed.to_string()));
            }
        }
    }

    options
}

fn default_model_for(catalog: &ModelCatalog, kind: ProviderKind) -> SharedString {
    model_options_for(catalog, kind)
        .into_iter()
        .next()
        .unwrap_or_else(|| SharedString::from("unknown"))
}

fn next_model_for(catalog: &ModelCatalog, kind: ProviderKind, current: &str) -> SharedString {
    let options = model_options_for(catalog, kind);
    if options.is_empty() {
        return SharedString::from(current.to_string());
    }
    let current_ix = options
        .iter()
        .position(|option| option.as_ref() == current)
        .unwrap_or(0);
    let next_ix = (current_ix + 1) % options.len();
    options[next_ix].clone()
}

impl Render for AuiApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::layout::render_app(self, window, cx)
    }
}

#[allow(dead_code)]
fn render_menu_bar(
    view: &AuiApp,
    width: gpui::Pixels,
    height: gpui::Pixels,
    cx: &mut Context<AuiApp>,
) -> gpui::AnyElement {
    let panel_bg = linear_gradient(
        180.0,
        linear_color_stop(rgb(0xffffff), 0.0),
        linear_color_stop(rgb(0xf5f8ff), 1.0),
    );
    let settings_bg = if view.settings_open {
        hsla(0.48, 0.52, 0.9, 0.25)
    } else {
        hsla(0.0, 0.0, 1.0, 0.0)
    };

    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_between()
        .rounded_xl()
        .px(px(14.))
        .bg(panel_bg)
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.06))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(0.0, 0.0, 0.0, 0.12),
            offset: gpui::point(px(0.), px(12.)),
            blur_radius: px(28.),
            spread_radius: px(-14.),
        }])
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(36.))
                        .h(px(36.))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                        .bg(settings_bg)
                        .text_sm()
                        .text_color(rgb(0x0f172a))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.toggle_settings(cx)),
                        )
                        .child("⚙"),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x0b1220))
                        .child("AUI Desktop"),
                ),
        )
        .child(div().text_xs().text_color(rgb(0x64748b)).child("Menu"))
        .into_any_element()
}

#[allow(dead_code)]
fn render_settings_panel(
    view: &AuiApp,
    active_session: Option<&Session>,
    cx: &mut Context<AuiApp>,
) -> gpui::AnyElement {
    let panel_bg = linear_gradient(
        180.0,
        linear_color_stop(rgb(0xffffff), 0.0),
        linear_color_stop(rgb(0xf7faff), 1.0),
    );

    let mut panel = div()
        .h(px(340.))
        .rounded_xl()
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.06))
        .bg(panel_bg)
        .p(px(16.))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(0.0, 0.0, 0.0, 0.12),
            offset: gpui::point(px(0.), px(10.)),
            blur_radius: px(26.),
            spread_radius: px(-12.),
        }])
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x0b1220))
                        .child("Settings"),
                )
                .child(
                    div()
                        .px(px(8.))
                        .py(px(2.))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(rgb(0x64748b))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.close_settings(cx)),
                        )
                        .child("x"),
                ),
        );

    let Some(session) = active_session else {
        return panel
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x5b6777))
                    .child("Select a session to configure provider and model."),
            )
            .into_any_element();
    };

    let session_id = session.id;
    let kind = session.provider.kind;
    let refreshing = view.model_refreshing.contains(&kind);
    let updated_at = view.model_catalog.updated_at(kind);
    let error = view.model_refresh_errors.get(&kind).cloned();
    let options = model_options_for(&view.model_catalog, kind);
    let show_all = view.settings_show_all_models;
    let list_limit = 10usize;
    let show_toggle = options.len() > list_limit;

    let refresh_label = if refreshing {
        "Refreshing…".to_string()
    } else {
        "↻ Refresh".to_string()
    };

    let meta = updated_at
        .map(|ts| format!("Updated {}", format_age(ts)))
        .unwrap_or_else(|| "Using built-in list".to_string());

    panel = panel.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().text_color(rgb(0x64748b)).child("Provider"))
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                    .bg(hsla(0.0, 0.0, 1.0, 0.0))
                    .text_xs()
                    .text_color(rgb(0x0f172a))
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _, _, cx| {
                            view.cycle_session_provider(session_id, cx)
                        }),
                    )
                    .child(SharedString::from(session.provider.name.clone())),
            ),
    );

    panel = panel.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_xs().text_color(rgb(0x64748b)).child("Model"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_xs().text_color(rgb(0x64748b)).child(meta))
                    .child({
                        let style = div()
                            .px(px(10.))
                            .py(px(4.))
                            .rounded_full()
                            .border_1()
                            .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                            .text_xs()
                            .cursor(CursorStyle::PointingHand);

                        if refreshing {
                            style
                                .bg(hsla(0.48, 0.52, 0.9, 0.18))
                                .text_color(rgb(0x475569))
                                .child(refresh_label)
                        } else {
                            style
                                .bg(hsla(0.48, 0.52, 0.9, 0.22))
                                .text_color(rgb(0x0f172a))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.refresh_active_model_catalog(cx)
                                    }),
                                )
                                .child(refresh_label)
                        }
                    }),
            ),
    );

    if let Some(err) = error {
        if updated_at.is_none() {
            panel = panel.child(
                div()
                    .rounded_lg()
                    .border_1()
                    .border_color(hsla(0.0, 0.7, 0.55, 0.18))
                    .bg(hsla(0.0, 0.75, 0.98, 0.55))
                    .px(px(12.))
                    .py(px(8.))
                    .text_xs()
                    .text_color(rgb(0x7a1f1f))
                    .child(err),
            );
        }
    }

    panel = panel.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                    .bg(hsla(0.0, 0.0, 1.0, 0.0))
                    .text_xs()
                    .text_color(rgb(0x0f172a))
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _, _, cx| view.cycle_session_model(session_id, cx)),
                    )
                    .child(format!(
                        "Next: {}",
                        short_model_label(session.model.as_ref(), 28)
                    )),
            )
            .child({
                if show_toggle {
                    let label = if show_all {
                        "Show fewer".to_string()
                    } else {
                        format!("Show all ({})", options.len())
                    };
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                        .bg(hsla(0.0, 0.0, 1.0, 0.0))
                        .text_xs()
                        .text_color(rgb(0x0f172a))
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| view.toggle_settings_show_all_models(cx)),
                        )
                        .child(label)
                } else {
                    div()
                }
            }),
    );

    let shown: Vec<SharedString> = if show_all {
        options
    } else {
        options.into_iter().take(list_limit).collect()
    };

    let mut model_rows = Vec::new();
    for option in shown {
        let label = short_model_label(option.as_ref(), 56);
        let option_for_click = option.clone();
        let is_active = option.as_ref() == session.model.as_ref();
        let bg = if is_active {
            hsla(0.48, 0.52, 0.9, 0.25)
        } else {
            hsla(0.0, 0.0, 1.0, 0.0)
        };
        let text = if is_active {
            rgb(0x0b1220)
        } else {
            rgb(0x334155)
        };
        model_rows.push(
            div()
                .px(px(10.))
                .py(px(6.))
                .rounded_lg()
                .border_1()
                .border_color(hsla(0.0, 0.0, 0.0, 0.06))
                .bg(bg)
                .text_xs()
                .text_color(text)
                .cursor(CursorStyle::PointingHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _, _, cx| {
                        view.set_session_model(session_id, option_for_click.as_ref(), cx)
                    }),
                )
                .child(label)
                .into_any_element(),
        );
    }

    panel = panel.child(div().flex().flex_col().gap_2().children(model_rows));

    panel = panel.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x64748b))
                    .child("Custom model"),
            )
            .child(div().w(px(260.)).child(view.model_input.clone()))
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                    .bg(hsla(0.0, 0.0, 1.0, 0.0))
                    .text_xs()
                    .text_color(rgb(0x0f172a))
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| view.apply_model_input(cx)),
                    )
                    .child("Apply"),
            ),
    );

    panel.into_any_element()
}

fn restore_sessions(
    gateway: &ProviderGateway,
    storage: &SessionStorage,
    sessions: &mut SessionManager,
    model_catalog: &ModelCatalog,
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
        let provider = resolve_stored_provider(gateway, &stored);
        let model = stored
            .provider_model
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| SharedString::from(value.to_string()))
            .unwrap_or_else(|| default_model_for(model_catalog, provider.kind));
        let session = crate::session::Session {
            id: stored.id,
            title: SharedString::from(stored.title),
            provider,
            model,
            status: SessionStatus::Idle,
            stats: crate::session::SessionStats::new(),
            messages: stored.messages,
        };
        sessions.restore_session(session);
    }
}

fn select_provider(gateway: &ProviderGateway, kind: ProviderKind) -> ProviderInfo {
    gateway
        .providers()
        .iter()
        .find(|provider| provider.kind == kind)
        .cloned()
        .unwrap_or_else(|| {
            ProviderInfo::new("anthropic", "Anthropic (Claude)", ProviderKind::Anthropic)
        })
}

fn resolve_stored_provider(gateway: &ProviderGateway, stored: &StoredSession) -> ProviderInfo {
    if let Some(provider) = gateway.provider_by_id(&stored.provider_id) {
        return provider;
    }
    // If the stored provider no longer exists (e.g. removed), fall back to a supported one.
    select_provider(gateway, ProviderKind::Anthropic)
}

fn friendly_model_refresh_error(kind: ProviderKind, raw: &str) -> String {
    let message = raw.to_ascii_lowercase();
    if message.contains("missing") && message.contains("api_key") {
        let var = match kind {
            ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            ProviderKind::OpenAI => "OPENAI_API_KEY",
            ProviderKind::Gemini => "GEMINI_API_KEY or GOOGLE_API_KEY",
        };
        return format!("Configure {var} to refresh models.");
    }
    if message.contains("timeout") {
        return "Model refresh timed out. Using cached/built-in list.".to_string();
    }
    if message.contains("http") || message.contains("unauthorized") || message.contains("401") {
        return "Model refresh failed. Using cached/built-in list.".to_string();
    }
    "Model refresh failed. Using cached/built-in list.".to_string()
}

#[allow(dead_code)]
fn format_age(epoch_secs: u64) -> String {
    let now = now_epoch_secs();
    let delta = now.saturating_sub(epoch_secs);
    if delta < 20 {
        return "just now".to_string();
    }
    if delta < 60 {
        return format!("{delta}s ago");
    }
    let minutes = delta / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    format!("{days}d ago")
}

#[allow(dead_code)]
fn short_model_label(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aui_ai::ConversationRole;
    use gpui::TestAppContext;
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn seeded_model_catalog() -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        let now = now_epoch_secs();
        for kind in [
            ProviderKind::Anthropic,
            ProviderKind::OpenAI,
            ProviderKind::Gemini,
        ] {
            catalog.set_models(kind, Vec::new(), now);
        }
        catalog
    }

    #[test]
    fn session_to_provider_history_maps_tool_as_assistant() {
        let session = Session {
            id: SessionId::new(1),
            title: SharedString::from("Test".to_string()),
            provider: ProviderInfo::new(
                Arc::from("anthropic"),
                Arc::from("Anthropic"),
                ProviderKind::Anthropic,
            ),
            model: SharedString::from("test".to_string()),
            status: SessionStatus::Idle,
            stats: crate::session::SessionStats::new(),
            messages: vec![crate::session::SessionMessage {
                role: SessionRole::Tool,
                content: SessionContent::text("Tool output: read_file\nhello"),
                timestamp: SystemTime::now(),
            }],
        };

        let history = session_to_provider_history(&session);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, ConversationRole::Assistant);
    }

    #[gpui::test]
    fn app_renders_without_sessions(cx: &mut TestAppContext) {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });

        app.update_in(cx, |view, window, cx| {
            let _ = view.render(window, cx).into_any_element();
        });
    }

    #[gpui::test]
    fn app_renders_with_settings_open(cx: &mut TestAppContext) {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });

        app.update_in(cx, |view, window, cx| {
            view.toggle_settings(cx);
            let _ = view.render(window, cx).into_any_element();
        });
    }

    #[gpui::test]
    fn app_renders_with_session_and_settings(cx: &mut TestAppContext) {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });

        app.update_in(cx, |view, window, cx| {
            view.new_session(cx);
            view.toggle_settings(cx);
            let _ = view.render(window, cx).into_any_element();
        });
    }

    #[gpui::test]
    fn gui_main_path_uses_runtime_for_session_lifecycle(cx: &mut TestAppContext) {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });

        app.update_in(cx, |view, _window, cx| {
            view.new_session(cx);
            assert_eq!(view.sessions().len(), 1);
            let id = view.active_session_id().expect("active id");
            view.delete_session(id, cx);
            assert!(view.sessions().is_empty());
        });
    }

    #[gpui::test]
    fn gui_model_provider_changes_go_through_runtime(cx: &mut TestAppContext) {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });

        app.update_in(cx, |view, _window, cx| {
            view.new_session(cx);
            let id = view.active_session_id().expect("active id");
            let before = view
                .active_session()
                .expect("active")
                .provider
                .id
                .as_ref()
                .to_string();
            view.cycle_session_provider(id, cx);
            let after = view
                .active_session()
                .expect("active")
                .provider
                .id
                .as_ref()
                .to_string();
            assert_ne!(before, after);

            view.set_session_model(id, "runtime-model", cx);
            assert_eq!(
                view.active_session().expect("active").model.as_ref(),
                "runtime-model"
            );
        });
    }

    #[gpui::test]
    fn conversation_scrolls_and_scrollbar_drives_scroll(cx: &mut TestAppContext) {
        use gpui::{Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, point, px, size};

        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let config = config::Config {
            default_provider_id: "anthropic".to_string(),
            debug: false,
        };
        let catalog = seeded_model_catalog();

        let (app, cx) = cx.add_window_view(|_, cx| {
            let text_input = cx.new(|cx| TextInput::new(cx));
            let model_input = cx.new(|cx| TextInput::new_compact(cx, "Custom model"));
            let gateway = ProviderGateway::new();
            AuiApp::new_with(
                cx,
                text_input,
                model_input,
                gateway,
                config.clone(),
                storage.clone(),
                catalog.clone(),
                false,
            )
        });
        cx.simulate_resize(size(px(900.), px(500.)));

        app.update_in(cx, |view, _, cx| {
            view.new_session(cx);
            let id = view.active_session_id().expect("active session");
            for ix in 0..80usize {
                view.sessions.append_message(
                    id,
                    SessionRole::Assistant,
                    format!("message {ix}\n\n{}\n", "lorem ipsum ".repeat(20)),
                );
            }
            cx.notify();
        });

        cx.draw(point(px(0.), px(0.)), size(px(900.), px(500.)), |_, _| {
            app.clone()
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        assert_eq!(viewport, size(px(900.), px(500.)));

        let max_offset = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.max_offset().height.to_f64() as f32
        });
        assert!(max_offset > 0.5, "expected conversation to be scrollable");

        let mid_offset_y = -max_offset * 0.5;
        app.update_in(cx, |view, _, cx| {
            view.conversation_scroll
                .set_offset(point(px(0.), px(mid_offset_y)));
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(900.), px(500.)), |_, _| {
            app.clone()
        });

        let scroll_bounds = cx
            .debug_bounds("conversation-scroll")
            .expect("conversation-scroll bounds");
        let scrollbar_bounds = cx
            .debug_bounds("conversation-scrollbar")
            .expect("conversation-scrollbar bounds");
        assert!(
            scrollbar_bounds.right() <= viewport.width,
            "expected scrollbar to be within viewport (scrollbar_bounds={scrollbar_bounds:?} viewport={viewport:?})"
        );
        assert!(
            scroll_bounds.right() <= viewport.width,
            "expected conversation-scroll to be within viewport (scroll_bounds={scroll_bounds:?} viewport={viewport:?})"
        );

        let scroll_pos = point(scroll_bounds.left() + px(4.), scroll_bounds.top() + px(4.));
        assert!(
            scroll_bounds.contains(&scroll_pos),
            "expected scroll_pos to be within conversation-scroll bounds"
        );
        let before_scroll = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });
        cx.simulate_event(ScrollWheelEvent {
            position: scroll_pos,
            delta: ScrollDelta::Pixels(point(px(0.), px(120.))),
            ..Default::default()
        });

        let after_scroll = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });
        assert!(
            after_scroll > before_scroll,
            "expected wheel scroll to move offset (before_scroll={before_scroll} after_scroll={after_scroll})"
        );

        let start_drag = scrollbar_bounds.center();
        assert!(
            scrollbar_bounds.contains(&start_drag),
            "expected start_drag to be within conversation-scrollbar bounds (bounds={scrollbar_bounds:?} start_drag={start_drag:?})"
        );

        let before_bar_wheel = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });

        cx.simulate_event(ScrollWheelEvent {
            position: start_drag,
            delta: ScrollDelta::Pixels(point(px(0.), px(60.))),
            ..Default::default()
        });
        let after_bar_wheel = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });
        assert!(
            after_bar_wheel > before_bar_wheel,
            "expected wheel over scrollbar to scroll (before={before_bar_wheel} after={after_bar_wheel})"
        );
        let before_drag = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });
        assert!(
            !cx.read_entity(&app, |view, _| view.conversation_scrollbar_drag.is_some()),
            "expected scrollbar drag state to be empty initially"
        );

        cx.simulate_mouse_down(start_drag, MouseButton::Left, Modifiers::none());
        assert!(
            cx.read_entity(&app, |view, _| view.conversation_scrollbar_drag.is_some()),
            "expected mouse down on scrollbar to start dragging"
        );
        cx.simulate_mouse_move(
            point(start_drag.x, start_drag.y + px(120.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(start_drag.x, start_drag.y + px(120.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert!(
            !cx.read_entity(&app, |view, _| view.conversation_scrollbar_drag.is_some()),
            "expected mouse up to end dragging"
        );

        let after_drag = cx.read_entity(&app, |view, _| {
            view.conversation_scroll.offset().y.to_f64() as f32
        });
        assert!(
            after_drag < before_drag,
            "expected drag to change offset (before_drag={before_drag} after_drag={after_drag})"
        );
    }
}
