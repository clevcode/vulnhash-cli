use std::sync::Once;
use std::time::Duration;

/// A response body read fully into memory.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

static CRYPTO_PROVIDER: Once = Once::new();

/// Select a rustls crypto provider for the process.
///
/// rustls will not choose one on its own when more than one backend is present
/// in the dependency graph, and the failure mode is a panic at the first TLS
/// handshake rather than an error at startup. Both transports call this before
/// touching TLS.
pub(crate) fn install_crypto_provider() {
    CRYPTO_PROVIDER.call_once(|| {
        // Errors only if another provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// User-Agent sent by both transports.
pub(crate) const USER_AGENT: &str = concat!("vulnhash-cli/", env!("CARGO_PKG_VERSION"));

/// Cap on a response body, so a hostile or broken server cannot exhaust memory.
pub(crate) const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Whole-request timeout, so the CLI cannot hang forever on a stalled server.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Make a direct (non-Tor) HTTP/HTTPS request.
pub async fn http_request(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    content_type: &str,
    auth_token: Option<&str>,
) -> Result<HttpResponse, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("parse url: {e}"))?;
    match parsed.scheme() {
        "https" => {}
        // Proof submissions carry an API token, so plaintext is refused unless
        // the target is a loopback development server.
        "http" if is_loopback(&parsed) => {}
        "http" => {
            return Err(format!(
                "refusing to send credentials over plaintext HTTP to {}; use https://",
                parsed.host_str().unwrap_or("the configured server")
            ))
        }
        other => return Err(format!("unsupported url scheme: {other}")),
    }

    install_crypto_provider();

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();

    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(https);

    let mut req_builder = hyper::Request::builder()
        .method(method)
        .uri(url)
        .header("Content-Type", content_type)
        .header("User-Agent", USER_AGENT);

    if let Some(token) = auth_token.filter(|t| !t.is_empty()) {
        req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
    }

    let req_body = http_body_util::Full::new(match body {
        Some(b) => bytes::Bytes::from(b.to_vec()),
        None => bytes::Bytes::new(),
    });

    let req = req_builder
        .body(req_body)
        .map_err(|e| format!("build request: {e}"))?;

    let resp = tokio::time::timeout(REQUEST_TIMEOUT, client.request(req))
        .await
        .map_err(|_| format!("request timed out after {}s", REQUEST_TIMEOUT.as_secs()))?
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status().as_u16();
    let body = read_body(resp.into_body()).await?;

    Ok(HttpResponse { status, body })
}

/// Collect a response body, refusing to buffer more than MAX_RESPONSE_BYTES.
pub(crate) async fn read_body<B>(body: B) -> Result<String, String>
where
    B: hyper::body::Body,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    use http_body_util::BodyExt;

    let limited = http_body_util::Limited::new(body, MAX_RESPONSE_BYTES);
    let collected = limited
        .collect()
        .await
        .map_err(|e| format!("read body: {e}"))?;

    Ok(String::from_utf8_lossy(&collected.to_bytes()).to_string())
}

/// Whether a URL points at this machine, where plaintext HTTP is acceptable.
fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host == "localhost" || host.ends_with(".localhost"),
        Some(url::Host::Ipv4(addr)) => addr.is_loopback(),
        Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_urls_are_recognised() {
        for url in [
            "http://localhost:8080/api",
            "http://127.0.0.1:8080/api",
            "http://[::1]:8080/api",
            "http://dev.localhost/api",
        ] {
            assert!(
                is_loopback(&url::Url::parse(url).unwrap()),
                "{url} should be loopback"
            );
        }

        for url in ["http://vulnhash.com/api", "http://192.168.1.5/api"] {
            assert!(
                !is_loopback(&url::Url::parse(url).unwrap()),
                "{url} should not be loopback"
            );
        }
    }

    #[tokio::test]
    async fn plaintext_to_a_remote_host_is_refused() {
        let err = http_request(
            "GET",
            "http://vulnhash.com/api/x",
            None,
            "application/json",
            Some("vh_token"),
        )
        .await
        .unwrap_err();
        assert!(err.contains("refusing"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn unsupported_schemes_are_refused() {
        let err = http_request("GET", "ftp://example.com/x", None, "application/json", None)
            .await
            .unwrap_err();
        assert!(
            err.contains("unsupported url scheme"),
            "unexpected error: {err}"
        );
    }
}
