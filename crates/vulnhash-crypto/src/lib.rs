//! HTTP and Tor transport for the VulnHash CLI.
//!
//! The server repository carries a larger crate of the same name that adds an
//! Ethereum signer, an encrypted keystore, and a C FFI layer for the Go
//! server. None of that is useful to the CLI, so this crate implements only the
//! two transports the CLI actually calls.

pub mod http;

#[cfg(feature = "tor")]
pub mod tor;

#[cfg(not(feature = "tor"))]
pub mod tor {
    use std::path::Path;

    use crate::http::HttpResponse;

    /// Placeholder used when the crate is built without the `tor` feature.
    pub struct TorClient;

    impl TorClient {
        pub async fn new(_state_dir: &Path, _ephemeral: bool) -> Result<Self, String> {
            Err("this build has no Tor support (rebuild with the 'tor' feature)".into())
        }

        pub async fn http_request(
            &self,
            _method: &str,
            _url: &str,
            _body: Option<&[u8]>,
            _content_type: &str,
            _auth_token: Option<&str>,
        ) -> Result<HttpResponse, String> {
            Err("this build has no Tor support (rebuild with the 'tor' feature)".into())
        }
    }
}
