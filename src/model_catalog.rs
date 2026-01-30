use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;
use crate::logger;
use crate::providers::ProviderKind;

const CACHE_FILE: &str = "models.json";
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6);

#[derive(Clone, Default)]
pub struct ModelCatalog {
    providers: HashMap<ProviderKind, ProviderModels>,
}

#[derive(Clone, Default)]
pub struct ProviderModels {
    pub models: Vec<String>,
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
                        models: entry.models,
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
                    models: entry.models.clone(),
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

    pub fn models_for(&self, kind: ProviderKind) -> Vec<String> {
        self.providers
            .get(&kind)
            .map(|entry| entry.models.clone())
            .unwrap_or_default()
    }

    pub fn updated_at(&self, kind: ProviderKind) -> Option<u64> {
        self.providers.get(&kind).and_then(|entry| entry.updated_at)
    }

    pub fn set_models(&mut self, kind: ProviderKind, models: Vec<String>, updated_at: u64) {
        self.providers.insert(
            kind,
            ProviderModels {
                models,
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
    match kind {
        ProviderKind::Anthropic => fetch_anthropic_models(),
        ProviderKind::OpenAI => fetch_openai_models(),
        ProviderKind::Gemini => fetch_gemini_models(),
    }
}

fn fetch_anthropic_models() -> Result<Vec<String>, String> {
    let api_key =
        env::var("ANTHROPIC_API_KEY").map_err(|_| "Missing ANTHROPIC_API_KEY".to_string())?;
    let base_url = env::var("ANTHROPIC_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = build_client()?;
    let response = client
        .get(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .map_err(|err| format!("Anthropic models request failed: {err}"))?;
    let payload = parse_json_response(response)?;
    let models = payload
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(normalize_models(models))
}

fn fetch_openai_models() -> Result<Vec<String>, String> {
    let api_key = env::var("OPENAI_API_KEY").map_err(|_| "Missing OPENAI_API_KEY".to_string())?;
    let base_url =
        env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = build_client()?;
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .map_err(|err| format!("OpenAI models request failed: {err}"))?;
    let payload = parse_json_response(response)?;
    let models = payload
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(normalize_models(models))
}

fn fetch_gemini_models() -> Result<Vec<String>, String> {
    let api_key = env::var("GEMINI_API_KEY")
        .ok()
        .or_else(|| env::var("GOOGLE_API_KEY").ok())
        .ok_or_else(|| "Missing GEMINI_API_KEY or GOOGLE_API_KEY".to_string())?;
    let base_url = env::var("GEMINI_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
    let api_version = env::var("GEMINI_API_VERSION").unwrap_or_else(|_| "v1beta".into());
    let url = format!(
        "{}/{}/models?key={}",
        base_url.trim_end_matches('/'),
        api_version.trim_matches('/'),
        api_key
    );
    let client = build_client()?;
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Gemini models request failed: {err}"))?;
    let payload = parse_json_response(response)?;
    let models = payload
        .get("models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(|value| value.trim_start_matches("models/").to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(normalize_models(models))
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
