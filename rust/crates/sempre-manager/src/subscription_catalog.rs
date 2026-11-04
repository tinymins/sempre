//! Creating and deleting profiles, shared by the CLI and the Web API so their rules
//! cannot drift apart.
use sempre_converter::Profile;
use sempre_subscription::{SubscriptionError, new_profile};
use serde_json::json;
use url::Url;

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    /// A local profile starts with every custom node; a remote profile's nodes are
    /// decided by its publisher.
    pub async fn create_subscription_profile(
        &self,
        name: &str,
        manifest_url: Option<&str>,
    ) -> Result<Profile, ManagerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SubscriptionError::Invalid("profile name is required".into()).into());
        }
        let mut profile = new_profile(name);
        if let Some(manifest_url) = manifest_url {
            if !valid_manifest_url(manifest_url) {
                return Err(SubscriptionError::Invalid(
                    "remote manifest URL must be absolute HTTP(S) without credentials".into(),
                )
                .into());
            }
            profile.extra.insert("mode".into(), json!("remote"));
            profile.extra.insert(
                "remote".into(),
                json!({ "manifest_url": manifest_url.trim() }),
            );
        }
        let _operation = self.operation_guard().await?;
        self.subscriptions.update(|catalog| {
            if manifest_url.is_none() {
                profile.custom_node_ids = catalog
                    .custom_nodes
                    .iter()
                    .map(|node| node.id.clone())
                    .collect();
            }
            catalog.profiles.push(profile.clone());
            Ok(())
        })?;
        Ok(profile)
    }

    /// The operation lock keeps activation from selecting the profile mid-delete.
    pub async fn delete_subscription_profile(&self, id: &str) -> Result<(), ManagerError> {
        let _operation = self.operation_guard().await?;
        if self.store.read()?.active_profile_id.as_deref() == Some(id) {
            return Err(ManagerError::InvalidOperation(
                "the active subscription profile cannot be deleted".into(),
            ));
        }
        self.subscriptions.update(|catalog| {
            if catalog.profiles.len() == 1 {
                return Err(SubscriptionError::Invalid(
                    "at least one subscription profile is required".into(),
                ));
            }
            let before = catalog.profiles.len();
            catalog.profiles.retain(|profile| profile.id != id);
            if catalog.profiles.len() == before {
                return Err(SubscriptionError::Invalid("profile was not found".into()));
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn valid_manifest_url(value: &str) -> bool {
    Url::parse(value.trim()).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests {
    use sempre_converter::CustomNode;
    use sempre_state::{Layout, Store};

    use crate::{Manager, ManagerError};

    fn manager() -> (tempfile::TempDir, Manager) {
        let root = tempfile::tempdir().expect("root");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        (root, manager)
    }

    #[tokio::test]
    async fn local_profiles_inherit_custom_nodes_but_remote_ones_do_not() {
        let (_root, manager) = manager();
        manager
            .subscriptions()
            .update(|catalog| {
                catalog.custom_nodes.push(
                    serde_json::from_value::<CustomNode>(
                        serde_json::json!({ "id": "node", "name": "node", "proxy": {} }),
                    )
                    .expect("custom node"),
                );
                Ok(())
            })
            .expect("custom node");
        let local = manager
            .create_subscription_profile("local", None)
            .await
            .expect("local");
        assert_eq!(local.custom_node_ids, ["node"]);
        let remote = manager
            .create_subscription_profile("remote", Some("https://example.com/m"))
            .await
            .expect("remote");
        assert!(remote.custom_node_ids.is_empty());
    }

    #[tokio::test]
    async fn manifest_urls_with_credentials_are_refused() {
        let (_root, manager) = manager();
        for url in [
            "ftp://example.com/m",
            "https://user:secret@example.com/m",
            "nope",
        ] {
            assert!(
                manager
                    .create_subscription_profile("remote", Some(url))
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn the_active_profile_cannot_be_deleted() {
        let (_root, manager) = manager();
        let active = manager
            .state()
            .expect("state")
            .active_profile_id
            .expect("active");
        let spare = manager
            .create_subscription_profile("spare", None)
            .await
            .expect("spare");
        assert!(matches!(
            manager.delete_subscription_profile(&active).await,
            Err(ManagerError::InvalidOperation(_))
        ));
        manager
            .delete_subscription_profile(&spare.id)
            .await
            .expect("delete");
        assert!(
            manager
                .delete_subscription_profile("missing")
                .await
                .is_err()
        );
    }
}
