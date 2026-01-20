use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::agent::{
    Agent, AgentInfo, AgentStatus, AgentStream, Attachment, StreamEvent, UserMessage,
    WorkingContext,
};
use crate::logger;

pub struct BridgeClient {
    agents: Vec<AgentInfo>,
    process: Option<Arc<BridgeProcess>>,
    id_source: Arc<AtomicU64>,
}

impl BridgeClient {
    pub fn new() -> Self {
        let agents = crate::agent::adapters::available_agents();
        logger::debug(&format!("bridge client init agents={}", agents.len()));
        let process = match BridgeProcess::spawn() {
            Ok(process) => Some(Arc::new(process)),
            Err(err) => {
                logger::warn(&format!("bridge spawn failed: {err}"));
                None
            }
        };

        Self {
            agents,
            process,
            id_source: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn agents(&self) -> &[AgentInfo] {
        &self.agents
    }

    pub fn agent_by_id(&self, id: &str) -> Option<AgentInfo> {
        self.agents.iter().find(|agent| agent.id == id).cloned()
    }

    pub fn connect(&self, info: &AgentInfo) -> Box<dyn Agent> {
        match &self.process {
            Some(process) => Box::new(BridgeAgent::new(
                info.clone(),
                Arc::clone(process),
                Arc::clone(&self.id_source),
            )),
            None => Box::new(NullAgent::new(info.clone())),
        }
    }
}

struct BridgeProcess {
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<BridgeResponse>>>>,
    _child: Mutex<Child>,
}

impl BridgeProcess {
    fn spawn() -> io::Result<Self> {
        let entry = bridge_entry_path()?;
        logger::debug(&format!("bridge entry path={}", entry.display()));
        if !entry.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("bridge entry missing: {}", entry.display()),
            ));
        }

        let mut child = Command::new("node")
            .arg(entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "bridge stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "bridge stdout unavailable"))?;

        let process = Self {
            stdin: Mutex::new(stdin),
            pending: Arc::new(Mutex::new(HashMap::new())),
            _child: Mutex::new(child),
        };

        logger::debug("bridge process spawned");
        process.start_reader(stdout);
        Ok(process)
    }

    fn start_reader(&self, stdout: impl io::Read + Send + 'static) {
        let pending = Arc::clone(&self.pending);
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<BridgeResponse>(&line) {
                            Ok(response) => {
                                let terminal = response.is_terminal();
                                let id = response.id.clone();
                                if let Ok(mut map) = pending.lock() {
                                    if let Some(sender) = map.get(&id) {
                                        if sender.send(response).is_err() {
                                            map.remove(&id);
                                            continue;
                                        }
                                    } else {
                                        logger::debug(&format!(
                                            "bridge response without pending id={}",
                                            id
                                        ));
                                    }
                                    if terminal {
                                        map.remove(&id);
                                    }
                                }
                            }
                            Err(err) => {
                                logger::warn(&format!("bridge parse error: {err}"));
                            }
                        }
                    }
                    Err(err) => {
                        logger::warn(&format!("bridge read error: {err}"));
                        break;
                    }
                }
            }
        });
    }

    fn request(&self, request: BridgeRequest) -> Result<mpsc::Receiver<BridgeResponse>, String> {
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| "bridge pending lock failed".to_string())?;
            pending.insert(request.id.clone(), tx);
        }

        let payload = serde_json::to_string(&request)
            .map_err(|err| format!("bridge serialize failed: {err}"))?;
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| "bridge stdin lock failed".to_string())?;
        stdin
            .write_all(payload.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|err| format!("bridge send failed: {err}"))?;

        Ok(rx)
    }
}

#[derive(Clone)]
struct BridgeAgent {
    info: AgentInfo,
    process: Arc<BridgeProcess>,
    id_source: Arc<AtomicU64>,
}

impl BridgeAgent {
    fn new(info: AgentInfo, process: Arc<BridgeProcess>, id_source: Arc<AtomicU64>) -> Self {
        Self {
            info,
            process,
            id_source,
        }
    }
}

impl Agent for BridgeAgent {
    fn send(&self, message: UserMessage) -> AgentStream {
        let (events_tx, events_rx) = mpsc::channel();
        let id = self.id_source.fetch_add(1, Ordering::Relaxed).to_string();
        let text_len = message.text.len();
        let attachments_len = message.attachments.len();
        let cwd = message
            .context
            .as_ref()
            .and_then(|context| context.cwd.as_ref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string());
        logger::debug(&format!(
            "bridge send id={} agent={} text_len={} attachments={} cwd={}",
            id.as_str(),
            self.info.id.as_str(),
            text_len,
            attachments_len,
            cwd
        ));
        let request = BridgeRequest {
            id: id.clone(),
            method: "send".to_string(),
            params: BridgeParams::from_message(&self.info, message),
        };

        match self.process.request(request) {
            Ok(rx) => {
                thread::spawn(move || {
                    loop {
                        match rx.recv() {
                            Ok(response) => {
                                if handle_bridge_response(response, &events_tx) {
                                    break;
                                }
                            }
                            Err(_) => {
                                logger::warn("bridge response channel closed");
                                let _ = events_tx.send(StreamEvent::Error(
                                    "bridge response channel closed".to_string(),
                                ));
                                break;
                            }
                        }
                    }
                });
            }
            Err(err) => {
                logger::warn(&format!("bridge request failed: {err}"));
                let _ = events_tx.send(StreamEvent::Error(err));
            }
        }

        AgentStream { events: events_rx }
    }

    fn abort(&self) {}

    fn status(&self) -> AgentStatus {
        AgentStatus::Idle
    }

    fn info(&self) -> AgentInfo {
        self.info.clone()
    }
}

#[derive(Serialize)]
struct BridgeRequest {
    id: String,
    method: String,
    params: BridgeParams,
}

#[derive(Serialize)]
struct BridgeParams {
    agent_id: String,
    text: String,
    attachments: Vec<BridgeAttachment>,
    context: Option<BridgeContext>,
}

impl BridgeParams {
    fn from_message(info: &AgentInfo, message: UserMessage) -> Self {
        Self {
            agent_id: info.id.clone(),
            text: message.text,
            attachments: message
                .attachments
                .into_iter()
                .map(BridgeAttachment::from)
                .collect(),
            context: message.context.map(BridgeContext::from),
        }
    }
}

#[derive(Serialize)]
struct BridgeAttachment {
    name: String,
    path: Option<String>,
}

impl From<Attachment> for BridgeAttachment {
    fn from(value: Attachment) -> Self {
        Self {
            name: value.name,
            path: value.path.map(|path| path.display().to_string()),
        }
    }
}

#[derive(Serialize)]
struct BridgeContext {
    cwd: Option<String>,
}

impl From<WorkingContext> for BridgeContext {
    fn from(value: WorkingContext) -> Self {
        Self {
            cwd: value.cwd.map(|path| path.display().to_string()),
        }
    }
}

#[derive(Deserialize)]
struct BridgeResponse {
    id: String,
    event: Option<BridgeEvent>,
    result: Option<BridgeResult>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct BridgeResult {
    text: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeEvent {
    TextDelta { delta: String },
    ToolStart { name: String, input: String },
    ToolResult { name: String, output: String },
    TokenUsage { input: u32, output: u32 },
    Done,
    Error { message: String },
}

impl BridgeResponse {
    fn is_terminal(&self) -> bool {
        if self.error.is_some() || self.result.is_some() {
            return true;
        }
        matches!(
            self.event,
            Some(BridgeEvent::Done | BridgeEvent::Error { .. })
        )
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
            "Bridge not running. Build bridge and restart the app.".to_string(),
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

fn stream_text(text: String, tx: &mpsc::Sender<StreamEvent>) {
    let mut chunk = String::new();
    for ch in text.chars() {
        chunk.push(ch);
        if chunk.len() >= 64 {
            let _ = tx.send(StreamEvent::TextDelta(chunk.clone()));
            chunk.clear();
        }
    }
    if !chunk.is_empty() {
        let _ = tx.send(StreamEvent::TextDelta(chunk));
    }
}

fn handle_bridge_response(response: BridgeResponse, tx: &mpsc::Sender<StreamEvent>) -> bool {
    if let Some(error) = response.error {
        logger::warn(&format!(
            "bridge response error id={} error={}",
            response.id.as_str(),
            error.as_str()
        ));
        let _ = tx.send(StreamEvent::Error(error));
        return true;
    }

    if let Some(result) = response.result {
        logger::debug(&format!(
            "bridge response ok id={} text_len={}",
            response.id.as_str(),
            result.text.len()
        ));
        stream_text(result.text, tx);
        let _ = tx.send(StreamEvent::Done);
        return true;
    }

    if let Some(event) = response.event {
        return handle_bridge_event(response.id, event, tx);
    }

    logger::debug(&format!(
        "bridge response empty id={}",
        response.id.as_str()
    ));
    let _ = tx.send(StreamEvent::Done);
    true
}

fn handle_bridge_event(id: String, event: BridgeEvent, tx: &mpsc::Sender<StreamEvent>) -> bool {
    match event {
        BridgeEvent::TextDelta { delta } => {
            logger::debug(&format!(
                "bridge event text_delta id={} len={}",
                id.as_str(),
                delta.len()
            ));
            let _ = tx.send(StreamEvent::TextDelta(delta));
            false
        }
        BridgeEvent::ToolStart { name, input } => {
            logger::debug(&format!(
                "bridge event tool_start id={} tool={}",
                id.as_str(),
                name.as_str()
            ));
            let _ = tx.send(StreamEvent::ToolStart { name, input });
            false
        }
        BridgeEvent::ToolResult { name, output } => {
            logger::debug(&format!(
                "bridge event tool_result id={} tool={}",
                id.as_str(),
                name.as_str()
            ));
            let _ = tx.send(StreamEvent::ToolResult { name, output });
            false
        }
        BridgeEvent::TokenUsage { input, output } => {
            logger::debug(&format!(
                "bridge event token_usage id={} in={} out={}",
                id.as_str(),
                input,
                output
            ));
            let _ = tx.send(StreamEvent::TokenUsage { input, output });
            false
        }
        BridgeEvent::Done => {
            logger::debug(&format!("bridge event done id={}", id.as_str()));
            let _ = tx.send(StreamEvent::Done);
            true
        }
        BridgeEvent::Error { message } => {
            logger::warn(&format!(
                "bridge event error id={} message={}",
                id.as_str(),
                message.as_str()
            ));
            let _ = tx.send(StreamEvent::Error(message));
            true
        }
    }
}

fn bridge_entry_path() -> io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(cwd.join("bridge").join("dist").join("index.js"))
}
