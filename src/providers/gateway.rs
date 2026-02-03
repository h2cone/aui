use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::logger;
use crate::providers::{
    ProviderClient, ProviderEvent, ProviderInfo, ProviderKind, ProviderStream, SessionStatus,
    UserMessage,
};

pub struct ProviderGateway {
    providers: Vec<ProviderInfo>,
}

impl ProviderGateway {
    pub fn new() -> Self {
        let providers = crate::providers::registry::available_providers();
        logger::debug(&format!(
            "provider gateway init providers={} transport=rust",
            providers.len()
        ));
        Self { providers }
    }

    pub fn providers(&self) -> &[ProviderInfo] {
        &self.providers
    }

    pub fn provider_by_id(&self, id: &str) -> Option<ProviderInfo> {
        let id = crate::providers::registry::canonicalize_provider_id(id);
        self.providers
            .iter()
            .find(|provider| provider.id.as_ref() == id.as_ref())
            .cloned()
    }

    pub fn connect(&self, info: &ProviderInfo) -> Box<dyn ProviderClient> {
        Box::new(RustProvider::new(info.clone()))
    }
}

fn stream_text(text: String, tx: &mpsc::Sender<ProviderEvent>) {
    let mut chunk = String::new();
    for ch in text.chars() {
        chunk.push(ch);
        if chunk.len() >= 64 {
            let send_chunk = std::mem::take(&mut chunk);
            let _ = tx.send(ProviderEvent::TextDelta(send_chunk));
        }
    }
    if !chunk.is_empty() {
        let _ = tx.send(ProviderEvent::TextDelta(chunk));
    }
}

#[derive(Clone)]
struct RustProvider {
    info: ProviderInfo,
}

impl RustProvider {
    fn new(info: ProviderInfo) -> Self {
        Self { info }
    }
}

impl ProviderClient for RustProvider {
    fn send(&self, message: UserMessage) -> ProviderStream {
        let (events_tx, events_rx) = mpsc::channel();
        let info = self.info.clone();
        thread::spawn(move || {
            let result = match info.kind {
                ProviderKind::Anthropic => send_anthropic_stream(&info, &message, &events_tx),
                ProviderKind::OpenAI => send_openai_stream(&info, &message, &events_tx),
                ProviderKind::Gemini => send_gemini_request(&info, &message, &events_tx),
            };
            if let Err(err) = result {
                let _ = events_tx.send(ProviderEvent::Error(err));
            }
        });
        ProviderStream { events: events_rx }
    }

    fn abort(&self) {}

    fn status(&self) -> SessionStatus {
        SessionStatus::Idle
    }

    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }
}

struct OpenAiToolCall {
    name: String,
    args: String,
}

fn send_openai_stream(
    info: &ProviderInfo,
    message: &UserMessage,
    tx: &mpsc::Sender<ProviderEvent>,
) -> Result<(), String> {
    let OpenAiConfig { api_key, base_url } = openai_config(info)?;
    let model = message.model.as_ref();
    let client = build_http_client()?;
    let prompt = build_prompt(message)?;
    let request_body = serde_json::json!({
        "model": model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": [{ "role": "user", "content": prompt }],
    });
    let url = format!("{}/chat/completions", base_url);
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .map_err(|err| format!("OpenAI request failed: {err}"))?;

    let response = ensure_success(response)?;
    let mut tool_calls: HashMap<String, OpenAiToolCall> = HashMap::new();
    parse_sse_stream(response, |data| {
        if data == "[DONE]" {
            if !tool_calls.is_empty() {
                flush_tool_calls(&mut tool_calls, tx);
            }
            let _ = tx.send(ProviderEvent::Done);
            return Ok(true);
        }

        let payload: Value = serde_json::from_str(data)
            .map_err(|err| format!("OpenAI stream decode failed: {err}"))?;
        let choice = payload.get("choices").and_then(|v| v.get(0));
        if let Some(content) = choice
            .and_then(|c| c.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
        {
            let _ = tx.send(ProviderEvent::TextDelta(content.to_string()));
        }

        if let Some(tool_calls_delta) = choice
            .and_then(|c| c.get("delta"))
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(Value::as_array)
        {
            for call in tool_calls_delta {
                let call_id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool-call")
                    .to_string();
                let name = call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .map(|value| value.to_string());
                let args = call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                    .map(|value| value.to_string());
                track_openai_tool_call(&mut tool_calls, call_id, name, args);
            }
        }

        if let Some(function_call) = choice
            .and_then(|c| c.get("delta"))
            .and_then(|delta| delta.get("function_call"))
        {
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .map(|value| value.to_string());
            let args = function_call
                .get("arguments")
                .and_then(Value::as_str)
                .map(|value| value.to_string());
            track_openai_tool_call(&mut tool_calls, "function-call".to_string(), name, args);
        }

        if let Some(finish_reason) = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(Value::as_str)
        {
            if finish_reason == "tool_calls" && !tool_calls.is_empty() {
                flush_tool_calls(&mut tool_calls, tx);
            }
        }

        if let Some(usage) = payload.get("usage") {
            let input = usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let output = usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let _ = tx.send(ProviderEvent::TokenUsage { input, output });
        }

        Ok(false)
    })
}

fn send_anthropic_stream(
    _info: &ProviderInfo,
    message: &UserMessage,
    tx: &mpsc::Sender<ProviderEvent>,
) -> Result<(), String> {
    let api_key =
        env::var("ANTHROPIC_API_KEY").map_err(|_| "Missing ANTHROPIC_API_KEY".to_string())?;
    let model = message.model.as_ref();
    let max_tokens = parse_u32_env("ANTHROPIC_MAX_TOKENS", 1024);
    let base_url = env::var("ANTHROPIC_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());

    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "stream": true,
        "messages": [{ "role": "user", "content": build_prompt(message)? }],
    });
    let client = build_http_client()?;
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("accept", "text/event-stream")
        .json(&request_body)
        .send()
        .map_err(|err| format!("Anthropic request failed: {err}"))?;

    let response = ensure_success(response)?;
    let mut input_tokens: Option<u32> = None;
    parse_sse_stream_with_event(response, |event, data| {
        let payload: Value = serde_json::from_str(data)
            .map_err(|err| format!("Anthropic stream decode failed: {err}"))?;
        match event {
            "message_start" => {
                if let Some(value) = payload
                    .get("message")
                    .and_then(|msg| msg.get("usage"))
                    .and_then(|usage| usage.get("input_tokens"))
                    .and_then(Value::as_u64)
                {
                    input_tokens = Some(value as u32);
                }
            }
            "content_block_delta" => {
                if let Some(text) = payload
                    .get("delta")
                    .and_then(|delta| delta.get("text"))
                    .and_then(Value::as_str)
                {
                    let _ = tx.send(ProviderEvent::TextDelta(text.to_string()));
                }
            }
            "content_block_start" => {
                let block = payload.get("content_block");
                if block
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
                    == Some("tool_use")
                {
                    let name = block
                        .and_then(|value| value.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let input = block
                        .and_then(|value| value.get("input"))
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "{}".to_string());
                    let _ = tx.send(ProviderEvent::ToolStart { name, input });
                }
            }
            "message_delta" => {
                if let Some(output_tokens) = payload
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(Value::as_u64)
                {
                    let input = input_tokens.unwrap_or(0);
                    let _ = tx.send(ProviderEvent::TokenUsage {
                        input,
                        output: output_tokens as u32,
                    });
                }
            }
            "message_stop" => {
                let _ = tx.send(ProviderEvent::Done);
                return Ok(true);
            }
            "error" => {
                let message = payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic stream error")
                    .to_string();
                let _ = tx.send(ProviderEvent::Error(message));
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    })?;

    Ok(())
}

fn send_gemini_request(
    _info: &ProviderInfo,
    message: &UserMessage,
    tx: &mpsc::Sender<ProviderEvent>,
) -> Result<(), String> {
    let api_key = env::var("GEMINI_API_KEY")
        .ok()
        .or_else(|| env::var("GOOGLE_API_KEY").ok())
        .ok_or_else(|| "Missing GEMINI_API_KEY or GOOGLE_API_KEY".to_string())?;
    let model = message.model.as_ref();
    let base_url = env::var("GEMINI_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
    let api_version = env::var("GEMINI_API_VERSION").unwrap_or_else(|_| "v1beta".into());
    let prompt = build_prompt(message)?;

    let url = format!(
        "{}/{}/models/{}:generateContent?key={}",
        base_url.trim_end_matches('/'),
        api_version.trim_matches('/'),
        model,
        api_key
    );
    let request_body = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }]
    });
    let client = build_http_client()?;
    let response = client
        .post(url)
        .json(&request_body)
        .send()
        .map_err(|err| format!("Gemini request failed: {err}"))?;

    let mut response = ensure_success(response)?;
    let mut body = String::new();
    response
        .read_to_string(&mut body)
        .map_err(|err| format!("Gemini response read failed: {err}"))?;
    let payload: Value =
        serde_json::from_str(&body).map_err(|err| format!("Gemini decode failed: {err}"))?;

    let mut text = String::new();
    if let Some(parts) = payload
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
    {
        for part in parts {
            if let Some(chunk) = part.get("text").and_then(Value::as_str) {
                text.push_str(chunk);
            }
        }
    }

    if !text.is_empty() {
        stream_text(text, tx);
    }

    if let Some(usage) = payload.get("usageMetadata") {
        let input = usage
            .get("promptTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let output = usage
            .get("candidatesTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let _ = tx.send(ProviderEvent::TokenUsage { input, output });
    }

    let _ = tx.send(ProviderEvent::Done);
    Ok(())
}

struct OpenAiConfig {
    api_key: String,
    base_url: String,
}

fn openai_config(info: &ProviderInfo) -> Result<OpenAiConfig, String> {
    match info.kind {
        ProviderKind::OpenAI => {
            let api_key =
                env::var("OPENAI_API_KEY").map_err(|_| "Missing OPENAI_API_KEY".to_string())?;
            let base_url =
                env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
            Ok(OpenAiConfig {
                api_key,
                base_url: base_url.trim_end_matches('/').to_string(),
            })
        }
        _ => Err(format!(
            "Unsupported OpenAI provider: {}",
            info.kind.label()
        )),
    }
}

fn build_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| format!("HTTP client init failed: {err}"))
}

fn ensure_success(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response
        .text()
        .unwrap_or_else(|_| "unable to read error response".to_string());
    Err(format!("HTTP {status}: {body}"))
}

fn parse_sse_stream<F>(response: reqwest::blocking::Response, mut handle: F) -> Result<(), String>
where
    F: FnMut(&str) -> Result<bool, String>,
{
    parse_sse_stream_with_event(response, |_, data| handle(data))
}

fn parse_sse_stream_with_event<F>(
    response: reqwest::blocking::Response,
    mut handle: F,
) -> Result<(), String>
where
    F: FnMut(&str, &str) -> Result<bool, String>,
{
    let mut reader = BufReader::new(response);
    let mut event_type: Option<String> = None;
    let mut data = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("SSE read failed: {err}"))?;
        if bytes == 0 {
            if !data.is_empty() || event_type.is_some() {
                let event = event_type.as_deref().unwrap_or("");
                let payload = data.trim_end();
                if handle(event, payload)? {
                    return Ok(());
                }
            }
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            if !data.is_empty() || event_type.is_some() {
                let event = event_type.as_deref().unwrap_or("");
                let payload = data.trim_end();
                if handle(event, payload)? {
                    return Ok(());
                }
            }
            event_type = None;
            data.clear();
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("event:") {
            event_type = Some(rest.trim().to_string());
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim_start());
        }
    }

    Ok(())
}

fn track_openai_tool_call(
    tool_calls: &mut HashMap<String, OpenAiToolCall>,
    id: String,
    name: Option<String>,
    args: Option<String>,
) {
    let entry = tool_calls.entry(id).or_insert_with(|| OpenAiToolCall {
        name: "tool".to_string(),
        args: String::new(),
    });
    if let Some(name) = name {
        entry.name = name;
    }
    if let Some(args) = args {
        entry.args.push_str(&args);
    }
}

fn flush_tool_calls(
    tool_calls: &mut HashMap<String, OpenAiToolCall>,
    tx: &mpsc::Sender<ProviderEvent>,
) {
    for (_, call) in tool_calls.drain() {
        let _ = tx.send(ProviderEvent::ToolStart {
            name: call.name,
            input: call.args,
        });
    }
}

fn parse_u32_env(key: &str, fallback: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(fallback)
}

fn build_prompt(message: &UserMessage) -> Result<String, String> {
    const MAX_BYTES: u64 = 200_000;

    let mut out = String::new();
    if let Some(context) = message.context.as_ref() {
        if let Some(cwd) = context.cwd.as_ref() {
            out.push_str("Working directory: ");
            out.push_str(&cwd.display().to_string());
            out.push_str("\n\n");
        }
    }

    out.push_str(message.text.as_ref());

    if message.attachments.is_empty() {
        return Ok(out);
    }

    out.push_str("\n\n---\nAttachments:\n");

    for attachment in &message.attachments {
        out.push_str("- ");
        out.push_str(&attachment.name);
        if let Some(path) = attachment.path.as_ref() {
            out.push_str(" (");
            out.push_str(&path.display().to_string());
            out.push_str(")\n");

            let meta = std::fs::metadata(path).map_err(|err| {
                format!("Attachment metadata failed for {}: {err}", path.display())
            })?;
            let size = meta.len();
            if size > MAX_BYTES {
                out.push_str("  [skipped: too large]\n");
                continue;
            }

            let bytes = std::fs::read(path)
                .map_err(|err| format!("Attachment read failed for {}: {err}", path.display()))?;
            if bytes.is_empty() {
                out.push_str("  [empty file]\n");
                continue;
            }

            let language = fence_language_for_path(path);
            let content = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(_) => {
                    out.push_str("  [binary file omitted]\n");
                    continue;
                }
            };

            out.push_str("```");
            out.push_str(language);
            out.push_str("\n");
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
        } else {
            out.push('\n');
        }
    }

    Ok(out)
}

fn fence_language_for_path(path: &Path) -> &'static str {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return "text";
    };
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" => "javascript",
        "jsx" => "jsx",
        "py" => "python",
        "go" => "go",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" => "markdown",
        "diff" => "diff",
        "sh" => "sh",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_text_chunks_and_reassembles() {
        let (tx, rx) = mpsc::channel();
        let text = "a".repeat(65);
        stream_text(text.clone(), &tx);
        drop(tx);

        let mut chunks = Vec::new();
        for event in rx.into_iter() {
            match event {
                ProviderEvent::TextDelta(chunk) => chunks.push(chunk),
                _ => panic!("unexpected provider event"),
            }
        }

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 64);
        assert_eq!(chunks[1].len(), 1);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn track_openai_tool_call_updates_name_and_args() {
        let mut tool_calls: HashMap<String, OpenAiToolCall> = HashMap::new();
        track_openai_tool_call(
            &mut tool_calls,
            "a".to_string(),
            None,
            Some("x".to_string()),
        );
        let first = tool_calls.get("a").expect("missing tool call");
        assert_eq!(first.name, "tool");
        assert_eq!(first.args, "x");

        track_openai_tool_call(
            &mut tool_calls,
            "a".to_string(),
            Some("run".to_string()),
            Some("y".to_string()),
        );
        let updated = tool_calls.get("a").expect("missing tool call");
        assert_eq!(updated.name, "run");
        assert_eq!(updated.args, "xy");

        track_openai_tool_call(
            &mut tool_calls,
            "b".to_string(),
            Some("first".to_string()),
            None,
        );
        let second = tool_calls.get("b").expect("missing tool call");
        assert_eq!(second.name, "first");
        assert!(second.args.is_empty());
    }
}
