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
            cfg.server = v.to_string();
            cfg.save()?;
            if json {
                output::print(&serde_json::json!({"status": "ok", "key": "server"}));
            } else {
                println!("Server set to: {v}");
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
                "unknown config key: {k} (valid: server, token, tor, tor-ephemeral)"
            ));
        }
        (None, _) if json => {
            // The token itself is never emitted: a script that needs it can
            // read the config file, and printing it invites it into logs.
            output::print(&serde_json::json!({
                "server": cfg.server,
                "auth_token_set": !cfg.auth_token.is_empty(),
                "tor": cfg.tor,
                "tor_ephemeral": cfg.tor_ephemeral,
                "config_path": config::path().display().to_string(),
                "data_dir": config::dir().display().to_string(),
            }));
        }
        (None, _) => {
            println!("Server:        {}", cfg.server);
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

#[cfg(test)]
mod tests {
    use super::parse_bool;

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
}
