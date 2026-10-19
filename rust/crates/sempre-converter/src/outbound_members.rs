use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

/// Selectable routing destinations, including endpoints and acyclic user groups.
/// Managed DNS selectors are never nested inside one another.
pub fn dns_outbound_members(document: &Value) -> Vec<String> {
    let mut entries = ["outbounds", "endpoints"]
        .into_iter()
        .flat_map(|key| document[key].as_array().into_iter().flatten())
        .filter_map(|item| Some((item["tag"].as_str()?, item)))
        .collect::<Vec<_>>();
    // Preserve the existing node-first default when expanding selector choices.
    entries.sort_by_key(|(_, item)| match item["type"].as_str() {
        Some("direct") => 1,
        Some("selector" | "urltest") => 2,
        _ => 0,
    });
    let by_name = entries.iter().copied().collect::<HashMap<_, _>>();
    entries
        .iter()
        .filter(|(name, _)| routable(name, &by_name, &mut HashSet::new()))
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

fn routable<'a>(
    name: &'a str,
    entries: &HashMap<&'a str, &'a Value>,
    visiting: &mut HashSet<&'a str>,
) -> bool {
    if name.starts_with("DNS · ") || !visiting.insert(name) {
        return false;
    }
    let valid = entries
        .get(name)
        .is_some_and(|entry| match entry["type"].as_str() {
            None | Some("block" | "dns") => false,
            Some("selector" | "urltest") => entry["outbounds"].as_array().is_some_and(|members| {
                !members.is_empty()
                    && members.iter().all(|member| {
                        member
                            .as_str()
                            .is_some_and(|name| routable(name, entries, visiting))
                    })
            }),
            Some(_) => true,
        });
    visiting.remove(name);
    valid
}

/// Run after all core destinations exist so local and remote selectors agree.
pub fn populate_dns_selectors(document: &mut Value) {
    let members = dns_outbound_members(document);
    if members.is_empty() {
        return;
    }
    if let Some(outbounds) = document["outbounds"].as_array_mut() {
        for group in outbounds.iter_mut().filter(|group| {
            group["type"] == "selector"
                && group["tag"]
                    .as_str()
                    .is_some_and(|tag| tag.starts_with("DNS · "))
        }) {
            group["outbounds"] = json!(members);
            if !group["default"]
                .as_str()
                .is_some_and(|default| members.iter().any(|name| name == default))
            {
                group["default"] = json!(members[0]);
            }
        }
    }
}
