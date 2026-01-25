use crate::agent::{ProviderInfo, ProviderKind};

pub fn info() -> ProviderInfo {
    ProviderInfo::new("opencode-cli", "OpenCode", ProviderKind::OpenCode)
}
