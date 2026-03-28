use crate::config;

pub fn run(key: Option<&str>, value: Option<&str>) -> Result<(), String> {
    let mut cfg = config::load();

    match (key, value) {
        (Some("server"), Some(v)) => {
            cfg.server = v.to_string();
            cfg.save()?;
            println!("Server set to: {v}");
        }
        (Some("token"), Some(v)) => {
            cfg.auth_token = v.to_string();
            cfg.save()?;
            println!("Auth token saved.");
        }
        (Some(k), _) => {
            return Err(format!("unknown config key: {k} (valid: server, token)"));
        }
        (None, _) => {
            println!("Server:     {}", cfg.server);
            if !cfg.auth_token.is_empty() && cfg.auth_token.len() > 10 {
                println!("Auth Token: {}...{}", &cfg.auth_token[..6], &cfg.auth_token[cfg.auth_token.len()-4..]);
            } else if !cfg.auth_token.is_empty() {
                println!("Auth Token: (set)");
            } else {
                println!("Auth Token: (not set)");
            }
            println!("Config:     {}", config::path().display());
            println!("Data:       {}", config::dir().display());
        }
    }
    Ok(())
}
