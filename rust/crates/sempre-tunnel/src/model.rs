use std::{collections::BTreeSet, net::IpAddr, str::FromStr as _};

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::TunnelError;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Config {
    pub schema: u32,
    #[serde(default)]
    pub instances: Vec<Instance>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            instances: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    #[serde(default = "stopped")]
    pub desired_state: String,
    pub server_url: String,
    #[serde(default)]
    pub dns_resolvers: Vec<String>,
    #[serde(default)]
    pub prefer_ipv4: bool,
    #[serde(default = "default_ping")]
    pub websocket_ping: String,
    #[serde(default = "default_backoff")]
    pub connection_retry_max_backoff: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub upgrade_path_prefix: String,
    #[serde(default)]
    pub forwards: Vec<Forward>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Forward {
    pub id: String,
    pub name: String,
    pub listen_port: u16,
    #[serde(default = "loopback")]
    pub remote_host: String,
    pub remote_port: u16,
    #[serde(default)]
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ForwardEndpoint {
    pub instance_id: String,
    pub instance_name: String,
    pub forward_id: String,
    pub forward_name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BinaryStatus {
    pub version: String,
    pub installed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstanceStatus {
    pub id: String,
    pub state: String,
    pub restart_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_error: String,
    pub log_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Status {
    pub config: Config,
    pub binary: BinaryStatus,
    pub instances: Vec<InstanceStatus>,
    pub forwards: Vec<ForwardEndpoint>,
}

impl Config {
    pub fn normalize(&mut self) {
        self.schema = SCHEMA_VERSION;
        for instance in &mut self.instances {
            instance.id = instance.id.trim().into();
            instance.name = instance.name.trim().into();
            instance.server_url = instance.server_url.trim().into();
            instance.upgrade_path_prefix = instance.upgrade_path_prefix.trim().into();
            if instance.desired_state.is_empty() {
                instance.desired_state = stopped();
            }
            if instance.websocket_ping.is_empty() {
                instance.websocket_ping = default_ping();
            }
            if instance.connection_retry_max_backoff.is_empty() {
                instance.connection_retry_max_backoff = default_backoff();
            }
            for resolver in &mut instance.dns_resolvers {
                *resolver = resolver.trim().into();
            }
            for forward in &mut instance.forwards {
                forward.id = forward.id.trim().into();
                forward.name = forward.name.trim().into();
                forward.remote_host = forward.remote_host.trim().into();
                if forward.remote_host.is_empty() {
                    forward.remote_host = loopback();
                }
            }
        }
    }

    pub fn validate(&self) -> Result<(), TunnelError> {
        if self.schema != SCHEMA_VERSION {
            return Err(TunnelError::invalid(format!(
                "unsupported tunnel schema {}",
                self.schema
            )));
        }
        let id_pattern = Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}$").expect("static ID pattern");
        let mut instance_ids = BTreeSet::new();
        let mut forward_ids = BTreeSet::new();
        let mut listen_ports = BTreeSet::new();
        for instance in &self.instances {
            validate_instance(instance, &id_pattern, &mut instance_ids)?;
            for forward in &instance.forwards {
                validate_forward(forward, &id_pattern, &mut forward_ids, &mut listen_ports)?;
            }
        }
        Ok(())
    }

    pub fn forward(&self, id: &str) -> Option<ForwardEndpoint> {
        self.forward_endpoints()
            .into_iter()
            .find(|forward| forward.forward_id == id)
    }

    pub fn forward_endpoints(&self) -> Vec<ForwardEndpoint> {
        self.instances
            .iter()
            .flat_map(|instance| {
                instance.forwards.iter().map(|forward| ForwardEndpoint {
                    instance_id: instance.id.clone(),
                    instance_name: instance.name.clone(),
                    forward_id: forward.id.clone(),
                    forward_name: forward.name.clone(),
                    host: loopback(),
                    port: forward.listen_port,
                })
            })
            .collect()
    }
}

impl Instance {
    pub fn arguments(&self) -> Vec<String> {
        let mut arguments = vec![
            "--no-color=true".into(),
            "client".into(),
            "--tls-verify-certificate".into(),
            "--websocket-ping-frequency".into(),
            self.websocket_ping.clone(),
            "--connection-retry-max-backoff".into(),
            self.connection_retry_max_backoff.clone(),
        ];
        if self.prefer_ipv4 {
            arguments.push("--dns-resolver-prefer-ipv4".into());
        }
        for resolver in &self.dns_resolvers {
            arguments.extend(["--dns-resolver".into(), resolver.clone()]);
        }
        if !self.upgrade_path_prefix.is_empty() {
            arguments.extend([
                "--http-upgrade-path-prefix".into(),
                self.upgrade_path_prefix.clone(),
            ]);
        }
        for forward in &self.forwards {
            let host = if forward.remote_host.contains(':') {
                format!("[{}]", forward.remote_host)
            } else {
                forward.remote_host.clone()
            };
            arguments.extend([
                "--local-to-remote".into(),
                format!(
                    "udp://127.0.0.1:{}:{host}:{}?timeout_sec={}",
                    forward.listen_port, forward.remote_port, forward.timeout_seconds
                ),
            ]);
        }
        arguments.push(self.server_url.clone());
        arguments
    }
}

fn validate_instance(
    instance: &Instance,
    pattern: &Regex,
    identifiers: &mut BTreeSet<String>,
) -> Result<(), TunnelError> {
    if !pattern.is_match(&instance.id) || !identifiers.insert(instance.id.clone()) {
        return Err(TunnelError::invalid(format!(
            "invalid or duplicate tunnel instance ID {:?}",
            instance.id
        )));
    }
    if instance.name.is_empty() {
        return Err(TunnelError::invalid(format!(
            "tunnel instance {:?} requires a name",
            instance.id
        )));
    }
    if !matches!(instance.desired_state.as_str(), "running" | "stopped") {
        return Err(TunnelError::invalid(format!(
            "tunnel instance {:?} has invalid desired state",
            instance.id
        )));
    }
    let server = Url::parse(&instance.server_url).ok();
    if server.as_ref().is_none_or(|url| {
        url.scheme() != "wss"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
    }) {
        return Err(TunnelError::invalid(
            "server URL must be an absolute wss:// URL without credentials",
        ));
    }
    validate_duration(&instance.websocket_ping)?;
    validate_duration(&instance.connection_retry_max_backoff)?;
    for resolver in &instance.dns_resolvers {
        validate_resolver(resolver)?;
    }
    if instance.desired_state == "running" && instance.forwards.is_empty() {
        return Err(TunnelError::invalid(format!(
            "running tunnel instance {:?} requires a forward",
            instance.id
        )));
    }
    Ok(())
}

fn validate_forward(
    forward: &Forward,
    pattern: &Regex,
    identifiers: &mut BTreeSet<String>,
    ports: &mut BTreeSet<u16>,
) -> Result<(), TunnelError> {
    if !pattern.is_match(&forward.id) || !identifiers.insert(forward.id.clone()) {
        return Err(TunnelError::invalid(format!(
            "invalid or duplicate tunnel forward ID {:?}",
            forward.id
        )));
    }
    if forward.name.is_empty() || forward.listen_port == 0 || forward.remote_port == 0 {
        return Err(TunnelError::invalid(format!(
            "tunnel forward {:?} requires a name and valid ports",
            forward.id
        )));
    }
    if !ports.insert(forward.listen_port) {
        return Err(TunnelError::invalid(format!(
            "tunnel listen port {} is duplicated",
            forward.listen_port
        )));
    }
    if forward.remote_host.is_empty()
        || forward.remote_host.chars().any(char::is_whitespace)
        || (forward.remote_host.contains(':') && IpAddr::from_str(&forward.remote_host).is_err())
    {
        return Err(TunnelError::invalid(format!(
            "tunnel forward {:?} has invalid remote host",
            forward.id
        )));
    }
    Ok(())
}

fn validate_duration(value: &str) -> Result<(), TunnelError> {
    if humantime::parse_duration(value).is_ok_and(|duration| duration.as_secs() >= 1) {
        Ok(())
    } else {
        Err(TunnelError::invalid(
            "tunnel duration must be at least one second",
        ))
    }
}

fn validate_resolver(value: &str) -> Result<(), TunnelError> {
    let parsed = Url::parse(value)
        .map_err(|_| TunnelError::invalid(format!("invalid DNS resolver {value:?}")))?;
    if !matches!(parsed.scheme(), "dns" | "dns+https" | "dns+tls" | "system")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(TunnelError::invalid(format!(
            "unsupported DNS resolver {value:?}"
        )));
    }
    Ok(())
}

fn stopped() -> String {
    "stopped".into()
}
fn default_ping() -> String {
    "15s".into()
}
fn default_backoff() -> String {
    "30s".into()
}
fn loopback() -> String {
    "127.0.0.1".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        serde_json::from_value(serde_json::json!({
            "schema": 1, "instances": [{
                "id": "hz", "name": "Hangzhou", "desired_state": "running",
                "server_url": "wss://hz.example.com:443",
                "dns_resolvers": ["dns://192.0.2.53:53"], "prefer_ipv4": true,
                "websocket_ping": "15s", "connection_retry_max_backoff": "30s",
                "forwards": [{ "id": "hz-wg", "name": "WG", "listen_port": 52001,
                    "remote_host": "127.0.0.1", "remote_port": 31088, "timeout_seconds": 0 }]
            }]
        }))
        .expect("config")
    }

    #[test]
    fn validates_and_builds_wstunnel_arguments() {
        let config = config();
        config.validate().expect("valid config");
        let arguments = config.instances[0].arguments();
        assert!(arguments.contains(&"--dns-resolver-prefer-ipv4".into()));
        assert!(arguments.contains(&"udp://127.0.0.1:52001:127.0.0.1:31088?timeout_sec=0".into()));
        assert_eq!(config.forward("hz-wg").expect("forward").port, 52001);
    }

    #[test]
    fn rejects_cleartext_and_duplicate_ports() {
        let mut cleartext = config();
        cleartext.instances[0].server_url = "ws://hz.example.com".into();
        assert!(cleartext.validate().is_err());
        let mut duplicate = config();
        let mut second = duplicate.instances[0].forwards[0].clone();
        second.id = "backup".into();
        duplicate.instances[0].forwards.push(second);
        assert!(duplicate.validate().is_err());
    }
}
