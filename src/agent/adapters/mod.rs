pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;

use crate::agent::ProviderInfo;

pub fn available_providers() -> Vec<ProviderInfo> {
    vec![
        claude::info(),
        codex::info(),
        gemini::info(),
        opencode::info(),
    ]
}
