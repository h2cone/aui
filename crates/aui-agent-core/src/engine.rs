use std::collections::HashMap;
use std::time::SystemTime;

use crate::ProviderEvent;
use crate::{
    Attachment, ConversationMessage, ConversationRole, ProviderInfo, ProviderRequest, Session,
    SessionId, SessionMessage, SessionRole, SessionStats, SessionStatus, WorkingContext,
};

#[derive(Debug, Clone)]
pub struct CoreState {
    sessions: Vec<Session>,
    active_id: Option<SessionId>,
    next_id: u64,
    stream_targets: HashMap<SessionId, usize>,
}

impl CoreState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_id: None,
            next_id: 1,
            stream_targets: HashMap::new(),
        }
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn active_id(&self) -> Option<SessionId> {
        self.active_id
    }

    pub fn active_session(&self) -> Option<&Session> {
        let id = self.active_id?;
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn dispatch(&mut self, command: Command) -> ReduceResult {
        match command {
            Command::RestoreSessions {
                mut sessions,
                active_id,
            } => {
                sessions.sort_by_key(|session| session.id.value());
                self.next_id = sessions
                    .last()
                    .map(|session| session.id.value().saturating_add(1))
                    .unwrap_or(1);
                self.active_id = active_id
                    .filter(|id| sessions.iter().any(|session| session.id == *id))
                    .or_else(|| sessions.first().map(|session| session.id));
                self.sessions = sessions;
                self.stream_targets.clear();
                ReduceResult::default()
            }
            Command::CreateSession {
                title,
                provider,
                model,
            } => {
                let id = SessionId::new(self.next_id);
                self.next_id = self.next_id.saturating_add(1);
                self.sessions.push(Session {
                    id,
                    title,
                    provider,
                    model,
                    status: SessionStatus::Idle,
                    stats: SessionStats::new(),
                    messages: Vec::new(),
                });
                self.active_id = Some(id);
                ReduceResult {
                    effects: vec![Effect::PersistSession { id }],
                }
            }
            Command::SelectSession { id } => {
                if self.sessions.iter().any(|session| session.id == id) {
                    self.active_id = Some(id);
                }
                ReduceResult::default()
            }
            Command::DeleteSession { id } => {
                let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
                    return ReduceResult::default();
                };

                self.sessions.remove(index);
                self.stream_targets.remove(&id);
                if self.active_id == Some(id) {
                    self.active_id = if self.sessions.is_empty() {
                        None
                    } else {
                        let next_index = index.min(self.sessions.len().saturating_sub(1));
                        Some(self.sessions[next_index].id)
                    };
                }

                ReduceResult {
                    effects: vec![Effect::DeletePersistedSession { id }],
                }
            }
            Command::SetSessionProvider { id, provider } => {
                if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                    session.provider = provider;
                    return ReduceResult {
                        effects: vec![Effect::PersistSession { id }],
                    };
                }
                ReduceResult::default()
            }
            Command::SetSessionModel { id, model } => {
                if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                    if !model.trim().is_empty() {
                        session.model = model;
                        return ReduceResult {
                            effects: vec![Effect::PersistSession { id }],
                        };
                    }
                }
                ReduceResult::default()
            }
            Command::StartUserTurn { text } => {
                let Some(id) = self.active_id else {
                    return ReduceResult::default();
                };
                if text.trim().is_empty() {
                    return ReduceResult::default();
                }

                let Some(session) = self.sessions.iter_mut().find(|session| session.id == id)
                else {
                    return ReduceResult::default();
                };

                session.messages.push(SessionMessage {
                    role: SessionRole::User,
                    content: text,
                    timestamp: SystemTime::now(),
                });
                session.messages.push(SessionMessage {
                    role: SessionRole::Assistant,
                    content: String::new(),
                    timestamp: SystemTime::now(),
                });

                let assistant_index = session.messages.len().saturating_sub(1);
                self.stream_targets.insert(id, assistant_index);
                session.status = SessionStatus::Thinking;

                ReduceResult {
                    effects: vec![Effect::PersistSession { id }],
                }
            }
            Command::BeginUserMessage { text } => {
                let Some(id) = self.active_id else {
                    return ReduceResult::default();
                };
                if text.trim().is_empty() {
                    return ReduceResult::default();
                }
                let Some(session) = self.sessions.iter_mut().find(|session| session.id == id)
                else {
                    return ReduceResult::default();
                };
                session.messages.push(SessionMessage {
                    role: SessionRole::Assistant,
                    content: text,
                    timestamp: SystemTime::now(),
                });
                ReduceResult {
                    effects: vec![Effect::PersistSession { id }],
                }
            }
            Command::SubmitUserMessage {
                text,
                attachments,
                context,
            } => {
                let Some(id) = self.active_id else {
                    return ReduceResult::default();
                };
                if text.trim().is_empty() {
                    return ReduceResult::default();
                }

                let Some(session) = self.sessions.iter_mut().find(|session| session.id == id)
                else {
                    return ReduceResult::default();
                };

                session.messages.push(SessionMessage {
                    role: SessionRole::User,
                    content: text.clone(),
                    timestamp: SystemTime::now(),
                });
                session.messages.push(SessionMessage {
                    role: SessionRole::Assistant,
                    content: String::new(),
                    timestamp: SystemTime::now(),
                });

                let assistant_index = session.messages.len().saturating_sub(1);
                self.stream_targets.insert(id, assistant_index);
                session.status = SessionStatus::Thinking;

                let request = ProviderRequest {
                    history: session_to_provider_history(session),
                    text,
                    attachments,
                    context,
                    model: session.model.clone(),
                };

                ReduceResult {
                    effects: vec![
                        Effect::PersistSession { id },
                        Effect::SendProviderRequest {
                            session_id: id,
                            provider: session.provider.clone(),
                            request,
                        },
                    ],
                }
            }
            Command::ReceiveProviderEvent { session_id, event } => {
                let Some(session) = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                else {
                    return ReduceResult::default();
                };

                match event {
                    ProviderEvent::TextDelta(delta) => {
                        if let Some(target_index) = self.stream_targets.get(&session_id).copied() {
                            if let Some(message) = session.messages.get_mut(target_index) {
                                message.content.push_str(&delta);
                            }
                        }
                        ReduceResult::default()
                    }
                    ProviderEvent::ToolStart { name, .. } => {
                        session.status = SessionStatus::Executing { tool: name };
                        ReduceResult::default()
                    }
                    ProviderEvent::ToolResult { name, output } => {
                        session.messages.push(SessionMessage {
                            role: SessionRole::Tool,
                            content: format!("{name}\n{output}"),
                            timestamp: SystemTime::now(),
                        });
                        ReduceResult {
                            effects: vec![Effect::PersistSession { id: session_id }],
                        }
                    }
                    ProviderEvent::TokenUsage { input, output } => {
                        session.stats.tokens_in = session.stats.tokens_in.saturating_add(input);
                        session.stats.tokens_out = session.stats.tokens_out.saturating_add(output);
                        ReduceResult::default()
                    }
                    ProviderEvent::Done => {
                        self.stream_targets.remove(&session_id);
                        session.status = SessionStatus::Idle;
                        ReduceResult {
                            effects: vec![Effect::PersistSession { id: session_id }],
                        }
                    }
                    ProviderEvent::Error(message) => {
                        self.stream_targets.remove(&session_id);
                        session.status = SessionStatus::Error { message };
                        ReduceResult {
                            effects: vec![Effect::PersistSession { id: session_id }],
                        }
                    }
                }
            }
        }
    }
}

impl Default for CoreState {
    fn default() -> Self {
        Self::new()
    }
}

fn session_to_provider_history(session: &Session) -> Vec<ConversationMessage> {
    session
        .messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                SessionRole::User => ConversationRole::User,
                SessionRole::Assistant => ConversationRole::Assistant,
                SessionRole::Tool => ConversationRole::Assistant,
            };
            if msg.content.trim().is_empty() {
                return None;
            }
            Some(ConversationMessage {
                role,
                content: msg.content.clone(),
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub enum Command {
    RestoreSessions {
        sessions: Vec<Session>,
        active_id: Option<SessionId>,
    },
    CreateSession {
        title: String,
        provider: ProviderInfo,
        model: String,
    },
    SelectSession {
        id: SessionId,
    },
    DeleteSession {
        id: SessionId,
    },
    SetSessionProvider {
        id: SessionId,
        provider: ProviderInfo,
    },
    SetSessionModel {
        id: SessionId,
        model: String,
    },
    StartUserTurn {
        text: String,
    },
    BeginUserMessage {
        text: String,
    },
    SubmitUserMessage {
        text: String,
        attachments: Vec<Attachment>,
        context: Option<WorkingContext>,
    },
    ReceiveProviderEvent {
        session_id: SessionId,
        event: ProviderEvent,
    },
}

#[derive(Debug, Clone)]
pub enum Effect {
    PersistSession {
        id: SessionId,
    },
    DeletePersistedSession {
        id: SessionId,
    },
    SendProviderRequest {
        session_id: SessionId,
        provider: ProviderInfo,
        request: ProviderRequest,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ReduceResult {
    pub effects: Vec<Effect>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProviderKind;

    fn provider() -> ProviderInfo {
        ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)
    }

    #[test]
    fn create_select_delete_session_flow() {
        let mut state = CoreState::new();

        state.dispatch(Command::CreateSession {
            title: "one".to_string(),
            provider: provider(),
            model: "gpt-4.1".to_string(),
        });
        state.dispatch(Command::CreateSession {
            title: "two".to_string(),
            provider: provider(),
            model: "gpt-4o".to_string(),
        });
        assert_eq!(state.sessions().len(), 2);

        let first = state.sessions()[0].id;
        let second = state.sessions()[1].id;

        state.dispatch(Command::SelectSession { id: first });
        assert_eq!(state.active_id(), Some(first));

        state.dispatch(Command::DeleteSession { id: first });
        assert_eq!(state.sessions().len(), 1);
        assert_eq!(state.active_id(), Some(second));
    }

    #[test]
    fn submit_user_message_generates_provider_effect() {
        let mut state = CoreState::new();
        state.dispatch(Command::CreateSession {
            title: "one".to_string(),
            provider: provider(),
            model: "gpt-4.1".to_string(),
        });

        let result = state.dispatch(Command::SubmitUserMessage {
            text: "hello".to_string(),
            attachments: Vec::new(),
            context: None,
        });

        assert_eq!(result.effects.len(), 2);
        assert!(matches!(result.effects[0], Effect::PersistSession { .. }));
        assert!(matches!(
            result.effects[1],
            Effect::SendProviderRequest { .. }
        ));

        let session = state.active_session().expect("active session");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, SessionRole::User);
        assert_eq!(session.messages[1].role, SessionRole::Assistant);
        assert_eq!(session.status, SessionStatus::Thinking);
    }

    #[test]
    fn provider_stream_events_update_assistant_message() {
        let mut state = CoreState::new();
        state.dispatch(Command::CreateSession {
            title: "one".to_string(),
            provider: provider(),
            model: "gpt-4.1".to_string(),
        });
        state.dispatch(Command::SubmitUserMessage {
            text: "hello".to_string(),
            attachments: Vec::new(),
            context: None,
        });
        let id = state.active_id().expect("active id");

        state.dispatch(Command::ReceiveProviderEvent {
            session_id: id,
            event: ProviderEvent::TextDelta("world".to_string()),
        });
        state.dispatch(Command::ReceiveProviderEvent {
            session_id: id,
            event: ProviderEvent::TokenUsage {
                input: 11,
                output: 22,
            },
        });
        let done = state.dispatch(Command::ReceiveProviderEvent {
            session_id: id,
            event: ProviderEvent::Done,
        });

        let session = state.session(id).expect("session");
        assert_eq!(session.messages[1].content, "world");
        assert_eq!(session.stats.tokens_in, 11);
        assert_eq!(session.stats.tokens_out, 22);
        assert_eq!(session.status, SessionStatus::Idle);
        assert_eq!(done.effects.len(), 1);
        assert!(matches!(done.effects[0], Effect::PersistSession { id: _ }));
    }

    #[test]
    fn begin_user_message_appends_assistant_text() {
        let mut state = CoreState::new();
        state.dispatch(Command::CreateSession {
            title: "one".to_string(),
            provider: provider(),
            model: "gpt-4.1".to_string(),
        });

        let result = state.dispatch(Command::BeginUserMessage {
            text: "ready".to_string(),
        });

        let session = state.active_session().expect("active session");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, SessionRole::Assistant);
        assert_eq!(session.messages[0].content, "ready");
        assert_eq!(result.effects.len(), 1);
        assert!(matches!(result.effects[0], Effect::PersistSession { .. }));
    }

    #[test]
    fn start_user_turn_sets_streaming_target_and_thinking() {
        let mut state = CoreState::new();
        state.dispatch(Command::CreateSession {
            title: "one".to_string(),
            provider: provider(),
            model: "gpt-4.1".to_string(),
        });

        let result = state.dispatch(Command::StartUserTurn {
            text: "hello".to_string(),
        });

        let session = state.active_session().expect("active");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, SessionRole::User);
        assert_eq!(session.messages[1].role, SessionRole::Assistant);
        assert_eq!(session.status, SessionStatus::Thinking);
        assert_eq!(result.effects.len(), 1);
        assert!(matches!(result.effects[0], Effect::PersistSession { .. }));
    }
}
