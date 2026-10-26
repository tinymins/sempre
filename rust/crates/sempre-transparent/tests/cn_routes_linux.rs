#![cfg(target_os = "linux")]

use std::process::Command;

use sempre_transparent::{CnRouteIntent, Controller};

#[tokio::test]
#[ignore = "requires NET_ADMIN inside an isolated Linux network namespace"]
async fn cn_routes_kernel_priority_update_recovery_and_cleanup() {
    assert_eq!(std::env::var("SEMPRE_CN_NETNS_TEST").as_deref(), Ok("1"));
    ip(&["link", "add", "cn-uplink", "type", "dummy"]);
    ip(&["address", "add", "192.0.2.1/24", "dev", "cn-uplink"]);
    ip(&["link", "set", "cn-uplink", "up"]);
    ip(&[
        "route",
        "replace",
        "default",
        "via",
        "192.0.2.2",
        "dev",
        "cn-uplink",
    ]);
    ip(&["link", "add", "cn-tun", "type", "dummy"]);
    ip(&["address", "add", "172.19.0.1/30", "dev", "cn-tun"]);
    ip(&["link", "set", "cn-tun", "up"]);
    ip(&["-6", "address", "add", "fd00:2::1/64", "dev", "cn-uplink"]);
    ip(&[
        "-6",
        "address",
        "add",
        "fdfe:dcba:9876::1/126",
        "dev",
        "cn-tun",
    ]);
    ip(&[
        "-6",
        "route",
        "add",
        "default",
        "via",
        "fd00:2::2",
        "dev",
        "cn-uplink",
    ]);
    let directory = tempfile::tempdir().expect("directory");
    let layout = sempre_state::Layout::system_at(directory.path());
    let controller = Controller::new(&layout);
    let mut intent = CnRouteIntent {
        direct: true,
        prefixes: vec![
            "223.5.5.0/24".into(),
            "114.114.114.0/24".into(),
            "2400:3200::/32".into(),
        ],
        tun_addresses: vec!["172.19.0.1/30".into(), "fdfe:dcba:9876::1/126".into()],
        capture_prefixes: vec!["10.88.0.0/16".into()],
        intranet_exclusions: None,
        outbound_interfaces: [Some("cn-uplink".into()), Some("cn-uplink".into())],
    };
    if let Ok(path) = std::env::var("SEMPRE_CN_SNAPSHOT") {
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("snapshot")).expect("rules JSON");
        intent.prefixes = snapshot["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .flat_map(|r| r["ip_cidr"].as_array().expect("CIDRs"))
            .map(|p| p.as_str().expect("CIDR").to_owned())
            .collect();
    }
    let started = std::time::Instant::now();
    controller.apply_cn_routes(&intent).await.expect("install");
    eprintln!(
        "installed {} CN CIDRs in {:?}",
        intent.prefixes.len(),
        started.elapsed()
    );
    assert!(ip(&["route", "get", "223.5.5.5"]).contains("dev cn-uplink"));
    assert!(ip(&["route", "get", "1.1.1.1"]).contains("dev cn-tun"));
    assert!(ip(&["route", "get", "10.88.1.1"]).contains("dev cn-tun"));
    assert!(ip(&["route", "get", "1.1.1.1", "mark", "0x53500002"]).contains("dev cn-uplink"));
    assert!(ip(&["-6", "route", "get", "2400:3200::1"]).contains("dev cn-uplink"));
    assert!(ip(&["-6", "route", "get", "2606:4700:4700::1111"]).contains("dev cn-tun"));
    intent.prefixes.push("120.0.0.0/8".into());
    controller.apply_cn_routes(&intent).await.expect("update");
    assert!(ip(&["route", "get", "120.1.1.1"]).contains("dev cn-uplink"));
    ip(&[
        "route",
        "replace",
        "default",
        "via",
        "192.0.2.3",
        "dev",
        "cn-uplink",
    ]);
    controller
        .apply_cn_routes(&intent)
        .await
        .expect("gateway change");
    assert!(ip(&["route", "get", "223.5.5.5"]).contains("via 192.0.2.3"));
    // Service recovery keeps CN routes; the next cycle reconciles them.
    let restarted = Controller::new(&layout);
    restarted.recover_stale_system_dns().await.expect("recover");
    assert!(ip(&["route", "get", "223.5.5.5"]).contains("via 192.0.2.3"));
    restarted
        .apply_cn_routes(&intent)
        .await
        .expect("reconcile after restart");
    assert!(ip(&["route", "get", "223.5.5.5"]).contains("via 192.0.2.3"));
    // A fresh controller removes the previous process's routes from its journal.
    let started = std::time::Instant::now();
    Controller::new(&layout)
        .cleanup_cn_routes()
        .await
        .expect("cleanup");
    eprintln!("cleaned routes in {:?}", started.elapsed());
    assert!(ip(&["route", "get", "1.1.1.1"]).contains("dev cn-uplink"));
    assert!(!ip(&["rule", "show"]).contains("20243:"));
    assert!(!layout.home.join("cn-routes.json").exists());
}

fn ip(arguments: &[&str]) -> String {
    let output = Command::new("ip")
        .args(arguments)
        .output()
        .expect("iproute2");
    assert!(
        output.status.success(),
        "ip {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8")
}
