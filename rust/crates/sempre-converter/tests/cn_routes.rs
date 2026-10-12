use sempre_converter::{
    CompileRequest, Profile, Target, apply_dns_frontend_settings, compile, dns_frontend_policy,
};
use serde_json::{Value, json};

#[test]
fn cn_routes_compile_real_ip_dns_and_disable_native_auto_routes_on_all_platforms() {
    for platform in ["macos", "windows", "linux"] {
        let target = Target {
            platform: platform.into(),
            ..Target::parse("sing-box-v13").expect("target")
        };
        let mut profile = Profile::default();
        profile
            .extra
            .insert("sempre_cn_direct_routes".into(), json!(true));
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
            custom_nodes: vec![],
        })
        .expect("compile");
        let config: Value = serde_json::from_str(&result.content).expect("config");
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
