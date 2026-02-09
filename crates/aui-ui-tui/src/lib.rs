use aui_bridge::{
    BridgeCommand, BridgeEvent, convert_bridge_attachments, convert_context, parse_provider_kind,
    to_bridge_session,
};
use aui_core_domain::{ProviderInfo, SessionId};
use aui_core_engine::Command;
use aui_runtime_native::CoreRuntime;

pub struct TuiAdapter;

impl TuiAdapter {
    pub fn name(&self) -> &'static str {
        "tui"
    }

    pub fn apply(
        &self,
        runtime: &mut CoreRuntime,
        providers: &[ProviderInfo],
        payload: &str,
    ) -> Result<String, String> {
        let command: BridgeCommand =
            serde_json::from_str(payload).map_err(|err| format!("tui decode failed: {err}"))?;
        apply_bridge_command(runtime, providers, command)?;
        let snapshot = BridgeEvent::Snapshot {
            active_id: runtime.state().active_id().map(|id| id.value()),
            sessions: runtime
                .state()
                .sessions()
                .iter()
                .map(to_bridge_session)
                .collect(),
        };
        serde_json::to_string(&snapshot).map_err(|err| format!("tui encode failed: {err}"))
    }
}

pub fn apply_bridge_command(
    runtime: &mut CoreRuntime,
    providers: &[ProviderInfo],
    command: BridgeCommand,
) -> Result<(), String> {
    match command {
        BridgeCommand::CreateSession {
            title,
            provider_id,
            provider_name,
            provider_kind,
            model,
        } => {
            let kind = parse_provider_kind(&provider_kind)
                .ok_or_else(|| format!("unknown provider kind: {provider_kind}"))?;
            let provider = providers
                .iter()
                .find(|p| p.id == provider_id)
                .cloned()
                .unwrap_or_else(|| ProviderInfo::new(provider_id, provider_name, kind));
            runtime.dispatch(Command::CreateSession {
                title,
                provider,
                model,
            })
        }
        BridgeCommand::SelectSession { id } => runtime.dispatch(Command::SelectSession {
            id: SessionId::new(id),
        }),
        BridgeCommand::DeleteSession { id } => runtime.dispatch(Command::DeleteSession {
            id: SessionId::new(id),
        }),
        BridgeCommand::SubmitUserMessage {
            text,
            attachments,
            context_cwd,
        } => runtime.dispatch(Command::SubmitUserMessage {
            text,
            attachments: convert_bridge_attachments(attachments),
            context: convert_context(context_cwd),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aui_bridge::BridgeAttachment;
    use aui_core_domain::{ModelCatalog, ProviderKind, ProviderRequest, Session};
    use aui_core_ports::{
        ModelCatalogPort, ProviderEvent, ProviderPort, ProviderResponseStream, SessionStorePort,
    };
    use aui_runtime_native::CoreRuntime;

    #[derive(Default)]
    struct FakeProvider;

    impl ProviderPort for FakeProvider {
        fn providers(&self) -> Vec<ProviderInfo> {
            vec![ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)]
        }

        fn send(
            &self,
            _provider: &ProviderInfo,
            _request: ProviderRequest,
        ) -> ProviderResponseStream {
            ProviderResponseStream {
                events: vec![
                    ProviderEvent::TextDelta("ok".to_string()),
                    ProviderEvent::Done,
                ],
            }
        }
    }

    #[derive(Default)]
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

    fn runtime() -> CoreRuntime {
        CoreRuntime::new(
            Box::new(FakeProvider),
            Box::new(FakeStore),
            Box::new(FakeCatalog),
        )
    }

    fn providers() -> Vec<ProviderInfo> {
        vec![ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)]
    }

    #[test]
    fn adapter_submit_message_roundtrip_updates_snapshot() {
        let mut runtime = runtime();
        let adapter = TuiAdapter;
        let providers = providers();

        let create_payload = serde_json::to_string(&BridgeCommand::CreateSession {
            title: "alpha".to_string(),
            provider_id: "openai".to_string(),
            provider_name: "OpenAI".to_string(),
            provider_kind: "openai".to_string(),
            model: "gpt-4.1".to_string(),
        })
        .expect("encode create");
        adapter
            .apply(&mut runtime, &providers, &create_payload)
            .expect("create apply");

        let submit_payload = serde_json::to_string(&BridgeCommand::SubmitUserMessage {
            text: "hello".to_string(),
            attachments: vec![BridgeAttachment {
                name: "a.txt".to_string(),
                path: Some("a.txt".to_string()),
            }],
            context_cwd: Some(".".to_string()),
        })
        .expect("encode submit");

        let response = adapter
            .apply(&mut runtime, &providers, &submit_payload)
            .expect("submit apply");
        let snapshot: BridgeEvent = serde_json::from_str(&response).expect("decode snapshot");

        let BridgeEvent::Snapshot { sessions, .. } = snapshot else {
            panic!("expected snapshot event");
        };
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].messages.len(), 2);
        assert_eq!(sessions[0].messages[1].content, "ok");
    }
}
