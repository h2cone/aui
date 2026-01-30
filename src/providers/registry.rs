use std::borrow::Cow;

use crate::providers::{ProviderInfo, ProviderKind};

pub fn available_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo::new("anthropic", "Anthropic (Claude)", ProviderKind::Anthropic),
        ProviderInfo::new("openai", "OpenAI", ProviderKind::OpenAI),
        ProviderInfo::new("gemini", "Google Gemini", ProviderKind::Gemini),
    ]
}

pub fn canonicalize_provider_id(id: &str) -> Cow<'_, str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed(trimmed);
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        // Canonical.
        "anthropic" => Cow::Borrowed("anthropic"),
        "openai" => Cow::Borrowed("openai"),
        "gemini" => Cow::Borrowed("gemini"),

        // Back-compat with earlier UI/provider IDs.
        "claude-code" | "claude" | "anthropic-api" => Cow::Borrowed("anthropic"),
        "codex-cli" | "codex" | "openai-api" => Cow::Borrowed("openai"),
        "gemini-cli" | "google" | "google-gemini" | "google_gemini" | "gemini-api" => {
            Cow::Borrowed("gemini")
        }
        _ => Cow::Owned(lower),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_provider_id_accepts_legacy_ids() {
        assert_eq!(
            canonicalize_provider_id("claude-code").as_ref(),
            "anthropic"
        );
        assert_eq!(canonicalize_provider_id("codex-cli").as_ref(), "openai");
        assert_eq!(canonicalize_provider_id("gemini-cli").as_ref(), "gemini");
        assert_eq!(canonicalize_provider_id("google").as_ref(), "gemini");
    }
}
