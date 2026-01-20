use crate::agent::{AgentInfo, AgentKind};

pub fn info() -> AgentInfo {
    AgentInfo::new("opencode-cli", "OpenCode CLI", AgentKind::OpenCode)
}
