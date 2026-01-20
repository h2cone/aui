use std::sync::mpsc;

use crate::agent::{Agent, AgentInfo, AgentStatus, AgentStream, StreamEvent, UserMessage};

pub struct BridgeClient {
    agents: Vec<AgentInfo>,
}

impl BridgeClient {
    pub fn new() -> Self {
        Self {
            agents: crate::agent::adapters::available_agents(),
        }
    }

    pub fn agents(&self) -> &[AgentInfo] {
        &self.agents
    }

    pub fn connect(&self, info: &AgentInfo) -> Box<dyn Agent> {
        Box::new(NullAgent::new(info.clone()))
    }
}

#[derive(Clone)]
struct NullAgent {
    info: AgentInfo,
}

impl NullAgent {
    fn new(info: AgentInfo) -> Self {
        Self { info }
    }
}

impl Agent for NullAgent {
    fn send(&self, _message: UserMessage) -> AgentStream {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(StreamEvent::TextDelta(
            "Bridge not connected yet. Configure the Node.js bridge to enable replies.".to_string(),
        ));
        let _ = tx.send(StreamEvent::Done);
        AgentStream { events: rx }
    }

    fn abort(&self) {}

    fn status(&self) -> AgentStatus {
        AgentStatus::Idle
    }

    fn info(&self) -> AgentInfo {
        self.info.clone()
    }
}
