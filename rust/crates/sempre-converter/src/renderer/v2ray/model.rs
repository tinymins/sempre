use std::collections::HashSet;

use crate::{CompileError, Profile, Proxy};

pub(super) struct RuntimeGroup {
    pub(super) name: String,
    pub(super) group_type: String,
    pub(super) members: Vec<String>,
    pub(super) default: String,
    pub(super) interval: u64,
}

pub(super) struct RuntimeModel<'a> {
    pub(super) profile: &'a Profile,
    pub(super) groups: Vec<RuntimeGroup>,
    pub(super) final_outbound: String,
}

impl<'a> RuntimeModel<'a> {
    pub(super) fn new(
        profile: &'a Profile,
        proxies: &[Proxy],
        represented: &HashSet<&str>,
        core: &str,
    ) -> Result<Self, CompileError> {
        let ordered_names = proxies
            .iter()
            .map(|proxy| proxy.name.as_str())
            .collect::<Vec<_>>();
        let mut groups = Vec::new();

        if profile.groups.is_empty() {
            let members = ordered_names
                .iter()
                .filter(|name| represented.contains(**name))
                .map(|name| (*name).into())
                .collect::<Vec<String>>();
            groups.push(RuntimeGroup {
                name: "proxy".into(),
                group_type: "select".into(),
                default: members[0].clone(),
                members,
                interval: 0,
            });
        } else {
            for configured in &profile.groups {
                if configured.name.trim().is_empty() {
                    return Err(CompileError::Render("proxy group name is required".into()));
                }
                let mut members = configured.proxies.clone();
                if !configured.readonly || configured.include_all || members.is_empty() {
                    append_unique(&mut members, &ordered_names);
                }
                if members.is_empty() {
                    return Err(group_error(&configured.name, "has no members"));
                }
                let configured_default = if configured.default.is_empty() {
                    members[0].clone()
                } else {
                    configured.default.clone()
                };
                if !members.iter().any(|member| member == &configured_default) {
                    return Err(group_error(
                        &configured.name,
                        &format!(
                            "default {:?} is not an available member",
                            configured.default
                        ),
                    ));
                }
                members.retain(|member| represented.contains(member.as_str()));
                if members.is_empty() {
                    return Err(group_error(
                        &configured.name,
                        &format!("has no members supported by {core}"),
                    ));
                }
                let default = if represented.contains(configured_default.as_str()) {
                    configured_default
                } else {
                    members[0].clone()
                };
                groups.push(RuntimeGroup {
                    name: configured.name.clone(),
                    group_type: if configured.group_type.is_empty() {
                        "select".into()
                    } else {
                        configured.group_type.clone()
                    },
                    members,
                    default,
                    interval: configured.interval,
                });
            }
        }

        let final_outbound = groups
            .iter()
            .find(|group| group.name == "⚓️ 其他流量")
            .unwrap_or(&groups[0])
            .name
            .clone();
        Ok(Self {
            profile,
            groups,
            final_outbound,
        })
    }
}

fn append_unique(target: &mut Vec<String>, values: &[&str]) {
    for value in values {
        if !target.iter().any(|existing| existing == value) {
            target.push((*value).into());
        }
    }
}

fn group_error(name: &str, detail: &str) -> CompileError {
    CompileError::Render(format!("proxy group {name:?} {detail}"))
}
