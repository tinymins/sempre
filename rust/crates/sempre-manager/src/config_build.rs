use std::collections::BTreeMap;

use sempre_converter::{Profile, Target};
use sempre_state::ConfigBuild;
use sempre_subscription::SubscriptionError;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{DnsSettings, ManagerError};

const CONFIG_BUILD_SCHEMA: u32 = 7;

pub(crate) fn config_build(
    profile: &Profile,
    target: &Target,
    dns_settings: &DnsSettings,
) -> Result<ConfigBuild, ManagerError> {
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
    })
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
}
