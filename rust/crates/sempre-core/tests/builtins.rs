use sempre_core::{Adapter, BuiltInAdapter, BuiltInKind, Target, built_in_registry};

fn target(os: &str, arch: &str) -> Target {
    Target {
        os: os.into(),
        arch: arch.into(),
        amd64_level: 0,
    }
}

#[test]
fn registry_contains_all_builtin_cores() {
    assert_eq!(
        built_in_registry().ids(),
        ["clash-rs", "dae", "mihomo", "sing-box", "v2ray", "xray"]
    );
}

#[test]
fn commands_match_existing_core_clis() {
    let sing_box = BuiltInAdapter::new(BuiltInKind::SingBox);
    assert_eq!(
        sing_box
            .validation_command("sing-box", "config.json", "data")
            .arguments,
        [
            "check",
            "-c",
            "config.json",
            "-D",
            "data",
            "--disable-color"
        ]
    );
    assert!(
        sing_box
            .validation_command("sing-box", "config.json", "data")
            .working_directory
            .is_none()
    );
    let xray = BuiltInAdapter::new(BuiltInKind::Xray);
    let validation = xray.validation_command("/opt/xray", "config.json", "data");
    assert_eq!(validation.environment["xray.location.asset"], "/opt");
    assert_eq!(
        validation
            .working_directory
            .expect("working directory")
            .to_string_lossy(),
        "data"
    );
}

#[test]
fn parses_all_version_outputs() {
    let fixtures = [
        (
            BuiltInKind::SingBox,
            "sing-box version 1.13.18\n",
            "1.13.18",
        ),
        (
            BuiltInKind::Mihomo,
            "Mihomo Meta v1.19.29 linux amd64\n",
            "1.19.29",
        ),
        (
            BuiltInKind::Xray,
            "Xray 26.3.27 (Xray, Penetrates Everything.)",
            "26.3.27",
        ),
        (
            BuiltInKind::V2Ray,
            "V2Ray 5.40.0 (V2Fly, a community-driven edition)",
            "5.40.0",
        ),
        (BuiltInKind::ClashRs, "clash-rs 0.10.8", "0.10.8"),
        (BuiltInKind::Dae, "dae version v2.0.0", "2.0.0"),
    ];
    for (kind, output, expected) in fixtures {
        assert_eq!(
            BuiltInAdapter::new(kind)
                .parse_version(output)
                .expect("version"),
            expected
        );
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
fn sing_box_compiler_tracks_supported_versions() {
    let adapter = BuiltInAdapter::new(BuiltInKind::SingBox);
    assert_eq!(
        adapter
            .compiler_target(Some("1.12.20"), &target("darwin", "arm64"))
            .expect("target")
            .format,
        "sing-box-v12-macos"
    );
    assert!(
        !adapter
            .compiler_target(Some("1.15.0"), &target("darwin", "arm64"))
            .expect("target")
            .warnings
            .is_empty()
    );
}
