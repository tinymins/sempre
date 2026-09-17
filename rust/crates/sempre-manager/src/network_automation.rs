use std::time::Duration;

use sempre_core_control::Client;
use sempre_network::DefaultInterface;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::{Instant, sleep};

use crate::{KnownNetwork, Manager, ManagerError, ValidationRunner, VersionRunner};

const NORMAL_MODE: &str = "Rule";
const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkIdentity {
    supported: bool,
    name: String,
    addresses: Vec<String>,
    gateway: String,
    gateway_mac: String,
}

impl From<DefaultInterface> for NetworkIdentity {
    fn from(interface: DefaultInterface) -> Self {
        Self {
            supported: interface.supported,
            name: interface.name,
            addresses: interface.addresses,
            gateway: interface.gateway,
            gateway_mac: interface.gateway_mac,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkAutomationStatus {
    pub enabled: bool,
    pub active: bool,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interface: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gateway: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gateway_mac: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_error: Option<String>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn network_automation_status(&self) -> Result<NetworkAutomationStatus, ManagerError> {
        let settings = self.network_settings.read();
        let document = self.store.read()?;
        let (observed, probe_error) = observe();
        let matched = match_network(&settings.known_networks, &observed.gateway_mac);
        let path = if !settings.automatic_switching || !runtime_active(&document) {
            "inactive"
        } else if automation_config_pending(&document) || observed.gateway_mac.is_empty() {
            "unknown"
        } else if matched.is_some_and(|network| network.disable_proxy) {
            "direct"
        } else {
            "proxy"
        };
        Ok(status(&settings, observed, probe_error, matched, path))
    }

    pub(crate) async fn activate_network_automation(&self, grace: Duration) {
        if !self.network_settings.read().automatic_switching {
            let _ = self.sync_network_mode().await;
            return;
        }
        let deadline = Instant::now() + grace;
        loop {
            if self.sync_network_mode().await.is_ok() || Instant::now() >= deadline {
                return;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    pub(crate) async fn monitor_network_automation(&self)
    where
        R: ValidationRunner,
    {
        let mut last_enabled = None;
        loop {
            let enabled = self.network_settings.read().automatic_switching;
            if (enabled || last_enabled != Some(false))
                && let Err(error) = self.sync_network_mode().await
            {
                let _ = self.log_supervisor(&format!("network automation probe failed: {error}"));
            }
            last_enabled = Some(enabled);
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn sync_network_mode(&self) -> Result<(), String> {
        let settings = self.network_settings.read();
        let desired = if settings.automatic_switching {
            let observed =
                sempre_network::default_interface().map_err(|error| error.to_string())?;
            match_network(&settings.known_networks, &observed.gateway_mac).map_or_else(
                || NORMAL_MODE.into(),
                |network| sempre_converter::network_mode(&network.id),
            )
        } else {
            NORMAL_MODE.into()
        };
        let client = Client::from_file(&self.store.layout().core_control)
            .map_err(|error| error.to_string())?;
        let config = client.config().await.map_err(|error| error.to_string())?;
        let desired = if desired == NORMAL_MODE
            || config
                .get("mode-list")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .any(|mode| mode.eq_ignore_ascii_case(&desired))
        {
            desired
        } else {
            NORMAL_MODE.into()
        };
        let current = config
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if current.eq_ignore_ascii_case(&desired) {
            return Ok(());
        }
        client
            .patch_config(json!({ "mode": desired }))
            .await
            .map_err(|error| error.to_string())
    }
}

pub(crate) async fn wait_for_network_change() {
    wait_for_network_change_with(POLL_INTERVAL, observe_network).await;
}

async fn wait_for_network_change_with(
    interval: Duration,
    mut observe: impl FnMut() -> Option<NetworkIdentity>,
) {
    let initial = observe();
    loop {
        sleep(interval).await;
        if network_changed(initial.as_ref(), observe().as_ref()) {
            return;
        }
    }
}

fn network_changed(initial: Option<&NetworkIdentity>, current: Option<&NetworkIdentity>) -> bool {
    match (initial, current) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(initial), Some(current)) => {
            initial.supported != current.supported
                || initial.name != current.name
                || initial.addresses != current.addresses
                || initial.gateway != current.gateway
                || (!initial.gateway_mac.is_empty()
                    && !current.gateway_mac.is_empty()
                    && initial.gateway_mac != current.gateway_mac)
        }
    }
}

fn observe_network() -> Option<NetworkIdentity> {
    sempre_network::default_interface()
        .ok()
        .map(NetworkIdentity::from)
}

fn observe() -> (DefaultInterface, Option<String>) {
    match sempre_network::default_interface() {
        Ok(value) => (value, None),
        Err(error) => (DefaultInterface::default(), Some(error.to_string())),
    }
}

fn match_network<'a>(networks: &'a [KnownNetwork], gateway_mac: &str) -> Option<&'a KnownNetwork> {
    let observed = sempre_network::normalize_mac(gateway_mac)?;
    networks.iter().find(|network| {
        sempre_network::normalize_mac(&network.gateway_mac).as_deref() == Some(observed.as_str())
    })
}

fn status(
    settings: &crate::NetworkSettings,
    observed: DefaultInterface,
    probe_error: Option<String>,
    matched: Option<&KnownNetwork>,
    path: &str,
) -> NetworkAutomationStatus {
    NetworkAutomationStatus {
        enabled: settings.automatic_switching,
        active: settings.automatic_switching && path != "inactive",
        path: path.into(),
        network_id: matched.map(|network| network.id.clone()),
        network_name: matched.map(|network| network.name.clone()),
        interface: observed.name,
        gateway: observed.gateway,
        gateway_mac: observed.gateway_mac,
        probe_error,
    }
}

fn runtime_active(document: &sempre_state::Document) -> bool {
    document.runtime.pid.is_some()
        && matches!(
            document.runtime.state,
            sempre_state::RuntimeState::Starting
                | sempre_state::RuntimeState::Running
                | sempre_state::RuntimeState::Stopping
                | sempre_state::RuntimeState::Restarting
        )
}

fn automation_config_pending(document: &sempre_state::Document) -> bool {
    document.pending_config_fields.iter().any(|field| {
        matches!(
            field,
            sempre_state::PendingConfigField::TransparentProxy
                | sempre_state::PendingConfigField::Dns
                | sempre_state::PendingConfigField::PrivateAccess
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(address: &str, gateway_mac: &str) -> DefaultInterface {
        DefaultInterface {
            supported: true,
            name: "en0".into(),
            addresses: vec![address.into()],
            gateway: "10.23.0.1".into(),
            gateway_mac: gateway_mac.into(),
        }
    }

    #[test]
    fn gateway_mac_selects_the_matching_network() {
        let networks = vec![KnownNetwork {
            id: "d286d2f8-33c5-4f1e-b871-d22a9ba91143".into(),
            name: "Home".into(),
            gateway_mac: "AA-BB-CC-DD-EE-FF".into(),
            disable_proxy: true,
        }];
        assert_eq!(
            match_network(&networks, "aa:bb:cc:dd:ee:ff").map(|value| value.name.as_str()),
            Some("Home")
        );
        assert!(match_network(&networks, "00:11:22:33:44:55").is_none());
    }

    #[test]
    fn network_identity_ignores_gateway_mac_refresh() {
        let initial = NetworkIdentity::from(interface("10.23.2.158/21", ""));
        let refreshed = NetworkIdentity::from(interface("10.23.2.158/21", "10:8f:fe:6b:a0:02"));
        assert!(!network_changed(Some(&initial), Some(&refreshed)));
    }

    #[test]
    fn network_identity_detects_known_gateway_change() {
        let initial = NetworkIdentity::from(interface("10.23.2.158/21", "10:8f:fe:6b:a0:02"));
        let changed = NetworkIdentity::from(interface("10.23.2.158/21", "a0:b1:c2:d3:e4:f5"));
        assert!(network_changed(Some(&initial), Some(&changed)));
    }

    #[tokio::test]
    async fn network_change_wakes_the_waiter() {
        let mut samples = [
            Some(NetworkIdentity::from(interface("10.23.2.158/21", ""))),
            Some(NetworkIdentity::from(interface("10.23.2.158/21", ""))),
            Some(NetworkIdentity::from(interface("10.23.2.159/21", ""))),
        ]
        .into_iter();

        tokio::time::timeout(
            Duration::from_millis(100),
            wait_for_network_change_with(Duration::from_millis(1), || {
                samples.next().expect("network sample")
            }),
        )
        .await
        .expect("network change");
    }
}
