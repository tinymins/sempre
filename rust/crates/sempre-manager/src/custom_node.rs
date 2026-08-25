use chrono::{SecondsFormat, Utc};
use sempre_converter::{CustomNode, Proxy};
use sempre_subscription::SubscriptionError;
use uuid::Uuid;

use crate::{CoreChange, Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub fn custom_nodes(&self) -> Result<Vec<CustomNode>, ManagerError> {
        let mut nodes = self.subscriptions.read()?.custom_nodes;
        nodes.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(nodes)
    }

    pub fn save_custom_node(&self, mut candidate: CustomNode) -> Result<CustomNode, ManagerError> {
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
        let mut saved = None;
        self.subscriptions.update(|catalog| {
            if let Some(index) = catalog
                .custom_nodes
                .iter()
                .position(|node| node.id == candidate.id)
            {
                candidate
                    .created_at
                    .clone_from(&catalog.custom_nodes[index].created_at);
                catalog.custom_nodes[index] = candidate.clone();
                for profile in &mut catalog.profiles {
                    if profile.custom_node_ids.contains(&candidate.id) {
                        profile.revision += 1;
                    }
                }
                saved = Some(candidate.clone());
                return Ok(());
            }
            if !create {
                return Err(invalid(format!(
                    "custom node {:?} was not found",
                    candidate.id
                )));
            }
            candidate.created_at = Some(now);
            catalog.custom_nodes.push(candidate.clone());
            saved = Some(candidate.clone());
            Ok(())
        })?;
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

        manager
            .subscriptions()
            .update(|catalog| {
                catalog.profiles[0].custom_node_ids.push(saved.id.clone());
                Ok(())
            })
            .expect("reference node");
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
}
