use sempre_core::{Adapter, BuiltInAdapter, BuiltInKind, Target};

fn target(os: &str, arch: &str) -> Target {
    Target {
        os: os.into(),
        arch: arch.into(),
        amd64_level: 0,
        openwrt: false,
    }
}

#[test]
fn selects_cpu_and_platform_assets() {
    let mihomo = BuiltInAdapter::new(BuiltInKind::Mihomo);
    let mut linux = target("linux", "amd64");
    linux.amd64_level = 3;
    assert_eq!(
        mihomo
            .package_assets("1.19.29", &linux)
            .expect("assets")
            .names[0],
        "mihomo-linux-amd64-v3-v1.19.29.gz"
    );
    let clash = BuiltInAdapter::new(BuiltInKind::ClashRs);
    assert_eq!(
        clash
            .package_assets("0.10.8", &target("windows", "amd64"))
            .expect("asset")
            .names[0],
        "clash-rs-x86_64-pc-windows-msvc.exe"
    );
}

#[test]
fn published_controller_is_kept_and_reached_over_loopback() {
    let directory = tempfile::tempdir().expect("directory");
    let config = directory.path().join("input.json");
    std::fs::write(
        &config,
        r#"{"experimental":{"clash_api":{"external_controller":"0.0.0.0:9999","secret":"s3","external_ui":"/etc/sb/ui"}}}"#,
    )
    .expect("config");
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    let runtime = adapter
        .prepare_runtime_keeping_controller(&config, &directory.path().join("control"))
        .expect("runtime");
    let control = runtime.control.expect("control");
    assert_eq!(control.base_url, "http://127.0.0.1:9999");
    assert_eq!(control.secret, "s3");
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runtime.config).expect("runtime")).expect("JSON");
    let api = &written["experimental"]["clash_api"];
    assert_eq!(api["external_controller"], "0.0.0.0:9999");
    assert_eq!(api["external_ui"], "ui");
}

#[test]
fn configuration_without_a_controller_gets_the_private_one() {
    let directory = tempfile::tempdir().expect("directory");
    let config = directory.path().join("input.json");
    std::fs::write(&config, "{}").expect("config");
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    let runtime = adapter
        .prepare_runtime_keeping_controller(&config, &directory.path().join("control"))
        .expect("runtime");
    assert!(
        runtime
            .control
            .expect("control")
            .base_url
            .starts_with("http://127.0.0.1:")
    );
}

#[test]
fn openwrt_hosts_prefer_the_musl_sing_box_asset() {
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    let mut host = target("linux", "amd64");
    assert_eq!(
        adapter
            .package_assets("1.14.4", &host)
            .expect("assets")
            .names,
        ["sing-box-1.14.4-linux-amd64.tar.gz"]
    );
    host.openwrt = true;
    assert_eq!(
        adapter
            .package_assets("1.14.4", &host)
            .expect("assets")
            .names,
        [
            "sing-box-1.14.4-linux-amd64-musl.tar.gz",
            "sing-box-1.14.4-linux-amd64.tar.gz"
        ]
    );
}

#[test]
fn openwrt_editors_only_offer_what_a_router_applies() {
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    let mut host = target("linux", "amd64");
    let desktop = adapter.capabilities(Some("1.13.21"), &host).features;
    assert!(desktop.iter().any(|feature| feature == "private_access"));
    host.openwrt = true;
    let router = adapter.capabilities(Some("1.13.21"), &host).features;
    assert!(router.iter().any(|feature| feature == "dns.local_upstream"));
    for feature in &router {
        assert!(
            feature != "private_access"
                && feature != "dns.system_takeover"
                && feature != "inbound.local_proxy"
                && !feature.starts_with("transparent."),
            "{feature}"
        );
    }
}

#[test]
fn openwrt_hosts_compile_the_openwrt_target() {
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    let mut host = target("linux", "amd64");
    host.openwrt = true;
    let compiler = adapter
        .compiler_target(Some("1.13.21"), &host)
        .expect("target");
    assert_eq!(compiler.format, "sing-box-v13-openwrt");
    assert_eq!(compiler.platform, "openwrt");
    assert_eq!(
        adapter
            .compiler_target(Some("1.13.21"), &target("linux", "amd64"))
            .expect("target")
            .format,
        "sing-box-v13"
    );
}
