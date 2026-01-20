use std::time::{Duration, Instant, SystemTime};

use crate::agent::{AgentInfo, AgentStatus};

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
    pub title: String,
    pub agent: AgentInfo,
    pub status: AgentStatus,
    pub stats: SessionStats,
    pub messages: Vec<SessionMessage>,
}

impl Session {
    pub const fn status_label(&self) -> &'static str {
        match self.status {
            AgentStatus::Idle => "Idle",
            AgentStatus::Thinking => "Thinking",
            AgentStatus::Executing { .. } => "Executing",
            AgentStatus::WaitingInput { .. } => "Waiting",
            AgentStatus::Error { .. } => "Error",
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

    pub fn create_session(&mut self, title: impl Into<String>, agent: AgentInfo) -> SessionId {
        let id = SessionId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let session = Session {
            id,
            title: title.into(),
            agent,
            status: AgentStatus::Idle,
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

    pub fn set_status(&mut self, id: SessionId, status: AgentStatus) {
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
}
