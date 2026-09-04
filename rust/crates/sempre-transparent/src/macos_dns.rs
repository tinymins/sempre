use std::{fs, io, net::IpAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{TransparentError, command};

#[path = "macos_preferences.rs"]
mod preferences;

const NETWORK_SETUP: &str = "/usr/sbin/networksetup";
const SCUTIL: &str = "/usr/sbin/scutil";
const MANAGED_SERVER: &str = "127.0.0.1";
const STANDARD_DNS_PORT: u16 = 53;

pub(crate) struct SystemDns {
    allowed: bool,
    state_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct State {
    #[serde(default)]
    original_upstreams: Vec<String>,
    #[serde(default = "standard_dns_port")]
    managed_port: u16,
    services: Vec<ServiceState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ServiceState {
    #[serde(default)]
    id: Option<String>,
    name: String,
    original: Vec<String>,
    #[serde(default)]
    original_port: Option<u16>,
}

const fn standard_dns_port() -> u16 {
    STANDARD_DNS_PORT
}

impl SystemDns {
    pub(crate) fn new(allowed: bool, state_dir: PathBuf) -> Self {
        Self { allowed, state_dir }
    }

    pub(crate) const fn allowed(&self) -> bool {
        self.allowed
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
        let servers = if let Some(state) = self.read_state()? {
            state.original_upstreams
        } else {
            let output =
                command::require_success(SCUTIL, runner.run(SCUTIL, &["--dns"], None).await?)?;
            parse_scutil_dns(&output.stdout)
        };
        if servers.is_empty() {
            Err(TransparentError::Invalid(
                "macOS has no usable original DNS servers".into(),
            ))
        } else {
            Ok(servers)
        }
    }

    pub(crate) async fn apply(
        &self,
        runner: &dyn command::Runner,
        original_upstreams: &[String],
        listen_port: u16,
    ) -> Result<(), TransparentError> {
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
            .map_err(|source| Self::io("create macOS DNS state directory", source))?;
        self.write_state(&State {
            original_upstreams: original_upstreams.to_vec(),
            managed_port: listen_port,
            services: services.clone(),
        })?;
        let mut changed: Vec<ServiceState> = Vec::new();
        for service in &services {
            let configuration = preferences::DnsConfiguration {
                servers: vec![MANAGED_SERVER.into()],
                port: Some(listen_port),
            };
            if let Err(error) = preferences::set_dns_configuration(
                runner,
                service.id.as_deref().expect("captured service identifier"),
                &configuration,
            )
            .await
            {
                for service in changed.iter().rev() {
                    let _ = restore_service(runner, service).await;
                }
                return Err(error);
            }
            changed.push(service.clone());
        }
        self.verify(runner, listen_port).await
    }

    pub(crate) async fn restore(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        if !self.allowed {
            return Ok(());
        }
        let Some(state) = self.read_state()? else {
            return Ok(());
        };
        let active_services = preferences::active_services(runner).await?;
        for service in &state.services {
            let id = resolve_service_id(service, &active_services)?;
            let current = preferences::dns_configuration(runner, id).await?;
            if current.servers == [MANAGED_SERVER]
                && current.port.unwrap_or(STANDARD_DNS_PORT) == state.managed_port
            {
                restore_service_with_id(runner, service, id).await?;
            }
        }
        match fs::remove_file(self.state_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Self::io("remove macOS DNS state", source)),
        }
    }

    pub(crate) async fn verify(
        &self,
        runner: &dyn command::Runner,
        expected_port: u16,
    ) -> Result<(), TransparentError> {
        let state = self.read_state()?.ok_or_else(|| {
            TransparentError::Invalid("macOS DNS takeover has no ownership state".into())
        })?;
        if state.managed_port != expected_port {
            return Err(TransparentError::Invalid(format!(
                "macOS DNS takeover owns port {}, expected {expected_port}",
                state.managed_port
            )));
        }
        let active_services = preferences::active_services(runner).await?;
        for service in state.services {
            let id = resolve_service_id(&service, &active_services)?;
            let current = preferences::dns_configuration(runner, id).await?;
            if current.servers != [MANAGED_SERVER]
                || current.port.unwrap_or(STANDARD_DNS_PORT) != expected_port
            {
                return Err(TransparentError::Invalid(format!(
                    "macOS network service {:?} is not using the Sempre DNS frontend on port {expected_port}",
                    service.name,
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
        let active_services = preferences::active_services(runner).await?;
        let mut services = Vec::new();
        for name in parse_services(&output.stdout) {
            let service = active_services
                .iter()
                .find(|service| service.name == name)
                .ok_or_else(|| {
                    TransparentError::Invalid(format!(
                        "enabled macOS network service {name:?} is not in the active location"
                    ))
                })?;
            let configuration = preferences::dns_configuration(runner, &service.id).await?;
            services.push(ServiceState {
                id: Some(service.id.clone()),
                original: configuration.servers,
                original_port: configuration.port,
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
            .map_err(|source| Self::io("write macOS DNS state", source))
    }

    fn read_state(&self) -> Result<Option<State>, TransparentError> {
        match fs::read(self.state_path()) {
            Ok(data) => serde_json::from_slice(&data).map(Some).map_err(|error| {
                TransparentError::Invalid(format!("decode macOS DNS state: {error}"))
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Self::io("read macOS DNS state", source)),
        }
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join("network-services.json")
    }

    fn io(context: &str, source: io::Error) -> TransparentError {
        TransparentError::Io {
            context: context.into(),
            source,
        }
    }
}

fn resolve_service_id<'a>(
    service: &ServiceState,
    active_services: &'a [preferences::NetworkService],
) -> Result<&'a str, TransparentError> {
    active_services
        .iter()
        .find(|candidate| {
            service.id.as_ref().is_some_and(|id| id == &candidate.id)
                || candidate.name == service.name
        })
        .map(|service| service.id.as_str())
        .ok_or_else(|| {
            TransparentError::Invalid(format!(
                "macOS network service {:?} is not in the active location",
                service.name
            ))
        })
}

async fn restore_service(
    runner: &dyn command::Runner,
    service: &ServiceState,
) -> Result<(), TransparentError> {
    restore_service_with_id(
        runner,
        service,
        service.id.as_deref().expect("captured service identifier"),
    )
    .await
}

async fn restore_service_with_id(
    runner: &dyn command::Runner,
    service: &ServiceState,
    id: &str,
) -> Result<(), TransparentError> {
    preferences::set_dns_configuration(
        runner,
        id,
        &preferences::DnsConfiguration {
            servers: service.original.clone(),
            port: service.original_port,
        },
    )
    .await
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
    use std::{future::Future, pin::Pin, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<String>>,
    }

    impl command::Runner for FakeRunner {
        fn run<'a>(
            &'a self,
            program: &'a str,
            arguments: &'a [&'a str],
            input: Option<&'a [u8]>,
        ) -> Pin<Box<dyn Future<Output = Result<command::Output, TransparentError>> + Send + 'a>>
        {
            Box::pin(async move {
                let input = input.map(String::from_utf8_lossy).unwrap_or_default();
                self.calls
                    .lock()
                    .expect("calls")
                    .push(format!("{program} {}\n{input}", arguments.join(" ")));
                let stdout = if program == "/usr/sbin/scselect" {
                    "Defined sets include: (* == current set)\n * SET-ID\t(Automatic)\n".into()
                } else if input.contains("list /Sets/SET-ID/Network/Service") {
                    "path [0] = /Sets/SET-ID/Network/Service/SERVICE-A\npath [1] = /Sets/SET-ID/Network/Service/SERVICE-B\n".into()
                } else if input.contains("/Sets/SET-ID/Network/Service/SERVICE-A") {
                    "<dictionary> {\n UserDefinedName : Wi-Fi\n}\n".into()
                } else if input.contains("/Sets/SET-ID/Network/Service/SERVICE-B") {
                    "<dictionary> {\n UserDefinedName : USB LAN\n}\n".into()
                } else if input.contains("d.show") {
                    "<dictionary> {\n ServerAddresses : <array> {\n  0 : 127.0.0.1\n }\n ServerPort : 20554\n}\n".into()
                } else {
                    String::new()
                };
                Ok(command::Output {
                    success: true,
                    stdout,
                    stderr: String::new(),
                })
            })
        }
    }

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

    #[tokio::test]
    async fn stale_ownership_restores_static_and_dhcp_services() {
        let root = tempfile::tempdir().expect("temporary directory");
        let dns = SystemDns::new(true, root.path().into());
        fs::create_dir_all(root.path()).expect("state directory");
        dns.write_state(&State {
            original_upstreams: vec!["223.6.6.6".into()],
            managed_port: 20554,
            services: vec![
                ServiceState {
                    id: Some("SERVICE-A".into()),
                    name: "Wi-Fi".into(),
                    original: Vec::new(),
                    original_port: None,
                },
                ServiceState {
                    id: Some("SERVICE-B".into()),
                    name: "USB LAN".into(),
                    original: vec!["223.6.6.6".into()],
                    original_port: Some(5353),
                },
            ],
        })
        .expect("ownership state");
        let runner = FakeRunner::default();
        dns.restore(&runner).await.expect("restore stale ownership");
        let calls = runner.calls.lock().expect("calls");
        assert!(calls.iter().any(|call| {
            call.contains("get /NetworkServices/SERVICE-A/DNS")
                && call.contains("commit\napply\nquit")
                && !call.contains("d.add ServerAddresses")
        }));
        assert!(calls.iter().any(|call| {
            call.contains("get /NetworkServices/SERVICE-B/DNS")
                && call.contains("d.add ServerAddresses * 223.6.6.6")
                && call.contains("d.add ServerPort # 5353")
        }));
        assert!(!dns.state_path().exists());
    }

    #[test]
    fn old_ownership_state_defaults_to_standard_dns_port() {
        let state: State = serde_json::from_str(
            r#"{"original_upstreams":["223.6.6.6"],"services":[{"name":"Wi-Fi","original":[]}]}"#,
        )
        .expect("old ownership state");
        assert_eq!(state.managed_port, 53);
        assert_eq!(state.services[0].id, None);
        assert_eq!(state.services[0].original_port, None);
    }

    #[tokio::test]
    async fn disabled_controller_never_consumes_relative_ownership_state() {
        let runner = FakeRunner::default();
        SystemDns::new(false, PathBuf::new())
            .restore(&runner)
            .await
            .expect("disabled restore");
        assert!(runner.calls.lock().expect("calls").is_empty());
    }
}
