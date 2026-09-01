use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{CompileError, Profile, Target, prepare_profile};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsFrontendPolicy {
    pub enabled: bool,
    pub fakeip_enabled: bool,
    pub fakeip_ipv4_range: String,
    pub fakeip_ipv6_range: String,
    pub core_listen_port: u16,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn apply_dns_frontend_settings(
    profile: &Profile,
    target: &Target,
    enabled: bool,
) -> Result<Profile, CompileError> {
    if !managed_frontend_target(target) {
        return Ok(profile.clone());
    }
    let mut profile = prepare_profile(profile, target)?;
    let shared = shared_mut(&mut profile.dns);
    shared.insert("systemDnsTakeoverEnabled".into(), Value::Bool(enabled));
    shared.insert("systemDnsListenPort".into(), json!(53));
    shared.insert("systemDnsListenHosts".into(), json!(["127.0.0.1"]));
    shared.insert("managedDnsFrontend".into(), Value::Bool(true));
    shared.insert("systemDnsTakeoverHost".into(), Value::Bool(true));
    profile.editor.dns_config.clear();
    Ok(profile)
}

pub fn dns_frontend_policy(
    profile: &Profile,
    target: &Target,
) -> Result<DnsFrontendPolicy, CompileError> {
    let profile = prepare_profile(profile, target)?;
    let shared = profile.dns.get("shared").unwrap_or(&profile.dns);
    Ok(DnsFrontendPolicy {
        enabled: managed_frontend_target(target)
            && shared
                .get("systemDnsTakeoverEnabled")
                .and_then(Value::as_bool)
                == Some(true),
        fakeip_enabled: boolean(shared, "fakeipEnabled", true),
        fakeip_ipv4_range: string(shared, "fakeipIpv4Range", "198.18.0.0/15"),
        fakeip_ipv6_range: string(shared, "fakeipIpv6Range", "fc00::/18"),
        core_listen_port: match profile.transparent_proxy.tproxy.dns_listen_port {
            0 => 1053,
            port => port,
        },
        complete: true,
        warnings: Vec::new(),
    })
}

fn managed_frontend_target(target: &Target) -> bool {
    target.core == "sing-box"
}

fn shared_mut(dns: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !dns.is_object() {
        *dns = json!({});
    }
    let object = dns.as_object_mut().expect("DNS object");
    let shared = object.entry("shared").or_insert_with(|| json!({}));
    if !shared.is_object() {
        *shared = json!({});
    }
    shared.as_object_mut().expect("shared DNS object")
}

fn boolean(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn string(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn desktop_overlay_only_changes_frontend_plumbing() {
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": {
                "systemDnsTakeoverEnabled": false,
                "remoteDns": "8.8.8.8",
                "fakeipEnabled": false
            }}
        }))
        .expect("profile");
        let target = Target::parse("sing-box-v14-windows").expect("target");
        let overlaid = apply_dns_frontend_settings(&profile, &target, true).expect("overlay");
        assert_eq!(overlaid.dns["shared"]["remoteDns"], "8.8.8.8");
        assert_eq!(overlaid.dns["shared"]["fakeipEnabled"], false);
        assert_eq!(overlaid.dns["shared"]["systemDnsTakeoverEnabled"], true);
    }

    #[test]
    fn policy_reads_core_mode_without_importing_profile_routes() {
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": {
                "systemDnsTakeoverEnabled": true,
                "fakeipEnabled": false
            }},
            "rules": ["DOMAIN-SUFFIX,example.com,DIRECT"]
        }))
        .expect("profile");
        let policy = dns_frontend_policy(
            &profile,
            &Target::parse("sing-box-v13-macos").expect("target"),
        )
        .expect("policy");
        assert!(policy.enabled);
        assert!(!policy.fakeip_enabled);
        assert!(policy.complete);
    }

    #[test]
    fn linux_sing_box_uses_the_same_managed_frontend_boundary() {
        let profile = Profile::default();
        let target = Target::parse("sing-box-v14").expect("target");
        let overlaid = apply_dns_frontend_settings(&profile, &target, true).expect("overlay");
        assert_eq!(overlaid.dns["shared"]["managedDnsFrontend"], true);
        let policy = dns_frontend_policy(&overlaid, &target).expect("policy");
        assert!(policy.enabled);
        assert_eq!(policy.core_listen_port, 1053);
    }
}
