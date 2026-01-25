use crate::agent::{ProviderInfo, ProviderKind};

pub fn info() -> ProviderInfo {
    ProviderInfo::new("gemini-cli", "Google", ProviderKind::Google)
}
