use std::time::{Duration, Instant, SystemTime};

use gpui::SharedString;

use crate::providers::{ProviderInfo, SessionStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub enum SessionRole {
    User,
    Assistant,
    Tool,
}

impl SessionRole {
    pub const fn as_str(&self) -> &'static str {
        match self {
            SessionRole::User => "user",
            SessionRole::Assistant => "assistant",
            SessionRole::Tool => "tool",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "user" => Some(SessionRole::User),
            "assistant" => Some(SessionRole::Assistant),
            "tool" => Some(SessionRole::Tool),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SessionMessage {
    pub role: SessionRole,
    pub content: String,
    pub timestamp: SystemTime,
}

#[derive(Clone, Debug)]
pub struct SessionStats {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f32,
    pub started_at: Instant,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            started_at: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: SessionId,
    pub title: SharedString,
    pub provider: ProviderInfo,
    pub model: SharedString,
    pub status: SessionStatus,
    pub stats: SessionStats,
    pub messages: Vec<SessionMessage>,
}

impl Session {
    pub const fn status_label(&self) -> &'static str {
        match self.status {
            SessionStatus::Idle => "Idle",
            SessionStatus::Thinking => "Thinking",
            SessionStatus::Executing { .. } => "Executing",
            SessionStatus::WaitingInput { .. } => "Waiting",
            SessionStatus::Error { .. } => "Error",
        }
    }
}

pub struct SessionManager {
    sessions: Vec<Session>,
    active_id: Option<SessionId>,
    next_id: u64,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_id: None,
            next_id: 1,
        }
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn active_id(&self) -> Option<SessionId> {
        self.active_id
    }

    pub fn active(&self) -> Option<&Session> {
        let id = self.active_id?;
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn active_mut(&mut self) -> Option<&mut Session> {
        let id = self.active_id?;
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    pub fn set_active(&mut self, id: SessionId) {
        if self.sessions.iter().any(|session| session.id == id) {
            self.active_id = Some(id);
        }
    }

    pub fn create_session(
        &mut self,
        title: impl Into<SharedString>,
        provider: ProviderInfo,
        model: impl Into<SharedString>,
    ) -> SessionId {
        let id = SessionId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let session = Session {
            id,
            title: title.into(),
            provider,
            model: model.into(),
            status: SessionStatus::Idle,
            stats: SessionStats::new(),
            messages: Vec::new(),
        };
        self.sessions.push(session);
        self.active_id = Some(id);
        id
    }

    pub fn push_message(
        &mut self,
        id: SessionId,
        role: SessionRole,
        content: String,
    ) -> Option<usize> {
        let session = self.sessions.iter_mut().find(|session| session.id == id)?;
        session.messages.push(SessionMessage {
            role,
            content,
            timestamp: SystemTime::now(),
        });
        Some(session.messages.len().saturating_sub(1))
    }

    pub fn append_message(&mut self, id: SessionId, role: SessionRole, content: String) {
        let _ = self.push_message(id, role, content);
    }

    pub fn set_status(&mut self, id: SessionId, status: SessionStatus) {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.status = status;
        }
    }

    pub fn bump_usage(&mut self, id: SessionId, input: u32, output: u32, cost: f32) {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.stats.tokens_in = session.stats.tokens_in.saturating_add(input);
            session.stats.tokens_out = session.stats.tokens_out.saturating_add(output);
            session.stats.cost_usd += cost;
        }
    }

    pub fn restore_session(&mut self, session: Session) {
        let id_value = session.id.value();
        if self.active_id.is_none() {
            self.active_id = Some(session.id);
        }
        if self.next_id <= id_value {
            self.next_id = id_value.saturating_add(1);
        }
        self.sessions.push(session);
    }

    pub fn remove_session(&mut self, id: SessionId) -> bool {
        let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
            return false;
        };
        self.sessions.remove(index);
        if self.active_id == Some(id) {
            if self.sessions.is_empty() {
                self.active_id = None;
            } else {
                let next_index = index.min(self.sessions.len() - 1);
                self.active_id = Some(self.sessions[next_index].id);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderInfo, ProviderKind};

    fn sample_provider() -> ProviderInfo {
        ProviderInfo::new("test", "Test Provider", ProviderKind::Anthropic)
    }

    #[test]
    fn create_session_sets_active() {
        let mut manager = SessionManager::new();
        let id = manager.create_session("alpha", sample_provider(), "model-x");
        assert_eq!(manager.active_id(), Some(id));
        assert_eq!(manager.sessions().len(), 1);
    }

    #[test]
    fn push_message_returns_index() {
        let mut manager = SessionManager::new();
        let id = manager.create_session("alpha", sample_provider(), "model-x");
        let index = manager.push_message(id, SessionRole::User, "hello".to_string());
        assert_eq!(index, Some(0));
        let session = manager.session(id).expect("session missing");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "hello");
    }

    #[test]
    fn bump_usage_saturates_tokens() {
        let mut manager = SessionManager::new();
        let id = manager.create_session("alpha", sample_provider(), "model-x");
        manager.bump_usage(id, u32::MAX - 1, u32::MAX - 2, 0.0);
        manager.bump_usage(id, 10, 20, 1.5);
        let session = manager.session(id).expect("session missing");
        assert_eq!(session.stats.tokens_in, u32::MAX);
        assert_eq!(session.stats.tokens_out, u32::MAX);
        assert!((session.stats.cost_usd - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn remove_session_updates_active() {
        let mut manager = SessionManager::new();
        let first = manager.create_session("alpha", sample_provider(), "model-x");
        let second = manager.create_session("beta", sample_provider(), "model-y");
        assert_eq!(manager.active_id(), Some(second));
        assert!(manager.remove_session(second));
        assert_eq!(manager.active_id(), Some(first));
        assert!(manager.remove_session(first));
        assert_eq!(manager.active_id(), None);
    }
}
