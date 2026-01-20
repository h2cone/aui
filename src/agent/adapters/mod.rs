pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;

use crate::agent::AgentInfo;

pub fn available_agents() -> Vec<AgentInfo> {
    vec![
        claude::info(),
        codex::info(),
        gemini::info(),
        opencode::info(),
    ]
}
