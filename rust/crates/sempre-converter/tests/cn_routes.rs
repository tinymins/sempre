use sempre_converter::{
    CompileRequest, Profile, Target, apply_dns_frontend_settings, compile, dns_frontend_policy,
};
use serde_json::{Value, json};

#[test]
fn cn_routes_compile_real_ip_dns_and_disable_native_auto_routes_on_all_platforms() {
    for (platform, mode) in ["macos", "windows", "linux"]
        .into_iter()
        .flat_map(|platform| {
            ["direct-route", "proxy"]
                .into_iter()
                .map(move |mode| (platform, mode))
        })
    {
        let target = Target {
            platform: platform.into(),
            ..Target::parse("sing-box-v13").expect("target")
        };
        let mut profile = Profile::default();
        profile.custom_node_ids.push("test-node".into());
        profile
            .extra
            .insert("sempre_cn_direct_routes".into(), json!(true));
        profile
            .extra
            .insert("sempre_cn_routing_mode".into(), json!(mode));
        profile.rules.push(json!("DOMAIN,example.cn,DIRECT"));
        profile.transparent_proxy.mode = "tun-router".into();
        let profile = apply_dns_frontend_settings(&profile, &target, true).expect("policy");
        assert!(
            !dns_frontend_policy(&profile, &target)
                .expect("DNS")
                .fakeip_enabled
        );
        assert!(apply_dns_frontend_settings(&profile, &target, false).is_err());
        let result = compile(&CompileRequest {
            protocol: 1,
            profile,
            target,
            snapshots: vec![],
            custom_nodes: vec![serde_json::from_value(json!({
                "id": "test-node", "name": "test-node",
                "proxy": {"name": "test-node", "type": "socks5", "server": "192.0.2.1", "port": 1080}
            })).expect("node")],
        })
        .expect("compile");
        let config: Value = serde_json::from_str(&result.content).expect("config");
        if mode == "proxy" {
            let group = config["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|item| item["tag"] == sempre_converter::CN_PROXY_GROUP)
                .expect("CN group");
            assert_eq!(group["default"], "test-node");
            assert_eq!(group["outbounds"], json!(["test-node"]));
            let rules = config["route"]["rules"].as_array().expect("routes");
            let cn = rules
                .iter()
                .position(|item| item["outbound"] == sempre_converter::CN_PROXY_GROUP)
                .expect("CN rule");
            let domain = rules
                .iter()
                .position(|item| item["domain"] == "example.cn")
                .expect("domain rule");
            assert!(cn < domain);
        }
        let tun = config["inbounds"]
            .as_array()
            .expect("inbounds")
            .iter()
            .find(|i| i["type"] == "tun")
            .expect("TUN");
        assert_eq!(tun["auto_route"], false, "{platform}");
        assert_eq!(tun["strict_route"], false, "{platform}");
        assert!(
            config["dns"]["servers"]
                .as_array()
                .expect("DNS servers")
                .iter()
                .all(|s| s["type"] != "fakeip")
        );
        assert!(
            config["route"]["rule_set"]
                .as_array()
                .expect("rule sets")
                .iter()
                .any(|s| s["tag"] == "geoip-cn")
        );
    }
}
