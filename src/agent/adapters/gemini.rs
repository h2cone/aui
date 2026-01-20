use crate::agent::{AgentInfo, AgentKind};

pub fn info() -> AgentInfo {
    AgentInfo::new("gemini-cli", "Gemini CLI", AgentKind::Gemini)
}
