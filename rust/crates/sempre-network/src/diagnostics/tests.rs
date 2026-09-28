use std::sync::Arc;

use super::tls_config;

#[test]
fn tls_config_selects_a_provider_when_multiple_are_enabled() {
    let first = tls_config();
    let second = tls_config();

    assert!(Arc::ptr_eq(&first, &second));
}
