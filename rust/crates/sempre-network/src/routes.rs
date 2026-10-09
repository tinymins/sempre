use std::{io, net::IpAddr, process::Command};

#[cfg(target_os = "linux")]
use serde_json::Value;

use crate::NetworkError;

pub fn route_prefixes() -> Result<Vec<String>, NetworkError> {
    route_prefixes_excluding(&[])
}

/// Exclude routes owned by interface names (Unix) or interface addresses (Windows).
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
    let mut routes =
        parse_windows_ipv4_routes(&command_output("route.exe", &["PRINT", "-4"])?, interfaces);
    routes.extend(parse_windows_ipv6_routes(
        &command_output("route.exe", &["PRINT", "-6"])?,
        interfaces,
    ));
    Ok(routes)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_routes(_interfaces: &[String]) -> Result<Vec<String>, NetworkError> {
    Ok(Vec::new())
}

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

#[cfg(target_os = "windows")]
fn parse_windows_ipv4_routes(output: &str, interfaces: &[String]) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 5 || interfaces.iter().any(|interface| interface == fields[3]) {
                return None;
            }
            let address = fields[0].parse::<std::net::Ipv4Addr>().ok()?;
            let mask = u32::from(fields[1].parse::<std::net::Ipv4Addr>().ok()?);
            let prefix = u8::try_from(mask.count_ones()).ok()?;
            let expected = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (mask == expected).then(|| format!("{address}/{prefix}"))
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn parse_windows_ipv6_routes(output: &str, interfaces: &[String]) -> Vec<String> {
    let indexes = output
        .lines()
        .filter_map(|line| {
            let (index, rest) = line.trim().split_once("...")?;
            let name = rest.rsplit("...").next()?.trim();
            interfaces
                .iter()
                .any(|interface| interface == name)
                .then(|| index.trim().to_owned())
        })
        .collect::<Vec<_>>();
    output
        .lines()
        .filter(|line| {
            !indexes
                .iter()
                .any(|index| line.split_whitespace().next() == Some(index))
        })
        .flat_map(|line| line.split_whitespace())
        .filter_map(parse_route_destination)
        .filter(|route| route.contains(':'))
        .collect()
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
