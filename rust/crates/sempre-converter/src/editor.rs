use std::collections::HashMap;

use serde::Deserialize;

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
        let providers: HashMap<String, Vec<EditorRuleProvider>> =
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
        profile.dns = parse("dns_config", &editor.dns_config)?;
    }
    if !editor.private_access_config.trim().is_empty() {
        profile.private_access = parse("private_access_config", &editor.private_access_config)?;
    }
    if !editor.servers.trim().is_empty() {
        profile.manual_servers = parse("servers", &editor.servers)?;
    }
    validate_groups(&profile.groups)?;
    Ok(profile)
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
    fn rejects_unterminated_comment() {
        assert!(clean_jsonc("[/*").is_err());
    }
}
