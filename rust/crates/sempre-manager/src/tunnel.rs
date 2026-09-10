use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use sempre_converter::Profile;
use sempre_tunnel::{BinaryStatus, Config, Status};
use tokio::sync::watch;

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub fn tunnel_status(&self) -> Result<Status, ManagerError> {
        Ok(self.tunnels.status()?)
    }

    pub async fn install_tunnel_tool(&self) -> Result<BinaryStatus, ManagerError> {
        Ok(self.tunnels.install().await?)
    }

    pub fn tunnel_log(&self, id: &str) -> Result<String, ManagerError> {
        Ok(self.tunnels.log(id)?)
    }

    pub async fn run_tunnels(&self, shutdown: watch::Receiver<bool>) -> Result<(), ManagerError> {
        Ok(Arc::clone(&self.tunnels).run(shutdown).await?)
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn update_tunnels(&self, config: Config) -> Result<(Status, bool), ManagerError> {
        let catalog = self.subscriptions.read()?;
        let references = referenced_forwards(&catalog.profiles)?;
        for (id, profiles) in &references {
            if config.forward(id).is_none() {
                return Err(ManagerError::InvalidOperation(format!(
                    "tunnel forward {id:?} is referenced by subscription profile {:?}",
                    profiles[0]
                )));
            }
        }
        let saved = Arc::clone(&self.tunnels).update(config).await?;
        if references.is_empty() {
            return Ok((self.tunnels.status()?, false));
        }
        let referenced_profile_ids = catalog
            .profiles
            .iter()
            .map(|profile| Ok((profile.id.clone(), profile_forward_refs(profile)?)))
            .collect::<Result<Vec<_>, sempre_converter::CompileError>>()?
            .into_iter()
            .filter_map(|(id, references)| (!references.is_empty()).then_some(id))
            .collect::<BTreeSet<_>>();
        let catalog = self.subscriptions.update(|stored| {
            for profile in &mut stored.profiles {
                if referenced_profile_ids.contains(&profile.id) {
                    profile.revision += 1;
                }
            }
            Ok(())
        })?;
        let document = self.store.read()?;
        let restart = if let Some(active_id) = document.active_profile_id.as_deref()
            && document.selected.is_some()
            && catalog
                .profiles
                .iter()
                .find(|profile| profile.id == active_id)
                .map(profile_forward_refs)
                .transpose()?
                .is_some_and(|references| !references.is_empty())
        {
            let (change, _) = self.recompile_subscription_profile(active_id).await?;
            if change.needs_restart {
                self.request_runtime_reload();
            }
            change.needs_restart
        } else {
            false
        };
        let mut status = self.tunnels.status()?;
        status.config = saved;
        Ok((status, restart))
    }

    pub async fn tunnel_action(&self, id: &str, action: &str) -> Result<Status, ManagerError> {
        Ok(Arc::clone(&self.tunnels).action(id, action).await?)
    }
}

fn referenced_forwards(
    profiles: &[Profile],
) -> Result<BTreeMap<String, Vec<String>>, sempre_converter::CompileError> {
    let mut references: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for profile in profiles {
        for id in profile_forward_refs(profile)? {
            references
                .entry(id)
                .or_default()
                .push(if profile.name.is_empty() {
                    profile.id.clone()
                } else {
                    profile.name.clone()
                });
        }
    }
    Ok(references)
}

fn profile_forward_refs(profile: &Profile) -> Result<Vec<String>, sempre_converter::CompileError> {
    Ok(sempre_converter::profile_from_editor(profile)?
        .private_access
        .get("connectors")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|connector| {
            connector
                .get("transport_endpoint_ref")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};
    use serde_json::json;

    use super::*;

    #[test]
    fn finds_editor_transport_references() {
        let mut profile = Profile::default();
        profile.editor.private_access_config = json!({ "connectors": [
            { "type": "wireguard", "transport_endpoint_ref": "primary" }
        ]})
        .to_string();
        assert_eq!(
            profile_forward_refs(&profile).expect("editor config"),
            ["primary"]
        );
    }

    #[tokio::test]
    async fn referenced_forwards_cannot_be_removed() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        manager
            .subscriptions
            .update(|catalog| {
                catalog.profiles[0].name = "Home".into();
                catalog.profiles[0].editor.private_access_config = json!({
                    "connectors": [{
                        "type": "wireguard",
                        "transport_endpoint_ref": "home-wg"
                    }]
                })
                .to_string();
                Ok(())
            })
            .expect("seed reference");
        let error = manager
            .update_tunnels(Config::default())
            .await
            .expect_err("referenced forward removal must fail");
        assert!(error.to_string().contains("home-wg"));
        assert!(error.to_string().contains("Home"));
    }
}
