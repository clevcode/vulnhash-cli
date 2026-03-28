use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_SERVER: &str = "https://vulnhash.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: String,
    #[serde(default)]
    pub auth_token: String,
}

fn default_server() -> String {
    DEFAULT_SERVER.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: DEFAULT_SERVER.to_string(),
            auth_token: String::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".vulnhash")
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
