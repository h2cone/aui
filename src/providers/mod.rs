use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use gpui::SharedString;

pub mod gateway;
pub mod registry;

#[derive(Clone, Debug)]
pub struct ProviderInfo {
    pub id: Arc<str>,
    pub name: Arc<str>,
    pub kind: ProviderKind,
}

impl ProviderInfo {
    pub fn new(id: impl Into<Arc<str>>, name: impl Into<Arc<str>>, kind: ProviderKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
        }
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

pub trait ProviderClient: Send + Sync {
    fn send(&self, message: UserMessage) -> ProviderStream;
    fn abort(&self);
    fn status(&self) -> SessionStatus;
    fn info(&self) -> ProviderInfo;
}

#[derive(Clone, Debug)]
pub struct UserMessage {
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub context: Option<WorkingContext>,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub name: String,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct WorkingContext {
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Thinking,
    Executing { tool: String },
    WaitingInput { prompt: SharedString },
    Error { message: SharedString },
}

pub struct ProviderStream {
    pub events: Receiver<ProviderEvent>,
}

#[derive(Clone, Debug)]
pub enum ProviderEvent {
    TextDelta(String),
    ToolStart { name: String, input: String },
    ToolResult { name: String, output: String },
    TokenUsage { input: u32, output: u32 },
    Done,
    Error(String),
}
