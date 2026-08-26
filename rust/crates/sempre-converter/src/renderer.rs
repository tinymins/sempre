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
    match target.format.as_str() {
        "clash" | "clash-meta" | "clash-rs" => clash::render(profile, proxies, target),
        "xray" | "v2ray" => v2ray::render(profile, proxies, target),
        "dae" => dae::render(profile, proxies),
        _ if target.core == "sing-box" => singbox::render(profile, proxies, target, snapshots),
        _ => Err(CompileError::UnsupportedTarget(target.format.clone())),
    }
}
