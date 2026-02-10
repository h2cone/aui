use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gpui::SharedString;
use serde_json::Value;

use crate::{
    ConversationMessage, ConversationRole, ProviderClient, ProviderEvent, ProviderInfo,
    ProviderKind, ProviderStream, SessionStatus, UserMessage,
};

#[derive(Clone)]
pub struct ProviderGateway {
    providers: Vec<ProviderInfo>,
}

impl ProviderGateway {
    pub fn new() -> Self {
        let providers = crate::registry::available_providers();
        Self { providers }
    }

    pub fn providers(&self) -> &[ProviderInfo] {
        &self.providers
    }

    pub fn provider_by_id(&self, id: &str) -> Option<ProviderInfo> {
        let id = crate::registry::canonicalize_provider_id(id);
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
    let window = ContextWindowConfig::from_env();
    let prompt = build_prompt(message, window)?;
    let prompt_len = count_chars(&prompt);
    let history = select_history_for_window(&message.history, prompt_len, window);
    let messages = openai_messages(&history, &prompt);
    let request_body = serde_json::json!({
        "model": model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
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

    let window = ContextWindowConfig::from_env();
    let prompt = build_prompt(message, window)?;
    let prompt_len = count_chars(&prompt);
    let history = select_history_for_window(&message.history, prompt_len, window);
    let (system, messages) = anthropic_messages(&history, &prompt);
    let mut request_body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "stream": true,
        "messages": messages,
    });
    if let Some(system) = system {
        if let Some(obj) = request_body.as_object_mut() {
            obj.insert("system".to_string(), Value::String(system));
        }
    }
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
    let window = ContextWindowConfig::from_env();
    let prompt = build_prompt(message, window)?;
    let prompt_len = count_chars(&prompt);
    let history = select_history_for_window(&message.history, prompt_len, window);

    let url = format!(
        "{}/{}/models/{}:generateContent?key={}",
        base_url.trim_end_matches('/'),
        api_version.trim_matches('/'),
        model,
        api_key
    );
    let request_body = serde_json::json!({
        "contents": gemini_contents(&history, &prompt),
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

fn parse_usize_env(key: &str, fallback: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(fallback)
}

#[derive(Clone, Copy, Debug)]
struct ContextWindowConfig {
    max_context_chars: usize,
    reserved_history_chars: usize,
    pinned_history_messages: usize,
}

impl ContextWindowConfig {
    fn from_env() -> Self {
        Self {
            max_context_chars: parse_usize_env("AUI_MAX_CONTEXT_CHARS", 120_000),
            reserved_history_chars: parse_usize_env("AUI_RESERVED_HISTORY_CHARS", 30_000),
            pinned_history_messages: parse_usize_env("AUI_PINNED_HISTORY_MESSAGES", 2),
        }
    }
}

fn count_chars(text: &str) -> usize {
    text.chars().count()
}

fn truncate_to_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn truncate_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if count_chars(text) <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let suffix_chars = count_chars(suffix);
    if suffix_chars >= max_chars {
        return truncate_to_chars(suffix, max_chars);
    }
    let head_chars = max_chars - suffix_chars;
    let mut out = String::new();
    out.push_str(&truncate_to_chars(text, head_chars));
    out.push_str(suffix);
    out
}

fn message_cost_chars(msg: &ConversationMessage) -> usize {
    // Rough protocol overhead; keeps budgeting conservative without needing a tokenizer.
    const OVERHEAD: usize = 16;
    OVERHEAD.saturating_add(count_chars(msg.content.as_ref()))
}

fn omission_marker_message(omitted: usize) -> ConversationMessage {
    ConversationMessage {
        role: ConversationRole::System,
        content: SharedString::from(format!("[…] {omitted} message(s) omitted.")),
    }
}

fn select_history_for_window(
    history: &[ConversationMessage],
    prompt_len: usize,
    window: ContextWindowConfig,
) -> Vec<ConversationMessage> {
    const PROTECT_RECENT_MESSAGES: usize = 2;
    const MAX_MARKER_DROPS: usize = 1;

    let history_budget = window.max_context_chars.saturating_sub(prompt_len);
    if history_budget == 0 || history.is_empty() {
        return Vec::new();
    }

    let mut indices = select_history_indices(history, window, history_budget);
    if indices.len() == history.len() {
        return indices.into_iter().map(|ix| history[ix].clone()).collect();
    }

    let mut used_cost: usize = indices
        .iter()
        .map(|&ix| message_cost_chars(&history[ix]))
        .sum();
    let remaining = history_budget.saturating_sub(used_cost);

    let marker_cost = message_cost_chars(&omission_marker_message(history.len()));
    let mut remaining = remaining;

    // Optionally trade a small amount of non-critical history for a gap marker when we keep a
    // pinned head and a recent tail.
    let protect_recent_from = history.len().saturating_sub(PROTECT_RECENT_MESSAGES);
    let mut drops = 0usize;
    while remaining < marker_cost && drops < MAX_MARKER_DROPS {
        let Some(pos) = indices.iter().position(|&ix| {
            ix >= window.pinned_history_messages
                && history[ix].role != ConversationRole::System
                && ix < protect_recent_from
        }) else {
            break;
        };
        let removed = indices.remove(pos);
        used_cost = used_cost.saturating_sub(message_cost_chars(&history[removed]));
        remaining = history_budget.saturating_sub(used_cost);
        drops += 1;
    }

    if remaining < marker_cost {
        return indices.into_iter().map(|ix| history[ix].clone()).collect();
    }

    let omitted = history.len().saturating_sub(indices.len());
    let marker = omission_marker_message(omitted);

    // Insert after any kept "head" context (system + pinned head), before the recent tail.
    let insert_at = indices
        .iter()
        .take_while(|&&ix| {
            ix < window.pinned_history_messages || history[ix].role == ConversationRole::System
        })
        .count();

    let mut out: Vec<ConversationMessage> =
        indices.into_iter().map(|ix| history[ix].clone()).collect();
    out.insert(insert_at, marker);
    out
}

fn select_history_indices(
    history: &[ConversationMessage],
    window: ContextWindowConfig,
    mut budget: usize,
) -> Vec<usize> {
    if budget == 0 {
        return Vec::new();
    }

    let mut included = vec![false; history.len()];
    let mut indices: Vec<usize> = Vec::new();

    // Prefer keeping system messages.
    for (ix, msg) in history.iter().enumerate() {
        if msg.role != ConversationRole::System {
            continue;
        }
        let cost = message_cost_chars(msg);
        if cost > budget {
            break;
        }
        budget -= cost;
        included[ix] = true;
        indices.push(ix);
    }

    // Keep a small pinned head of the conversation.
    for ix in 0..history.len().min(window.pinned_history_messages) {
        if included[ix] {
            continue;
        }
        let cost = message_cost_chars(&history[ix]);
        if cost > budget {
            break;
        }
        budget -= cost;
        included[ix] = true;
        indices.push(ix);
    }

    // Fill the remaining budget from the most recent messages backwards.
    for ix in (0..history.len()).rev() {
        if included[ix] {
            continue;
        }
        let cost = message_cost_chars(&history[ix]);
        if cost > budget {
            continue;
        }
        budget -= cost;
        included[ix] = true;
        indices.push(ix);
    }

    indices.sort_unstable();
    indices
}

fn openai_role(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::System => "system",
        ConversationRole::User => "user",
        ConversationRole::Assistant => "assistant",
    }
}

fn anthropic_role(role: ConversationRole) -> Option<&'static str> {
    match role {
        ConversationRole::System => None,
        ConversationRole::User => Some("user"),
        ConversationRole::Assistant => Some("assistant"),
    }
}

fn gemini_role(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::System | ConversationRole::User => "user",
        ConversationRole::Assistant => "model",
    }
}

fn openai_messages(history: &[ConversationMessage], prompt: &str) -> Vec<Value> {
    let mut out: Vec<Value> = history
        .iter()
        .map(|msg| {
            serde_json::json!({
                "role": openai_role(msg.role),
                "content": msg.content.as_ref(),
            })
        })
        .collect();
    out.push(serde_json::json!({
        "role": "user",
        "content": prompt,
    }));
    out
}

fn anthropic_messages(
    history: &[ConversationMessage],
    prompt: &str,
) -> (Option<String>, Vec<Value>) {
    let mut system_parts: Vec<&str> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();

    for msg in history {
        match msg.role {
            ConversationRole::System => system_parts.push(msg.content.as_ref()),
            role => {
                let Some(role) = anthropic_role(role) else {
                    continue;
                };
                messages.push(serde_json::json!({
                    "role": role,
                    "content": msg.content.as_ref(),
                }));
            }
        }
    }

    messages.push(serde_json::json!({
        "role": "user",
        "content": prompt,
    }));

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    (system, messages)
}

fn gemini_contents(history: &[ConversationMessage], prompt: &str) -> Vec<Value> {
    let mut out: Vec<Value> = history
        .iter()
        .map(|msg| {
            serde_json::json!({
                "role": gemini_role(msg.role),
                "parts": [{ "text": msg.content.as_ref() }],
            })
        })
        .collect();
    out.push(serde_json::json!({
        "role": "user",
        "parts": [{ "text": prompt }],
    }));
    out
}

fn build_prompt(message: &UserMessage, window: ContextWindowConfig) -> Result<String, String> {
    const MAX_BYTES: u64 = 200_000;
    const USER_TRUNCATION_SUFFIX: &str =
        "\n\n[... user message truncated to fit the context window ...]\n";
    const ATTACHMENT_TRUNCATION_SUFFIX: &str =
        "\n\n[... attachment truncated to fit the context window ...]\n";

    let mut out = String::new();
    if let Some(context) = message.context.as_ref() {
        if let Some(cwd) = context.cwd.as_ref() {
            out.push_str("Working directory: ");
            out.push_str(&cwd.display().to_string());
            out.push_str("\n\n");
        }
    }

    // Hard limit: never allow the prompt itself to exceed the full window.
    let mut out_chars = count_chars(&out);
    let user_budget = window.max_context_chars.saturating_sub(out_chars);
    let user_text =
        truncate_with_suffix(message.text.as_ref(), user_budget, USER_TRUNCATION_SUFFIX);
    out_chars = out_chars.saturating_add(count_chars(&user_text));
    out.push_str(&user_text);

    if message.attachments.is_empty() {
        return Ok(out);
    }

    // Soft limit for attachments: keep some room for history by default.
    let soft_prompt_max = window
        .max_context_chars
        .saturating_sub(window.reserved_history_chars)
        .min(window.max_context_chars);
    if out_chars >= soft_prompt_max {
        return Ok(out);
    }

    let header = "\n\n---\nAttachments:\n";
    let header_chars = count_chars(header);
    if out_chars.saturating_add(header_chars) > soft_prompt_max {
        return Ok(out);
    }
    out_chars = out_chars.saturating_add(header_chars);
    out.push_str(header);

    for attachment in &message.attachments {
        if out_chars >= soft_prompt_max {
            break;
        }

        let mut line = format!("- {}", attachment.name);
        if let Some(path) = attachment.path.as_ref() {
            let with_path = format!("{line} ({})\n", path.display());
            let without_path = format!("{line}\n");
            line = if out_chars.saturating_add(count_chars(&with_path)) <= soft_prompt_max {
                with_path
            } else if out_chars.saturating_add(count_chars(&without_path)) <= soft_prompt_max {
                without_path
            } else {
                break;
            };
        } else {
            line.push('\n');
            if out_chars.saturating_add(count_chars(&line)) > soft_prompt_max {
                break;
            }
        }
        out_chars = out_chars.saturating_add(count_chars(&line));
        out.push_str(&line);

        if let Some(path) = attachment.path.as_ref() {
            let meta = std::fs::metadata(path).map_err(|err| {
                format!("Attachment metadata failed for {}: {err}", path.display())
            })?;
            let size = meta.len();
            if size > MAX_BYTES {
                let note = "  [skipped: too large]\n";
                let note_chars = count_chars(note);
                if out_chars.saturating_add(note_chars) > soft_prompt_max {
                    break;
                }
                out_chars = out_chars.saturating_add(note_chars);
                out.push_str(note);
                continue;
            }

            let bytes = std::fs::read(path)
                .map_err(|err| format!("Attachment read failed for {}: {err}", path.display()))?;
            if bytes.is_empty() {
                let note = "  [empty file]\n";
                let note_chars = count_chars(note);
                if out_chars.saturating_add(note_chars) > soft_prompt_max {
                    break;
                }
                out_chars = out_chars.saturating_add(note_chars);
                out.push_str(note);
                continue;
            }

            let language = fence_language_for_path(path);
            let content = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(_) => {
                    let note = "  [binary file omitted]\n";
                    let note_chars = count_chars(note);
                    if out_chars.saturating_add(note_chars) > soft_prompt_max {
                        break;
                    }
                    out_chars = out_chars.saturating_add(note_chars);
                    out.push_str(note);
                    continue;
                }
            };

            if out_chars >= soft_prompt_max {
                break;
            }

            let fence_open = format!("```{language}\n");
            let fence_close = "```\n";
            let overhead = count_chars(&fence_open) + count_chars(fence_close);
            if out_chars.saturating_add(overhead) >= soft_prompt_max {
                let note = "  [omitted: context budget]\n";
                let note_chars = count_chars(note);
                if out_chars.saturating_add(note_chars) > soft_prompt_max {
                    break;
                }
                out_chars = out_chars.saturating_add(note_chars);
                out.push_str(note);
                continue;
            }

            let available = soft_prompt_max - out_chars - overhead;
            let snippet = if count_chars(content) <= available {
                content.to_string()
            } else {
                truncate_with_suffix(content, available, ATTACHMENT_TRUNCATION_SUFFIX)
            };

            let fence_open_chars = count_chars(&fence_open);
            out_chars = out_chars.saturating_add(fence_open_chars);
            out.push_str(&fence_open);
            out_chars = out_chars.saturating_add(count_chars(&snippet));
            out.push_str(&snippet);
            if !snippet.ends_with('\n') {
                out_chars = out_chars.saturating_add(1);
                out.push('\n');
            }
            out_chars = out_chars.saturating_add(count_chars(fence_close));
            out.push_str(fence_close);
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
    use gpui::SharedString;

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

    #[test]
    fn select_history_keeps_pinned_and_recent_and_adds_marker_when_possible() {
        let long = |label: &str| SharedString::from(format!("{label}{}", "x".repeat(120)));
        let history = vec![
            ConversationMessage {
                role: ConversationRole::User,
                content: long("a"),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: long("b"),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: long("c"),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: long("d"),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: long("e"),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: long("f"),
            },
        ];

        let pinned_cost = message_cost_chars(&history[0]);
        let recent_cost = message_cost_chars(&history[4]) + message_cost_chars(&history[5]);
        let removable_cost = message_cost_chars(&history[3]);
        let window = ContextWindowConfig {
            max_context_chars: pinned_cost + recent_cost + removable_cost,
            reserved_history_chars: 0,
            pinned_history_messages: 1,
        };
        let selected = select_history_for_window(&history, 0, window);
        assert_eq!(selected.len(), 4);
        assert!(selected[0].content.as_ref().starts_with('a'));
        assert_eq!(selected[1].role, ConversationRole::System);
        assert!(selected[1].content.as_ref().contains("omitted"));
        assert!(selected[2].content.as_ref().starts_with('e'));
        assert!(selected[3].content.as_ref().starts_with('f'));

        let window = ContextWindowConfig {
            max_context_chars: pinned_cost + recent_cost,
            reserved_history_chars: 0,
            pinned_history_messages: 1,
        };
        let selected = select_history_for_window(&history, 0, window);
        assert_eq!(selected.len(), 3);
        assert!(selected[0].content.as_ref().starts_with('a'));
        assert!(selected[1].content.as_ref().starts_with('e'));
        assert!(selected[2].content.as_ref().starts_with('f'));
    }

    #[test]
    fn select_history_counts_unicode_chars_not_utf8_bytes() {
        let history = vec![
            ConversationMessage {
                role: ConversationRole::User,
                content: SharedString::from("汉".to_string()),
            },
            ConversationMessage {
                role: ConversationRole::Assistant,
                content: SharedString::from("汉".to_string()),
            },
        ];

        // Each message should be costed by character count, not UTF-8 byte length.
        let budget = message_cost_chars(&history[0]) + message_cost_chars(&history[1]);
        let window = ContextWindowConfig {
            max_context_chars: budget,
            reserved_history_chars: 0,
            pinned_history_messages: 0,
        };
        let selected = select_history_for_window(&history, 0, window);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn openai_messages_include_history_and_prompt() {
        let history = vec![
            ConversationMessage {
                role: ConversationRole::System,
                content: SharedString::from("rules".to_string()),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: SharedString::from("hi".to_string()),
            },
            ConversationMessage {
                role: ConversationRole::Assistant,
                content: SharedString::from("hello".to_string()),
            },
        ];

        let messages = openai_messages(&history, "next");
        assert_eq!(messages.len(), 4);
        assert_eq!(
            messages[0].get("role").and_then(Value::as_str),
            Some("system")
        );
        assert_eq!(
            messages[1].get("role").and_then(Value::as_str),
            Some("user")
        );
        assert_eq!(
            messages[2].get("role").and_then(Value::as_str),
            Some("assistant")
        );
        assert_eq!(
            messages[3].get("content").and_then(Value::as_str),
            Some("next")
        );
    }

    #[test]
    fn anthropic_messages_extract_system_prompt() {
        let history = vec![
            ConversationMessage {
                role: ConversationRole::System,
                content: SharedString::from("be terse".to_string()),
            },
            ConversationMessage {
                role: ConversationRole::User,
                content: SharedString::from("hi".to_string()),
            },
        ];

        let (system, messages) = anthropic_messages(&history, "next");
        assert_eq!(system.as_deref(), Some("be terse"));
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].get("role").and_then(Value::as_str),
            Some("user")
        );
        assert_eq!(
            messages[1].get("content").and_then(Value::as_str),
            Some("next")
        );
    }

    #[test]
    fn gemini_contents_map_assistant_to_model() {
        let history = vec![ConversationMessage {
            role: ConversationRole::Assistant,
            content: SharedString::from("hello".to_string()),
        }];

        let contents = gemini_contents(&history, "next");
        assert_eq!(contents.len(), 2);
        assert_eq!(
            contents[0].get("role").and_then(Value::as_str),
            Some("model")
        );
        assert_eq!(
            contents[1]
                .get("parts")
                .and_then(Value::as_array)
                .and_then(|parts| parts.first())
                .and_then(|part| part.get("text"))
                .and_then(Value::as_str),
            Some("next")
        );
    }

    #[test]
    fn build_prompt_truncates_user_text_to_window() {
        let message = UserMessage {
            history: Vec::new(),
            text: SharedString::from("x".repeat(500)),
            attachments: Vec::new(),
            context: None,
            model: SharedString::from("test".to_string()),
        };
        let window = ContextWindowConfig {
            max_context_chars: 80,
            reserved_history_chars: 0,
            pinned_history_messages: 0,
        };
        let prompt = build_prompt(&message, window).expect("prompt");
        assert!(count_chars(&prompt) <= window.max_context_chars);
        assert!(prompt.contains("user message truncated"));
    }

    #[test]
    fn build_prompt_truncates_attachments_to_preserve_reserved_history() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("big.txt");
        std::fs::write(&path, "a".repeat(2000)).expect("write attachment");

        let message = UserMessage {
            history: Vec::new(),
            text: SharedString::from("hi".to_string()),
            attachments: vec![crate::Attachment {
                name: "big.txt".to_string(),
                path: Some(path),
            }],
            context: None,
            model: SharedString::from("test".to_string()),
        };
        let window = ContextWindowConfig {
            max_context_chars: 400,
            reserved_history_chars: 150,
            pinned_history_messages: 0,
        };

        let prompt = build_prompt(&message, window).expect("prompt");
        let soft_max = window.max_context_chars - window.reserved_history_chars;
        assert!(count_chars(&prompt) <= soft_max);
        assert!(prompt.contains("Attachments:"));
        assert!(prompt.contains("attachment truncated"));
    }
}
