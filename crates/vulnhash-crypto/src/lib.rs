/// Stub crate for vulnhash-crypto.
/// This will be replaced by the real implementation (symlink to dep/vulnhash-crypto).

pub mod tor {
    use std::path::Path;

    pub struct TorClient;

    impl TorClient {
        pub async fn new(_state_dir: &Path, _ephemeral: bool) -> Result<Self, String> {
            Err("Tor support not yet available (stub crate)".into())
        }

        pub async fn http_request(
            &self,
            _method: &str,
            _url: &str,
            _body: Option<&[u8]>,
            _content_type: &str,
        ) -> Result<crate::http::HttpResponse, String> {
            Err("Tor support not yet available (stub crate)".into())
        }
    }
}

pub mod http {
    pub struct HttpResponse {
        pub status: u16,
        pub body: String,
    }

    pub async fn http_request(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        content_type: &str,
        auth_token: Option<&str>,
    ) -> Result<HttpResponse, String> {
        // Minimal HTTP client using std (no external deps in stub)
        // This is a placeholder — the real crate uses hyper/tokio-rustls
        Err(format!(
            "HTTP support not yet available (stub crate): {} {}",
            method, url
        ))
    }
}
