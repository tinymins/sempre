use std::collections::HashSet;

use sempre_converter::{Profile, Source};
use url::Url;
use uuid::Uuid;

use crate::{CATALOG_SCHEMA, Catalog, SubscriptionError};

pub(crate) fn catalog(catalog: &Catalog) -> Result<(), SubscriptionError> {
    if catalog.schema != CATALOG_SCHEMA {
        return invalid(format!("unsupported schema {}", catalog.schema));
    }
    if catalog.profiles.is_empty() {
        return invalid("at least one profile is required");
    }
    let custom_ids: HashSet<&str> = catalog
        .custom_nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    if custom_ids.len() != catalog.custom_nodes.len()
        || catalog
            .custom_nodes
            .iter()
            .any(|node| node.id.is_empty() || node.name.trim().is_empty())
    {
        return invalid("custom nodes require unique IDs and names");
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for (index, profile) in catalog.profiles.iter().enumerate() {
        validate_profile(profile, index, &custom_ids, &mut ids, &mut names)?;
    }
    Ok(())
}

fn validate_profile<'a>(
    profile: &'a Profile,
    index: usize,
    custom_ids: &HashSet<&str>,
    ids: &mut HashSet<&'a str>,
    names: &mut HashSet<String>,
) -> Result<(), SubscriptionError> {
    if Uuid::parse_str(&profile.id).is_err() || !ids.insert(&profile.id) {
        return invalid(format!(
            "profile {:?} has an invalid or duplicate ID",
            profile.name
        ));
    }
    let name = profile.name.trim().to_lowercase();
    if (index > 0 && name.is_empty()) || !names.insert(name) {
        return invalid(format!(
            "profile name {:?} is empty or duplicated",
            profile.name
        ));
    }
    if profile.revision == 0 {
        return invalid(format!("profile {:?} has no revision", profile.name));
    }
    let mut source_ids = HashSet::new();
    for source in &profile.sources {
        if !source_ids.insert(source.id.as_str()) {
            return invalid(format!(
                "profile {:?} has duplicate source IDs",
                profile.name
            ));
        }
        validate_source(source)?;
    }
    if profile
        .custom_node_ids
        .iter()
        .any(|id| !custom_ids.contains(id.as_str()))
    {
        return invalid(format!(
            "profile {:?} references a missing custom node",
            profile.name
        ));
    }
    let mode = profile
        .extra
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("local");
    if !matches!(mode, "local" | "remote") {
        return invalid(format!(
            "profile {:?} has unsupported mode {mode:?}",
            profile.name
        ));
    }
    if mode == "remote" && profile.extra.get("remote").is_none() {
        return invalid(format!(
            "remote profile {:?} has no remote settings",
            profile.name
        ));
    }
    Ok(())
}

fn validate_source(source: &Source) -> Result<(), SubscriptionError> {
    if source.id.trim().is_empty() {
        return invalid("source ID is required");
    }
    match source.kind.as_str() {
        "url" => {
            let url = Url::parse(source.url.trim())
                .map_err(|_| SubscriptionError::Invalid("invalid source URL".into()))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return invalid("source URL must be absolute HTTP(S) without credentials");
            }
        }
        "raw" if source.content.trim().is_empty() => return invalid("raw source is empty"),
        "raw" => {}
        value => return invalid(format!("unsupported source type {value:?}")),
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SubscriptionError> {
    Err(SubscriptionError::Invalid(message.into()))
}
