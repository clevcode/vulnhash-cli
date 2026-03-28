use serde::Deserialize;
use crate::config::Config;

#[derive(Debug, Deserialize)]
pub struct HashResponse {
    pub id: String,
    pub hash: String,
    pub algorithm: String,
    pub title: String,
    pub description: String,
    pub published: bool,
    pub submitted_at: String,
    pub published_at: Option<String>,
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

    pub async fn submit_hash(&self, hash: &str, algorithm: &str) -> Result<HashResponse, String> {
        let body = serde_json::json!({"hash": hash, "algorithm": algorithm});
        let resp = self.do_request("POST", "/api/hashes", Some(&body)).await?;
        serde_json::from_str(&resp).map_err(|e| format!("parse response: {e}"))
    }

    pub async fn list_hashes(&self) -> Result<Vec<HashResponse>, String> {
        let resp = self.do_request("GET", "/api/hashes", None).await?;
        serde_json::from_str(&resp).map_err(|e| format!("parse response: {e}"))
    }

    pub async fn publish_hash(&self, hash: &str, content: &str) -> Result<(), String> {
        let body = serde_json::json!({"content": content});
        self.do_request("POST", &format!("/api/hashes/{hash}/publish"), Some(&body)).await?;
        Ok(())
    }

    pub async fn verify_hash(&self, hash: &str) -> Result<VerifyResponse, String> {
        let resp = self.do_request("GET", &format!("/api/verify/{hash}"), None).await?;
        serde_json::from_str(&resp).map_err(|e| format!("parse response: {e}"))
    }

    async fn do_request(&self, method: &str, path: &str, body: Option<&serde_json::Value>) -> Result<String, String> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = body.map(|b| serde_json::to_vec(b).unwrap());

        let resp = if let Some(tor) = &self.tor {
            tor.http_request(
                method,
                &url,
                body_bytes.as_deref(),
                "application/json",
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

        if resp.status >= 400 {
            #[derive(Deserialize)]
            struct ErrResp { error: Option<String> }
            if let Ok(e) = serde_json::from_str::<ErrResp>(&resp.body) {
                if let Some(msg) = e.error {
                    return Err(format!("server error ({}): {}", resp.status, msg));
                }
            }
            return Err(format!("server error ({}): {}", resp.status, resp.body));
        }

        Ok(resp.body)
    }
}
