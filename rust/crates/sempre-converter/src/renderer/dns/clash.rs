use serde_json::{Value, json};

use crate::{Profile, Target};

use super::SharedDns;

pub(super) fn render(
    _profile: &Profile,
    target: &Target,
    final_group: &str,
    shared: &SharedDns,
) -> Option<Value> {
    match target.core.as_str() {
        "mihomo" | "clash-rs" => Some(managed(target.core.as_str(), final_group, shared)),
        _ => None,
    }
}

fn managed(core: &str, final_group: &str, shared: &SharedDns) -> Value {
    let detour = if shared.remote_detour.is_empty() {
        final_group
    } else {
        &shared.remote_detour
    };
    let mut remote = format!("tls://{}:{}", shared.remote_dns, shared.remote_port);
    if !detour.is_empty() {
        remote.push('#');
        remote.push_str(detour);
    }
    let local = format!("{}:{}", shared.local_dns, shared.local_port);
    if core == "clash-rs" {
        let mut result = json!({
            "enable": true, "ipv6": true, "respect-rules": true,
            "enhanced-mode": if shared.fakeip_enabled() { "fake-ip" } else { "redir-host" },
            "default-nameserver": [shared.bootstrap_dns],
            "proxy-server-nameserver": [shared.bootstrap_dns], "nameserver": [remote]
        });
        if shared.cn_domain_local_dns() {
            result["nameserver-policy"] = json!({ "geosite:cn": format!("udp://{local}") });
        }
        if shared.fakeip_enabled() {
            result["fake-ip-range"] = json!(shared.fakeip_ipv4_range);
        }
        return result;
    }
    if shared.reject_https() {
        remote.push_str(if remote.contains('#') {
            "&disable-qtype-65=true"
        } else {
            "#disable-qtype-65=true"
        });
    }
    let local = if shared.reject_https() {
        format!("{local}#disable-qtype-65=true")
    } else {
        local
    };
    let mut result = json!({
        "enable": true, "ipv6": true, "respect-rules": true,
        "enhanced-mode": if shared.fakeip_enabled() { "fake-ip" } else { "redir-host" },
        "default-nameserver": [shared.bootstrap_dns],
        "proxy-server-nameserver": [shared.bootstrap_dns],
        "direct-nameserver": [local], "nameserver": [remote]
    });
    if shared.cn_domain_local_dns() {
        result["nameserver-policy"] = json!({ "geosite:cn": [local] });
    }
    if shared.fakeip_enabled() {
        result["fake-ip-range"] = json!(shared.fakeip_ipv4_range);
        result["fake-ip-range6"] = json!(shared.fakeip_ipv6_range);
        result["fake-ip-ttl"] = json!(shared.fakeip_ttl);
    }
    result
}
