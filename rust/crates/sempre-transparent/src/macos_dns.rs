use std::{fs, io, net::IpAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{TransparentError, command};

const NETWORK_SETUP: &str = "/usr/sbin/networksetup";
const SCUTIL: &str = "/usr/sbin/scutil";
const MANAGED_SERVER: &str = "127.0.0.1";

pub(crate) struct SystemDns {
    allowed: bool,
    state_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct State {
    services: Vec<ServiceState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ServiceState {
    name: String,
    original: Vec<String>,
}

impl SystemDns {
    pub(crate) fn new(allowed: bool, state_dir: PathBuf) -> Self {
        Self { allowed, state_dir }
    }

    pub(crate) async fn discover_upstreams(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<Vec<String>, TransparentError> {
        if !self.allowed {
            return Err(TransparentError::Invalid(
                "macOS DNS takeover requires system mode".into(),
            ));
        }
        let output = command::require_success(SCUTIL, runner.run(SCUTIL, &["--dns"], None).await?)?;
        let servers = parse_scutil_dns(&output.stdout);
        if servers.is_empty() {
            Err(TransparentError::Invalid(
                "macOS has no usable original DNS servers".into(),
            ))
        } else {
            Ok(servers)
        }
    }

    pub(crate) async fn apply(&self, runner: &dyn command::Runner) -> Result<(), TransparentError> {
        if !self.allowed {
            return Err(TransparentError::Invalid(
                "macOS DNS takeover requires system mode".into(),
            ));
        }
        let services = self.capture_services(runner).await?;
        if services.is_empty() {
            return Err(TransparentError::Invalid(
                "macOS has no enabled network services".into(),
            ));
        }
        fs::create_dir_all(&self.state_dir)
            .map_err(|source| self.io("create macOS DNS state directory", source))?;
        self.write_state(&State {
            services: services.clone(),
        })?;
        let mut changed: Vec<ServiceState> = Vec::new();
        for service in &services {
            if let Err(error) = set_servers(runner, &service.name, &[MANAGED_SERVER.into()]).await {
                for service in changed.iter().rev() {
                    let _ = set_servers(runner, &service.name, &service.original).await;
                }
                return Err(error);
            }
            changed.push(service.clone());
        }
        self.verify(runner).await
    }

    pub(crate) async fn restore(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        let Some(state) = self.read_state()? else {
            return Ok(());
        };
        for service in &state.services {
            let current = configured_servers(runner, &service.name).await?;
            if current == [MANAGED_SERVER] {
                set_servers(runner, &service.name, &service.original).await?;
            }
        }
        match fs::remove_file(self.state_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(self.io("remove macOS DNS state", source)),
        }
    }

    pub(crate) async fn verify(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        let state = self.read_state()?.ok_or_else(|| {
            TransparentError::Invalid("macOS DNS takeover has no ownership state".into())
        })?;
        for service in state.services {
            if configured_servers(runner, &service.name).await? != [MANAGED_SERVER] {
                return Err(TransparentError::Invalid(format!(
                    "macOS network service {:?} is not using the Sempre DNS frontend",
                    service.name
                )));
            }
        }
        Ok(())
    }

    async fn capture_services(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<Vec<ServiceState>, TransparentError> {
        let output = command::require_success(
            NETWORK_SETUP,
            runner
                .run(NETWORK_SETUP, &["-listallnetworkservices"], None)
                .await?,
        )?;
        let mut services = Vec::new();
        for name in parse_services(&output.stdout) {
            services.push(ServiceState {
                original: configured_servers(runner, &name).await?,
                name,
            });
        }
        Ok(services)
    }

    fn write_state(&self, state: &State) -> Result<(), TransparentError> {
        let mut data = serde_json::to_vec_pretty(state).map_err(|error| {
            TransparentError::Invalid(format!("encode macOS DNS state: {error}"))
        })?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.state_path(), &data, 0o600)
            .map_err(|source| self.io("write macOS DNS state", source))
    }

    fn read_state(&self) -> Result<Option<State>, TransparentError> {
        match fs::read(self.state_path()) {
            Ok(data) => serde_json::from_slice(&data).map(Some).map_err(|error| {
                TransparentError::Invalid(format!("decode macOS DNS state: {error}"))
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(self.io("read macOS DNS state", source)),
        }
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join("network-services.json")
    }

    fn io(&self, context: &str, source: io::Error) -> TransparentError {
        TransparentError::Io {
            context: context.into(),
            source,
        }
    }
}

async fn configured_servers(
    runner: &dyn command::Runner,
    service: &str,
) -> Result<Vec<String>, TransparentError> {
    let output = command::require_success(
        NETWORK_SETUP,
        runner
            .run(NETWORK_SETUP, &["-getdnsservers", service], None)
            .await?,
    )?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|value| value.parse::<IpAddr>().is_ok())
        .map(str::to_owned)
        .collect())
}

async fn set_servers(
    runner: &dyn command::Runner,
    service: &str,
    servers: &[String],
) -> Result<(), TransparentError> {
    let values = if servers.is_empty() {
        vec!["Empty"]
    } else {
        servers.iter().map(String::as_str).collect()
    };
    let mut arguments = vec!["-setdnsservers", service];
    arguments.extend(values);
    command::require_success(
        NETWORK_SETUP,
        runner.run(NETWORK_SETUP, &arguments, None).await?,
    )?;
    Ok(())
}

fn parse_scutil_dns(data: &str) -> Vec<String> {
    let mut first_resolver = false;
    let mut servers = Vec::new();
    for line in data.lines().map(str::trim) {
        if line == "resolver #1" {
            if first_resolver {
                break;
            }
            first_resolver = true;
            continue;
        }
        if first_resolver && line.starts_with("resolver #") {
            break;
        }
        if first_resolver
            && let Some(value) = line
                .split_once(':')
                .and_then(|(key, value)| key.starts_with("nameserver[").then_some(value.trim()))
            && value.parse::<IpAddr>().is_ok()
            && !value.starts_with("127.")
            && value != "::1"
            && !servers.iter().any(|server| server == value)
        {
            servers.push(value.into());
        }
    }
    servers
}

fn parse_services(data: &str) -> Vec<String> {
    data.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("An asterisk") && !line.starts_with('*')
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_primary_resolver_without_scoped_or_loopback_servers() {
        let data = "DNS configuration\n\nresolver #1\n  nameserver[0] : 127.0.0.1\n  nameserver[1] : 223.6.6.6\n  nameserver[2] : 61.130.254.35\nresolver #2\n  domain : local\n\nDNS configuration (for scoped queries)\nresolver #1\n  nameserver[0] : 9.9.9.9\n";
        assert_eq!(parse_scutil_dns(data), ["223.6.6.6", "61.130.254.35"]);
    }

    #[test]
    fn parses_only_enabled_network_services() {
        assert_eq!(
            parse_services(
                "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*VPN\nUSB LAN\n"
            ),
            ["Wi-Fi", "USB LAN"]
        );
    }
}
