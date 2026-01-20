use crate::agent::{AgentInfo, AgentKind};

pub fn info() -> AgentInfo {
    AgentInfo::new("codex-cli", "Codex CLI", AgentKind::Codex)
}
