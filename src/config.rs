use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_SERVER: &str = "https://vulnhash.com";
const DEFAULT_BASE_RPC: &str = "https://mainnet.base.org";
const BUILT_IN_ONION_SERVER: &str = match option_env!("VULNHASH_OFFICIAL_ONION_SERVER") {
    Some(value) => value,
    None => "",
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: String,
    /// True when the user deliberately selected `server`, including the
    /// clearnet default. Over Tor that makes an exit connection explicit.
    #[serde(default)]
    pub server_explicit: bool,
    /// Optional override for the official onion endpoint compiled into a
    /// signed release. Kept distinct from the clearnet server.
    #[serde(default)]
    pub official_onion_server: String,
    #[serde(default)]
    pub auth_token: String,
    /// Read-only endpoint used to independently check Base anchor evidence.
    #[serde(default = "default_base_rpc")]
    pub base_rpc: String,
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

fn default_base_rpc() -> String {
    DEFAULT_BASE_RPC.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: DEFAULT_SERVER.to_string(),
            server_explicit: false,
            official_onion_server: String::new(),
            auth_token: String::new(),
            base_rpc: DEFAULT_BASE_RPC.to_string(),
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
    pub fn tor_server(&self) -> Result<(String, TorRoute), String> {
        let explicitly_configured = self.server_explicit || self.server != DEFAULT_SERVER;
        if explicitly_configured {
            let route = if is_onion_url(&self.server) {
                validate_onion_url(&self.server)?;
                TorRoute::Onion
            } else {
                let parsed = url::Url::parse(&self.server)
                    .map_err(|e| format!("invalid configured server URL: {e}"))?;
                if parsed.scheme() != "https" {
                    return Err("clearnet-over-Tor requires an HTTPS server URL".into());
                }
                TorRoute::Exit
            };
            return Ok((self.server.clone(), route));
        }

        let onion = if self.official_onion_server.is_empty() {
            BUILT_IN_ONION_SERVER
        } else {
            &self.official_onion_server
        };
        validate_onion_url(onion).map_err(|error| {
            if onion.is_empty() {
                "Tor was requested but this build has no official onion endpoint; configure one with 'vulnhash config onion-server http://<address>.onion', or explicitly select clearnet-over-exit with 'vulnhash config server https://vulnhash.com'".to_string()
            } else {
                format!("invalid configured official onion endpoint: {error}")
            }
        })?;
        Ok((onion.to_string(), TorRoute::Onion))
    }

    pub fn effective_onion_server(&self) -> &str {
        if self.official_onion_server.is_empty() {
            BUILT_IN_ONION_SERVER
        } else {
            &self.official_onion_server
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let d = dir();
        std::fs::create_dir_all(&d).map_err(|e| format!("create config dir: {e}"))?;
        let data = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(path(), data).map_err(|e| format!("write config: {e}"))?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TorRoute {
    Onion,
    Exit,
}

pub fn is_onion_url(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.ends_with(".onion")))
        .unwrap_or(false)
}

pub fn validate_onion_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|e| format!("invalid onion URL: {e}"))?;
    if url.scheme() != "http" {
        return Err("official onion URL must use http (Tor provides transport encryption)".into());
    }
    if url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("official onion URL must contain only the v3 onion host".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "official onion URL has no host".to_string())?;
    let id = host
        .strip_suffix(".onion")
        .ok_or_else(|| "official onion URL does not end in .onion".to_string())?;
    if id.len() != 56
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    {
        return Err("official onion URL is not a canonical v3 onion address".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onion() -> String {
        format!("http://{}.onion", "a".repeat(56))
    }

    #[test]
    fn only_canonical_v3_onion_origins_are_accepted() {
        assert!(validate_onion_url(&onion()).is_ok());
        assert!(validate_onion_url("https://example.onion").is_err());
        assert!(validate_onion_url("http://short.onion").is_err());
        assert!(validate_onion_url(&format!("{}/path", onion())).is_err());
        assert!(validate_onion_url(&format!("http://{}.onion", "0".repeat(56))).is_err());
    }

    #[test]
    fn tor_prefers_onion_unless_clearnet_exit_was_explicit() {
        let mut cfg = Config {
            official_onion_server: onion(),
            ..Config::default()
        };
        assert_eq!(cfg.tor_server().unwrap(), (onion(), TorRoute::Onion));

        cfg.server_explicit = true;
        assert_eq!(
            cfg.tor_server().unwrap(),
            ("https://vulnhash.com".into(), TorRoute::Exit)
        );

        cfg.server = onion();
        assert_eq!(cfg.tor_server().unwrap(), (onion(), TorRoute::Onion));
    }

    #[test]
    #[allow(clippy::const_is_empty)]
    fn tor_fails_closed_without_an_onion_or_explicit_exit() {
        let cfg = Config::default();
        if BUILT_IN_ONION_SERVER.is_empty() {
            assert!(cfg.tor_server().unwrap_err().contains("no official onion"));
        }
    }
}
