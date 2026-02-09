use aui_core_domain::{
    Attachment, ConversationMessage, ConversationRole, ProviderKind, Session, SessionId,
    SessionMessage, SessionRole, SessionStatus, WorkingContext,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeSession {
    pub id: u64,
    pub title: String,
    pub provider_id: String,
    pub provider_name: String,
    pub provider_kind: String,
    pub model: String,
    pub status: BridgeSessionStatus,
    pub messages: Vec<BridgeSessionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeSessionMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum BridgeSessionStatus {
    Idle,
    Thinking,
    Executing { tool: String },
    WaitingInput { prompt: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum BridgeCommand {
    CreateSession {
        title: String,
        provider_id: String,
        provider_name: String,
        provider_kind: String,
        model: String,
    },
    SelectSession {
        id: u64,
    },
    DeleteSession {
        id: u64,
    },
    SubmitUserMessage {
        text: String,
        attachments: Vec<BridgeAttachment>,
        context_cwd: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeAttachment {
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum BridgeEvent {
    Snapshot {
        active_id: Option<u64>,
        sessions: Vec<BridgeSession>,
    },
    Ack,
    Error {
        message: String,
    },
}

pub fn to_bridge_session(session: &Session) -> BridgeSession {
    BridgeSession {
        id: session.id.value(),
        title: session.title.clone(),
        provider_id: session.provider.id.clone(),
        provider_name: session.provider.name.clone(),
        provider_kind: session.provider.kind.key().to_string(),
        model: session.model.clone(),
        status: to_bridge_status(&session.status),
        messages: session
            .messages
            .iter()
            .map(|msg| BridgeSessionMessage {
                role: msg.role.as_str().to_string(),
                content: msg.content.clone(),
            })
            .collect(),
    }
}

pub fn to_bridge_status(status: &SessionStatus) -> BridgeSessionStatus {
    match status {
        SessionStatus::Idle => BridgeSessionStatus::Idle,
        SessionStatus::Thinking => BridgeSessionStatus::Thinking,
        SessionStatus::Executing { tool } => BridgeSessionStatus::Executing { tool: tool.clone() },
        SessionStatus::WaitingInput { prompt } => BridgeSessionStatus::WaitingInput {
            prompt: prompt.clone(),
        },
        SessionStatus::Error { message } => BridgeSessionStatus::Error {
            message: message.clone(),
        },
    }
}

pub fn parse_provider_kind(value: &str) -> Option<ProviderKind> {
    ProviderKind::from_key(value)
}

pub fn convert_bridge_attachments(attachments: Vec<BridgeAttachment>) -> Vec<Attachment> {
    attachments
        .into_iter()
        .map(|attachment| Attachment {
            name: attachment.name,
            path: attachment.path.map(std::path::PathBuf::from),
        })
        .collect()
}

pub fn convert_context(context_cwd: Option<String>) -> Option<WorkingContext> {
    context_cwd.map(|cwd| WorkingContext {
        cwd: Some(std::path::PathBuf::from(cwd)),
    })
}

pub fn conversation_to_wire(messages: &[ConversationMessage]) -> Vec<BridgeSessionMessage> {
    messages
        .iter()
        .map(|msg| BridgeSessionMessage {
            role: match msg.role {
                ConversationRole::System => "system".to_string(),
                ConversationRole::User => "user".to_string(),
                ConversationRole::Assistant => "assistant".to_string(),
            },
            content: msg.content.clone(),
        })
        .collect()
}

pub fn session_message_from_wire(role: &str, content: impl Into<String>) -> SessionMessage {
    SessionMessage {
        role: SessionRole::from_str(role).unwrap_or(SessionRole::Assistant),
        content: content.into(),
        timestamp: std::time::SystemTime::now(),
    }
}

pub fn session_id_from_wire(id: u64) -> SessionId {
    SessionId::new(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aui_core_domain::{ProviderInfo, Session, SessionStats};

    #[test]
    fn bridge_session_roundtrip_shape_is_json_friendly() {
        let session = Session {
            id: SessionId::new(3),
            title: "alpha".to_string(),
            provider: ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI),
            model: "gpt-4.1".to_string(),
            status: SessionStatus::Executing {
                tool: "shell".to_string(),
            },
            stats: SessionStats::new(),
            messages: vec![SessionMessage {
                role: SessionRole::User,
                content: "hello".to_string(),
                timestamp: std::time::SystemTime::now(),
            }],
        };

        let bridge = to_bridge_session(&session);
        let json = serde_json::to_string(&bridge).expect("serialize bridge");
        let decoded: BridgeSession = serde_json::from_str(&json).expect("deserialize bridge");

        assert_eq!(decoded.id, 3);
        assert_eq!(decoded.provider_kind, "openai");
        assert_eq!(decoded.messages[0].role, "user");
        assert!(matches!(
            decoded.status,
            BridgeSessionStatus::Executing { .. }
        ));
    }

    #[test]
    fn convert_bridge_attachments_maps_path() {
        let attachments = convert_bridge_attachments(vec![BridgeAttachment {
            name: "a.txt".to_string(),
            path: Some("C:/tmp/a.txt".to_string()),
        }]);
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].name, "a.txt");
        assert!(attachments[0].path.is_some());
    }
}
