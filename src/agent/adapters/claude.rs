use crate::agent::{AgentInfo, AgentKind};

pub fn info() -> AgentInfo {
    AgentInfo::new("claude-code", "Claude Code", AgentKind::Claude)
}
