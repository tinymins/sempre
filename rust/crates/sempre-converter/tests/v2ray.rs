use sempre_converter::{CompileRequest, Profile, Target, compile};
use serde_json::{Value, json};

fn request(format: &str, profile: Value) -> CompileRequest {
    CompileRequest {
        protocol: 1,
        profile: serde_json::from_value::<Profile>(profile).expect("profile"),
        snapshots: Vec::new(),
        custom_nodes: Vec::new(),
        target: Target {
            core: String::new(),
            format: format.into(),
            version: String::new(),
            platform: String::new(),
        },
    }
}

fn document(input: &CompileRequest) -> Value {
    let result = compile(input).expect("compile v2ray family");
    serde_json::from_str(&result.content).expect("valid JSON")
}

fn outbound<'a>(config: &'a Value, tag: &str) -> &'a Value {
    config["outbounds"]
        .as_array()
        .and_then(|values| values.iter().find(|value| value["tag"] == tag))
        .expect("outbound")
}

#[test]
fn xray_uses_modern_outbounds_transports_and_local_auth() {
    let input = request(
        "xray",
        json!({
            "manual_servers": [
                {
                    "name": "vmess-ws", "type": "vmess", "server": "vmess.example.com",
                    "port": 443, "uuid": "11111111-1111-4111-8111-111111111111",
                    "cipher": "auto", "tls": true, "servername": "vmess.example.com",
                    "client-fingerprint": "chrome", "ws-opts": { "path": "/ws" }
                },
                {
                    "name": "vless-reality", "type": "vless", "server": "reality.example.com",
                    "port": 443, "uuid": "22222222-2222-4222-8222-222222222222",
                    "servername": "www.microsoft.com", "client-fingerprint": "chrome",
                    "reality-opts": { "public-key": "public", "short-id": "0123456789abcdef" }
                }
            ],
            "groups": [{ "name": "foreign", "type": "url-test", "include_all": true, "interval": 120 }],
            "local_proxy": { "username": "user", "password": "pass" }
        }),
    );
    let config = document(&input);
    let vmess = outbound(&config, "vmess-ws");
    assert_eq!(vmess["settings"]["address"], "vmess.example.com");
    assert!(vmess["settings"].get("vnext").is_none());
    assert_eq!(vmess["streamSettings"]["method"], "websocket");
    assert_eq!(vmess["streamSettings"]["wsSettings"]["path"], "/ws");
    assert_eq!(
        vmess["streamSettings"]["tlsSettings"]["fingerprint"],
        "chrome"
    );
    let reality = outbound(&config, "vless-reality");
    assert_eq!(reality["streamSettings"]["security"], "reality");
    assert_eq!(
        reality["streamSettings"]["realitySettings"]["password"],
        "public"
    );
    assert_eq!(
        config["inbounds"][0]["settings"]["users"][0]["user"],
        "user"
    );
    assert_eq!(config["observatory"]["probeInterval"], "120s");
}

#[test]
fn v2ray_uses_legacy_schema_and_filters_reality_from_groups() {
    let input = request(
        "v2ray",
        json!({
            "manual_servers": [
                {
                    "name": "reality", "type": "vless", "server": "reality.example.com",
                    "port": 443, "uuid": "22222222-2222-4222-8222-222222222222",
                    "reality-opts": { "public-key": "public" }
                },
                {
                    "name": "legacy", "type": "vmess", "server": "legacy.example.com",
                    "port": 443, "uuid": "11111111-1111-4111-8111-111111111111",
                    "alterId": 0, "network": "ws", "ws-opts": { "path": "/legacy" }
                }
            ],
            "groups": [{
                "name": "foreign", "type": "url-test", "include_all": true,
                "default": "reality"
            }],
            "local_proxy": { "username": "user", "password": "pass" }
        }),
    );
    let result = compile(&input).expect("compile legacy V2Ray");
    assert_eq!(result.node_count, 1);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|item| item.message.contains("Reality is not supported"))
    );
    let config: Value = serde_json::from_str(&result.content).expect("valid JSON");
    let legacy = outbound(&config, "legacy");
    assert_eq!(
        legacy["settings"]["vnext"][0]["address"],
        "legacy.example.com"
    );
    assert_eq!(legacy["streamSettings"]["network"], "ws");
    assert_eq!(
        config["routing"]["balancers"][0]["selector"],
        json!(["legacy"])
    );
    assert_eq!(
        config["inbounds"][0]["settings"]["accounts"][0]["user"],
        "user"
    );
}

#[test]
fn routing_maps_rules_to_balancers_and_reports_providers() {
    let input = request(
        "xray",
        json!({
            "manual_servers": [{
                "name": "edge", "type": "socks5", "server": "edge.example.com", "port": 1080
            }],
            "groups": [{ "name": "foreign", "type": "select", "include_all": true }],
            "rules": ["DOMAIN-SUFFIX,example.com,foreign", "DST-PORT,53,DIRECT"],
            "rule_providers": [{ "tag": "external", "url": "https://rules.example/list" }]
        }),
    );
    let result = compile(&input).expect("compile routing");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|item| item.message.contains("rule provider external"))
    );
    let config: Value = serde_json::from_str(&result.content).expect("valid JSON");
    let rules = config["routing"]["rules"].as_array().expect("rules");
    assert!(rules.iter().any(|rule| {
        rule["domain"] == json!(["domain:example.com"]) && rule["balancerTag"] == "foreign"
    }));
    assert!(
        rules
            .iter()
            .any(|rule| { rule["port"] == "53" && rule["outboundTag"] == "direct" })
    );
    assert!(rules.iter().any(|rule| {
        rule["inboundTag"] == json!(["remote-dns"]) && rule["balancerTag"] == "foreign"
    }));
}

#[test]
fn safe_core_overrides_merge_and_managed_boundaries_are_rejected() {
    let profile = json!({
        "manual_servers": [{
            "name": "edge", "type": "socks5", "server": "edge.example.com", "port": 1080
        }],
        "core_overrides": { "xray": { "log": { "access": "/tmp/access.log" } } }
    });
    let config = document(&request("xray", profile.clone()));
    assert_eq!(config["log"]["access"], "/tmp/access.log");
    assert_eq!(config["log"]["loglevel"], "info");

    let mut blocked = profile;
    blocked["core_overrides"]["xray"] = json!({ "inbounds": [] });
    let error = compile(&request("xray", blocked)).expect_err("managed inbounds");
    assert!(error.to_string().contains("authenticated local proxy"));
}
