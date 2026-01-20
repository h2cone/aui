use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub default_agent_id: String,
}

impl Config {
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        if let Ok(contents) = fs::read_to_string(path) {
            if let Some(value) = parse_value(&contents, "default_agent_id") {
                return Self {
                    default_agent_id: value,
                };
            }
        }
        Self {
            default_agent_id: "claude-code".to_string(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::var_os("USERPROFILE")
                    .map(|home| PathBuf::from(home).join("AppData").join("Roaming"))
                    .unwrap_or_else(|| PathBuf::from("."))
            })
            .join("aui")
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("aui")
    }
}

pub fn data_dir() -> PathBuf {
    config_dir()
}

fn parse_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return None;
        }
        let (left, right) = trimmed.split_once('=')?;
        if left.trim() != key {
            return None;
        }
        let value = right.trim().trim_matches('"');
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}
