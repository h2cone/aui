use aui_agent_core::CoreRuntime;
use aui_agent_core::{BridgeCommand, BridgeEvent};
use aui_agent_core::{
    ModelCatalog, ProviderInfo, ProviderKind, ProviderRequest, Session, SessionId,
};
use aui_agent_core::{
    ModelCatalogPort, ProviderEvent, ProviderPort, ProviderResponseStream, SessionStorePort,
};
use aui_ui_tui::TuiAdapter;
use aui_ui_web::WebAdapter;

#[derive(Default)]
struct FakeProvider;

impl ProviderPort for FakeProvider {
    fn providers(&self) -> Vec<ProviderInfo> {
        vec![ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI)]
    }

    fn send(&self, _provider: &ProviderInfo, _request: ProviderRequest) -> ProviderResponseStream {
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
fn tui_and_web_emit_identical_snapshot_for_same_command() {
    let mut runtime_tui = runtime();
    let mut runtime_web = runtime();
    let payload = serde_json::to_string(&BridgeCommand::CreateSession {
        title: "alpha".to_string(),
        provider_id: "openai".to_string(),
        provider_name: "OpenAI".to_string(),
        provider_kind: "openai".to_string(),
        model: "gpt-4.1".to_string(),
    })
    .expect("encode create");

    let tui = TuiAdapter;
    let web = WebAdapter;

    let event_tui = tui
        .apply(&mut runtime_tui, &providers(), &payload)
        .expect("tui apply");
    let event_web = web
        .apply(&mut runtime_web, &providers(), &payload)
        .expect("web apply");

    let decoded_tui: BridgeEvent = serde_json::from_str(&event_tui).expect("decode tui");
    let decoded_web: BridgeEvent = serde_json::from_str(&event_web).expect("decode web");
    assert_eq!(decoded_tui, decoded_web);
}
