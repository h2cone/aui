use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub default_provider_id: String,
    pub debug: bool,
}

impl Config {
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        let mut debug = false;
        let mut default_provider_id = "anthropic".to_string();
        if let Ok(contents) = fs::read_to_string(path) {
            // Backward compatible with older configs.
            if let Some(value) = parse_value(&contents, "default_provider_id")
                .or_else(|| parse_value(&contents, "default_agent_id"))
            {
                default_provider_id = value;
            }
            debug = parse_bool(&contents, "debug")
                .or_else(|| parse_bool(&contents, "debug_mode"))
                .unwrap_or(false);
        }
        Self {
            default_provider_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_value_reads_key() {
        let contents = r#"
# comment
default_provider_id = "codex-cli"
debug = true
"#;
        assert_eq!(
            parse_value(contents, "default_provider_id"),
            Some("codex-cli".to_string())
        );
        assert_eq!(parse_value(contents, "missing"), None);
    }

    #[test]
    fn parse_bool_accepts_values() {
        let contents = r#"
debug = true
debug_mode = "no"
"#;
        assert_eq!(parse_bool(contents, "debug"), Some(true));
        assert_eq!(parse_bool(contents, "debug_mode"), Some(false));
        assert_eq!(parse_bool(contents, "missing"), None);
    }

    #[test]
    fn parse_bool_value_cases() {
        assert_eq!(parse_bool_value("YES"), Some(true));
        assert_eq!(parse_bool_value("off"), Some(false));
        assert_eq!(parse_bool_value("maybe"), None);
    }
}
