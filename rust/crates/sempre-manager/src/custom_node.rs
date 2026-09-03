use chrono::{SecondsFormat, Utc};
use sempre_converter::{CustomNode, Proxy};
use sempre_subscription::SubscriptionError;
use uuid::Uuid;

use crate::{
    CoreChange, Manager, ManagerError, VersionRunner,
    pending_changes::{has_pending_profile_revision, record_pending_fields},
    subscription::profile_mode,
};

impl<R: VersionRunner> Manager<R> {
    pub fn custom_nodes(&self) -> Result<Vec<CustomNode>, ManagerError> {
        let mut nodes = self.subscriptions.read()?.custom_nodes;
        nodes.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(nodes)
    }

    pub fn save_custom_node(&self, candidate: CustomNode) -> Result<CustomNode, ManagerError> {
        self.save_custom_node_with_subscriptions(candidate, None)
    }

    pub fn save_custom_node_with_subscriptions(
        &self,
        mut candidate: CustomNode,
        subscription_ids: Option<&[String]>,
    ) -> Result<CustomNode, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let create = candidate.id.trim().is_empty();
        if create {
            candidate.id = Uuid::new_v4().to_string();
        }
        let mut proxy = Proxy::from_value(candidate.proxy)
            .map_err(|error| invalid(format!("invalid custom node proxy: {error}")))?;
        candidate.name = candidate.name.trim().to_owned();
        if candidate.name.is_empty() {
            candidate.name = proxy.name.trim().to_owned();
        }
        if candidate.name.is_empty() {
            return Err(invalid("custom node name is required").into());
        }
        if proxy.port == 0 {
            return Err(invalid("custom node port must be greater than zero").into());
        }
        proxy.name.clone_from(&candidate.name);
        candidate.proxy = proxy.as_value();
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        candidate.updated_at = Some(now.clone());
        let document = self.store.read()?;
        let mut saved = None;
        let mut active_affected = false;
        let mut append = false;
        self.subscriptions.update(|catalog| {
            if let Some(ids) = subscription_ids {
                for id in ids {
                    let profile = catalog
                        .profiles
                        .iter()
                        .find(|profile| &profile.id == id)
                        .ok_or_else(|| {
                            invalid(format!("subscription profile {id:?} was not found"))
                        })?;
                    if profile_mode(profile) == "remote" {
                        return Err(invalid(
                            "remote profiles are read-only; edit the profile on its Sempre server",
                        ));
                    }
                }
            }
            if let Some(index) = catalog
                .custom_nodes
                .iter()
                .position(|node| node.id == candidate.id)
            {
                candidate
                    .created_at
                    .clone_from(&catalog.custom_nodes[index].created_at);
                catalog.custom_nodes[index] = candidate.clone();
            } else if !create {
                return Err(invalid(format!(
                    "custom node {:?} was not found",
                    candidate.id
                )));
            } else {
                candidate.created_at = Some(now);
                catalog.custom_nodes.push(candidate.clone());
            }
            for profile in &mut catalog.profiles {
                if profile_mode(profile) == "remote" {
                    continue;
                }
                let referenced = profile.custom_node_ids.contains(&candidate.id);
                let selected =
                    subscription_ids.map_or(create || referenced, |ids| ids.contains(&profile.id));
                if referenced != selected || (referenced && !create) {
                    if document.active_profile_id.as_deref() == Some(&profile.id) {
                        active_affected = true;
                        append = has_pending_profile_revision(&document, profile);
                    }
                    if selected && !referenced {
                        profile.custom_node_ids.push(candidate.id.clone());
                    } else if !selected {
                        profile.custom_node_ids.retain(|id| id != &candidate.id);
                    }
                    profile.revision += 1;
                }
            }
            saved = Some(candidate.clone());
            Ok(())
        })?;
        if active_affected && document.selected.is_some() {
            self.store.update(|document| {
                record_pending_fields(document, &[sempre_state::PendingConfigField::Nodes], append);
                Ok(())
            })?;
        }
        Ok(saved.expect("custom node mutation stores its result"))
    }

    pub fn remove_custom_node(&self, id: &str) -> Result<CoreChange, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        self.subscriptions.update(|catalog| {
            if let Some(profile) = catalog
                .profiles
                .iter()
                .find(|profile| profile.custom_node_ids.iter().any(|node_id| node_id == id))
            {
                return Err(invalid(format!(
                    "custom node is referenced by subscription profile {:?}",
                    profile.name
                )));
            }
            let before = catalog.custom_nodes.len();
            catalog.custom_nodes.retain(|node| node.id != id);
            if catalog.custom_nodes.len() == before {
                return Err(invalid(format!("custom node {id:?} was not found")));
            }
            Ok(())
        })?;
        Ok(CoreChange {
            changed: true,
            message: "custom node removed".into(),
            ..CoreChange::default()
        })
    }
}

fn invalid(message: impl Into<String>) -> SubscriptionError {
    SubscriptionError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};
    use serde_json::json;

    use super::*;

    fn manager(root: &tempfile::TempDir) -> Manager {
        Manager::new(Store::new(Layout::at(root.path()))).expect("manager")
    }

    fn candidate(name: &str) -> CustomNode {
        CustomNode {
            id: String::new(),
            name: name.into(),
            proxy: json!({
                "name": "proxy-name", "type": "socks5",
                "server": "edge.example.com", "port": 1080
            }),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn custom_node_mutations_preserve_references_and_revision_semantics() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = manager(&root);
        let saved = manager
            .save_custom_node(candidate(""))
            .expect("create node");
        assert_eq!(saved.name, "proxy-name");
        assert_eq!(saved.proxy["name"], "proxy-name");
        assert!(saved.created_at.is_some());

        assert_eq!(
            manager.subscriptions().read().unwrap().profiles[0].custom_node_ids,
            std::slice::from_ref(&saved.id)
        );
        let before = manager.subscriptions().read().expect("catalog").profiles[0].revision;
        let mut update = saved.clone();
        update.name = "renamed".into();
        let updated = manager.save_custom_node(update).expect("update node");
        assert_eq!(updated.proxy["name"], "renamed");
        let catalog = manager.subscriptions().read().expect("updated catalog");
        assert_eq!(catalog.profiles[0].revision, before + 1);
        assert!(manager.remove_custom_node(&saved.id).is_err());

        manager
            .subscriptions()
            .update(|catalog| {
                catalog.profiles[0].custom_node_ids.clear();
                Ok(())
            })
            .expect("remove reference");
        assert!(
            manager
                .remove_custom_node(&saved.id)
                .expect("remove")
                .changed
        );
        assert!(manager.custom_nodes().expect("nodes").is_empty());
    }

    #[test]
    fn custom_nodes_require_a_valid_nonzero_port_proxy() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = manager(&root);
        let mut invalid = candidate("invalid");
        invalid.proxy["port"] = json!(0);
        assert!(manager.save_custom_node(invalid).is_err());
    }

    #[test]
    fn batch_node_save_updates_existing_profile_links_and_pending_state() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = manager(&root);
        let other = manager
            .save_custom_node_with_subscriptions(candidate("other"), Some(&[]))
            .expect("other node");
        let first = manager.subscriptions.read().unwrap().profiles[0].id.clone();
        let second = sempre_subscription::new_profile("second");
        let second_id = second.id.clone();
        manager
            .subscriptions
            .update(|catalog| {
                catalog.profiles[0].custom_node_ids.push(other.id.clone());
                catalog.profiles.push(second);
                Ok(())
            })
            .unwrap();
        manager
            .store
            .update(|document| {
                document.active_profile_id = Some(first.clone());
                document.selected = Some(sempre_state::Selection {
                    core: "sing-box".into(),
                    repository: None,
                    reference: "1.13.18".into(),
                });
                document.core_mut("sing-box").default.installed.insert(
                    "1.13.18".into(),
                    sempre_state::Installation {
                        explicit: true,
                        digest: "a".repeat(64),
                        source: "https://example.com/core.zip".into(),
                        installed_at: Utc::now(),
                    },
                );
                Ok(())
            })
            .unwrap();

        let node = manager
            .save_custom_node_with_subscriptions(
                candidate("shared"),
                Some(&[first.clone(), second_id.clone(), first.clone()]),
            )
            .expect("create and assign once");
        let catalog = manager.subscriptions.read().unwrap();
        assert_eq!(
            catalog.profiles[0].custom_node_ids,
            [other.id.clone(), node.id.clone()]
        );
        assert_eq!(
            catalog.profiles[1].custom_node_ids,
            std::slice::from_ref(&node.id)
        );
        assert_eq!(catalog.profiles[0].revision, 2);
        assert_eq!(catalog.profiles[1].revision, 2);
        assert!(
            manager
                .state()
                .unwrap()
                .pending_config_fields
                .contains(&sempre_state::PendingConfigField::Nodes)
        );
        let stored = serde_json::to_value(&catalog).unwrap();
        assert!(stored["custom_nodes"][1].get("subscription_ids").is_none());

        manager
            .save_custom_node_with_subscriptions(node.clone(), Some(&[second_id]))
            .unwrap();
        let catalog = manager.subscriptions.read().unwrap();
        assert_eq!(catalog.profiles[0].custom_node_ids, [other.id]);
        assert_eq!(
            catalog.profiles[1].custom_node_ids,
            std::slice::from_ref(&node.id)
        );
        assert_eq!(catalog.profiles[0].revision, 3);
        assert_eq!(catalog.profiles[1].revision, 3);

        manager
            .save_custom_node(node.clone())
            .expect("omitted selection preserves links");
        assert_eq!(
            manager.subscriptions.read().unwrap().profiles[1].custom_node_ids,
            std::slice::from_ref(&node.id)
        );
        manager
            .save_custom_node_with_subscriptions(node, Some(&[]))
            .unwrap();
        assert!(
            manager.subscriptions.read().unwrap().profiles[1]
                .custom_node_ids
                .is_empty()
        );
    }

    #[test]
    fn invalid_batch_assignments_leave_node_and_all_profiles_unchanged() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = manager(&root);
        let node = manager.save_custom_node(candidate("original")).unwrap();
        let mut remote = sempre_subscription::new_profile("remote");
        remote.extra.insert("mode".into(), json!("remote"));
        remote.extra.insert(
            "remote".into(),
            json!({ "manifest_url": "https://example.com/manifest" }),
        );
        let remote_id = remote.id.clone();
        manager
            .subscriptions
            .update(|catalog| {
                catalog.profiles.push(remote);
                Ok(())
            })
            .unwrap();
        let catalog_path = &manager.store.layout().subscription_catalog;
        let before = std::fs::read(catalog_path).unwrap();
        let first = manager.subscriptions.read().unwrap().profiles[0].id.clone();
        for rejected in ["missing".to_owned(), remote_id] {
            let selected = [first.clone(), rejected];
            assert!(
                manager
                    .save_custom_node_with_subscriptions(candidate("new"), Some(&selected))
                    .is_err()
            );
            let mut edited = node.clone();
            edited.name = "changed".into();
            assert!(
                manager
                    .save_custom_node_with_subscriptions(edited, Some(&selected))
                    .is_err()
            );
            assert_eq!(std::fs::read(catalog_path).unwrap(), before);
        }
    }
}
