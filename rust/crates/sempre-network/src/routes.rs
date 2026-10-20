use std::net::IpAddr;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{io, process::Command};

#[cfg(target_os = "linux")]
use serde_json::Value;

use crate::NetworkError;

pub fn route_prefixes() -> Result<Vec<String>, NetworkError> {
    route_prefixes_excluding(&[])
}

/// Exclude routes by interface names (Unix) or native adapter names/addresses (Windows).
pub fn route_prefixes_excluding(interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    let mut routes = platform_routes(interfaces)?;
    routes.sort();
    routes.dedup();
    Ok(routes)
}

#[cfg(target_os = "macos")]
fn platform_routes(interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    let mut routes = parse_macos_routes(
        &command_output("/usr/sbin/netstat", &["-rn", "-f", "inet"])?,
        interfaces,
    );
    routes.extend(parse_macos_routes(
        &command_output("/usr/sbin/netstat", &["-rn", "-f", "inet6"])?,
        interfaces,
    ));
    Ok(routes)
}

#[cfg(target_os = "linux")]
fn platform_routes(interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    let mut routes = parse_linux_routes(
        &command_output("ip", &["-j", "-4", "route", "show", "table", "all"])?,
        interfaces,
    )?;
    routes.extend(parse_linux_routes(
        &command_output("ip", &["-j", "-6", "route", "show", "table", "all"])?,
        interfaces,
    )?);
    Ok(routes)
}

#[cfg(target_os = "windows")]
fn platform_routes(interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    let has_names = interfaces
        .iter()
        .any(|interface| interface.parse::<IpAddr>().is_err());
    let owned = crate::windows::adapters()?
        .into_iter()
        .filter(|adapter| {
            interfaces.iter().any(|interface| {
                interface == &adapter.name
                    || interface
                        .trim_matches(['{', '}'])
                        .eq_ignore_ascii_case(&adapter.guid)
                    || (!has_names
                        && interface.parse::<IpAddr>().is_ok_and(|ip| {
                            adapter.addresses.iter().any(|address| {
                                address
                                    .split('/')
                                    .next()
                                    .and_then(|value| value.parse::<IpAddr>().ok())
                                    == Some(ip)
                            })
                        }))
            })
        })
        .collect::<Vec<_>>();
    Ok(crate::windows::routes()?
        .into_iter()
        .filter(|route| {
            !owned.iter().any(|adapter| {
                if route.ipv6 {
                    adapter.ipv6_index == route.index
                } else {
                    adapter.index == route.index
                }
            })
        })
        .filter_map(|route| parse_route_destination(&route.destination))
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_routes(_interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    Ok(Vec::new())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn command_output(program: &str, arguments: &[&str]) -> Result<String, NetworkError> {
    let output = Command::new(program).args(arguments).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{program} {} exited with {}",
            arguments.join(" "),
            output.status
        ))
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn parse_macos_routes(output: &str, interfaces: &[String]) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let flags = *fields.get(2)?;
            (!flags.contains('W')
                && !interfaces
                    .iter()
                    .any(|interface| Some(&interface.as_str()) == fields.get(3)))
            .then(|| parse_route_destination(fields.first()?))
            .flatten()
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn parse_linux_routes(output: &str, interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    let routes = serde_json::from_str::<Value>(output)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(routes
        .as_array()
        .into_iter()
        .flatten()
        .filter(|route| {
            !interfaces
                .iter()
                .any(|interface| route.get("dev").and_then(Value::as_str) == Some(interface))
        })
        .filter_map(|route| {
            route
                .get("dst")
                .and_then(Value::as_str)
                .and_then(parse_route_destination)
        })
        .collect())
}

fn parse_route_destination(value: &str) -> Option<String> {
    if value == "default" {
        return None;
    }
    if !value.contains('/') {
        let address = value.parse::<IpAddr>().ok()?;
        let prefix = if address.is_ipv4() { 32 } else { 128 };
        return Some(format!("{address}/{prefix}"));
    }
    let (address, prefix) = value.split_once('/')?;
    let address = address.split('%').next().unwrap_or(address);
    let address = if address.contains('.') {
        let mut octets = address.split('.').collect::<Vec<_>>();
        if octets.len() > 4 {
            return None;
        }
        octets.resize(4, "0");
        octets.join(".")
    } else {
        address.to_owned()
    };
    let address = address.parse::<IpAddr>().ok()?;
    let prefix = prefix.parse::<u8>().ok()?;
    let maximum = if address.is_ipv4() { 32 } else { 128 };
    (prefix <= maximum).then(|| format!("{address}/{prefix}"))
}
