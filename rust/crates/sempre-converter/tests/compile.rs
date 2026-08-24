use sempre_converter::{CompileRequest, Profile, SourceSnapshot, Target, compile};
use serde_json::{Value, json};

fn request(format: &str) -> CompileRequest {
    let profile: Profile = serde_json::from_value(json!({
        "id": "profile-1",
        "revision": 7,
        "name": "Remote",
        "sources": [{ "id": "source-1", "enabled": true, "prefix": "HK" }],
        "groups": [{ "name": "proxy", "type": "select", "proxies": ["direct"] }],
        "rules": ["DOMAIN-SUFFIX,example.com,proxy"],
        "local_proxy": { "socks_port": 1080, "http_port": 8080 }
    }))
    .expect("fixture profile");
    CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![SourceSnapshot {
            source_id: "source-1".into(),
            content: "proxies:\n  - { name: edge, type: vless, server: edge.example.com, port: 443, uuid: first, tls: true, servername: edge.example.com, unsupported-option: true }\n  - { name: edge, type: socks5, server: backup.example.com, port: 1080 }".into(),
            content_hash: String::new(),
        }],
        custom_nodes: vec![],
        target: Target { core: String::new(), format: format.into(), version: String::new(), platform: String::new() },
    }
}

#[test]
fn compile_is_deterministic_and_reports_loss() {
    let first = compile(&request("sing-box-v13")).expect("first compile");
    let second = compile(&request("sing-box-v13")).expect("second compile");
    assert_eq!(first.artifact_hash, second.artifact_hash);
    assert_eq!(first.content, second.content);
    assert_eq!(first.node_count, 2);
    assert!(first.content.contains("HK edge (2)"));
    assert!(
        first
            .diagnostics
            .iter()
            .any(|item| item.message.contains("unsupported-option"))
    );
}

#[test]
fn clash_output_round_trips_as_yaml() {
    let result = compile(&request("clash-meta")).expect("compile clash-meta");
    let document: Value = serde_yaml::from_str(&result.content).expect("valid YAML output");
    assert_eq!(document["proxies"].as_array().map(Vec::len), Some(2));
    assert_eq!(result.node_count, 2);
}

#[test]
fn missing_snapshot_is_rejected_without_network_fallback() {
    let mut input = request("sing-box-v13");
    input.snapshots.clear();
    let error = compile(&input).expect_err("missing snapshot must fail");
    assert!(error.to_string().contains("no supplied snapshot"));
}

#[test]
fn filters_only_remove_source_nodes_and_origins_follow_unique_names() {
    let mut input = request("sing-box-v13");
    input.profile.filters = vec!["edge".into()];
    input.profile.manual_servers = vec![json!({
        "name": "edge", "type": "socks5", "server": "local.example.com", "port": 1080
    })];
    let result = compile(&input).expect("manual node survives source filter");
    assert_eq!(result.node_count, 1);
    assert_eq!(
        result.node_origins.get("edge").map(String::as_str),
        Some("manual-server")
    );

    let unique = compile(&request("sing-box-v13")).expect("duplicate names compile");
    assert_eq!(
        unique.node_origins.get("HK edge").map(String::as_str),
        Some("source:source-1")
    );
    assert_eq!(
        unique.node_origins.get("HK edge (2)").map(String::as_str),
        Some("source:source-1")
    );
}
