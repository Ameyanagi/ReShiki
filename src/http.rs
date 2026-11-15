//! HTTPS clients for the update check and online name lookup.

/// Starts a client with TLS through ring.
///
/// reqwest is built without a crypto provider, and building a client before one
/// is installed panics, so every client starts here.
pub(crate) fn client() -> reqwest::ClientBuilder {
    // An error only means that a provider is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn clients_use_ring_and_the_lockfile_has_no_aws_lc() {
        assert!(!include_str!("../Cargo.lock").contains("name = \"aws-lc"));
        super::client().build().unwrap();
        let provider = rustls::crypto::CryptoProvider::get_default().unwrap();
        let ring = rustls::crypto::ring::default_provider();
        assert_eq!(
            format!("{:?}", provider.kx_groups),
            format!("{:?}", ring.kx_groups)
        );
    }

    /// The update check's server, over the network:
    /// `cargo test --lib http:: -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn ring_completes_a_handshake_with_the_release_server() {
        let client = super::client()
            .https_only(true)
            .user_agent("ReShiki")
            .build()
            .unwrap();
        let response = client.get("https://api.github.com/").send().await.unwrap();
        assert!(response.status().is_success());
    }
}
