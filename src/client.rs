use serde::Deserialize;
use crate::config::Config;

/// The parts of a submission the CLI has a use for.
///
/// The server sends more — title, tags, parent id, publication time — and
/// serde drops what is not named here. Everything the CLI shows about a
/// submission it holds locally, so there is nothing to gain by mirroring the
/// full record.
#[derive(Debug, Deserialize)]
pub struct HashResponse {
    pub hash: String,
    pub published: bool,
    pub submitted_at: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyResponse {
    pub hash: String,
    pub exists: bool,
    pub submitted_at: Option<String>,
    pub published: bool,
    pub username: Option<String>,
}

pub struct Client {
    base_url: String,
    auth_token: String,
    tor: Option<vulnhash_crypto::tor::TorClient>,
}

impl Client {
    pub fn new(cfg: &Config) -> Self {
        Self {
            base_url: cfg.server.clone(),
            auth_token: cfg.auth_token.clone(),
            tor: None,
        }
    }

    pub async fn with_tor(cfg: &Config, ephemeral: bool) -> Result<Self, String> {
        let state_dir = crate::config::dir().join("tor");
        let tor = vulnhash_crypto::tor::TorClient::new(&state_dir, ephemeral).await?;
        Ok(Self {
            base_url: cfg.server.clone(),
            auth_token: cfg.auth_token.clone(),
            tor: Some(tor),
        })
    }

    /// Build a client for one command.
    ///
    /// `tor` is the per-command override from --tor / --no-tor; None means the
    /// configured setting applies.
    pub async fn connect(cfg: &Config, tor: Option<bool>, ephemeral: bool) -> Result<Self, String> {
        if tor.unwrap_or(cfg.tor) {
            eprintln!("Routing over Tor (bootstrapping may take a moment)...");
            Self::with_tor(cfg, ephemeral || cfg.tor_ephemeral).await
        } else {
            Ok(Self::new(cfg))
        }
    }

    /// Submit a hash, optionally as a follow-up to an earlier submission of
    /// the caller's, named by its hash.
    pub async fn submit_hash(
        &self,
        hash: &str,
        algorithm: &str,
        parent: Option<&str>,
    ) -> Result<HashResponse, String> {
        let mut body = serde_json::json!({"hash": hash, "algorithm": algorithm});
        if let Some(parent) = parent {
            body["parent"] = serde_json::Value::String(parent.to_string());
        }
        let resp = self.do_request("POST", "/api/hashes", Some(&body)).await?;
        serde_json::from_str(&resp).map_err(|e| format!("parse response: {e}"))
    }

    /// Everything the server knows to descend from this submission. Answered
    /// in full for the owner, and published-only for anyone else.
    pub async fn descendants(&self, hash: &str) -> Result<Vec<HashResponse>, String> {
        let resp = self
            .do_request("GET", &format!("/api/hashes/{hash}/descendants"), None)
            .await?;
        serde_json::from_str(&resp).map_err(|e| format!("parse response: {e}"))
    }

    /// Set the details a finding shows publicly.
    ///
    /// These are visible from the moment they are set, disclosed or not: a
    /// researcher can say what a sealed finding is about without revealing the
    /// document behind it.
    pub async fn update_hash(
        &self,
        hash: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        let body = serde_json::json!({"title": title, "description": description});
        self.do_request("PUT", &format!("/api/hashes/{hash}"), Some(&body)).await?;
        Ok(())
    }

    /// Disclose a sealed finding to one researcher, privately.
    ///
    /// The report travels with the grant: the server has no copy of a sealed
    /// document until it is published or disclosed, and verifies what arrives
    /// against the registered hash. Naming the finding by its raw hash is fine
    /// here — the server resolves the caller's own document.
    pub async fn disclose(&self, hash: &str, username: &str, content: &str) -> Result<(), String> {
        let body = serde_json::json!({"username": username, "content": content});
        self.do_request("POST", &format!("/api/hashes/{hash}/disclose"), Some(&body)).await?;
        Ok(())
    }

    pub async fn publish_hash(&self, hash: &str, content: &str) -> Result<(), String> {
        let body = serde_json::json!({"content": content});
        self.do_request("POST", &format!("/api/hashes/{hash}/publish"), Some(&body)).await?;
        Ok(())
    }

    pub async fn verify_hash(&self, hash: &str) -> Result<VerifyResponse, String> {
        // An unrecorded hash is a normal answer to "does this exist?", not a
        // failure, and the server reports it as 404 with the same body shape.
        let (status, body) = self
            .request_raw("GET", &format!("/api/verify/{hash}"), None)
            .await?;
        if status != 404 {
            check_status(status, &body)?;
        }
        serde_json::from_str(&body).map_err(|e| format!("parse response: {e}"))
    }

    async fn do_request(&self, method: &str, path: &str, body: Option<&serde_json::Value>) -> Result<String, String> {
        let (status, body) = self.request_raw(method, path, body).await?;
        check_status(status, &body)?;
        Ok(body)
    }

    async fn request_raw(&self, method: &str, path: &str, body: Option<&serde_json::Value>) -> Result<(u16, String), String> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = body.map(|b| serde_json::to_vec(b).unwrap());

        let resp = if let Some(tor) = &self.tor {
            tor.http_request(
                method,
                &url,
                body_bytes.as_deref(),
                "application/json",
                Some(&self.auth_token),
            ).await?
        } else {
            vulnhash_crypto::http::http_request(
                method,
                &url,
                body_bytes.as_deref(),
                "application/json",
                Some(&self.auth_token),
            ).await?
        };

        Ok((resp.status, resp.body))
    }
}

/// Turn an error status into a message, preferring the server's own wording.
fn check_status(status: u16, body: &str) -> Result<(), String> {
    if status < 400 {
        return Ok(());
    }

    #[derive(Deserialize)]
    struct ErrResp {
        error: Option<String>,
    }
    if let Ok(e) = serde_json::from_str::<ErrResp>(body) {
        if let Some(msg) = e.error {
            return Err(format!("server error ({status}): {msg}"));
        }
    }
    Err(format!("server error ({status}): {body}"))
}
