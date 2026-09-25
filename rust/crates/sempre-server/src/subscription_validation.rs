use sempre_converter::{EditorConfig, Profile, parse_jsonc_value, profile_from_editor};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::ApiError, subscriptions::SubscriptionFields};

pub(crate) async fn validate(
    pool: &PgPool,
    fields: &SubscriptionFields,
    selected: &[Uuid],
) -> Result<(), ApiError> {
    validate_editor_fields(fields)?;
    if !matches!(
        fields.log_level.as_str(),
        "off" | "error" | "warn" | "info" | "debug"
    ) {
        return Err(ApiError::bad_request("invalid logLevel"));
    }
    if fields.cache_ttl_minutes.is_some_and(|value| value < 0) {
        return Err(ApiError::bad_request("cacheTtlMinutes must be nonnegative"));
    }
    if fields
        .subscribe_items
        .as_ref()
        .is_some_and(|value| !value.is_array())
    {
        return Err(ApiError::bad_request("subscribeItems must be an array"));
    }
    if !fields.authorized_user_ids.is_empty() {
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

fn validate_editor_fields(fields: &SubscriptionFields) -> Result<(), ApiError> {
    let editor = EditorConfig {
        rule_list: effective_default_on_empty(
            fields.use_system_rule_list,
            fields.rule_list.as_ref(),
            "",
        )?,
        group: effective_default_on_empty(fields.use_system_group, fields.group.as_ref(), "")?,
        filter: active(fields.use_system_filter, fields.filter.as_ref()),
        custom_config: active(
            fields.use_system_custom_config,
            fields.custom_config.as_ref(),
        ),
        dns_config: active(fields.use_system_dns_config, fields.dns_config.as_ref()),
        private_access_config: fields.private_access_config.clone().unwrap_or_default(),
        servers: fields.servers.clone().unwrap_or_default(),
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

pub(crate) fn effective_default_on_empty(
    use_system: bool,
    value: Option<&String>,
    default: &str,
) -> Result<String, ApiError> {
    if use_system {
        return Ok(default.into());
    }
    let value = active(use_system, value);
    if value.trim().is_empty() {
        return Ok(default.into());
    }
    let parsed =
        parse_jsonc_value(&value).map_err(|error| ApiError::bad_request(error.to_string()))?;
    if parsed.is_null()
        || parsed.as_array().is_some_and(Vec::is_empty)
        || parsed.as_object().is_some_and(serde_json::Map::is_empty)
    {
        return Ok(default.into());
    }
    Ok(value)
}
