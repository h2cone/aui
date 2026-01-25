use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub mod adapters;
pub mod bridge;

#[derive(Clone, Debug)]
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
pub enum ProviderKind {
    Anthropic,
    OpenAI,
    Google,
    OpenCode,
}

impl ProviderKind {
    pub const fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::OpenAI => "OpenAI",
            ProviderKind::Google => "Google",
            ProviderKind::OpenCode => "OpenCode",
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
    WaitingInput { prompt: String },
    Error { message: String },
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
