use std::collections::HashSet;

use sempre_converter::{EditorConfig, Profile, profile_from_editor};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::ApiError, subscriptions::SubscriptionFields};

pub(crate) async fn validate(
    pool: &PgPool,
    fields: &SubscriptionFields,
    selected: &[Uuid],
    changed: Option<&HashSet<String>>,
) -> Result<(), ApiError> {
    validate_editor_fields(fields, changed)?;
    let includes = |name: &str| changed.is_none_or(|changed| changed.contains(name));
    if includes("logLevel")
        && !matches!(
            fields.log_level.as_str(),
            "off" | "error" | "warn" | "info" | "debug"
        )
    {
        return Err(ApiError::bad_request("invalid logLevel"));
    }
    if includes("cacheTtlMinutes") && fields.cache_ttl_minutes.is_some_and(|value| value < 0) {
        return Err(ApiError::bad_request("cacheTtlMinutes must be nonnegative"));
    }
    if includes("subscribeItems")
        && fields
            .subscribe_items
            .as_ref()
            .is_some_and(|value| !value.is_array())
    {
        return Err(ApiError::bad_request("subscribeItems must be an array"));
    }
    if includes("authorizedUserIds") && !fields.authorized_user_ids.is_empty() {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
            .bind(&fields.authorized_user_ids)
            .fetch_one(pool)
            .await?;
        if usize::try_from(count).ok() != Some(fields.authorized_user_ids.len()) {
            return Err(ApiError::bad_request("authorized user does not exist"));
        }
    }
    let mut unique = std::collections::HashSet::new();
    if !selected.iter().all(|id| unique.insert(id)) {
        return Err(ApiError::bad_request(
            "selectedCustomNodeIds contains duplicates",
        ));
    }
    Ok(())
}

fn validate_editor_fields(
    fields: &SubscriptionFields,
    changed: Option<&HashSet<String>>,
) -> Result<(), ApiError> {
    let includes = |names: &[&str]| {
        changed.is_none_or(|changed| names.iter().any(|name| changed.contains(*name)))
    };
    if changed.is_some_and(|changed| {
        ![
            "ruleList",
            "useSystemRuleList",
            "group",
            "useSystemGroup",
            "filter",
            "useSystemFilter",
            "customConfig",
            "useSystemCustomConfig",
            "dnsConfig",
            "useSystemDnsConfig",
            "privateAccessConfig",
            "servers",
        ]
        .iter()
        .any(|name| changed.contains(*name))
    }) {
        return Ok(());
    }
    let editor = EditorConfig {
        rule_list: if includes(&["ruleList", "useSystemRuleList"]) {
            active(fields.use_system_rule_list, fields.rule_list.as_ref())
        } else {
            String::new()
        },
        group: if includes(&["group", "useSystemGroup"]) {
            active(fields.use_system_group, fields.group.as_ref())
        } else {
            String::new()
        },
        filter: if includes(&["filter", "useSystemFilter"]) {
            active(fields.use_system_filter, fields.filter.as_ref())
        } else {
            String::new()
        },
        custom_config: if includes(&["customConfig", "useSystemCustomConfig"]) {
            active(
                fields.use_system_custom_config,
                fields.custom_config.as_ref(),
            )
        } else {
            String::new()
        },
        dns_config: if includes(&["dnsConfig", "useSystemDnsConfig"]) {
            active(fields.use_system_dns_config, fields.dns_config.as_ref())
        } else {
            String::new()
        },
        private_access_config: if includes(&["privateAccessConfig"]) {
            fields.private_access_config.clone().unwrap_or_default()
        } else {
            String::new()
        },
        servers: if includes(&["servers"]) {
            fields.servers.clone().unwrap_or_default()
        } else {
            String::new()
        },
    };
    profile_from_editor(&Profile {
        editor,
        ..Profile::default()
    })
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(())
}

fn active(use_system: bool, value: Option<&String>) -> String {
    if use_system {
        String::new()
    } else {
        value.cloned().unwrap_or_default()
    }
}
