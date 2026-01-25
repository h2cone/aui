use crate::agent::{ProviderInfo, ProviderKind};

pub fn info() -> ProviderInfo {
    ProviderInfo::new("codex-cli", "OpenAI", ProviderKind::OpenAI)
}
