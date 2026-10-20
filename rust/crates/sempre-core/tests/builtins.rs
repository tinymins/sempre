use sempre_core::{Adapter, BuiltInAdapter, BuiltInKind, Target};

fn target(os: &str, arch: &str) -> Target {
    Target {
        os: os.into(),
        arch: arch.into(),
        amd64_level: 0,
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
