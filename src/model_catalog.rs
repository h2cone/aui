use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::SharedString;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;
use crate::logger;
use crate::providers::ProviderKind;

const CACHE_FILE: &str = "models.json";
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6);
const MODELS_DEV_SCHEMA_URL: &str = "https://models.dev/model-schema.json";
const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";

#[derive(Clone, Default)]
pub struct ModelCatalog {
    providers: HashMap<ProviderKind, ProviderModels>,
}

#[derive(Clone, Default)]
pub struct ProviderModels {
    pub models: Vec<SharedString>,
    pub updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
struct CachedCatalog {
    providers: HashMap<String, CachedProviderModels>,
}

#[derive(Serialize, Deserialize, Default)]
struct CachedProviderModels {
    models: Vec<String>,
    updated_at: Option<u64>,
}

impl ModelCatalog {
    pub fn load() -> Self {
        let path = cache_path();
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        let Ok(cached) = serde_json::from_slice::<CachedCatalog>(&bytes) else {
            return Self::default();
        };

        let mut providers = HashMap::new();
        for (key, entry) in cached.providers {
            if let Some(kind) = ProviderKind::from_key(&key) {
                providers.insert(
                    kind,
                    ProviderModels {
                        models: entry.models.into_iter().map(SharedString::from).collect(),
                        updated_at: entry.updated_at,
                    },
                );
            }
        }

        Self { providers }
    }

    pub fn save(&self) {
        let mut providers = HashMap::new();
        for (kind, entry) in &self.providers {
            providers.insert(
                kind.key().to_string(),
                CachedProviderModels {
                    models: entry
                        .models
                        .iter()
                        .map(|model| model.as_ref().to_string())
                        .collect(),
                    updated_at: entry.updated_at,
                },
            );
        }

        let payload = CachedCatalog { providers };
        let path = cache_path();
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                logger::warn(&format!("model cache mkdir failed: {err}"));
                return;
            }
        }
        let bytes = match serde_json::to_vec(&payload) {
            Ok(bytes) => bytes,
            Err(err) => {
                logger::warn(&format!("model cache encode failed: {err}"));
                return;
            }
        };
        if let Err(err) = fs::write(path, bytes) {
            logger::warn(&format!("model cache write failed: {err}"));
        }
    }

    pub fn models_for(&self, kind: ProviderKind) -> &[SharedString] {
        const EMPTY: &[SharedString] = &[];
        self.providers
            .get(&kind)
            .map(|entry| entry.models.as_slice())
            .unwrap_or(EMPTY)
    }

    pub fn updated_at(&self, kind: ProviderKind) -> Option<u64> {
        self.providers.get(&kind).and_then(|entry| entry.updated_at)
    }

    pub fn set_models(&mut self, kind: ProviderKind, models: Vec<String>, updated_at: u64) {
        self.providers.insert(
            kind,
            ProviderModels {
                models: models.into_iter().map(SharedString::from).collect(),
                updated_at: Some(updated_at),
            },
        );
    }
}

pub fn cache_path() -> PathBuf {
    config::data_dir().join(CACHE_FILE)
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn should_refresh(updated_at: Option<u64>) -> bool {
    let Some(updated_at) = updated_at else {
        return true;
    };
    let now = now_epoch_secs();
    now.saturating_sub(updated_at) > CACHE_TTL.as_secs()
}

pub fn fetch_models(kind: ProviderKind) -> Result<Vec<String>, String> {
    match fetch_models_dev_payload(models_dev_schema_url())
        .and_then(|payload| extract_models_from_schema(&payload, kind))
    {
        Ok(models) if !models.is_empty() => return Ok(models),
        Ok(_) => logger::debug(&format!(
            "models.dev schema returned no models kind={} fallback=api",
            kind.key()
        )),
        Err(err) => logger::debug(&format!(
            "models.dev schema refresh failed kind={} error={err} fallback=api",
            kind.key()
        )),
    }

    let payload = fetch_models_dev_payload(models_dev_api_url())?;
    extract_models_for_provider(&payload, kind)
}

fn models_dev_schema_url() -> String {
    std::env::var("MODELS_DEV_SCHEMA_URL").unwrap_or_else(|_| MODELS_DEV_SCHEMA_URL.to_string())
}

fn models_dev_api_url() -> String {
    std::env::var("MODELS_DEV_API_URL").unwrap_or_else(|_| MODELS_DEV_API_URL.to_string())
}

fn fetch_models_dev_payload(url: String) -> Result<Value, String> {
    let client = build_client()?;
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("models.dev request failed: {err}"))?;
    parse_json_response(response)
}

fn extract_models_from_schema(payload: &Value, kind: ProviderKind) -> Result<Vec<String>, String> {
    let model_ids = payload
        .get("$defs")
        .and_then(|value| value.get("Model"))
        .and_then(|value| value.get("enum"))
        .and_then(Value::as_array)
        .ok_or_else(|| "models.dev schema missing $defs.Model.enum".to_string())?;

    let provider_keys = models_dev_provider_keys(kind);
    let mut models = Vec::new();
    for value in model_ids {
        let Some(full_id) = value.as_str() else {
            continue;
        };
        let Some((provider_id, model_id)) = full_id.split_once('/') else {
            continue;
        };
        if !provider_keys
            .iter()
            .any(|key| provider_id.eq_ignore_ascii_case(key))
        {
            continue;
        }

        let model_id = model_id.trim();
        if model_id.is_empty() {
            continue;
        }
        models.push(model_id.to_string());
    }

    if models.is_empty() {
        return Err(format!(
            "models.dev schema has no models for {}",
            kind.key()
        ));
    }

    Ok(normalize_models(models))
}

fn extract_models_for_provider(payload: &Value, kind: ProviderKind) -> Result<Vec<String>, String> {
    let catalog = payload
        .as_object()
        .ok_or_else(|| "models.dev response root is not an object".to_string())?;

    let mut models = Vec::new();
    let mut provider_found = false;
    for provider_key in models_dev_provider_keys(kind) {
        let Some(provider) = catalog.get(*provider_key) else {
            continue;
        };
        provider_found = true;
        let Some(provider_models) = provider.get("models").and_then(Value::as_object) else {
            continue;
        };
        for (model_key, model_entry) in provider_models {
            let model_id = model_entry
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(model_key)
                .trim();
            if model_id.is_empty() {
                continue;
            }
            models.push(model_id.to_string());
        }
    }

    if !provider_found {
        return Err(format!("models.dev provider not found for {}", kind.key()));
    }

    Ok(normalize_models(models))
}

fn models_dev_provider_keys(kind: ProviderKind) -> &'static [&'static str] {
    match kind {
        ProviderKind::Anthropic => &["anthropic"],
        ProviderKind::OpenAI => &["openai"],
        ProviderKind::Gemini => &["google", "gemini"],
    }
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| format!("HTTP client init failed: {err}"))
}

fn parse_json_response(response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "unable to read error response".to_string());
        return Err(format!("HTTP {status}: {body}"));
    }
    response
        .json::<Value>()
        .map_err(|err| format!("models response decode failed: {err}"))
}

fn normalize_models(mut models: Vec<String>) -> Vec<String> {
    models.retain(|model| !model.trim().is_empty());
    models.sort();
    models.dedup();
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_models_converts_and_models_for_borrows() {
        let mut catalog = ModelCatalog::default();
        catalog.set_models(
            ProviderKind::OpenAI,
            vec!["gpt-test".to_string(), "gpt-test-2".to_string()],
            123,
        );

        let models = catalog.models_for(ProviderKind::OpenAI);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].as_ref(), "gpt-test");
        assert_eq!(models[1].as_ref(), "gpt-test-2");
        assert_eq!(catalog.updated_at(ProviderKind::OpenAI), Some(123));

        let missing = catalog.models_for(ProviderKind::Anthropic);
        assert!(missing.is_empty());
    }

    #[test]
    fn extract_models_for_provider_reads_models_dev_api_shape() {
        let payload = serde_json::json!({
            "openai": {
                "models": {
                    "gpt-4.1": { "id": "gpt-4.1" },
                    "gpt-4o": {}
                }
            }
        });

        let models = extract_models_for_provider(&payload, ProviderKind::OpenAI).expect("models");
        assert_eq!(models, vec!["gpt-4.1".to_string(), "gpt-4o".to_string()]);
    }

    #[test]
    fn extract_models_for_provider_uses_google_for_gemini() {
        let payload = serde_json::json!({
            "google": {
                "models": {
                    "gemini-2.5-pro": { "id": "gemini-2.5-pro" },
                    "gemini-2.5-flash": { "id": "gemini-2.5-flash" }
                }
            }
        });

        let models = extract_models_for_provider(&payload, ProviderKind::Gemini).expect("models");
        assert_eq!(
            models,
            vec!["gemini-2.5-flash".to_string(), "gemini-2.5-pro".to_string()]
        );
    }

    #[test]
    fn extract_models_for_provider_reports_missing_provider() {
        let payload = serde_json::json!({
            "openai": { "models": {} }
        });

        let err = extract_models_for_provider(&payload, ProviderKind::Anthropic)
            .expect_err("expected missing provider error");
        assert!(err.contains("provider not found"));
    }

    #[test]
    fn extract_models_from_schema_reads_provider_prefixed_ids() {
        let payload = serde_json::json!({
            "$defs": {
                "Model": {
                    "enum": [
                        "openai/gpt-4.1",
                        "openai/gpt-4o",
                        "anthropic/claude-sonnet-4-5"
                    ]
                }
            }
        });

        let models = extract_models_from_schema(&payload, ProviderKind::OpenAI).expect("models");
        assert_eq!(models, vec!["gpt-4.1".to_string(), "gpt-4o".to_string()]);
    }

    #[test]
    fn extract_models_from_schema_uses_google_for_gemini() {
        let payload = serde_json::json!({
            "$defs": {
                "Model": {
                    "enum": [
                        "google/gemini-2.5-pro",
                        "google/gemini-2.5-flash"
                    ]
                }
            }
        });

        let models = extract_models_from_schema(&payload, ProviderKind::Gemini).expect("models");
        assert_eq!(
            models,
            vec!["gemini-2.5-flash".to_string(), "gemini-2.5-pro".to_string()]
        );
    }

    #[test]
    fn extract_models_from_schema_reports_missing_enum() {
        let payload = serde_json::json!({
            "$defs": {}
        });

        let err = extract_models_from_schema(&payload, ProviderKind::OpenAI)
            .expect_err("expected missing schema enum error");
        assert!(err.contains("schema missing"));
    }
}
