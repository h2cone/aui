use aui_core_domain::{ModelCatalog, Session, SessionId};
use aui_core_engine::{Command, CoreState, Effect};
use aui_core_ports::{ModelCatalogPort, ProviderPort, SessionStorePort};

pub struct CoreRuntime {
    state: CoreState,
    provider: Box<dyn ProviderPort>,
    store: Box<dyn SessionStorePort>,
    catalog: Box<dyn ModelCatalogPort>,
}

impl CoreRuntime {
    pub fn new(
        provider: Box<dyn ProviderPort>,
        store: Box<dyn SessionStorePort>,
        catalog: Box<dyn ModelCatalogPort>,
    ) -> Self {
        Self {
            state: CoreState::new(),
            provider,
            store,
            catalog,
        }
    }

    pub fn state(&self) -> &CoreState {
        &self.state
    }

    pub fn dispatch(&mut self, command: Command) -> Result<(), String> {
        let result = self.state.dispatch(command);
        for effect in result.effects {
            self.apply_effect(effect)?;
        }
        Ok(())
    }

    pub fn load_from_store(&mut self) -> Result<(), String> {
        let sessions = self.store.load_sessions()?;
        self.dispatch(Command::RestoreSessions {
            sessions,
            active_id: None,
        })
    }

    pub fn load_catalog(&self) -> Result<ModelCatalog, String> {
        self.catalog.load()
    }

    fn apply_effect(&mut self, effect: Effect) -> Result<(), String> {
        match effect {
            Effect::PersistSession { id } => self.persist_session(id),
            Effect::DeletePersistedSession { id } => self.store.delete_session(id),
            Effect::SendProviderRequest {
                session_id,
                provider,
                request,
            } => {
                let stream = self.provider.send(&provider, request);
                for event in stream.events {
                    let result = self
                        .state
                        .dispatch(Command::ReceiveProviderEvent { session_id, event });
                    for nested in result.effects {
                        self.apply_effect(nested)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn persist_session(&self, id: SessionId) -> Result<(), String> {
        let Some(session) = self.state.session(id) else {
            return Ok(());
        };
        self.store.save_session(session)
    }
}

#[derive(Default)]
pub struct InMemoryStore {
    sessions: std::sync::Mutex<Vec<Session>>,
}

impl SessionStorePort for InMemoryStore {
    fn save_session(&self, session: &Session) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|err| err.to_string())?;
        if let Some(existing) = sessions.iter_mut().find(|s| s.id == session.id) {
            *existing = session.clone();
        } else {
            sessions.push(session.clone());
        }
        Ok(())
    }

    fn load_sessions(&self) -> Result<Vec<Session>, String> {
        Ok(self.sessions.lock().map_err(|err| err.to_string())?.clone())
    }

    fn delete_session(&self, id: SessionId) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|err| err.to_string())?;
        sessions.retain(|session| session.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryCatalog {
    catalog: std::sync::Mutex<ModelCatalog>,
}

impl ModelCatalogPort for InMemoryCatalog {
    fn load(&self) -> Result<ModelCatalog, String> {
        Ok(self.catalog.lock().map_err(|err| err.to_string())?.clone())
    }

    fn save(&self, catalog: &ModelCatalog) -> Result<(), String> {
        *self.catalog.lock().map_err(|err| err.to_string())? = catalog.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aui_core_domain::{
        ModelCatalog, ProviderInfo, ProviderKind, ProviderRequest, SessionStats, SessionStatus,
    };
    use aui_core_ports::{
        ModelCatalogPort, ProviderEvent, ProviderPort, ProviderResponseStream, SessionStorePort,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeProvider {
        sent_requests: Arc<Mutex<Vec<ProviderRequest>>>,
    }

    impl ProviderPort for FakeProvider {
        fn providers(&self) -> Vec<ProviderInfo> {
            vec![ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)]
        }

        fn send(
            &self,
            _provider: &ProviderInfo,
            request: ProviderRequest,
        ) -> ProviderResponseStream {
            self.sent_requests
                .lock()
                .expect("requests lock")
                .push(request);
            ProviderResponseStream {
                events: vec![
                    ProviderEvent::TextDelta("hello".to_string()),
                    ProviderEvent::TokenUsage {
                        input: 10,
                        output: 20,
                    },
                    ProviderEvent::Done,
                ],
            }
        }
    }

    #[derive(Default)]
    struct FakeStore {
        saved_ids: Arc<Mutex<Vec<SessionId>>>,
        sessions: Arc<Mutex<Vec<Session>>>,
        deleted_ids: Arc<Mutex<Vec<SessionId>>>,
    }

    impl SessionStorePort for FakeStore {
        fn save_session(&self, session: &Session) -> Result<(), String> {
            self.saved_ids
                .lock()
                .expect("saved ids lock")
                .push(session.id);
            let mut sessions = self.sessions.lock().expect("sessions lock");
            if let Some(existing) = sessions.iter_mut().find(|s| s.id == session.id) {
                *existing = session.clone();
            } else {
                sessions.push(session.clone());
            }
            Ok(())
        }

        fn load_sessions(&self) -> Result<Vec<Session>, String> {
            Ok(self.sessions.lock().expect("sessions lock").clone())
        }

        fn delete_session(&self, id: SessionId) -> Result<(), String> {
            self.deleted_ids.lock().expect("deleted ids lock").push(id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeCatalog;

    impl ModelCatalogPort for FakeCatalog {
        fn load(&self) -> Result<ModelCatalog, String> {
            Ok(ModelCatalog::default())
        }

        fn save(&self, _catalog: &ModelCatalog) -> Result<(), String> {
            Ok(())
        }
    }

    fn provider() -> ProviderInfo {
        ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)
    }

    #[test]
    fn runtime_dispatch_executes_effects_and_persists() {
        let provider_impl = FakeProvider::default();
        let requests = Arc::clone(&provider_impl.sent_requests);
        let store_impl = FakeStore::default();
        let saved_ids = Arc::clone(&store_impl.saved_ids);

        let mut runtime = CoreRuntime::new(
            Box::new(provider_impl),
            Box::new(store_impl),
            Box::new(FakeCatalog),
        );

        runtime
            .dispatch(Command::CreateSession {
                title: "alpha".to_string(),
                provider: provider(),
                model: "gpt-4.1".to_string(),
            })
            .expect("create session");
        runtime
            .dispatch(Command::SubmitUserMessage {
                text: "question".to_string(),
                attachments: Vec::new(),
                context: None,
            })
            .expect("submit");

        let request_count = requests.lock().expect("requests lock").len();
        assert_eq!(request_count, 1);
        let saved_count = saved_ids.lock().expect("saved lock").len();
        assert!(saved_count >= 2);

        let active = runtime.state().active_session().expect("active session");
        assert_eq!(active.messages.len(), 2);
        assert_eq!(active.messages[1].content, "hello");
        assert_eq!(active.stats.tokens_in, 10);
        assert_eq!(active.stats.tokens_out, 20);
        assert_eq!(active.status, SessionStatus::Idle);
    }

    #[test]
    fn runtime_load_from_store_restores_sessions() {
        let store_impl = FakeStore::default();
        store_impl
            .sessions
            .lock()
            .expect("sessions lock")
            .push(Session {
                id: SessionId::new(7),
                title: "restored".to_string(),
                provider: provider(),
                model: "gpt".to_string(),
                status: SessionStatus::Idle,
                stats: SessionStats::new(),
                messages: Vec::new(),
            });

        let mut runtime = CoreRuntime::new(
            Box::new(FakeProvider::default()),
            Box::new(store_impl),
            Box::new(FakeCatalog),
        );

        runtime.load_from_store().expect("load from store");
        assert_eq!(runtime.state().sessions().len(), 1);
        assert_eq!(runtime.state().active_id(), Some(SessionId::new(7)));
    }

    #[test]
    fn runtime_delete_session_triggers_store_delete() {
        let store_impl = FakeStore::default();
        let deleted_ids = Arc::clone(&store_impl.deleted_ids);

        let mut runtime = CoreRuntime::new(
            Box::new(FakeProvider::default()),
            Box::new(store_impl),
            Box::new(FakeCatalog),
        );

        runtime
            .dispatch(Command::CreateSession {
                title: "alpha".to_string(),
                provider: provider(),
                model: "gpt".to_string(),
            })
            .expect("create");
        let id = runtime.state().active_id().expect("active id");
        runtime
            .dispatch(Command::DeleteSession { id })
            .expect("delete");

        let deleted = deleted_ids.lock().expect("deleted lock");
        assert_eq!(deleted.as_slice(), &[id]);
    }
}
