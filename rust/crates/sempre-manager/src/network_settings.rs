use std::{fs, path::PathBuf, sync::Mutex};

use sempre_converter::{Profile, Target};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ManagerError;

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkMode {
    #[default]
    Local,
    Gateway,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkSettings {
    pub schema: u32,
    pub revision: u64,
    pub mode: NetworkMode,
    pub gateway_capture_host: bool,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            revision: 1,
            mode: NetworkMode::Local,
            gateway_capture_host: false,
        }
    }
}

pub(crate) struct NetworkSettingsStore {
    path: PathBuf,
    settings: Mutex<NetworkSettings>,
}

impl NetworkSettingsStore {
    pub(crate) fn open(path: PathBuf) -> Result<Self, ManagerError> {
        let settings = match fs::read(&path) {
            Ok(data) => decode(&data)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = NetworkSettings::default();
                write(&path, &settings)?;
                settings
            }
            Err(error) => return Err(ManagerError::io("read network settings", error)),
        };
        Ok(Self {
            path,
            settings: Mutex::new(settings),
        })
    }

    pub(crate) fn read(&self) -> NetworkSettings {
        self.settings.lock().expect("network settings lock").clone()
    }

    pub(crate) fn replace(
        &self,
        mut candidate: NetworkSettings,
    ) -> Result<NetworkSettings, ManagerError> {
        validate(&candidate)?;
        let mut current = self.settings.lock().expect("network settings lock");
        candidate.revision = current.revision.saturating_add(1);
        write(&self.path, &candidate)?;
        *current = candidate.clone();
        Ok(candidate)
    }

    pub(crate) fn restore(&self, settings: NetworkSettings) -> Result<(), ManagerError> {
        write(&self.path, &settings)?;
        *self.settings.lock().expect("network settings lock") = settings;
        Ok(())
    }
}

fn decode(data: &[u8]) -> Result<NetworkSettings, ManagerError> {
    let settings = serde_json::from_slice(data).map_err(|error| {
        ManagerError::InvalidOperation(format!("decode network settings: {error}"))
    })?;
    validate(&settings)?;
    Ok(settings)
}

fn validate(settings: &NetworkSettings) -> Result<(), ManagerError> {
    if settings.schema != SCHEMA_VERSION {
        return Err(ManagerError::InvalidOperation(format!(
            "network settings schema must be {SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn write(path: &std::path::Path, settings: &NetworkSettings) -> Result<(), ManagerError> {
    let mut data = serde_json::to_vec_pretty(settings).map_err(|error| {
        ManagerError::InvalidOperation(format!("encode network settings: {error}"))
    })?;
    data.push(b'\n');
    sempre_state::write_atomic(path, &data, 0o600)
        .map_err(|error| ManagerError::io("write network settings", error))
}

impl<R: crate::VersionRunner> crate::Manager<R> {
    pub(crate) fn apply_network_settings(
        &self,
        profile: &Profile,
    ) -> Result<Profile, ManagerError> {
        let settings = self.network_settings.read();
        let mut profile = profile.clone();
        match settings.mode {
            NetworkMode::Local => {
                profile.transparent_proxy.capture_host = true;
                profile.transparent_proxy.lan_interfaces.clear();
            }
            NetworkMode::Gateway => {
                profile.transparent_proxy.mode = "tproxy".into();
                profile.transparent_proxy.capture_host = settings.gateway_capture_host;
                let gateway = self.gateway.read()?;
                profile.transparent_proxy.lan_interfaces =
                    if gateway.lan.interface.trim().is_empty() {
                        Vec::new()
                    } else {
                        vec![gateway.lan.interface]
                    };
            }
        }
        Ok(profile)
    }

    pub(crate) fn apply_dns_frontend_settings(
        &self,
        profile: &Profile,
        target: &Target,
        enabled: bool,
    ) -> Result<Profile, ManagerError> {
        let mut profile = sempre_converter::apply_dns_frontend_settings(profile, target, enabled)?;
        if !enabled || target.core != "sing-box" || target.platform != "default" {
            return Ok(profile);
        }
        let settings = self.network_settings.read();
        let shared = shared_dns_mut(&mut profile.dns);
        match settings.mode {
            NetworkMode::Local => {
                shared.insert("systemDnsListenPort".into(), json!(53));
                shared.insert("systemDnsListenHosts".into(), json!(["127.0.0.1"]));
                shared.insert("systemDnsTakeoverHost".into(), Value::Bool(true));
            }
            NetworkMode::Gateway => {
                shared.insert("systemDnsListenPort".into(), json!(1054));
                shared.insert("systemDnsListenHosts".into(), json!(["0.0.0.0"]));
                shared.insert("systemDnsTakeoverHost".into(), Value::Bool(false));
            }
        }
        Ok(profile)
    }
}

fn shared_dns_mut(dns: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !dns.is_object() {
        *dns = json!({});
    }
    let object = dns.as_object_mut().expect("DNS object");
    let shared = object.entry("shared").or_insert_with(|| json!({}));
    if !shared.is_object() {
        *shared = json!({});
    }
    shared.as_object_mut().expect("shared DNS object")
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};

    use super::*;

    #[test]
    fn defaults_to_local_and_round_trips_gateway_mode() {
        let root = tempfile::tempdir().expect("directory");
        let path = root.path().join("network.json");
        let store = NetworkSettingsStore::open(path.clone()).expect("store");
        assert_eq!(store.read().mode, NetworkMode::Local);
        let saved = store
            .replace(NetworkSettings {
                mode: NetworkMode::Gateway,
                gateway_capture_host: true,
                ..store.read()
            })
            .expect("save");
        assert_eq!(saved.revision, 2);
        let reopened = NetworkSettingsStore::open(path).expect("reopen");
        assert_eq!(reopened.read(), saved);
    }

    #[test]
    fn gateway_mode_derives_tproxy_scope_and_frontend_binding() {
        let root = tempfile::tempdir().expect("directory");
        let manager = crate::Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let mut gateway = manager.gateway.read().expect("gateway");
        gateway.lan.interface = "vmbr1".into();
        manager.gateway.update(&gateway).expect("save gateway");
        manager
            .network_settings
            .replace(NetworkSettings {
                mode: NetworkMode::Gateway,
                gateway_capture_host: false,
                ..manager.network_settings.read()
            })
            .expect("save mode");
        let profile = Profile::default();
        let network = manager
            .apply_network_settings(&profile)
            .expect("network overlay");
        assert_eq!(network.transparent_proxy.mode, "tproxy");
        assert!(!network.transparent_proxy.capture_host);
        assert_eq!(network.transparent_proxy.lan_interfaces, ["vmbr1"]);

        let target = Target::parse("sing-box-v14").expect("target");
        let dns = manager
            .apply_dns_frontend_settings(&network, &target, true)
            .expect("DNS overlay");
        assert_eq!(dns.dns["shared"]["systemDnsListenPort"], 1054);
        assert_eq!(
            dns.dns["shared"]["systemDnsListenHosts"],
            json!(["0.0.0.0"])
        );
        assert_eq!(dns.dns["shared"]["systemDnsTakeoverHost"], false);
    }
}
