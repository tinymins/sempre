mod clash;
mod dae;
mod dns;
mod singbox;
mod transparent;
mod v2ray;

use crate::{CompileError, FieldDiff, Profile, Proxy, SourceSnapshot, Target};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
    snapshots: &[SourceSnapshot],
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let rendered = match target.format.as_str() {
        "clash" | "clash-meta" | "clash-rs" => clash::render(profile, proxies, target),
        "xray" | "v2ray" => v2ray::render(profile, proxies, target),
        "dae" => dae::render(profile, proxies),
        _ if target.core == "sing-box" => singbox::render(profile, proxies, target, snapshots),
        _ => Err(CompileError::UnsupportedTarget(target.format.clone())),
    }?;
    let (content, diffs, mut warnings) = rendered;
    if has_private_connector(profile) && (target.core != "sing-box" || target.version == "11") {
        warnings.push(format!(
            "private access is not supported by target {}",
            target.format
        ));
    }
    Ok((content, diffs, warnings))
}

fn has_private_connector(profile: &Profile) -> bool {
    let config = &profile.private_access;
    config.get("enabled").and_then(serde_json::Value::as_bool) == Some(true)
        && config
            .get("connectors")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    if item.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
                        return false;
                    }
                    match item
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("outbound")
                    {
                        "wireguard" | "tailscale" => item
                            .get("endpoint")
                            .is_some_and(serde_json::Value::is_object),
                        "outbound" | "v2ray" | "xray" | "vmess" | "vless" | "trojan" | "socks"
                        | "socks5" | "http" | "ssh" | "hysteria2" | "tuic" | "anytls" => item
                            .get("outbound")
                            .is_some_and(serde_json::Value::is_object),
                        _ => false,
                    }
                })
            })
}
