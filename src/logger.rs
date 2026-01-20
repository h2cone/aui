use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static VERBOSE: OnceLock<bool> = OnceLock::new();

pub fn init() {
    let _ = log_internal("INFO", "logger init");
}

pub fn info(message: &str) {
    let _ = log_internal("INFO", message);
}

pub fn warn(message: &str) {
    let _ = log_internal("WARN", message);
}

#[allow(dead_code)]
pub fn error(message: &str) {
    let _ = log_internal("ERROR", message);
}

pub fn debug(message: &str) {
    if verbose_enabled() {
        let _ = log_internal("DEBUG", message);
    }
}

pub fn log_path() -> PathBuf {
    LOG_PATH
        .get_or_init(|| {
            let dir = config_dir().unwrap_or_else(|| PathBuf::from("."));
            dir.join("aui").join("aui.log")
        })
        .clone()
}

fn verbose_enabled() -> bool {
    *VERBOSE.get_or_init(|| match env::var("AUI_DEBUG") {
        Ok(value) => matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"),
        Err(_) => false,
    })
}

fn log_internal(level: &str, message: &str) -> std::io::Result<()> {
    let Some(file_lock) = open_log_file() else {
        return Ok(());
    };
    let mut file = file_lock
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "log lock poisoned"))?;
    writeln!(file, "{} [{}] {}", timestamp(), level, message)?;
    file.flush()?;
    Ok(())
}

fn open_log_file() -> Option<&'static Mutex<File>> {
    LOG_FILE
        .get_or_init(|| {
            let path = log_path();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
                .map(Mutex::new)
        })
        .as_ref()
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
            env::var_os("USERPROFILE")
                .map(|home| PathBuf::from(home).join("AppData").join("Roaming"))
        })
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
}
