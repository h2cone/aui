use crate::{ModelCatalog, ProviderInfo, ProviderRequest, Session, SessionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEvent {
    TextDelta(String),
    ToolStart { name: String, input: String },
    ToolResult { name: String, output: String },
    TokenUsage { input: u32, output: u32 },
    Done,
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderResponseStream {
    pub events: Vec<ProviderEvent>,
}

pub trait ProviderPort: Send + Sync {
    fn providers(&self) -> Vec<ProviderInfo>;
    fn send(&self, provider: &ProviderInfo, request: ProviderRequest) -> ProviderResponseStream;
}

pub trait SessionStorePort: Send + Sync {
    fn save_session(&self, session: &Session) -> Result<(), String>;
    fn load_sessions(&self) -> Result<Vec<Session>, String>;
    fn delete_session(&self, id: SessionId) -> Result<(), String>;
}

pub trait ModelCatalogPort: Send + Sync {
    fn load(&self) -> Result<ModelCatalog, String>;
    fn save(&self, catalog: &ModelCatalog) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Attachment, ConversationMessage, ConversationRole, ProviderKind, SessionStats,
        SessionStatus, WorkingContext,
    };
    use std::path::PathBuf;

    struct FakeProvider;

    impl ProviderPort for FakeProvider {
        fn providers(&self) -> Vec<ProviderInfo> {
            vec![ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)]
        }

        fn send(
            &self,
            provider: &ProviderInfo,
            request: ProviderRequest,
        ) -> ProviderResponseStream {
            let role = request
                .history
                .last()
                .map(|msg| match msg.role {
                    ConversationRole::System => "system",
                    ConversationRole::User => "user",
                    ConversationRole::Assistant => "assistant",
                })
                .unwrap_or("none");
            ProviderResponseStream {
                events: vec![
                    ProviderEvent::ToolStart {
                        name: provider.name.clone(),
                        input: request.model.clone(),
                    },
                    ProviderEvent::TextDelta(format!("{}:{}", role, request.text)),
                    ProviderEvent::Done,
                ],
            }
        }
    }

    struct FakeStore;

    impl SessionStorePort for FakeStore {
        fn save_session(&self, _session: &Session) -> Result<(), String> {
            Ok(())
        }

        fn load_sessions(&self) -> Result<Vec<Session>, String> {
            Ok(Vec::new())
        }

        fn delete_session(&self, _id: SessionId) -> Result<(), String> {
            Ok(())
        }
    }

    struct FakeCatalog;

    impl ModelCatalogPort for FakeCatalog {
        fn load(&self) -> Result<ModelCatalog, String> {
            Ok(ModelCatalog::default())
        }

        fn save(&self, _catalog: &ModelCatalog) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn provider_port_contract_can_be_mocked() {
        let provider = FakeProvider;
        let info = provider.providers().pop().expect("provider missing");
        let stream = provider.send(
            &info,
            ProviderRequest {
                history: vec![ConversationMessage {
                    role: ConversationRole::User,
                    content: "hi".to_string(),
                }],
                text: "question".to_string(),
                attachments: vec![Attachment {
                    name: "a.txt".to_string(),
                    path: Some(PathBuf::from("a.txt")),
                }],
                context: Some(WorkingContext {
                    cwd: Some(PathBuf::from(".")),
                }),
                model: "gpt-4.1".to_string(),
            },
        );

        assert_eq!(stream.events.len(), 3);
        assert_eq!(
            stream.events[1],
            ProviderEvent::TextDelta("user:question".to_string())
        );
    }

    #[test]
    fn store_and_catalog_ports_have_minimal_contract() {
        let store = FakeStore;
        let catalog = FakeCatalog;
        let provider = ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI);
        let session = Session {
            id: SessionId::new(1),
            title: "alpha".to_string(),
            provider,
            model: "gpt-4.1".to_string(),
            status: SessionStatus::Idle,
            stats: SessionStats::new(),
            messages: Vec::new(),
        };
        store.save_session(&session).expect("save session");
        assert!(store.load_sessions().expect("load sessions").is_empty());
        store
            .delete_session(SessionId::new(1))
            .expect("delete session");

        let loaded = catalog.load().expect("load catalog");
        assert!(loaded.models_for(ProviderKind::OpenAI).is_empty());
        catalog.save(&loaded).expect("save catalog");
    }
}
