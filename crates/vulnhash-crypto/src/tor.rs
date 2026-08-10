use std::path::Path;
use std::sync::Arc;

use arti_client::config::TorClientConfigBuilder;
use arti_client::{TorClient as ArtiClient, TorClientConfig};
use tor_rtcompat::PreferredRuntime;

use crate::http::{install_crypto_provider, read_body, HttpResponse, REQUEST_TIMEOUT, USER_AGENT};

pub struct TorClient {
    client: ArtiClient<PreferredRuntime>,
}

impl TorClient {
    /// Create a new Tor client.
    /// If ephemeral is true, no state is persisted to disk.
    pub async fn new(state_dir: &Path, ephemeral: bool) -> Result<Self, String> {
        install_crypto_provider();

        let config = if ephemeral {
            TorClientConfig::default()
        } else {
            let state = state_dir.join("state");
            let cache = state_dir.join("cache");
            std::fs::create_dir_all(&state).map_err(|e| format!("create state dir: {e}"))?;
            std::fs::create_dir_all(&cache).map_err(|e| format!("create cache dir: {e}"))?;
            TorClientConfigBuilder::from_directories(state, cache)
                .build()
                .map_err(|e| format!("tor config: {e}"))?
        };

        let client = ArtiClient::create_bootstrapped(config)
            .await
            .map_err(|e| format!("tor bootstrap: {e}"))?;

        Ok(Self { client })
    }

    /// Make an HTTP request over Tor: Tor stream -> TLS -> HTTP/1.1.
    pub async fn http_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        content_type: &str,
        auth_token: Option<&str>,
    ) -> Result<HttpResponse, String> {
        let parsed = url::Url::parse(url).map_err(|e| format!("parse url: {e}"))?;
        let host = parsed.host_str().ok_or("no host in url")?.to_string();
        let port = parsed.port_or_known_default().ok_or("no port")?;
        let is_https = parsed.scheme() == "https";

        // The request target must carry the query string; sending only the path
        // would silently drop parameters.
        let mut target = parsed.path().to_string();
        if let Some(query) = parsed.query() {
            target.push('?');
            target.push_str(query);
        }

        // Connect via Tor
        let stream = self
            .client
            .connect((&*host, port))
            .await
            .map_err(|e| format!("tor connect: {e}"))?;

        if is_https {
            // TLS handshake
            let mut root_store = rustls::RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let tls_config = rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();
            let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));
            let server_name = rustls::pki_types::ServerName::try_from(host.clone())
                .map_err(|e| format!("server name: {e}"))?;
            let tls_stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|e| format!("tls handshake: {e}"))?;

            self.do_http_request(tls_stream, &host, &target, method, body, content_type, auth_token)
                .await
        } else {
            self.do_http_request(stream, &host, &target, method, body, content_type, auth_token)
                .await
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn do_http_request<S>(
        &self,
        stream: S,
        host: &str,
        target: &str,
        method: &str,
        body: Option<&[u8]>,
        content_type: &str,
        auth_token: Option<&str>,
    ) -> Result<HttpResponse, String>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let io = hyper_util::rt::TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| format!("http handshake: {e}"))?;
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let body_bytes = body
            .map(|b| bytes::Bytes::from(b.to_vec()))
            .unwrap_or_default();

        let mut req_builder = hyper::Request::builder()
            .method(method)
            .uri(target)
            .header("Host", host)
            .header("Content-Type", content_type)
            .header("User-Agent", USER_AGENT);

        // Without this the CLI's authenticated commands (submit, list, publish)
        // reach the server anonymously over Tor and are rejected.
        if let Some(token) = auth_token.filter(|t| !t.is_empty()) {
            req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
        }

        let req = req_builder
            .body(http_body_util::Full::new(body_bytes))
            .map_err(|e| format!("build request: {e}"))?;

        let resp = tokio::time::timeout(REQUEST_TIMEOUT, sender.send_request(req))
            .await
            .map_err(|_| format!("request timed out after {}s", REQUEST_TIMEOUT.as_secs()))?
            .map_err(|e| format!("send request: {e}"))?;

        let status = resp.status().as_u16();
        let body = read_body(resp.into_body()).await?;

        Ok(HttpResponse { status, body })
    }
}
