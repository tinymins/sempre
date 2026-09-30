//! `OpenWrt` exports consume the router's listener and dashboard settings.
use serde::Deserialize;
use serde_json::Value;

use crate::{CompileError, Profile, parse_jsonc_value};

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
    let dns = if input.editor.dns_config.trim().is_empty() {
        input.dns.clone()
    } else {
        parse_jsonc_value(&input.editor.dns_config)?
    };
    let settings: Settings = serde_json::from_value(
        dns.get("shared")
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
    if let Some(url) = dns
        .pointer("/shared/gfwBlackRuleSetUrl")
        .and_then(Value::as_str)
    {
        profile.dns["shared"]["gfwBlackRuleSetUrl"] = url.into();
    }
    Ok(())
}
