use super::*;

#[test]
fn custom_core_sources_do_not_share_version_directories() {
    let layout = Layout::at(Path::new("sandbox"));
    let official = layout.core_binary("sing-box", None, "1.2.3");
    let custom = layout.core_binary("sing-box", Some("tinymins/sing-box"), "1.2.3");
    assert_ne!(official, custom);
    let expected =
        Path::new("sing-box/sources/tinymins/sing-box/1.2.3").join(executable_name("sing-box"));
    assert!(custom.ends_with(expected));
}

#[test]
fn openwrt_keeps_configuration_on_flash_and_churn_in_ram() {
    let layout = Layout::openwrt_at(Path::new("/"));
    assert!(layout.openwrt);
    for persistent in [
        &layout.root,
        &layout.service_executable,
        &layout.command_executable,
        &layout.cores,
        &layout.resources,
        &layout.state,
        &layout.subscription_catalog,
        &layout.configs,
        &layout.core_data,
        &layout.ui,
    ] {
        assert!(persistent.starts_with("/etc/sempre"), "{persistent:?}");
    }
    for volatile in [
        &layout.runtime,
        &layout.logs,
        &layout.manager_log,
        &layout.endpoint,
        &layout.traffic_history,
        &layout.dns_query_history,
        &layout.instance_lock,
    ] {
        assert!(volatile.starts_with("/tmp/sempre"), "{volatile:?}");
    }
}

#[test]
fn other_hosts_keep_core_data_in_the_runtime_directory() {
    let layout = Layout::at(Path::new("sandbox"));
    assert!(!layout.openwrt);
    assert_eq!(layout.core_data, layout.runtime);
}
