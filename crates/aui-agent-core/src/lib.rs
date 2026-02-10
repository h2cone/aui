mod bridge;
mod domain;
mod engine;
mod ports;
mod runtime;

pub use bridge::{
    BridgeAttachment, BridgeCommand, BridgeEvent, BridgeSession, BridgeSessionMessage,
    BridgeSessionStatus, conversation_to_wire, convert_bridge_attachments, convert_context,
    parse_provider_kind, session_id_from_wire, session_message_from_wire, to_bridge_session,
    to_bridge_status,
};
pub use domain::{
    Attachment, ConversationMessage, ConversationRole, MessageBlock, ModelCatalog, ProviderInfo,
    ProviderKind, ProviderModels, ProviderRequest, Session, SessionId, SessionMessage, SessionRole,
    SessionStats, SessionStatus, WorkingContext, parse_blocks,
};
pub use engine::{Command, CoreState, Effect, ReduceResult};
pub use ports::{
    ModelCatalogPort, ProviderEvent, ProviderPort, ProviderResponseStream, SessionStorePort,
};
pub use runtime::{CoreRuntime, InMemoryCatalog, InMemoryStore};
