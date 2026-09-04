use std::{fmt::Write as _, net::IpAddr};

use crate::{TransparentError, command};

const SCSELECT: &str = "/usr/sbin/scselect";
const SCUTIL: &str = "/usr/sbin/scutil";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DnsConfiguration {
    pub(crate) servers: Vec<String>,
    pub(crate) port: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkService {
    pub(crate) id: String,
    pub(crate) name: String,
}

pub(crate) async fn active_services(
    runner: &dyn command::Runner,
) -> Result<Vec<NetworkService>, TransparentError> {
    let output = command::require_success(SCSELECT, runner.run(SCSELECT, &[], None).await?)?;
    let set_id = parse_active_set(&output.stdout)
        .ok_or_else(|| TransparentError::Invalid("macOS has no active network location".into()))?;
    let service_root = format!("/Sets/{set_id}/Network/Service");
    let script = format!("list {service_root}\nquit\n");
    let output = command::require_success(
        SCUTIL,
        runner
            .run(SCUTIL, &["--prefs"], Some(script.as_bytes()))
            .await?,
    )?;
    let mut services = Vec::new();
    for id in parse_service_ids(&output.stdout, &service_root) {
        let path = format!("{service_root}/{id}");
        let script = format!("d.init\nget {path}\nd.show\nquit\n");
        let output = command::require_success(
            SCUTIL,
            runner
                .run(SCUTIL, &["--prefs"], Some(script.as_bytes()))
                .await?,
        )?;
        let name = parse_value(&output.stdout, "UserDefinedName").ok_or_else(|| {
            TransparentError::Invalid(format!("macOS network service {id} has no name"))
        })?;
        services.push(NetworkService { id, name });
    }
    Ok(services)
}

pub(crate) async fn dns_configuration(
    runner: &dyn command::Runner,
    service_id: &str,
) -> Result<DnsConfiguration, TransparentError> {
    validate_service_id(service_id)?;
    let path = dns_path(service_id);
    let script = format!("d.init\nget {path}\nd.show\nquit\n");
    let output = command::require_success(
        SCUTIL,
        runner
            .run(SCUTIL, &["--prefs"], Some(script.as_bytes()))
            .await?,
    )?;
    Ok(parse_dns_configuration(&output.stdout))
}

pub(crate) async fn set_dns_configuration(
    runner: &dyn command::Runner,
    service_id: &str,
    configuration: &DnsConfiguration,
) -> Result<(), TransparentError> {
    validate_service_id(service_id)?;
    if configuration
        .servers
        .iter()
        .any(|server| server.parse::<IpAddr>().is_err())
    {
        return Err(TransparentError::Invalid(
            "macOS DNS server is not an IP address".into(),
        ));
    }
    let script = update_script(service_id, configuration);
    command::require_success(
        SCUTIL,
        runner
            .run(SCUTIL, &["--prefs"], Some(script.as_bytes()))
            .await?,
    )?;
    Ok(())
}

fn update_script(service_id: &str, configuration: &DnsConfiguration) -> String {
    let path = dns_path(service_id);
    let mut script = format!("d.init\nget {path}\nd.remove ServerAddresses\nd.remove ServerPort\n");
    if !configuration.servers.is_empty() {
        script.push_str("d.add ServerAddresses *");
        for server in &configuration.servers {
            script.push(' ');
            script.push_str(server);
        }
        script.push('\n');
    }
    if let Some(port) = configuration.port {
        writeln!(script, "d.add ServerPort # {port}").expect("write DNS update command");
    }
    write!(script, "set {path}\ncommit\napply\nquit\n").expect("write DNS update commands");
    script
}

fn dns_path(service_id: &str) -> String {
    format!("/NetworkServices/{service_id}/DNS")
}

fn validate_service_id(service_id: &str) -> Result<(), TransparentError> {
    if !service_id.is_empty()
        && service_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-')
    {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "invalid macOS network service identifier {service_id:?}"
        )))
    }
}

fn parse_active_set(data: &str) -> Option<String> {
    data.lines().find_map(|line| {
        line.trim()
            .strip_prefix("* ")
            .and_then(|line| line.split_whitespace().next())
            .filter(|id| validate_service_id(id).is_ok())
            .map(str::to_owned)
    })
}

fn parse_service_ids(data: &str, service_root: &str) -> Vec<String> {
    let prefix = format!("{service_root}/");
    data.lines()
        .filter_map(|line| line.split_once('=').map(|(_, path)| path.trim()))
        .filter_map(|path| path.strip_prefix(&prefix))
        .filter(|id| !id.contains('/') && validate_service_id(id).is_ok())
        .map(str::to_owned)
        .collect()
}

fn parse_value(data: &str, key: &str) -> Option<String> {
    data.lines().find_map(|line| {
        line.trim()
            .split_once(':')
            .filter(|(candidate, _)| candidate.trim() == key)
            .map(|(_, value)| value.trim().to_owned())
    })
}

fn parse_dns_configuration(data: &str) -> DnsConfiguration {
    let mut servers = Vec::new();
    let mut in_servers = false;
    for line in data.lines().map(str::trim) {
        if line.starts_with("ServerAddresses : <array>") {
            in_servers = true;
            continue;
        }
        if in_servers && line == "}" {
            in_servers = false;
            continue;
        }
        if in_servers
            && let Some(value) = line.split_once(':').map(|(_, value)| value.trim())
            && value.parse::<IpAddr>().is_ok()
        {
            servers.push(value.to_owned());
        }
    }
    let port = parse_value(data, "ServerPort").and_then(|value| value.parse().ok());
    DnsConfiguration { servers, port }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_location_and_services() {
        let location = "Defined sets include: (* == current set)\n * SET-ID\t(Automatic)\n";
        assert_eq!(parse_active_set(location).as_deref(), Some("SET-ID"));
        let root = "/Sets/SET-ID/Network/Service";
        let services = format!("  path [0] = {root}/SERVICE-A\n  path [1] = {root}/SERVICE-B\n");
        assert_eq!(
            parse_service_ids(&services, root),
            ["SERVICE-A", "SERVICE-B"]
        );
    }

    #[test]
    fn parses_dns_addresses_and_port() {
        let data = "<dictionary> {\n  ServerAddresses : <array> {\n    0 : 127.0.0.1\n  }\n  ServerPort : 20554\n}\n";
        assert_eq!(
            parse_dns_configuration(data),
            DnsConfiguration {
                servers: vec!["127.0.0.1".into()],
                port: Some(20554),
            }
        );
    }

    #[test]
    fn update_preserves_other_dns_fields() {
        let script = update_script(
            "SERVICE-A",
            &DnsConfiguration {
                servers: vec!["127.0.0.1".into()],
                port: Some(20554),
            },
        );
        assert!(
            script.starts_with(
                "d.init\nget /NetworkServices/SERVICE-A/DNS\nd.remove ServerAddresses\n"
            )
        );
        assert!(script.contains("d.add ServerAddresses * 127.0.0.1\n"));
        assert!(script.contains("d.add ServerPort # 20554\n"));
        assert!(script.ends_with("commit\napply\nquit\n"));
    }
}
