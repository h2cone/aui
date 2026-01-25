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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredDiffDecision {
    pub message_index: usize,
    pub block_index: usize,
    pub accepted: bool,
}

impl SessionStorage {
    pub fn new() -> Self {
        Self {
            root: config::data_dir().join("sessions"),
        }
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
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

    pub fn delete_session(&self, id: SessionId) -> io::Result<()> {
        let dir = self.session_dir(id);
        if !dir.exists() {
            return Ok(());
        }
        fs::remove_dir_all(dir)
    }

    pub fn save_diff_decisions(
        &self,
        id: SessionId,
        decisions: &[StoredDiffDecision],
    ) -> io::Result<()> {
        let dir = self.session_dir(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("diff_decisions.json");
        let data = serde_json::to_vec(decisions)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        fs::write(path, data)
    }

    pub fn load_diff_decisions(&self, id: SessionId) -> io::Result<Vec<StoredDiffDecision>> {
        let dir = self.session_dir(id);
        let path = dir.join("diff_decisions.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(path)?;
        let decisions: Vec<StoredDiffDecision> = serde_json::from_slice(&bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        Ok(decisions)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use crate::agent::{AgentInfo, AgentKind, AgentStatus};
    use crate::session::{Session, SessionMessage, SessionRole, SessionStats};

    fn sample_session(id: u64) -> Session {
        let timestamp = UNIX_EPOCH + Duration::from_secs(123);
        Session {
            id: SessionId::new(id),
            title: "alpha".to_string(),
            agent: AgentInfo::new("test", "Test Agent", AgentKind::Claude),
            status: AgentStatus::Idle,
            stats: SessionStats::new(),
            messages: vec![
                SessionMessage {
                    role: SessionRole::User,
                    content: "hello".to_string(),
                    timestamp,
                },
                SessionMessage {
                    role: SessionRole::Assistant,
                    content: "world".to_string(),
                    timestamp,
                },
            ],
        }
    }

    #[test]
    fn load_sessions_empty_when_missing() {
        let dir = tempdir().expect("tempdir");
        let missing_root = dir.path().join("missing");
        let storage = SessionStorage::with_root(missing_root);
        let sessions = storage.load_sessions().expect("load");
        assert!(sessions.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let session = sample_session(1);
        storage.save_session(&session).expect("save");

        let loaded = storage.load_sessions().expect("load");
        assert_eq!(loaded.len(), 1);
        let stored = &loaded[0];
        assert_eq!(stored.id.value(), session.id.value());
        assert_eq!(stored.title, session.title);
        assert_eq!(stored.agent_id, session.agent.id);
        assert_eq!(stored.agent_name, session.agent.name);
        assert_eq!(stored.messages.len(), session.messages.len());
        assert_eq!(stored.messages[0].content, "hello");
        let ts = stored.messages[0]
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(ts, 123);
    }

    #[test]
    fn delete_session_removes_dir() {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let session = sample_session(7);
        storage.save_session(&session).expect("save");
        let session_dir = storage.root().join("session-7");
        assert!(session_dir.exists());
        storage.delete_session(session.id).expect("delete");
        assert!(!session_dir.exists());
    }

    #[test]
    fn save_and_load_diff_decisions_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let storage = SessionStorage::with_root(dir.path().join("sessions"));
        let id = SessionId::new(42);
        let decisions = vec![
            StoredDiffDecision {
                message_index: 1,
                block_index: 0,
                accepted: true,
            },
            StoredDiffDecision {
                message_index: 3,
                block_index: 2,
                accepted: false,
            },
        ];
        storage.save_diff_decisions(id, &decisions).expect("save");
        let loaded = storage.load_diff_decisions(id).expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].message_index, 1);
        assert!(loaded[0].accepted);
        assert_eq!(loaded[1].block_index, 2);
        assert!(!loaded[1].accepted);
    }
}
