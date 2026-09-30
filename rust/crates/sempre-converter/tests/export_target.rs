use sempre_converter::Target;

#[test]
fn openwrt_is_an_explicit_consumer_for_each_singbox_version() {
    for (format, version) in [
        ("sing-box-openwrt", "11"),
        ("sing-box-v12-openwrt", "12"),
        ("sing-box-v13-openwrt", "13"),
        ("sing-box-v14-openwrt", "14"),
    ] {
        let target = Target::parse(format).expect("OpenWrt target");
        assert_eq!(target.core, "sing-box");
        assert_eq!(target.platform, "openwrt");
        assert_eq!(target.version, version);
    }
}

#[test]
fn existing_desktop_and_managed_targets_keep_their_meaning() {
    for (format, platform, version) in [
        ("sing-box", "default", "11"),
        ("sing-box-v13", "default", "13"),
        ("sing-box-windows", "windows", "11"),
        ("sing-box-v14-macos", "macos", "14"),
    ] {
        let target = Target::parse(format).unwrap();
        assert_eq!(target.platform, platform);
        assert_eq!(target.version, version);
    }
    assert!(Target::parse("sing-box-v15-openwrt").is_err());
    assert!(Target::parse("sing-box-v13-openwrt-windows").is_err());
}
