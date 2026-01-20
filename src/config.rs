use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub default_agent_id: String,
    pub debug: bool,
}

impl Config {
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        let mut debug = false;
        let mut default_agent_id = "claude-code".to_string();
        if let Ok(contents) = fs::read_to_string(path) {
            if let Some(value) = parse_value(&contents, "default_agent_id") {
                default_agent_id = value;
            }
            debug = parse_bool(&contents, "debug")
                .or_else(|| parse_bool(&contents, "debug_mode"))
                .unwrap_or(false);
        }
        Self {
            default_agent_id,
            debug,
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

pub fn debug_enabled() -> bool {
    Config::load().debug
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

fn parse_bool(contents: &str, key: &str) -> Option<bool> {
    contents.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return None;
        }
        let (left, right) = trimmed.split_once('=')?;
        if left.trim() != key {
            return None;
        }
        parse_bool_value(right.trim().trim_matches('"'))
    })
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
