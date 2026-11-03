//! The one way to create an HTTP client, so every client shares one TLS provider.
use std::sync::Once;

use reqwest::ClientBuilder;

static CRYPTO_PROVIDER: Once = Once::new();

/// Returns a client builder after installing the process TLS provider.
///
/// `reqwest` builds without a bundled provider and panics if none was installed.
pub fn client_builder() -> ClientBuilder {
    CRYPTO_PROVIDER.call_once(|| {
        // Another component installing a provider first is equally usable.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_client_with_the_shared_tls_provider() {
        assert!(super::client_builder().build().is_ok());
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
