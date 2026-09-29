//! Server field names adapted to the converter's canonical editor contract.
use sempre_converter::{EditorConfig, recommended_editor_defaults};
use serde_json::{Value, json};

use crate::subscriptions::SubscriptionFields;

pub(crate) fn defaults() -> Value {
    let defaults = recommended_editor_defaults().editor;
    json!({
        "ruleList": defaults.rule_list, "group": defaults.group,
        "filter": defaults.filter, "customConfig": defaults.custom_config,
        "dnsConfig": defaults.dns_config,
    })
}

pub(crate) fn editor(fields: &SubscriptionFields, core: &str) -> EditorConfig {
    let mut defaults = recommended_editor_defaults();
    let defaults = defaults.by_core.remove(core).unwrap_or(defaults.editor);
    EditorConfig {
        rule_list: effective(
            fields.use_system_rule_list,
            fields.rule_list.as_ref(),
            defaults.rule_list,
        ),
        group: effective(
            fields.use_system_group,
            fields.group.as_ref(),
            defaults.group,
        ),
        filter: effective(
            fields.use_system_filter,
            fields.filter.as_ref(),
            defaults.filter,
        ),
        custom_config: effective(
            fields.use_system_custom_config,
            fields.custom_config.as_ref(),
            defaults.custom_config,
        ),
        dns_config: effective(
            fields.use_system_dns_config,
            fields.dns_config.as_ref(),
            defaults.dns_config,
        ),
        private_access_config: fields.private_access_config.clone().unwrap_or_default(),
        servers: fields.servers.clone().unwrap_or_default(),
    }
}

fn effective(inherited: bool, custom: Option<&String>, default: String) -> String {
    if inherited {
        default
    } else {
        custom.cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sempre_converter::{Profile, Target, prepare_profile};

    #[test]
    fn inherited_server_settings_match_client_for_each_core() {
        let fields: SubscriptionFields = serde_json::from_value(json!({})).unwrap();
        for core in ["sing-box", "mihomo", "clash-rs", "xray", "v2ray", "dae"] {
            let target = Target {
                core: core.into(),
                ..Target::parse("clash-meta").unwrap()
            };
            let server = prepare_profile(
                &Profile {
                    editor: editor(&fields, core),
                    ..Profile::default()
                },
                &target,
            )
            .unwrap();
            let client = prepare_profile(
                &Profile {
                    extra: [
                        "use_system_groups",
                        "use_system_rules",
                        "use_system_filters",
                        "use_system_dns",
                        "use_system_custom_config",
                    ]
                    .map(|flag| (flag.into(), json!(true)))
                    .into_iter()
                    .collect(),
                    ..Profile::default()
                },
                &target,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(&server.groups).unwrap(),
                serde_json::to_value(&client.groups).unwrap()
            );
            assert_eq!(
                serde_json::to_value(&server.rule_providers).unwrap(),
                serde_json::to_value(&client.rule_providers).unwrap()
            );
            assert_eq!(server.filters, client.filters);
            assert_eq!(server.dns["shared"], client.dns["shared"]);
            assert_eq!(server.dns["overrides"], json!({}));
            assert_eq!(server.rules, client.rules);
        }
    }

    #[test]
    fn explicit_empty_settings_do_not_restore_defaults() {
        let fields: SubscriptionFields = serde_json::from_value(
            json!({"useSystemGroup":false,"group":"[]","useSystemRuleList":false,"ruleList":"{}"}),
        )
        .unwrap();
        let profile = sempre_converter::profile_from_editor(&Profile {
            editor: editor(&fields, "mihomo"),
            ..Profile::default()
        })
        .unwrap();
        assert!(profile.groups.is_empty());
        assert!(profile.rule_providers.is_empty());
    }
}
