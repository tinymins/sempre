use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{CompileError, Profile, ProxyGroup};

#[derive(Debug, Deserialize)]
struct EditorRuleProvider {
    name: String,
    url: String,
    #[serde(rename = "type", default)]
    behavior: String,
    #[serde(default)]
    format: String,
}

pub(super) fn apply(input: &Profile) -> Result<Profile, CompileError> {
    let mut profile = input.clone();
    let editor = &profile.editor;
    if !editor.group.trim().is_empty() {
        profile.groups = parse("group", &editor.group)?;
    }
    if !editor.rule_list.trim().is_empty() {
        let providers: IndexMap<String, Vec<EditorRuleProvider>> =
            parse("rule_list", &editor.rule_list)?;
        profile.rule_providers = providers
            .into_iter()
            .flat_map(|(outbound, items)| {
                items
                    .into_iter()
                    .map(move |item| crate::model::RuleProvider {
                        tag: item.name,
                        url: item.url,
                        outbound: outbound.clone(),
                        format: item.format,
                        behavior: item.behavior,
                        priority: false,
                    })
            })
            .collect();
    }
    if !editor.filter.trim().is_empty() {
        profile.filters = parse("filter", &editor.filter)?;
    }
    if !editor.custom_config.trim().is_empty() {
        profile.rules = parse("custom_config", &editor.custom_config)?;
    }
    if !editor.dns_config.trim().is_empty() {
        profile.dns = parse_dns(&editor.dns_config)?;
    }
    if !editor.private_access_config.trim().is_empty() {
        profile.private_access = parse_private_access(&editor.private_access_config)?;
    }
    if !editor.servers.trim().is_empty() {
        profile.manual_servers = parse("servers", &editor.servers)?;
    }
    validate_groups(&profile.groups)?;
    Ok(profile)
}

fn parse_dns(input: &str) -> Result<Value, CompileError> {
    const SHARED_FIELDS: &[&str] = &[
        "localDnsTransport",
        "localDns",
        "localDnsPort",
        "localServerName",
        "bootstrapDns",
        "bootstrapDnsPort",
        "bootstrapServerName",
        "remoteDns",
        "remoteDnsPort",
        "remoteServerName",
        "remoteDetour",
        "fakeipIpv4Range",
        "fakeipIpv6Range",
        "fakeipEnabled",
        "fakeipTtl",
        "rejectHttps",
        "cnDomainLocalDns",
        "cnIpLocalDns",
        "excludeHkFromCnIp",
        "cnDomainRuleSetEnabled",
        "cnDomainRuleSetUrl",
        "cnDomainRuleSetDetour",
        "cnIpRuleSetEnabled",
        "cnIpRuleSetUrl",
        "cnIpRuleSetDetour",
        "hkIpRuleSetEnabled",
        "hkIpRuleSetUrl",
        "hkIpRuleSetDetour",
        "preferIpv4",
        "systemDnsTakeoverEnabled",
        "systemDnsListenPort",
        "systemDnsListenHosts",
    ];
    let parsed: Value = parse("dns_config", input)?;
    let shared = parsed
        .get("shared")
        .and_then(Value::as_object)
        .ok_or_else(|| CompileError::InvalidEditor {
            field: "dns_config",
            detail: "shared settings object is required".into(),
        })?;
    let filtered = SHARED_FIELDS
        .iter()
        .filter_map(|field| {
            shared
                .get(*field)
                .map(|value| ((*field).into(), value.clone()))
        })
        .collect::<Map<_, _>>();
    let overrides = parsed
        .get("overrides")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter(|(key, value)| {
                    matches!(
                        key.as_str(),
                        "singbox" | "singboxV12" | "clash" | "clashMeta"
                    ) && !value.is_null()
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    Ok(serde_json::json!({ "shared": filtered, "overrides": overrides }))
}

fn parse_private_access(input: &str) -> Result<Value, CompileError> {
    let parsed: Value = parse("private_access_config", input)?;
    let connectors = parsed
        .get("connectors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(sanitize_connector)
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "enabled": parsed.get("enabled").and_then(Value::as_bool) == Some(true),
        "connectors": connectors,
    }))
}

fn sanitize_connector(connector: &Map<String, Value>) -> Option<Value> {
    const TYPES: &[&str] = &[
        "wireguard",
        "tailscale",
        "outbound",
        "v2ray",
        "xray",
        "socks5",
        "vmess",
        "vless",
        "trojan",
        "socks",
        "http",
        "ssh",
        "hysteria2",
        "tuic",
        "anytls",
    ];
    let kind = connector.get("type")?.as_str()?;
    if !TYPES.contains(&kind) {
        return None;
    }
    Some(Value::Object(connector.clone()))
}

fn parse<T: serde::de::DeserializeOwned>(
    field: &'static str,
    input: &str,
) -> Result<T, CompileError> {
    let cleaned =
        clean_jsonc(input).map_err(|detail| CompileError::InvalidEditor { field, detail })?;
    serde_json::from_str(&cleaned).map_err(|error| CompileError::InvalidEditor {
        field,
        detail: error.to_string(),
    })
}

pub fn parse_jsonc_value(input: &str) -> Result<Value, CompileError> {
    parse("jsonc", input)
}

fn validate_groups(groups: &[ProxyGroup]) -> Result<(), CompileError> {
    for group in groups {
        if group.name.trim().is_empty() {
            return Err(CompileError::InvalidEditor {
                field: "group",
                detail: "group name is required".into(),
            });
        }
    }
    Ok(())
}

fn clean_jsonc(input: &str) -> Result<String, String> {
    let mut characters = input.chars().peekable();
    let mut output = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    while let Some(current) = characters.next() {
        if in_string {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            continue;
        }
        if current == '"' {
            in_string = true;
            output.push('"');
            continue;
        }
        if current == '/' && characters.peek() == Some(&'/') {
            characters.next();
            for character in characters.by_ref() {
                if character == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }
        if current == '/' && characters.peek() == Some(&'*') {
            characters.next();
            let mut closed = false;
            while let Some(character) = characters.next() {
                if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("unterminated block comment".into());
            }
            continue;
        }
        output.push(current);
    }
    if in_string {
        return Err("unterminated string".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{apply, clean_jsonc};
    use crate::Profile;

    #[test]
    fn applies_jsonc_editor_fields() {
        let mut profile = Profile::default();
        profile.editor.group = "/* group */ [{\"name\":\"proxy\",\"type\":\"select\"}]".into();
        profile.editor.servers = "[// local\n{\"name\":\"edge\",\"type\":\"socks5\",\"server\":\"edge.example.com\",\"port\":1080}]".into();
        let applied = apply(&profile).expect("editor applies");
        assert_eq!(applied.groups[0].name, "proxy");
        assert_eq!(applied.manual_servers.len(), 1);
    }

    #[test]
    fn preserves_unicode_in_jsonc_editor_fields() {
        let mut profile = Profile::default();
        profile.editor.dns_config =
            r#"{/* route */"shared":{"remoteDetour":"🔰 国外流量"}}"#.into();
        let applied = apply(&profile).expect("editor applies");
        assert_eq!(applied.dns["shared"]["remoteDetour"], "🔰 国外流量");
    }

    #[test]
    fn keeps_only_dns_fields_with_ui_controls() {
        let mut profile = Profile::default();
        profile.editor.dns_config = r#"{
            "shared": {
                "remoteDns": "1.1.1.1",
                "managedDnsFrontend": true,
                "unknown": "hidden"
            }
        }"#
        .into();
        let applied = apply(&profile).expect("editor applies");
        assert_eq!(
            applied.dns,
            serde_json::json!({
                "shared": { "remoteDns": "1.1.1.1" }, "overrides": {}
            })
        );
    }

    #[test]
    fn preserves_private_access_fields_used_by_renderer() {
        let mut profile = Profile::default();
        profile.editor.private_access_config = serde_json::json!({
            "enabled": true,
            "connectors": [{
                "type": "wireguard",
                "tag": "home",
                "endpoint": {
                    "address": ["10.0.0.2/32"],
                    "privateKey": "private",
                    "hidden": true,
                    "peers": [{ "address": "vpn.test", "port": 51820 }, { "address": "ghost" }]
                },
                "routes": { "ipCidrs": ["10.0.0.0/24"], "domainKeywords": ["hidden"] },
                "dns": [{ "server": "10.0.0.1" }, { "server": "10.0.0.2" }]
            }, {
                "type": "tailscale",
                "endpoint": {}
            }]
        })
        .to_string();
        let applied = apply(&profile).expect("editor applies");
        let connectors = applied.private_access["connectors"]
            .as_array()
            .expect("connectors");
        assert_eq!(connectors.len(), 2);
        assert_eq!(
            connectors[0]["endpoint"]["peers"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(connectors[0]["endpoint"]["hidden"], true);
        assert_eq!(
            connectors[0]["routes"]["domainKeywords"],
            serde_json::json!(["hidden"])
        );
        assert_eq!(connectors[0]["dns"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn rejects_unterminated_comment() {
        assert!(clean_jsonc("[/*").is_err());
    }
}
