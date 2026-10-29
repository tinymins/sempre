use std::time::Duration;

use sempre_core_control::Client;
use sempre_network::DefaultInterface;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::{Instant, sleep};

use crate::{KnownNetwork, Manager, ManagerError, ValidationRunner, VersionRunner};

const NORMAL_MODE: &str = "Rule";
// Probes after a network event: the first follows the 2s interface cache, the
// rest wait for the gateway MAC to resolve. Nothing polls in between events.
const SETTLE_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(3),
    Duration::from_secs(4),
];

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
        let mut events = match sempre_transparent::RouteEvents::open() {
            Ok(events) => Some(events),
            Err(error) => {
                self.log_supervisor(&format!(
                    "network automation events unavailable; following settings only: {error}"
                ));
                None
            }
        };
        loop {
            let settings_changed = tokio::select! {
                result = crate::cn_routes::route_changed(&mut events) => result.map(|()| false),
                () = self.network_settings.changed() => Ok(true),
            };
            match settings_changed {
                Err(error) => {
                    self.log_supervisor(&format!(
                        "network automation events failed; following settings only: {error}"
                    ));
                    events = None;
                }
                Ok(_) if self.network_settings.read().automatic_switching => {
                    self.settle_network_mode().await;
                }
                // Disabling restores the normal mode once; idle events probe nothing.
                Ok(true) => {
                    if let Err(error) = self.sync_network_mode().await {
                        self.log_supervisor(&format!("network automation probe failed: {error}"));
                    }
                }
                Ok(false) => {}
            }
        }
    }

    async fn settle_network_mode(&self)
    where
        R: ValidationRunner,
    {
        let mut result = self.sync_network_mode().await;
        for (index, delay) in SETTLE_DELAYS.into_iter().enumerate() {
            if index > 0 && result == Ok(true) {
                break;
            }
            sleep(delay).await;
            result = self.sync_network_mode().await;
        }
        if let Err(error) = result {
            self.log_supervisor(&format!("network automation probe failed: {error}"));
        }
    }

    /// Returns whether the gateway was identified, so the mode is final.
    async fn sync_network_mode(&self) -> Result<bool, String> {
        let settings = self.network_settings.read();
        let mut resolved = true;
        let desired = if settings.automatic_switching {
            let observed =
                sempre_network::default_interface().map_err(|error| error.to_string())?;
            resolved = observed.gateway.is_empty() || !observed.gateway_mac.is_empty();
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
            return Ok(resolved);
        }
        client
            .patch_config(json!({ "mode": desired }))
            .await
            .map_err(|error| error.to_string())?;
        Ok(resolved)
    }
}

pub(crate) async fn wait_for_network_change() {
    let Ok(mut events) = sempre_transparent::RouteEvents::open() else {
        // The retry timer still fires.
        return std::future::pending().await;
    };
    let initial = observe_network();
    loop {
        if events.changed().await.is_err() {
            return std::future::pending().await;
        }
        // Confirm once past the interface cache before waiting for more events.
        for delay in [Duration::ZERO, SETTLE_DELAYS[0]] {
            sleep(delay).await;
            if network_changed(initial.as_ref(), observe_network().as_ref()) {
                return;
            }
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
