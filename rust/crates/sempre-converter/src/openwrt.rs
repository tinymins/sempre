//! `OpenWrt` exports consume the router's listener and dashboard settings.
use crate::{CompileError, Profile};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    dns_listen_port: Option<u16>,
    tproxy_port: Option<u16>,
    clash_api_port: Option<u16>,
    clash_api_secret: Option<String>,
    clash_api_ui_path: Option<String>,
}

pub(crate) fn apply(input: &Profile, profile: &mut Profile) -> Result<(), CompileError> {
    if !profile.dns.is_object() {
        profile.dns = serde_json::json!({});
    }
    let shared = profile
        .dns
        .as_object_mut()
        .expect("DNS object")
        .entry("shared")
        .or_insert_with(|| serde_json::json!({}));
    if !shared.is_object() {
        *shared = serde_json::json!({});
    }
    let shared = shared.as_object_mut().expect("shared DNS object");
    for (key, value) in crate::defaults::openwrt_dns_defaults() {
        shared.entry(key).or_insert(value);
    }
    let settings: Settings = serde_json::from_value(
        profile
            .dns
            .get("shared")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .map_err(|error| CompileError::InvalidEditor {
        field: "dns_config",
        detail: error.to_string(),
    })?;
    if [
        settings.dns_listen_port,
        settings.tproxy_port,
        settings.clash_api_port,
    ]
    .contains(&Some(0))
    {
        return Err(CompileError::InvalidEditor {
            field: "dns_config",
            detail: "OpenWrt listener ports must be between 1 and 65535".into(),
        });
    }
    profile.transparent_proxy.mode = "tproxy".into();
    profile.transparent_proxy.tproxy.dns_listen_port = settings.dns_listen_port.unwrap_or(
        if input.transparent_proxy.tproxy.dns_listen_port == 0 {
            1053
        } else {
            input.transparent_proxy.tproxy.dns_listen_port
        },
    );
    profile.transparent_proxy.tproxy.listen_port =
        settings
            .tproxy_port
            .unwrap_or(if input.transparent_proxy.tproxy.listen_port == 0 {
                7893
            } else {
                input.transparent_proxy.tproxy.listen_port
            });
    if let Some(secret) = settings.clash_api_secret {
        profile.management_api.secret = secret;
    }
    if profile.management_api.external_controller.is_empty()
        && !profile.management_api.secret.is_empty()
    {
        profile.management_api.external_controller =
            format!("0.0.0.0:{}", settings.clash_api_port.unwrap_or(9999));
    } else if let Some(port) = settings.clash_api_port {
        profile.management_api.external_controller = format!("0.0.0.0:{port}");
    }
    if let Some(path) = settings.clash_api_ui_path {
        profile.management_api.external_ui = path;
    } else if profile.management_api.external_ui.is_empty() {
        profile.management_api.external_ui = "/etc/sb/ui".into();
    }
    Ok(())
}
