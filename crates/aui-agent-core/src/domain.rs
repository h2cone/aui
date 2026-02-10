use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Anthropic,
    OpenAI,
    Gemini,
}

impl ProviderKind {
    pub const fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::OpenAI => "OpenAI",
            ProviderKind::Gemini => "Google Gemini",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAI => "openai",
            ProviderKind::Gemini => "gemini",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(ProviderKind::Anthropic),
            "openai" => Some(ProviderKind::OpenAI),
            "gemini" | "google" | "google-gemini" | "google_gemini" => Some(ProviderKind::Gemini),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
}

impl ProviderInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: ProviderKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: ConversationRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkingContext {
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRequest {
    pub history: Vec<ConversationMessage>,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub context: Option<WorkingContext>,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Thinking,
    Executing { tool: String },
    WaitingInput { prompt: String },
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRole {
    User,
    Assistant,
    Tool,
}

impl SessionRole {
    pub const fn as_str(&self) -> &'static str {
        match self {
            SessionRole::User => "user",
            SessionRole::Assistant => "assistant",
            SessionRole::Tool => "tool",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "user" => Some(SessionRole::User),
            "assistant" => Some(SessionRole::Assistant),
            "tool" => Some(SessionRole::Tool),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMessage {
    pub role: SessionRole,
    pub content: String,
    pub timestamp: SystemTime,
}

#[derive(Clone, Debug)]
pub struct SessionStats {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f32,
    pub started_at: Instant,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            started_at: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

impl Default for SessionStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub provider: ProviderInfo,
    pub model: String,
    pub status: SessionStatus,
    pub stats: SessionStats,
    pub messages: Vec<SessionMessage>,
}

impl Session {
    pub const fn status_label(&self) -> &'static str {
        match self.status {
            SessionStatus::Idle => "Idle",
            SessionStatus::Thinking => "Thinking",
            SessionStatus::Executing { .. } => "Executing",
            SessionStatus::WaitingInput { .. } => "Waiting",
            SessionStatus::Error { .. } => "Error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ModelCatalog {
    providers: HashMap<ProviderKind, ProviderModels>,
}

#[derive(Clone, Debug, Default)]
pub struct ProviderModels {
    pub models: Vec<String>,
    pub updated_at: Option<u64>,
}

impl ModelCatalog {
    pub fn models_for(&self, kind: ProviderKind) -> &[String] {
        const EMPTY: &[String] = &[];
        self.providers
            .get(&kind)
            .map(|entry| entry.models.as_slice())
            .unwrap_or(EMPTY)
    }

    pub fn updated_at(&self, kind: ProviderKind) -> Option<u64> {
        self.providers.get(&kind).and_then(|entry| entry.updated_at)
    }

    pub fn set_models(&mut self, kind: ProviderKind, mut models: Vec<String>, updated_at: u64) {
        models.retain(|model| !model.trim().is_empty());
        models.sort();
        models.dedup();
        self.providers.insert(
            kind,
            ProviderModels {
                models,
                updated_at: Some(updated_at),
            },
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageBlock {
    Text(String),
    Code { language: String, code: String },
    Diff { title: String, diff: String },
    Shell { title: String, output: String },
}

pub fn parse_blocks(content: &str) -> Vec<MessageBlock> {
    let mut blocks = Vec::new();
    let mut text_buffer = String::new();
    let mut code_buffer = String::new();
    let mut code_lang = String::new();
    let mut in_code = false;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if in_code {
                flush_code_block(&mut blocks, &mut code_lang, &mut code_buffer);
                in_code = false;
            } else {
                flush_text_block(&mut blocks, &mut text_buffer);
                code_lang = rest.trim().to_string();
                in_code = true;
            }
            continue;
        }

        if in_code {
            code_buffer.push_str(line);
            code_buffer.push('\n');
        } else {
            text_buffer.push_str(line);
            text_buffer.push('\n');
        }
    }

    if in_code {
        flush_code_block(&mut blocks, &mut code_lang, &mut code_buffer);
    }
    flush_text_block(&mut blocks, &mut text_buffer);

    blocks
}

fn flush_text_block(blocks: &mut Vec<MessageBlock>, buffer: &mut String) {
    let trimmed = buffer.trim();
    if !trimmed.is_empty() {
        blocks.push(MessageBlock::Text(trimmed.to_string()));
    }
    buffer.clear();
}

fn flush_code_block(blocks: &mut Vec<MessageBlock>, language: &mut String, buffer: &mut String) {
    let trimmed = buffer.trim_end();
    if trimmed.is_empty() {
        buffer.clear();
        language.clear();
        return;
    }
    let lang = if language.is_empty() {
        "code".to_string()
    } else {
        std::mem::take(language)
    };
    if lang == "diff" {
        blocks.push(MessageBlock::Diff {
            title: "Diff".to_string(),
            diff: trimmed.to_string(),
        });
    } else if is_shell_language(&lang) {
        blocks.push(MessageBlock::Shell {
            title: "Shell".to_string(),
            output: trimmed.to_string(),
        });
    } else {
        blocks.push(MessageBlock::Code {
            language: lang,
            code: trimmed.to_string(),
        });
    }
    buffer.clear();
    language.clear();
}

fn is_shell_language(language: &str) -> bool {
    matches!(
        language.to_ascii_lowercase().as_str(),
        "sh" | "bash" | "zsh" | "shell" | "console"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_parses_aliases() {
        assert_eq!(ProviderKind::from_key("openai"), Some(ProviderKind::OpenAI));
        assert_eq!(ProviderKind::from_key("GOOGLE"), Some(ProviderKind::Gemini));
        assert_eq!(
            ProviderKind::from_key("google-gemini"),
            Some(ProviderKind::Gemini)
        );
        assert_eq!(ProviderKind::from_key("unknown"), None);
    }

    #[test]
    fn session_role_roundtrip() {
        let roles = [SessionRole::User, SessionRole::Assistant, SessionRole::Tool];
        for role in roles {
            let key = role.as_str();
            assert_eq!(SessionRole::from_str(key), Some(role));
        }
        assert_eq!(SessionRole::from_str("oops"), None);
    }

    #[test]
    fn model_catalog_deduplicates_and_sorts() {
        let mut catalog = ModelCatalog::default();
        catalog.set_models(
            ProviderKind::OpenAI,
            vec![
                "gpt-4.1".to_string(),
                "".to_string(),
                "gpt-4.1".to_string(),
                "gpt-4o".to_string(),
            ],
            42,
        );

        assert_eq!(
            catalog.models_for(ProviderKind::OpenAI),
            &["gpt-4.1".to_string(), "gpt-4o".to_string()]
        );
        assert_eq!(catalog.updated_at(ProviderKind::OpenAI), Some(42));
    }

    #[test]
    fn parse_blocks_handles_code_diff_and_shell() {
        let content =
            "hello\n```rs\nlet x = 1;\n```\nmore\n```diff\n+add\n```\n```sh\necho hi\n```\n";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 5);

        match &blocks[0] {
            MessageBlock::Text(text) => assert_eq!(text, "hello"),
            _ => panic!("expected text block"),
        }
        match &blocks[1] {
            MessageBlock::Code { language, code } => {
                assert_eq!(language, "rs");
                assert_eq!(code, "let x = 1;");
            }
            _ => panic!("expected code block"),
        }
        match &blocks[2] {
            MessageBlock::Text(text) => assert_eq!(text, "more"),
            _ => panic!("expected text block"),
        }
        match &blocks[3] {
            MessageBlock::Diff { title, diff } => {
                assert_eq!(title, "Diff");
                assert_eq!(diff, "+add");
            }
            _ => panic!("expected diff block"),
        }
        match &blocks[4] {
            MessageBlock::Shell { title, output } => {
                assert_eq!(title, "Shell");
                assert_eq!(output, "echo hi");
            }
            _ => panic!("expected shell block"),
        }
    }
}
