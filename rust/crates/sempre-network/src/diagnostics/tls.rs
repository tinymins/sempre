use std::sync::{Arc, OnceLock};

use tokio_rustls::rustls::{ClientConfig, RootCertStore};

pub(super) fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    Arc::clone(CONFIG.get_or_init(|| {
        let roots = webpki_roots::TLS_SERVER_ROOTS
            .iter()
            .cloned()
            .collect::<RootCertStore>();
        Arc::new(
            ClientConfig::builder_with_provider(Arc::new(
                tokio_rustls::rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("TLS protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
        )
    }))
}
