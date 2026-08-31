use sempre_converter::{Profile, Source};
use sempre_subscription::SubscriptionError;
use serde_json::{Map, json};
use url::Url;
use uuid::Uuid;

use crate::{
    CoreChange, Manager, ManagerError, VersionRunner,
    pending_changes::{
        has_pending_profile_revision, profile_changed_fields, record_pending_fields,
    },
    subscription::profile_mode,
};

impl<R: VersionRunner> Manager<R> {
    pub fn save_subscription_profile(
        &self,
        id: &str,
        candidate: Profile,
        expected_context: Option<&str>,
    ) -> Result<(CoreChange, Profile), ManagerError> {
        let _operation = self.store.acquire_operation()?;
        if let Some(expected) = expected_context.filter(|value| !value.is_empty())
            && self.configuration_context()?.key != expected
        {
            return Err(ManagerError::ConfigurationContextChanged);
        }
        let document = self.store.read()?;
        let mut saved = None;
        let mut fields = Vec::new();
        let mut append = false;
        self.subscriptions.update(|catalog| {
            let current = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == id)
                .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
            if profile_mode(current) == "remote" {
                return Err(SubscriptionError::Invalid(
                    "remote profiles are read-only; edit the profile on its Sempre server".into(),
                ));
            }
            let mut candidate = candidate;
            if profile_mode(&candidate) != profile_mode(current)
                || candidate.extra.contains_key("remote")
            {
                return Err(SubscriptionError::Invalid(
                    "subscription profile mode cannot be changed through profile editing".into(),
                ));
            }
            preserve_source_metadata(&current.sources, &mut candidate.sources);
            preserve_compilation_metadata(current, &mut candidate);
            fields = profile_changed_fields(current, &candidate);
            append = has_pending_profile_revision(&document, current);
            candidate.id.clone_from(&current.id);
            candidate.name.clone_from(&current.name);
            candidate.revision = current.revision + 1;
            candidate.extra.insert(
                "last_result".into(),
                json!("profile saved; runtime configuration needs regeneration"),
            );
            candidate
                .extra
                .insert("last_runtime_validated".into(), json!(false));
            current.clone_from(&candidate);
            saved = Some(candidate);
            Ok(())
        })?;
        let needs_restart =
            document.selected.is_some() && document.active_profile_id.as_deref() == Some(id);
        if needs_restart {
            self.store.update(|document| {
                record_pending_fields(document, &fields, append);
                Ok(())
            })?;
        }
        Ok((
            CoreChange {
                changed: true,
                needs_restart,
                message:
                    "subscription profile saved locally; runtime configuration needs regeneration"
                        .into(),
                ..CoreChange::default()
            },
            saved.expect("subscription update stores the saved profile"),
        ))
    }

    pub fn set_subscription_source(&self, value: &str) -> Result<CoreChange, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let value = value.trim();
        let document = self.store.read()?;
        let catalog = self.subscriptions.read()?;
        let profile = document
            .active_profile_id
            .as_deref()
            .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id))
            .or_else(|| catalog.profiles.first())
            .ok_or_else(|| SubscriptionError::Invalid("no subscription profile exists".into()))?;
        if profile_mode(profile) == "remote" {
            return Err(SubscriptionError::Invalid(
                "remote profiles are read-only; edit the profile on its Sempre server".into(),
            )
            .into());
        }
        let profile_id = profile.id.clone();
        let append = has_pending_profile_revision(&document, profile);
        let sources = if value.is_empty() {
            Vec::new()
        } else {
            vec![url_source(value)?]
        };
        let unchanged = profile.sources.len() == sources.len()
            && profile
                .sources
                .iter()
                .zip(&sources)
                .all(|(current, next)| current.kind == next.kind && current.url == next.url);
        if unchanged {
            return Ok(CoreChange {
                message: if value.is_empty() {
                    "subscription sources are already clear".into()
                } else {
                    "subscription source is already current".into()
                },
                ..CoreChange::default()
            });
        }
        self.subscriptions.update(|catalog| {
            let profile = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
            profile.sources = sources;
            profile.revision += 1;
            profile.extra.insert(
                "last_result".into(),
                json!("profile saved; runtime configuration needs regeneration"),
            );
            profile
                .extra
                .insert("last_runtime_validated".into(), json!(false));
            Ok(())
        })?;
        self.store.update(|document| {
            if document.active_profile_id.is_none() {
                document.active_profile_id = Some(profile_id);
            }
            if value.is_empty() {
                document.subscription.url = None;
                document.subscription.last_check = None;
                document.subscription.last_change = None;
                document.subscription.last_result = None;
            } else {
                document.subscription.url = Some(value.into());
                record_pending_fields(document, &["sources".into()], append);
            }
            Ok(())
        })?;
        Ok(CoreChange {
            changed: true,
            needs_restart: !value.is_empty() && document.selected.is_some(),
            message: if value.is_empty() {
                "subscription sources cleared; the active configuration was retained".into()
            } else {
                "subscription profile saved locally; runtime configuration needs regeneration"
                    .into()
            },
            ..CoreChange::default()
        })
    }
}

fn preserve_source_metadata(previous: &[Source], candidate: &mut [Source]) {
    for source in candidate {
        let Some(before) = previous
            .iter()
            .find(|before| before.id == source.id && same_fetch_identity(before, source))
        else {
            continue;
        };
        for key in ["snapshot_hash", "fetched_at", "last_status", "last_error"] {
            if let Some(value) = before.extra.get(key) {
                source.extra.insert(key.into(), value.clone());
            }
        }
    }
}

fn same_fetch_identity(left: &Source, right: &Source) -> bool {
    left.kind == right.kind
        && left.url == right.url
        && defaulted(&left.user_agent, "clash.meta") == defaulted(&right.user_agent, "clash.meta")
        && extra_string(left, "fetch_mode", "auto") == extra_string(right, "fetch_mode", "auto")
}

fn defaulted<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn extra_string<'a>(source: &'a Source, key: &str, fallback: &'a str) -> &'a str {
    source
        .extra
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

fn preserve_compilation_metadata(current: &Profile, candidate: &mut Profile) {
    for key in [
        "last_check",
        "last_change",
        "last_config_hash",
        "last_compiler_target",
        "last_compiler_warnings",
    ] {
        if let Some(value) = current.extra.get(key) {
            candidate.extra.insert(key.into(), value.clone());
        }
    }
}

fn url_source(value: &str) -> Result<Source, ManagerError> {
    let url = Url::parse(value)
        .map_err(|_| SubscriptionError::Invalid("subscription URL is invalid".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(SubscriptionError::Invalid(
            "subscription URL must be absolute HTTP(S) without credentials".into(),
        )
        .into());
    }
    let mut extra = Map::new();
    extra.insert("fetch_mode".into(), json!("auto"));
    Ok(Source {
        id: Uuid::new_v4().to_string(),
        kind: "url".into(),
        enabled: true,
        url: value.into(),
        remark: "Subscription".into(),
        prefix: String::new(),
        content: String::new(),
        user_agent: "clash.meta".into(),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use sempre_converter::Source;
    use sempre_state::{Layout, Store};
    use serde_json::{Map, json};

    use super::*;

    #[test]
    fn set_and_clear_are_local_persistence_operations() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let saved = manager
            .set_subscription_source("https://example.com/subscription")
            .expect("save source without a selected core");
        assert!(saved.changed && !saved.needs_restart);
        let state = manager.state().expect("state");
        assert!(state.active_profile_id.is_some());
        assert_eq!(
            state.subscription.url.as_deref(),
            Some("https://example.com/subscription")
        );
        let cleared = manager.set_subscription_source("").expect("clear source");
        assert!(cleared.changed);
        assert!(manager.state().expect("state").subscription.url.is_none());
        assert!(
            manager.subscriptions().read().expect("catalog").profiles[0]
                .sources
                .is_empty()
        );
    }

    #[test]
    fn set_rejects_credentialed_and_non_http_urls() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        assert!(
            manager
                .set_subscription_source("https://user:secret@example.com/sub")
                .is_err()
        );
        assert!(manager.set_subscription_source("file:///tmp/sub").is_err());
    }

    #[test]
    fn profile_save_is_local_and_preserves_server_owned_fields() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let catalog = manager.subscriptions().read().expect("catalog");
        let current = catalog.profiles[0].clone();
        manager
            .subscriptions()
            .update(|catalog| {
                catalog.profiles[0].extra.insert(
                    "last_config_hash".into(),
                    json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                );
                Ok(())
            })
            .expect("compilation metadata");
        let mut candidate = current.clone();
        candidate.id = "client-controlled".into();
        candidate.name = "client-controlled".into();
        candidate.revision = 99;
        candidate.transparent_proxy.tun.interface_name.clear();
        candidate.sources = vec![Source {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "raw".into(),
            enabled: true,
            url: String::new(),
            remark: "offline".into(),
            prefix: String::new(),
            content: "not a supported node".into(),
            user_agent: String::new(),
            extra: Map::new(),
        }];

        let (change, saved) = manager
            .save_subscription_profile(&current.id, candidate, Some("common"))
            .expect("offline save");

        assert!(change.changed && !change.needs_restart);
        assert_eq!(saved.id, current.id);
        assert_eq!(saved.name, current.name);
        assert_eq!(saved.revision, current.revision + 1);
        assert!(saved.transparent_proxy.tun.interface_name.is_empty());
        assert_eq!(saved.sources[0].content, "not a supported node");
        assert_eq!(saved.extra["last_runtime_validated"], json!(false));
        assert_eq!(
            saved.extra["last_config_hash"],
            json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn profile_save_rejects_remote_and_stale_context_edits() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let current = manager.subscriptions().read().expect("catalog").profiles[0].clone();
        let profile_id = current.id.clone();
        assert!(matches!(
            manager.save_subscription_profile(&current.id, current.clone(), Some("stale")),
            Err(ManagerError::ConfigurationContextChanged)
        ));
        manager
            .subscriptions()
            .update(|catalog| {
                catalog.profiles[0]
                    .extra
                    .insert("mode".into(), json!("remote"));
                catalog.profiles[0].extra.insert(
                    "remote".into(),
                    json!({ "manifest_url": "https://example.com" }),
                );
                Ok(())
            })
            .expect("remote profile");
        assert!(
            manager
                .save_subscription_profile(&profile_id, current, None)
                .is_err()
        );
    }
}
