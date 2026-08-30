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
        target: Target { core: String::new(), format: format.into(), version: String::new(), platform: String::new(), standalone: false },
    }
}

#[test]
fn sing_box_compiles_a_direct_only_configuration_without_nodes() {
    let result = compile(&CompileRequest {
        protocol: 1,
        profile: Profile::default(),
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse("sing-box-v13").expect("target"),
    })
    .expect("compile direct-only sing-box configuration");
    let document: serde_json::Value = serde_json::from_str(&result.content).expect("config JSON");

    assert_eq!(result.node_count, 0);
    assert!(document["outbounds"].as_array().is_some_and(|outbounds| {
        outbounds.iter().any(|outbound| outbound["tag"] == "direct")
            && outbounds.iter().any(|outbound| {
                outbound["tag"] == "proxy" && outbound["outbounds"] == serde_json::json!(["direct"])
            })
    }));
    assert_eq!(document["route"]["final"], "proxy");
}

#[test]
fn non_sing_box_targets_still_reject_profiles_without_nodes() {
    let error = compile(&CompileRequest {
        protocol: 1,
        profile: Profile::default(),
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse("clash-meta").expect("target"),
    })
    .expect_err("empty Mihomo profile must fail");

    assert!(error.to_string().contains("produced no usable nodes"));
}

#[test]
fn compile_is_deterministic_and_reports_loss() {
    let first = compile(&request("sing-box-v13")).expect("first compile");
    let second = compile(&request("sing-box-v13")).expect("second compile");
    assert_eq!(first.artifact_hash, second.artifact_hash);
    assert_eq!(first.content, second.content);
    assert_eq!(first.node_count, 2);
    assert!(first.content.contains("🇭🇰 HK edge (2)"));
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
fn clash_targets_include_legacy_runtime_compatibility_fields() {
    let mut input = request("clash-meta");
    input.profile.transparent_proxy.mode = "tproxy".into();
    input.profile.transparent_proxy.tproxy.listen_port = 7893;
    input.profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    input.profile.management_api.secret = "controller-secret".into();
    let result = compile(&input).expect("compile clash-meta");
    let document: Value = serde_yaml::from_str(&result.content).expect("valid YAML output");
    assert_eq!(document["tproxy-port"], 7893);
    assert_eq!(document["secret"], "controller-secret");
    assert_eq!(document["global-client-fingerprint"], "chrome");
}

#[test]
fn system_switches_apply_ohmywrt_defaults_during_compilation() {
    let mut input = request("clash-meta");
    for key in [
        "use_system_groups",
        "use_system_rules",
        "use_system_filters",
        "use_system_dns",
        "use_system_custom_config",
    ] {
        input.profile.extra.insert(key.into(), json!(true));
    }
    let result = compile(&input).expect("compile with system defaults");
    let document: Value = serde_yaml::from_str(&result.content).expect("valid YAML output");
    assert_eq!(document["proxy-groups"].as_array().map(Vec::len), Some(24));
    assert_eq!(
        document["rule-providers"]
            .as_object()
            .map(serde_json::Map::len),
        Some(23)
    );
    assert!(result.content.contains("GoogleCIDRv2"));
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
        unique.node_origins.get("🇭🇰 HK edge").map(String::as_str),
        Some("source:source-1")
    );
    assert_eq!(
        unique
            .node_origins
            .get("🇭🇰 HK edge (2)")
            .map(String::as_str),
        Some("source:source-1")
    );
}

#[test]
fn profile_round_trip_preserves_forward_compatible_fields() {
    let input = json!({
        "id": "profile-1",
        "name": "Preserved",
        "use_system_groups": true,
        "transparent_proxy": {
            "mode": "tun-router",
            "capture_host": true,
            "route_exclusions": ["192.0.2.0/24"],
            "tun": { "interface_name": "sempre-tun" }
        },
        "editor": { "servers": "[]" },
        "sources": [{
            "id": "source-1", "type": "url", "enabled": true,
            "url": "https://example.com/sub", "fetch_mode": "domestic-direct",
            "snapshot_hash": "abc"
        }]
    });
    let profile: Profile = serde_json::from_value(input).expect("profile decodes");
    let output = serde_json::to_value(profile).expect("profile encodes");
    assert_eq!(output["use_system_groups"], true);
    assert_eq!(output["transparent_proxy"]["capture_host"], true);
    assert_eq!(
        output["transparent_proxy"]["route_exclusions"][0],
        "192.0.2.0/24"
    );
    assert_eq!(output["sources"][0]["fetch_mode"], "domestic-direct");
    assert_eq!(output["sources"][0]["snapshot_hash"], "abc");
}

#[test]
fn editor_manual_servers_are_compiled_by_the_shared_core() {
    let profile: Profile = serde_json::from_value(json!({
        "name": "Editor",
        "editor": {
            "group": "[{\"name\":\"proxy\",\"type\":\"select\"}]",
            "servers": "[{\"name\":\"manual\",\"type\":\"socks5\",\"server\":\"manual.example.com\",\"port\":1080}]"
        }
    })).expect("profile");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target {
            core: String::new(),
            format: "sing-box-v13".into(),
            version: String::new(),
            platform: String::new(),
            standalone: false,
        },
    })
    .expect("compile editor server");
    assert_eq!(result.node_count, 1);
    assert!(result.content.contains("manual.example.com"));
}

#[test]
fn sing_box_preserves_ohmywrt_protocol_conversion_semantics() {
    let profile: Profile = serde_json::from_value(json!({
        "name": "Protocols",
        "manual_servers": [
            {
                "name": "hy2", "type": "hysteria2", "server": "hy.example.com", "port": 443,
                "password": "secret", "ports": "20000-40000", "up": "200 Mbps", "down": "1000Mbps"
            },
            {
                "name": "tuic", "type": "tuic", "server": "tuic.example.com", "port": 443,
                "uuid": "id", "password": "secret", "heartbeat-interval": 5000,
                "reduce-rtt": true, "udp-over-stream": true
            },
            {
                "name": "ss", "type": "ss", "server": "ss.example.com", "port": 8388,
                "cipher": "aes-256-gcm", "password": "secret", "udp": false,
                "plugin": "obfs", "plugin-opts": { "mode": "http", "host": "edge.example.com" }
            },
            {
                "name": "vmess-http", "type": "vmess", "server": "vmess.example.com", "port": 443,
                "uuid": "id", "http-opts": {
                    "path": ["/tunnel"], "method": "GET", "headers": { "Host": ["edge.example.com"] }
                }
            }
        ]
    })).expect("profile");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target {
            core: String::new(),
            format: "sing-box-v13".into(),
            version: String::new(),
            platform: String::new(),
            standalone: false,
        },
    })
    .expect("compile protocols");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    let outbounds = config["outbounds"].as_array().expect("outbounds");
    let outbound = |tag: &str| {
        outbounds
            .iter()
            .find(|value| value["tag"] == tag)
            .unwrap_or_else(|| panic!("missing {tag}"))
    };
    assert_eq!(outbound("hy2")["server_ports"], json!(["20000:40000"]));
    assert_eq!(outbound("hy2")["up_mbps"], 200);
    assert_eq!(outbound("hy2")["down_mbps"], 1000);
    assert_eq!(outbound("tuic")["heartbeat"], "5s");
    assert_eq!(outbound("tuic")["zero_rtt_handshake"], true);
    assert_eq!(outbound("tuic")["udp_over_stream"], true);
    assert_eq!(outbound("ss")["network"], "tcp");
    assert_eq!(outbound("ss")["plugin"], "obfs-local");
    assert_eq!(
        outbound("ss")["plugin_opts"],
        "mode=http;host=edge.example.com"
    );
    assert_eq!(outbound("vmess-http")["transport"]["type"], "http");
    assert_eq!(outbound("vmess-http")["transport"]["path"], "/tunnel");
    assert_eq!(
        outbound("vmess-http")["transport"]["headers"]["Host"],
        "edge.example.com"
    );
}

#[test]
fn sing_box_compiles_string_and_native_custom_rules() {
    let profile: Profile = serde_json::from_value(json!({
        "name": "Rules",
        "groups": [{ "name": "proxy", "type": "select" }],
        "rules": [
            "DOMAIN-SUFFIX,example.com,proxy",
            { "process_name": ["git"], "outbound": "direct" }
        ],
        "manual_servers": [
            { "name": "edge", "type": "socks5", "server": "edge.example.com", "port": 1080 }
        ]
    }))
    .expect("profile");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target {
            core: String::new(),
            format: "sing-box-v13".into(),
            version: String::new(),
            platform: String::new(),
            standalone: false,
        },
    })
    .expect("compile rules");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    assert!(config["route"]["rules"].as_array().is_some_and(|rules| {
        rules
            .iter()
            .any(|rule| rule["domain_suffix"] == "example.com" && rule["outbound"] == "proxy")
            && rules
                .iter()
                .any(|rule| rule["process_name"] == json!(["git"]))
    }));
}

#[test]
fn transparent_runtime_is_rendered_for_sing_box_modes() {
    let mut input = request("sing-box-v13");
    input.profile.transparent_proxy = serde_json::from_value(json!({
        "mode": "tun-router",
        "route_exclusions": ["10.10.10.0/24"],
        "tun": { "interface_name": "sempre-tun", "address": "172.20.0.1/30" }
    }))
    .expect("transparent config");
    let result = compile(&input).expect("compile sing-box TUN");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    let tun = config["inbounds"]
        .as_array()
        .expect("inbounds")
        .iter()
        .find(|value| value["tag"] == "tun-in")
        .expect("TUN inbound");
    assert_eq!(tun["interface_name"], "sempre-tun");
    assert_eq!(tun["address"], json!(["172.20.0.1/30"]));
    assert_eq!(tun["route_exclude_address"], json!(["10.10.10.0/24"]));

    input.profile.transparent_proxy.mode = "tproxy".into();
    input.profile.transparent_proxy.tproxy.listen_port = 7893;
    input.profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    let result = compile(&input).expect("compile sing-box TProxy");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    assert!(config["inbounds"].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|value| value["tag"] == "tproxy-in" && value["listen_port"] == 7893)
            && values
                .iter()
                .any(|value| value["tag"] == "dns-in" && value["listen_port"] == 1053)
    }));
}

#[test]
fn transparent_runtime_is_rendered_for_mihomo_and_clash_rs() {
    let mut input = request("clash-meta");
    input.target.core = "mihomo".into();
    input.profile.transparent_proxy = serde_json::from_value(json!({
        "mode": "tun-router",
        "route_exclusions": ["192.0.2.0/24"],
        "interface_mode": "include",
        "interfaces": ["br-lan"],
        "tun": { "interface_name": "sempre-tun" }
    }))
    .expect("transparent config");
    let result = compile(&input).expect("compile Mihomo TUN");
    let config: Value = serde_yaml::from_str(&result.content).expect("Mihomo YAML");
    assert_eq!(config["tun"]["device"], "sempre-tun");
    assert_eq!(config["tun"]["include-interface"], json!(["br-lan"]));
    assert_eq!(
        config["tun"]["route-exclude-address"],
        json!(["192.0.2.0/24"])
    );

    input.target = Target::parse("clash-rs").expect("clash-rs target");
    input.profile.transparent_proxy.tun.address = "172.21.0.1/30".into();
    let result = compile(&input).expect("compile clash-rs TUN");
    let config: Value = serde_yaml::from_str(&result.content).expect("clash-rs YAML");
    assert_eq!(config["tun"]["gateway"], "172.21.0.1/30");

    input.profile.transparent_proxy.mode = "tproxy".into();
    input.profile.transparent_proxy.tproxy.listen_port = 7893;
    input.profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    let result = compile(&input).expect("compile clash-rs TProxy");
    let config: Value = serde_yaml::from_str(&result.content).expect("clash-rs YAML");
    assert_eq!(config["tproxy-port"], 7893);
    assert_eq!(config["listeners"][0]["port"], 1053);
}

#[test]
fn transparent_runtime_is_rendered_for_xray_and_v2ray() {
    let mut input = request("xray");
    input.profile.transparent_proxy = serde_json::from_value(json!({
        "mode": "tun-router",
        "tun": { "interface_name": "sempre-tun", "address": "172.22.0.1/30" }
    }))
    .expect("transparent config");
    let result = compile(&input).expect("compile Xray TUN");
    let config: Value = serde_json::from_str(&result.content).expect("Xray JSON");
    let tun = config["inbounds"]
        .as_array()
        .expect("inbounds")
        .iter()
        .find(|value| value["tag"] == "tun-in")
        .expect("TUN inbound");
    assert_eq!(tun["settings"]["name"], "sempre-tun");
    assert_eq!(tun["settings"]["gateway"], json!(["172.22.0.1/30"]));

    input.target = Target::parse("v2ray").expect("V2Ray target");
    input.profile.transparent_proxy.mode = "tproxy".into();
    input.profile.transparent_proxy.tproxy.listen_port = 7893;
    input.profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    input.profile.dns = json!({ "shared": { "remoteDns": "9.9.9.9" } });
    let result = compile(&input).expect("compile V2Ray TProxy");
    let config: Value = serde_json::from_str(&result.content).expect("V2Ray JSON");
    assert!(config["inbounds"].as_array().is_some_and(|values| {
        values.iter().any(|value| {
            value["tag"] == "tproxy-in" && value["streamSettings"]["sockopt"]["tproxy"] == "tproxy"
        }) && values
            .iter()
            .any(|value| value["tag"] == "dns-in" && value["settings"]["address"] == "9.9.9.9")
    }));
}

#[test]
fn disabled_transparent_mode_keeps_only_local_inbounds() {
    let mut input = request("sing-box-v13");
    input.profile.transparent_proxy.mode = "disabled".into();
    let result = compile(&input).expect("compile disabled mode");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    assert_eq!(config["inbounds"].as_array().map(Vec::len), Some(2));
}

#[test]
fn standalone_sing_box_keeps_only_deployable_inbounds_and_desktop_api() {
    let mut input = request("sing-box-v13");
    input.target.standalone = true;
    input.profile.transparent_proxy.mode = "tproxy".into();
    input.profile.transparent_proxy.tproxy.listen_port = 7893;
    input.profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    input.profile.management_api.external_controller = "0.0.0.0:9999".into();
    input.profile.management_api.external_ui = "/etc/sb/ui".into();
    input.profile.management_api.secret = "controller-secret".into();

    let result = compile(&input).expect("compile standalone router output");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    let inbounds = config["inbounds"].as_array().expect("inbounds");
    assert_eq!(inbounds.len(), 2);
    assert!(inbounds.iter().any(|value| value["tag"] == "dns-in"));
    assert!(inbounds.iter().any(|value| value["tag"] == "tproxy-in"));
    assert_eq!(
        config["experimental"]["clash_api"]["external_controller"],
        "0.0.0.0:9999"
    );

    input.target = Target::parse("sing-box-v13-macos").expect("macOS target");
    input.target.standalone = true;
    let result = compile(&input).expect("compile standalone desktop output");
    let config: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
    let inbounds = config["inbounds"].as_array().expect("inbounds");
    assert_eq!(inbounds.len(), 1);
    assert_eq!(inbounds[0]["tag"], "tun-in");
    assert_eq!(
        config["experimental"]["clash_api"]["external_controller"],
        "127.0.0.1:9999"
    );
    assert_eq!(config["experimental"]["clash_api"]["external_ui"], "./ui");
    assert_eq!(
        config["experimental"]["clash_api"]["secret"],
        "controller-secret"
    );
}
