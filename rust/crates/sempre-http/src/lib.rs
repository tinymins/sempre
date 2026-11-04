//! The one way to create an HTTP client, so every client shares one TLS provider.
use std::sync::Once;

use reqwest::ClientBuilder;

static CRYPTO_PROVIDER: Once = Once::new();

/// Returns a client builder after installing the process TLS provider.
///
/// `reqwest` builds without a bundled provider and panics if none was installed.
pub fn client_builder() -> ClientBuilder {
    CRYPTO_PROVIDER.call_once(|| {
        // Post-quantum key shares make the ClientHello span several TCP segments, as browsers
        // and curl do. A single-segment hello is reset by on-path SNI filters on some routes.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_client_with_the_shared_tls_provider() {
        assert!(super::client_builder().build().is_ok());
        let provider = rustls::crypto::CryptoProvider::get_default().expect("provider");
        assert_eq!(
            provider.kx_groups[0].name(),
            rustls::NamedGroup::X25519MLKEM768
        );
    }
}
