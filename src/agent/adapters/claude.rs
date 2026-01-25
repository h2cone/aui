use crate::agent::{ProviderInfo, ProviderKind};

pub fn info() -> ProviderInfo {
    ProviderInfo::new("claude-code", "Anthropic", ProviderKind::Anthropic)
}
