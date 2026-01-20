use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::session::{SessionId, SessionMessage, SessionRole};

pub struct SessionStorage {
    root: PathBuf,
}

#[derive(Debug)]
pub struct StoredSession {
    pub id: SessionId,
    pub title: String,
    pub agent_id: String,
    pub agent_name: String,
    pub messages: Vec<SessionMessage>,
}

impl SessionStorage {
    pub fn new() -> Self {
        Self {
            root: config::data_dir().join("sessions"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save_session(&self, session: &crate::session::Session) -> io::Result<()> {
        let dir = self.session_dir(session.id);
        fs::create_dir_all(&dir)?;
        self.write_meta(&dir, session)?;
        self.write_messages(&dir, &session.messages)?;
        Ok(())
    }

    pub fn load_sessions(&self) -> io::Result<Vec<StoredSession>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let meta_path = dir.join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            let meta_bytes = fs::read(&meta_path)?;
            let meta: Meta = serde_json::from_slice(&meta_bytes)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let messages = read_messages(&dir.join("messages.jsonl"))?;
            sessions.push(StoredSession {
                id: SessionId::new(meta.id),
                title: meta.title,
                agent_id: meta.agent_id,
                agent_name: meta.agent_name,
                messages,
            });
        }

        sessions.sort_by_key(|session| session.id.value());
        Ok(sessions)
    }

    fn session_dir(&self, id: SessionId) -> PathBuf {
        self.root.join(format!("session-{}", id.value()))
    }

    fn write_meta(&self, dir: &Path, session: &crate::session::Session) -> io::Result<()> {
        let meta_path = dir.join("meta.json");
        let payload = Meta {
            id: session.id.value(),
            title: session.title.clone(),
            agent_id: session.agent.id.clone(),
            agent_name: session.agent.name.clone(),
        };
        let data = serde_json::to_vec(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        fs::write(meta_path, data)
    }

    fn write_messages(&self, dir: &Path, messages: &[SessionMessage]) -> io::Result<()> {
        let messages_path = dir.join("messages.jsonl");
        let mut file = fs::File::create(messages_path)?;
        for message in messages {
            let ts = message
                .timestamp
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs();
            let payload = MessageLine {
                role: message.role.as_str().to_string(),
                content: message.content.clone(),
                ts,
            };
            let line = serde_json::to_string(&payload)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Meta {
    id: u64,
    title: String,
    agent_id: String,
    agent_name: String,
}

#[derive(Serialize, Deserialize)]
struct MessageLine {
    role: String,
    content: String,
    ts: u64,
}

fn read_messages(path: &Path) -> io::Result<Vec<SessionMessage>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let payload: MessageLine = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let role = SessionRole::from_str(&payload.role).unwrap_or(SessionRole::User);
        let timestamp = UNIX_EPOCH + Duration::from_secs(payload.ts);
        messages.push(SessionMessage {
            role,
            content: payload.content,
            timestamp,
        });
    }
    Ok(messages)
}
