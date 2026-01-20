use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub mod adapters;
pub mod bridge;

#[derive(Clone, Debug)]
pub struct AgentInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: AgentKind,
}

impl AgentInfo {
    pub const fn new(id: &'static str, name: &'static str, kind: AgentKind) -> Self {
        Self { id, name, kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

impl AgentKind {
    pub const fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
            AgentKind::Gemini => "Gemini",
            AgentKind::OpenCode => "OpenCode",
        }
    }
}

pub trait Agent: Send + Sync {
    fn send(&self, message: UserMessage) -> AgentStream;
    fn abort(&self);
    fn status(&self) -> AgentStatus;
    fn info(&self) -> AgentInfo;
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
pub enum AgentStatus {
    Idle,
    Thinking,
    Executing { tool: String },
    WaitingInput { prompt: String },
    Error { message: String },
}

pub struct AgentStream {
    pub events: Receiver<StreamEvent>,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    TextDelta(String),
    ToolStart { name: String, input: String },
    ToolResult { name: String, output: String },
    TokenUsage { input: u32, output: u32 },
    Done,
    Error(String),
}
