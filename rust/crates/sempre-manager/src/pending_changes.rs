use std::collections::BTreeSet;

use sempre_converter::{Profile, Source};
use sempre_core::CoreRef;
use sempre_state::{Deployment, Document, PendingConfigField};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimePendingChange {
    Core {
        previous: Option<String>,
        current: String,
    },
    Profile {
        previous: Option<String>,
        current: String,
    },
    Configuration {
        fields: Vec<PendingConfigField>,
        previous_revision: Option<u64>,
        current_revision: Option<u64>,
    },
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn runtime_pending_changes(
        &self,
        document: &Document,
        configuration_pending: bool,
    ) -> Vec<RuntimePendingChange> {
        if !document.pending && !configuration_pending {
            return Vec::new();
        }

        let mut changes = Vec::new();
        if document.pending
            && deployment_identity(document.previous.as_ref())
                != deployment_identity(document.active.as_ref())
            && let Some(current) = document.active.as_ref()
        {
            changes.push(RuntimePendingChange::Core {
                previous: document.previous.as_ref().map(deployment_label),
                current: deployment_label(current),
            });
        }

        let Ok(catalog) = self.subscriptions.read() else {
            return changes;
        };
        if document.pending && document.previous_profile_id != document.active_profile_id {
            changes.push(RuntimePendingChange::Profile {
                previous: document
                    .previous_profile_id
                    .as_deref()
                    .map(|id| profile_label(&catalog.profiles, id)),
                current: document.active_profile_id.as_deref().map_or_else(
                    || "unknown".into(),
                    |id| profile_label(&catalog.profiles, id),
                ),
            });
        }

        if !document.pending_config_fields.is_empty() {
            let profile = document
                .active_profile_id
                .as_deref()
                .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id));
            let previous_revision = document
                .selected
                .as_ref()
                .and_then(|selected| document.config_builds.get(&selected.core))
                .map(|build| build.profile_revision);
            changes.push(RuntimePendingChange::Configuration {
                fields: document.pending_config_fields.clone(),
                previous_revision,
                current_revision: profile.map(|profile| profile.revision),
            });
        }

        changes
    }
}

pub(crate) fn profile_changed_fields(
    current: &Profile,
    candidate: &Profile,
) -> Vec<PendingConfigField> {
    let mut fields = source_and_node_fields(current, candidate);
    fields.extend(routing_fields(current, candidate));
    fields.extend(runtime_fields(current, candidate));
    fields
}

pub(crate) struct PendingProfileChange {
    fields: Vec<PendingConfigField>,
    append: bool,
}

impl PendingProfileChange {
    pub(crate) fn record(self, document: &mut Document, changed: bool) {
        if changed && !self.fields.is_empty() {
            record_pending_fields(document, &self.fields, self.append);
        }
    }
}

pub(crate) fn profile_change(
    document: &Document,
    current: &Profile,
    candidate: &Profile,
) -> PendingProfileChange {
    PendingProfileChange {
        fields: profile_changed_fields(current, candidate),
        append: has_pending_profile_revision(document, current),
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn record_pending_profile_fields(
        &self,
        document: &Document,
        profile: &Profile,
        fields: &[PendingConfigField],
    ) -> Result<(), ManagerError> {
        let append = has_pending_profile_revision(document, profile);
        self.store.update(|document| {
            record_pending_fields(document, fields, append);
            Ok(())
        })?;
        Ok(())
    }
}

fn source_and_node_fields(current: &Profile, candidate: &Profile) -> Vec<PendingConfigField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        PendingConfigField::Sources,
        &source_settings(&current.sources),
        &source_settings(&candidate.sources),
    );
    push_changed(
        &mut fields,
        PendingConfigField::SubscriptionContent,
        &source_snapshots(&current.sources),
        &source_snapshots(&candidate.sources),
    );
    push_changed(
        &mut fields,
        PendingConfigField::Nodes,
        &json!([
            current.custom_node_ids,
            current.manual_servers,
            current.editor.servers
        ]),
        &json!([
            candidate.custom_node_ids,
            candidate.manual_servers,
            candidate.editor.servers
        ]),
    );
    fields
}

fn routing_fields(current: &Profile, candidate: &Profile) -> Vec<PendingConfigField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        PendingConfigField::Groups,
        &json!([
            current.groups,
            current.editor.group,
            current.extra.get("use_system_groups")
        ]),
        &json!([
            candidate.groups,
            candidate.editor.group,
            candidate.extra.get("use_system_groups")
        ]),
    );
    push_changed(
        &mut fields,
        PendingConfigField::Rules,
        &json!([
            current.rules,
            current.editor.rule_list,
            current.extra.get("use_system_rules")
        ]),
        &json!([
            candidate.rules,
            candidate.editor.rule_list,
            candidate.extra.get("use_system_rules")
        ]),
    );
    push_changed(
        &mut fields,
        PendingConfigField::RuleProviders,
        &json!(current.rule_providers),
        &json!(candidate.rule_providers),
    );
    push_changed(
        &mut fields,
        PendingConfigField::Filters,
        &json!([
            current.filters,
            current.editor.filter,
            current.extra.get("use_system_filters")
        ]),
        &json!([
            candidate.filters,
            candidate.editor.filter,
            candidate.extra.get("use_system_filters")
        ]),
    );
    fields
}

fn runtime_fields(current: &Profile, candidate: &Profile) -> Vec<PendingConfigField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        PendingConfigField::PrivateAccess,
        &json!([current.private_access, current.editor.private_access_config]),
        &json!([
            candidate.private_access,
            candidate.editor.private_access_config
        ]),
    );
    push_changed(
        &mut fields,
        PendingConfigField::LocalProxy,
        &json!(current.local_proxy),
        &json!(candidate.local_proxy),
    );
    push_changed(
        &mut fields,
        PendingConfigField::TransparentProxy,
        &json!(current.transparent_proxy),
        &json!(candidate.transparent_proxy),
    );
    push_changed(
        &mut fields,
        PendingConfigField::ManagementApi,
        &json!(current.management_api),
        &json!(candidate.management_api),
    );
    push_changed(
        &mut fields,
        PendingConfigField::Advanced,
        &json!([
            current.log_level,
            current.core_overrides,
            current.editor.custom_config,
            current.extra.get("use_system_custom_config"),
        ]),
        &json!([
            candidate.log_level,
            candidate.core_overrides,
            candidate.editor.custom_config,
            candidate.extra.get("use_system_custom_config"),
        ]),
    );
    fields
}

pub(crate) fn record_pending_fields(
    document: &mut Document,
    fields: &[PendingConfigField],
    append: bool,
) {
    let mut values = if append {
        document
            .pending_config_fields
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    values.extend(fields.iter().copied());
    document.pending_config_fields = values.into_iter().collect();
}

pub(crate) fn has_pending_profile_revision(document: &Document, profile: &Profile) -> bool {
    document
        .selected
        .as_ref()
        .and_then(|selected| document.config_builds.get(&selected.core))
        .is_some_and(|build| {
            build.profile_id == profile.id && build.profile_revision != profile.revision
        })
}

fn push_changed(
    fields: &mut Vec<PendingConfigField>,
    key: PendingConfigField,
    current: &Value,
    candidate: &Value,
) {
    if current != candidate {
        fields.push(key);
    }
}

fn source_settings(sources: &[Source]) -> Value {
    Value::Array(
        sources
            .iter()
            .map(|source| {
                json!({
                    "id": source.id,
                    "type": source.kind,
                    "enabled": source.enabled,
                    "url": source.url,
                    "remark": source.remark,
                    "prefix": source.prefix,
                    "content": source.content,
                    "user_agent": source.user_agent,
                    "fetch_mode": source.extra.get("fetch_mode"),
                })
            })
            .collect(),
    )
}

fn source_snapshots(sources: &[Source]) -> Value {
    Value::Array(
        sources
            .iter()
            .map(|source| {
                source
                    .extra
                    .get("snapshot_hash")
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .collect(),
    )
}

fn deployment_identity(deployment: Option<&Deployment>) -> Option<(&str, Option<&str>, &str)> {
    deployment.map(|value| {
        (
            value.core.as_str(),
            value.repository.as_deref(),
            value.version.as_str(),
        )
    })
}

fn deployment_label(deployment: &Deployment) -> String {
    CoreRef {
        core: deployment.core.clone(),
        repository: deployment.repository.clone(),
        reference: deployment.version.clone(),
    }
    .to_string()
}

fn profile_label(profiles: &[Profile], id: &str) -> String {
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .map_or_else(
            || id.to_owned(),
            |profile| {
                if profile.name.trim().is_empty() {
                    profile.id.clone()
                } else {
                    profile.name.clone()
                }
            },
        )
}
