use std::{fs, net::SocketAddr};

use sempre_core::{Adapter, BuiltInAdapter, BuiltInKind, ControlProtocol};
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn sing_box_runtime_isolates_control_without_changing_source() {
    let root = tempdir().expect("temporary directory");
    let source = root.path().join("source.json");
    let original = br#"{
  "custom": {"preserved": true},
  "experimental": {"other": "value", "clash_api": {"external_controller": "0.0.0.0:9090", "secret": "user-secret", "custom": 42}}
}"#;
    fs::write(&source, original).expect("source configuration");

    let spec = BuiltInAdapter::new(BuiltInKind::SingBox)
        .prepare_runtime(&source, &root.path().join("runtime"))
        .expect("runtime configuration");

    assert_eq!(fs::read(&source).expect("source"), original);
    let document: Value =
        serde_json::from_slice(&fs::read(&spec.config).expect("runtime")).expect("runtime JSON");
    let clash_api = &document["experimental"]["clash_api"];
    let control = spec.control.expect("private control");
    assert_eq!(document["custom"]["preserved"], true);
    assert_eq!(document["experimental"]["other"], "value");
    assert_eq!(clash_api["custom"], 42);
    assert_eq!(clash_api["secret"], control.secret);
    assert_eq!(clash_api["external_ui"], "");
    assert_eq!(clash_api["access_control_allow_private_network"], false);
    assert_private_control(&control.base_url, &control.secret);
}

#[test]
fn mihomo_runtime_removes_public_controller_settings() {
    let root = tempdir().expect("temporary directory");
    let source = root.path().join("source.yaml");
    let original = b"mode: rule\ncustom:\n  preserved: true\nexternal-controller: 0.0.0.0:9090\nexternal-controller-tls: 0.0.0.0:9443\nexternal-controller-unix: /tmp/mihomo.sock\nexternal-ui: ./ui\nexternal-ui-url: https://example.com/ui.zip\nsecret: user-secret\nexternal-controller-cors:\n  allow-origins: ['*']\n  allow-private-network: true\n";
    fs::write(&source, original).expect("source configuration");

    let spec = BuiltInAdapter::new(BuiltInKind::Mihomo)
        .prepare_runtime(&source, &root.path().join("runtime"))
        .expect("runtime configuration");

    assert_eq!(fs::read(&source).expect("source"), original);
    let document: Value =
        serde_yaml::from_slice(&fs::read(&spec.config).expect("runtime")).expect("runtime YAML");
    let control = spec.control.expect("private control");
    assert_eq!(document["custom"]["preserved"], true);
    assert_eq!(document["mode"], "rule");
    assert!(document.get("external-controller-tls").is_none());
    assert!(document.get("external-controller-unix").is_none());
    assert!(document.get("external-ui").is_none());
    assert!(document.get("external-ui-url").is_none());
    assert_eq!(document["secret"], control.secret);
    assert_eq!(
        document["external-controller-cors"]["allow-origins"],
        serde_json::json!(["http://localhost.invalid"])
    );
    assert_eq!(
        document["external-controller-cors"]["allow-private-network"],
        false
    );
    assert_private_control(&control.base_url, &control.secret);
}

#[test]
fn clash_rs_uses_its_native_cors_field() {
    let root = tempdir().expect("temporary directory");
    let source = root.path().join("source.yaml");
    fs::write(&source, "mode: rule\nexternal-controller-cors: {}\n").expect("source configuration");

    let spec = BuiltInAdapter::new(BuiltInKind::ClashRs)
        .prepare_runtime(&source, &root.path().join("runtime"))
        .expect("runtime configuration");
    let document: Value =
        serde_yaml::from_slice(&fs::read(spec.config).expect("runtime")).expect("runtime YAML");

    assert!(document.get("external-controller-cors").is_none());
    assert_eq!(
        document["cors-allow-origins"],
        serde_json::json!(["http://localhost.invalid"])
    );
}

#[test]
fn xray_runtime_adds_loopback_grpc_and_preserves_rules() {
    let root = tempdir().expect("temporary directory");
    let source = root.path().join("source.json");
    let original = br#"{"inbounds":[{"tag":"sempre-api-in"},{"tag":"user"}],"outbounds":[],"routing":{"domainStrategy":"AsIs","rules":[{"type":"field","ip":["geoip:private"]}]}}"#;
    fs::write(&source, original).expect("source configuration");

    let spec = BuiltInAdapter::new(BuiltInKind::Xray)
        .prepare_runtime(&source, &root.path().join("runtime"))
        .expect("runtime configuration");

    assert_eq!(fs::read(&source).expect("source"), original);
    let document: Value =
        serde_json::from_slice(&fs::read(&spec.config).expect("runtime")).expect("runtime JSON");
    let control = spec.control.expect("private control");
    assert_eq!(control.protocol, ControlProtocol::Grpc);
    assert_eq!(document["api"]["tag"], "sempre-api");
    assert_eq!(
        document["api"]["services"].as_array().map(Vec::len),
        Some(4)
    );
    assert_eq!(document["routing"]["domainStrategy"], "AsIs");
    assert_eq!(
        document["routing"]["rules"].as_array().map(Vec::len),
        Some(2)
    );
    let inbounds = document["inbounds"].as_array().expect("inbounds");
    assert_eq!(inbounds.len(), 2);
    assert_eq!(inbounds[0]["tag"], "user");
    assert_eq!(inbounds[1]["tag"], "sempre-api-in");
    assert_eq!(inbounds[1]["listen"], "127.0.0.1");
    assert_ne!(inbounds[1]["port"], 0);
    assert_private_control(&control.base_url, &control.secret);
}

#[test]
fn dae_keeps_the_source_configuration_without_control_api() {
    let root = tempdir().expect("temporary directory");
    let source = root.path().join("config.dae");
    fs::write(&source, "global {}\n").expect("source configuration");

    let spec = BuiltInAdapter::new(BuiltInKind::Dae)
        .prepare_runtime(&source, &root.path().join("unused"))
        .expect("runtime configuration");

    assert_eq!(spec.config, source);
    assert!(spec.control.is_none());
    assert!(!root.path().join("unused").exists());
}

fn assert_private_control(base_url: &str, secret: &str) {
    let address: SocketAddr = base_url
        .strip_prefix("http://")
        .expect("HTTP URL")
        .parse()
        .expect("socket address");
    assert_eq!(address.ip().to_string(), "127.0.0.1");
    assert_ne!(address.port(), 0);
    assert_eq!(secret.len(), 64);
    assert!(secret.bytes().all(|byte| byte.is_ascii_hexdigit()));
}
