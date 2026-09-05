use std::collections::BTreeMap;

use sempre_converter::{Profile, Target};
use sempre_state::ConfigBuild;
use sempre_subscription::SubscriptionError;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{DnsSettings, ManagerError};

const CONFIG_BUILD_SCHEMA: u32 = 8;

pub(crate) fn config_build(
    profile: &Profile,
    target: &Target,
    dns_settings: &DnsSettings,
) -> Result<ConfigBuild, ManagerError> {
    let private_access_policy = private_access_policy(profile, target)?;
    let dns_frontend_enabled = dns_settings.enabled
        && target.core == "sing-box"
        && matches!(target.platform.as_str(), "windows" | "macos");
    Ok(ConfigBuild {
        profile_id: profile.id.clone(),
        profile_revision: profile.revision,
        target_key: format!(
            "{}|{}|{}|front-dns:{dns_frontend_enabled}|build:{CONFIG_BUILD_SCHEMA}",
            target.format, target.version, target.platform
        ),
        runtime_key: Some(runtime_key(profile, dns_settings)?),
        private_access_policy,
    })
}

fn private_access_policy(profile: &Profile, target: &Target) -> Result<Value, ManagerError> {
    let config = sempre_converter::prepare_profile(profile, target)?.private_access;
    let connectors = config
        .get("connectors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|connector| {
            let home = connector.get("homeNetwork")?;
            (connector.get("type").and_then(Value::as_str) == Some("wireguard")
                && connector.get("enabled").and_then(Value::as_bool) != Some(false)
                && home.get("enabled").and_then(Value::as_bool) == Some(true))
            .then(|| {
                json!({
                    "enabled": true,
                    "type": "wireguard",
                    "tag": connector.get("tag").cloned().unwrap_or(Value::Null),
                    "homeNetwork": {
                        "enabled": true,
                        "addressCidrs": home.get("addressCidrs").cloned().unwrap_or(Value::Null),
                    },
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "enabled": config.get("enabled").and_then(Value::as_bool) == Some(true),
        "connectors": connectors,
    }))
}

fn runtime_key(profile: &Profile, dns_settings: &DnsSettings) -> Result<String, ManagerError> {
    let value = json!({
        "transparent_proxy": profile.transparent_proxy,
        "local_proxy": profile.local_proxy,
        "management_api": profile.management_api,
        "dns_frontend": {
            "enabled": dns_settings.enabled,
            "rule_sets": dns_settings.rule_sets,
        },
    });
    let data = serde_json::to_vec(&canonical(value)).map_err(|error| {
        SubscriptionError::Invalid(format!("encode profile runtime settings: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(data)))
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values.into_iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::{Profile, Target, config_build};
    use crate::{Manager, ProcessRunner};
    use sempre_state::{Layout, Store};

    #[test]
    fn frontend_upstreams_do_not_change_core_build_identity() {
        let profile = Profile::default();
        let directory = tempfile::tempdir().expect("directory");
        let manager = Manager::with_runner(Store::new(Layout::at(directory.path())), ProcessRunner)
            .expect("manager");
        let before = manager.dns_settings();
        let mut after = before.clone();
        after.direct_upstreams = vec!["udp://223.5.5.5".into()];
        assert!(!before.requires_core_rebuild(&after));
        let target = Target::parse("sing-box").expect("target");
        assert_eq!(
            config_build(&profile, &target, &before).expect("before"),
            config_build(&profile, &target, &after).expect("after")
        );
    }

    #[test]
    fn private_access_build_metadata_excludes_wireguard_keys() {
        let profile = Profile {
            private_access: serde_json::json!({
                "enabled": true,
                "connectors": [{
                    "enabled": true,
                    "type": "wireguard",
                    "tag": "home-wg",
                    "endpoint": { "privateKey": "must-not-appear" },
                    "homeNetwork": {
                        "enabled": true,
                        "addressCidrs": ["10.8.28.0/24"],
                        "note": "must-not-appear"
                    }
                }]
            }),
            ..Profile::default()
        };
        let directory = tempfile::tempdir().expect("directory");
        let manager = Manager::with_runner(Store::new(Layout::at(directory.path())), ProcessRunner)
            .expect("manager");
        let target = Target::parse("sing-box-v14").expect("target");
        let build = config_build(&profile, &target, &manager.dns_settings()).expect("build");
        let encoded = serde_json::to_string(&build.private_access_policy).expect("metadata");
        assert!(encoded.contains("10.8.28.0/24"));
        assert!(!encoded.contains("must-not-appear"));
    }
}
