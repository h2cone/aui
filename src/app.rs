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

use crate::actions::{AttachFiles, ClearAttachments, ExportSession, Submit};
use crate::config;
use crate::logger;
use crate::model_catalog::{ModelCatalog, fetch_models, now_epoch_secs, should_refresh};
use crate::providers::gateway::ProviderGateway;
use crate::providers::{
    Attachment, ProviderEvent, ProviderInfo, ProviderKind, SessionStatus, UserMessage,
    WorkingContext,
};
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
    gateway: ProviderGateway,
    attachments: Vec<Attachment>,
    new_session_provider_id: Arc<str>,
    storage: SessionStorage,
    stream_targets: HashMap<SessionId, usize>,
    dirty_sessions: HashSet<SessionId>,
    persist_generation: u64,
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

impl AuiApp {
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
            gateway,
            attachments: Vec::new(),
            new_session_provider_id,
            storage,
            stream_targets: HashMap::new(),
            dirty_sessions: HashSet::new(),
            persist_generation: 0,
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
        self.sessions.set_active(id);
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
        let id = self.sessions.create_session(title, provider, model);
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
        self.sessions
            .append_message(id, SessionRole::Assistant, "New session ready.");
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
        session.model = default_model_for(&self.model_catalog, session.provider.kind);
        let kind = session.provider.kind;
        if should_refresh(self.model_catalog.updated_at(kind)) {
            self.refresh_model_catalog(kind, false, cx);
        }
        self.persist_sessions_immediate([id]);
        cx.notify();
    }

    pub fn cycle_session_model(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.session_mut(id) else {
            return;
        };
        session.model = next_model_for(
            &self.model_catalog,
            session.provider.kind,
            session.model.as_ref(),
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
        if let Some(session) = self.sessions.session_mut(active_id) {
            session.model = SharedString::from(trimmed.to_string());
        }
        self.persist_sessions_immediate([active_id]);
        cx.notify();
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
        let Some(session) = self.sessions.session_mut(id) else {
            return;
        };
        session.model = SharedString::from(trimmed.to_string());
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
        self.sessions
            .append_message(active_id, SessionRole::User, message.clone());

        let assistant_index = self.sessions.push_message(
            active_id,
            SessionRole::Assistant,
            SessionContent::streaming(""),
        );
        if let Some(index) = assistant_index {
            self.stream_targets.insert(active_id, index);
        }
        self.conversation_scroll.scroll_to_bottom();

        self.sessions.set_status(active_id, SessionStatus::Thinking);
        self.persist_sessions_immediate([active_id]);
        cx.notify();

        let (provider, model) = self
            .sessions
            .session(active_id)
            .map(|session| (session.provider.clone(), session.model.clone()))
            .unwrap_or_else(|| {
                let provider = select_provider(&self.gateway, ProviderKind::Anthropic);
                let model = default_model_for(&self.model_catalog, provider.kind);
                (provider, model)
            });
        logger::debug(&format!(
            "provider send session={} provider={}",
            active_id.value(),
            provider.id.as_ref()
        ));
        let attachments = std::mem::take(&mut self.attachments);
        let stream = self.gateway.connect(&provider).send(UserMessage {
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
                        view.apply_stream_event(active_id, event, cx);
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

    fn apply_stream_event(&mut self, id: SessionId, event: ProviderEvent, cx: &mut Context<Self>) {
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
                self.persist_session_debounced(id, cx);
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
                    format!("Tool call: {name}\n{input}"),
                );
                self.persist_sessions_immediate([id]);
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
                    format!("Tool output: {name}\n{output}"),
                );
                self.persist_sessions_immediate([id]);
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
                self.persist_session_debounced(id, cx);
            }
            ProviderEvent::Done => {
                logger::debug(&format!("stream done session={}", id.value()));
                if let Some(index) = self.stream_targets.remove(&id) {
                    if let Some(session) = self.sessions.session_mut(id) {
                        if let Some(message) = session.messages.get_mut(index) {
                            message.content.finalize();
                        }
                    }
                }
                self.sessions.set_status(id, SessionStatus::Idle);
                self.persist_sessions_immediate([id]);
                self.conversation_scroll.scroll_to_bottom();
            }
            ProviderEvent::Error(message) => {
                logger::warn(&format!(
                    "stream error session={} message={}",
                    id.value(),
                    message.as_str()
                ));
                if let Some(index) = self.stream_targets.remove(&id) {
                    if let Some(session) = self.sessions.session_mut(id) {
                        if let Some(message) = session.messages.get_mut(index) {
                            message.content.finalize();
                        }
                    }
                }
                if let Some(fallback) = self.handle_invalid_model(id, &message, cx) {
                    self.sessions
                        .set_status(id, SessionStatus::Error { message: fallback });
                    self.persist_sessions_immediate([id]);
                    self.conversation_scroll.scroll_to_bottom();
                } else {
                    let user_message = friendly_error_message(&message);
                    self.sessions.set_status(
                        id,
                        SessionStatus::Error {
                            message: user_message,
                        },
                    );
                    self.persist_sessions_immediate([id]);
                    self.conversation_scroll.scroll_to_bottom();
                }
            }
        }
    }

    fn handle_invalid_model(
        &mut self,
        id: SessionId,
        raw: &str,
        cx: &mut Context<Self>,
    ) -> Option<SharedString> {
        if !is_model_unavailable(raw) {
            return None;
        }
        let session = self.sessions.session_mut(id)?;
        let current = session.model.clone();
        let options = model_options_for(&self.model_catalog, session.provider.kind);
        let fallback = options
            .into_iter()
            .find(|option| option.as_ref() != current.as_ref())
            .unwrap_or_else(|| default_model_for(&self.model_catalog, session.provider.kind));
        if fallback.as_ref() == current.as_ref() {
            return None;
        }
        let user_message = SharedString::from(format!(
            "Model '{}' is unavailable. Switched to '{}'. Please retry.",
            current.as_ref(),
            fallback.as_ref()
        ));
        session.model = fallback;
        self.persist_sessions_immediate([id]);
        cx.notify();
        Some(user_message)
    }

    fn persist_session_debounced(&mut self, id: SessionId, cx: &mut Context<Self>) {
        const DEBOUNCE: Duration = Duration::from_millis(450);

        self.dirty_sessions.insert(id);
        self.persist_generation = self.persist_generation.wrapping_add(1);
        let generation = self.persist_generation;
        let handle = cx.entity().downgrade();
        gpui::App::spawn(cx, async move |cx| {
            gpui::Timer::after(DEBOUNCE).await;
            let _ = handle.update(cx, |view, _cx| {
                if view.persist_generation != generation {
                    return;
                }
                let ids: Vec<SessionId> = view.dirty_sessions.drain().collect();
                view.persist_sessions_immediate(ids);
            });
        })
        .detach();
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
        ProviderKind::Gemini => (0.0, 0.0),
    };

    let prefix = match kind {
        ProviderKind::Anthropic => "ANTHROPIC",
        ProviderKind::OpenAI => "OPENAI",
        ProviderKind::Gemini => "GEMINI",
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

fn friendly_error_message(raw: &str) -> SharedString {
    let message = raw.to_ascii_lowercase();
    if message.contains("missing") && message.contains("api_key") {
        return SharedString::from("Provider credentials are not configured.");
    }
    if message.contains("unauthorized") || message.contains("401") || message.contains("403") {
        return SharedString::from("Provider authentication failed.");
    }
    if message.contains("timeout") {
        return SharedString::from("Provider request timed out.");
    }
    if message.contains("http") {
        return SharedString::from("Provider request failed. Check your network or settings.");
    }
    SharedString::from("Provider error. Check logs for details.")
}

fn is_model_unavailable(raw: &str) -> bool {
    let message = raw.to_ascii_lowercase();
    if !message.contains("model") {
        return false;
    }
    message.contains("not found")
        || message.contains("does not exist")
        || message.contains("invalid")
        || message.contains("unknown")
        || message.contains("unsupported")
        || message.contains("not available")
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
    use gpui::TestAppContext;
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
