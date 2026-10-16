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
    pub(crate) fn local_transport_profile(
        &self,
        profile: &Profile,
        target: &sempre_converter::Target,
    ) -> Result<Profile, ManagerError> {
        let mut compiled = sempre_converter::profile_from_editor(profile)?;
        let mut bound = false;
        let remote = crate::subscription::profile_mode(profile) == "remote";
        let enabled = compiled
            .private_access
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        if let Some(connectors) = compiled
            .private_access
            .get_mut("connectors")
            .and_then(serde_json::Value::as_array_mut)
        {
            for connector in connectors {
                let Some(id) = connector
                    .get("transport_endpoint_ref")
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                else {
                    if enabled
                        && connector
                            .get("enabled")
                            .and_then(serde_json::Value::as_bool)
                            != Some(false)
                        && connector.get("type").and_then(serde_json::Value::as_str)
                            == Some("wireguard")
                    {
                        validate_direct_peers(connector)?;
                    }
                    continue;
                };
                if remote {
                    return Err(ManagerError::InvalidOperation(
                        "remote profiles cannot use local transport tunnel bindings".into(),
                    ));
                }
                if !enabled
                    || connector
                        .get("enabled")
                        .and_then(serde_json::Value::as_bool)
                        == Some(false)
                {
                    continue;
                }
                if connector.get("type").and_then(serde_json::Value::as_str) != Some("wireguard") {
                    return Err(ManagerError::InvalidOperation(format!(
                        "tunnel forward {id:?} requires a WireGuard connector"
                    )));
                }
                if target.core != "sing-box" || target.version == "11" {
                    return Err(ManagerError::InvalidOperation(
                        "local transport tunnel bindings require sing-box 1.12 or newer".into(),
                    ));
                }
                bound = true;
                let forward = self.tunnels.read()?.forward(&id).ok_or_else(|| {
                    ManagerError::InvalidOperation(format!("tunnel forward {id:?} does not exist"))
                })?;
                let peers = connector
                    .get_mut("endpoint")
                    .and_then(|endpoint| endpoint.get_mut("peers"))
                    .and_then(serde_json::Value::as_array_mut)
                    .filter(|peers| peers.len() == 1)
                    .ok_or_else(|| {
                        ManagerError::InvalidOperation(format!(
                            "tunnel forward {id:?} requires exactly one WireGuard peer"
                        ))
                    })?;
                let peer = peers[0].as_object_mut().ok_or_else(|| {
                    ManagerError::InvalidOperation(format!(
                        "tunnel forward {id:?} requires a WireGuard peer object"
                    ))
                })?;
                peer.insert("address".into(), serde_json::json!(forward.host));
                peer.insert("port".into(), serde_json::json!(forward.port));
            }
        }
        if !bound {
            return Ok(profile.clone());
        }
        let mut result = profile.clone();
        result.private_access = compiled.private_access;
        result.editor.private_access_config = result.private_access.to_string();
        Ok(result)
    }

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

fn validate_direct_peers(connector: &serde_json::Value) -> Result<(), ManagerError> {
    let valid = connector
        .get("endpoint")
        .and_then(|endpoint| endpoint.get("peers"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|peers| {
            !peers.is_empty()
                && peers.iter().all(|peer| {
                    peer.get("address")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|address| !address.trim().is_empty())
                        && peer
                            .get("port")
                            .and_then(serde_json::Value::as_u64)
                            .is_some_and(|port| (1..=65535).contains(&port))
                })
        });
    if !valid {
        return Err(ManagerError::InvalidOperation(
            "direct WireGuard peers require an address and a port between 1 and 65535".into(),
        ));
    }
    Ok(())
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
