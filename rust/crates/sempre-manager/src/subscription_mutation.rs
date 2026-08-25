use sempre_converter::Source;
use sempre_subscription::SubscriptionError;
use serde_json::{Map, json};
use url::Url;
use uuid::Uuid;

use crate::{CoreChange, Manager, ManagerError, VersionRunner, subscription::profile_mode};

impl<R: VersionRunner> Manager<R> {
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
    use sempre_state::{Layout, Store};

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
}
