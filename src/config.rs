use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_SERVER: &str = "https://vulnhash.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: String,
    #[serde(default)]
    pub auth_token: String,
    /// Route every request through Tor without needing --tor each time.
    #[serde(default)]
    pub tor: bool,
    /// Keep no Tor state on disk. Slower to start, leaves no trace of use.
    #[serde(default)]
    pub tor_ephemeral: bool,
}

fn default_server() -> String {
    DEFAULT_SERVER.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: DEFAULT_SERVER.to_string(),
            auth_token: String::new(),
            tor: false,
            tor_ephemeral: false,
        }
    }
}

/// Root of the local store.
///
/// VULNHASH_HOME overrides the default, which is what keeps two stores apart
/// on a shared machine and what gives the tests a directory of their own.
pub fn dir() -> PathBuf {
    if let Some(root) = root_override() {
        return root;
    }
    match std::env::var_os("VULNHASH_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".vulnhash"),
    }
}

#[cfg(test)]
thread_local! {
    static TEST_ROOT: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Point this thread's store at `path`. Per-thread rather than per-process so
/// that tests running side by side cannot tread on each other.
#[cfg(test)]
pub fn set_dir_for_test(path: PathBuf) {
    TEST_ROOT.with(|r| *r.borrow_mut() = Some(path));
}

#[cfg(test)]
fn root_override() -> Option<PathBuf> {
    TEST_ROOT.with(|r| r.borrow().clone())
}

#[cfg(not(test))]
fn root_override() -> Option<PathBuf> {
    None
}

pub fn path() -> PathBuf {
    dir().join("config.json")
}

pub fn load() -> Config {
    let p = path();
    match std::fs::read_to_string(&p) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

impl Config {
    pub fn save(&self) -> Result<(), String> {
        let d = dir();
        std::fs::create_dir_all(&d).map_err(|e| format!("create config dir: {e}"))?;
        let data = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(path(), data).map_err(|e| format!("write config: {e}"))?;
        Ok(())
    }
}
