use crate::{config, output};

/// Parse the spellings people actually type for a boolean setting.
fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "y" | "1" | "enable" | "enabled" => Ok(true),
        "off" | "false" | "no" | "n" | "0" | "disable" | "disabled" => Ok(false),
        other => Err(format!("expected on or off, got {other:?}")),
    }
}

pub fn run(key: Option<&str>, value: Option<&str>, json: bool) -> Result<(), String> {
    let mut cfg = config::load();

    match (key, value) {
        (Some("server"), Some(v)) => {
            if config::is_onion_url(v) {
                config::validate_onion_url(v)?;
            }
            cfg.server = v.to_string();
            cfg.server_explicit = true;
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({"status": "ok", "key": "server"}));
            } else {
                println!("Server set to: {v}");
            }
        }
        (Some("onion-server"), Some(v)) => {
            if matches!(v, "off" | "none" | "") {
                cfg.official_onion_server.clear();
            } else {
                config::validate_onion_url(v)?;
                cfg.official_onion_server = v.to_string();
            }
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({
                    "status": "ok",
                    "key": "onion-server",
                    "host": endpoint_host(cfg.effective_onion_server()),
                }));
            } else if cfg.effective_onion_server().is_empty() {
                println!("No official onion endpoint is configured.");
            } else {
                println!(
                    "Official onion endpoint set to: {}",
                    endpoint_host(cfg.effective_onion_server())
                );
            }
        }
        (Some("token"), Some(v)) => {
            cfg.auth_token = v.to_string();
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({"status": "ok", "key": "token"}));
            } else {
                println!("Auth token saved.");
            }
        }
        (Some("base-rpc"), Some(v)) => {
            validate_rpc_url(v)?;
            cfg.base_rpc = v.to_string();
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({
                    "status": "ok",
                    "key": "base-rpc",
                    "host": rpc_host(v),
                }));
            } else {
                println!("Base RPC set to host: {}", rpc_host(v));
            }
        }
        (Some("tor"), Some(v)) => {
            cfg.tor = parse_bool(v)?;
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({"status": "ok", "key": "tor", "value": cfg.tor}));
            } else if cfg.tor {
                println!("Tor enabled: requests will be routed through Tor.");
            } else {
                println!("Tor disabled: requests go direct.");
            }
        }
        (Some("tor-ephemeral"), Some(v)) => {
            cfg.tor_ephemeral = parse_bool(v)?;
            cfg.save()?;
            if json {
                output::print(
                    &serde_json::json!({"status": "ok", "key": "tor-ephemeral", "value": cfg.tor_ephemeral}),
                );
            } else if cfg.tor_ephemeral {
                println!("Ephemeral Tor enabled: no Tor state is kept on disk.");
            } else {
                println!("Ephemeral Tor disabled: Tor state is cached between runs.");
            }
        }
        (Some(k), None) => {
            return Err(format!("no value given for {k}"));
        }
        (Some(k), _) => {
            return Err(format!(
                "unknown config key: {k} (valid: server, onion-server, token, base-rpc, tor, tor-ephemeral)"
            ));
        }
        (None, _) if json => {
            // The token itself is never emitted: a script that needs it can
            // read the config file, and printing it invites it into logs.
            output::print(&serde_json::json!({
                "server": cfg.server,
                "server_explicit": cfg.server_explicit,
                "official_onion_host": endpoint_host(cfg.effective_onion_server()),
                "auth_token_set": !cfg.auth_token.is_empty(),
                "base_rpc_host": rpc_host(&cfg.base_rpc),
                "tor": cfg.tor,
                "tor_ephemeral": cfg.tor_ephemeral,
                "config_path": config::path().display().to_string(),
                "data_dir": config::dir().display().to_string(),
            }));
        }
        (None, _) => {
            println!("Server:        {}", cfg.server);
            println!(
                "Onion:         {}",
                endpoint_host(cfg.effective_onion_server())
            );
            if cfg.auth_token.len() > 10 {
                println!(
                    "Auth Token:    {}...{}",
                    &cfg.auth_token[..6],
                    &cfg.auth_token[cfg.auth_token.len() - 4..]
                );
            } else if !cfg.auth_token.is_empty() {
                println!("Auth Token:    (set)");
            } else {
                println!("Auth Token:    (not set)");
            }
            println!("Tor:           {}", if cfg.tor { "on" } else { "off" });
            println!("Base RPC:      {}", rpc_host(&cfg.base_rpc));
            println!(
                "Tor Ephemeral: {}",
                if cfg.tor_ephemeral { "on" } else { "off" }
            );
            println!("Config:        {}", config::path().display());
            println!("Data:          {}", config::dir().display());
        }
    }
    Ok(())
}

fn validate_rpc_url(value: &str) -> Result<(), String> {
    let parsed = url::Url::parse(value).map_err(|e| format!("invalid Base RPC URL: {e}"))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1")) => Ok(()),
        "http" => {
            Err("Base RPC must use HTTPS unless it is a loopback development endpoint".into())
        }
        scheme => Err(format!("unsupported Base RPC URL scheme {scheme:?}")),
    }
}

fn rpc_host(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "(invalid)".into())
}

fn endpoint_host(value: &str) -> String {
    if value.is_empty() {
        return "(not configured)".into();
    }
    rpc_host(value)
}

#[cfg(test)]
mod tests {
    use super::{parse_bool, rpc_host, validate_rpc_url};

    #[test]
    fn accepts_the_usual_spellings() {
        for v in ["on", "ON", "true", "yes", "y", "1", " enabled "] {
            assert_eq!(parse_bool(v), Ok(true), "{v}");
        }
        for v in ["off", "OFF", "false", "no", "n", "0", "disabled"] {
            assert_eq!(parse_bool(v), Ok(false), "{v}");
        }
    }

    #[test]
    fn rejects_anything_else() {
        assert!(parse_bool("maybe").is_err());
        assert!(parse_bool("").is_err());
    }

    #[test]
    fn rpc_urls_require_tls_except_on_loopback() {
        assert!(validate_rpc_url("https://mainnet.base.org").is_ok());
        assert!(validate_rpc_url("http://127.0.0.1:8545").is_ok());
        assert!(validate_rpc_url("http://rpc.example").is_err());
        assert_eq!(
            rpc_host("https://secret.example/v1/api-key"),
            "secret.example"
        );
    }
}
