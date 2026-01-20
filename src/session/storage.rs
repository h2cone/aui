use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::config;
use crate::session::{Session, SessionId};

pub struct SessionStorage {
    root: PathBuf,
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

    pub fn save_session(&self, session: &Session) -> io::Result<()> {
        let dir = self.session_dir(session.id);
        fs::create_dir_all(&dir)?;
        self.write_meta(&dir, session)?;
        self.write_messages(&dir, session)?;
        Ok(())
    }

    pub fn load_sessions(&self) -> io::Result<Vec<Session>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        Ok(Vec::new())
    }

    fn session_dir(&self, id: SessionId) -> PathBuf {
        self.root.join(format!("session-{}", id.value()))
    }

    fn write_meta(&self, dir: &Path, session: &Session) -> io::Result<()> {
        let meta_path = dir.join("meta.json");
        let payload = format!(
            "{{\"id\":{},\"title\":\"{}\",\"agent_id\":\"{}\"}}",
            session.id.value(),
            json_escape(&session.title),
            json_escape(session.agent.id)
        );
        fs::write(meta_path, payload)
    }

    fn write_messages(&self, dir: &Path, session: &Session) -> io::Result<()> {
        let messages_path = dir.join("messages.jsonl");
        let mut file = fs::File::create(messages_path)?;
        for message in &session.messages {
            let ts = message
                .timestamp
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let line = format!(
                "{{\"role\":\"{}\",\"content\":\"{}\",\"ts\":{}}}\n",
                message.role.as_str(),
                json_escape(&message.content),
                ts
            );
            file.write_all(line.as_bytes())?;
        }
        Ok(())
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}
