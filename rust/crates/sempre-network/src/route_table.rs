#[cfg(not(target_os = "windows"))]
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;

use serde::Serialize;

use crate::NetworkError;

#[derive(Debug, Serialize)]
pub struct SystemRoute {
    pub family: &'static str,
    pub destination: String,
    pub gateway: Option<String>,
    pub interface: Option<String>,
    pub metric: Option<u64>,
    pub kind: Option<String>,
    pub table: Option<String>,
}

/// Read the host routing table without changing network state.
pub fn route_table() -> Result<Vec<SystemRoute>, NetworkError> {
    platform_routes()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn output(program: &str, arguments: &[&str]) -> Result<String, NetworkError> {
    let result = Command::new(program).args(arguments).output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "{program} exited with {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ))
        .into());
    }
    String::from_utf8(result.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
}

#[cfg(target_os = "macos")]
fn platform_routes() -> Result<Vec<SystemRoute>, NetworkError> {
    let mut routes = Vec::new();
    for (family, protocol) in [("ipv4", "inet"), ("ipv6", "inet6")] {
        let text = output("/usr/sbin/netstat", &["-rn", "-f", protocol])?;
        let mut interface_column = None;
        for line in text.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.first() == Some(&"Destination") {
                interface_column = fields.iter().position(|field| *field == "Netif");
                continue;
            }
            let Some(index) = interface_column else {
                continue;
            };
            let Some(interface) = fields.get(index) else {
                continue;
            };
            if fields.len() < 3 {
                continue;
            }
            let destination = normalize_destination(fields[0], family);
            routes.push(SystemRoute {
                family,
                destination,
                gateway: Some(fields[1].to_owned()),
                interface: Some((*interface).to_owned()),
                metric: None,
                kind: Some(fields[2].to_owned()),
                table: None,
            });
        }
    }
    Ok(routes)
}

#[cfg(target_os = "macos")]
fn normalize_destination(value: &str, family: &str) -> String {
    if value == "default" {
        return if family == "ipv4" {
            "0.0.0.0/0"
        } else {
            "::/0"
        }
        .to_owned();
    }
    if family == "ipv6" {
        return if value.contains('/') {
            value.to_owned()
        } else {
            format!("{value}/128")
        };
    }
    let implicit_prefix = (value.split('.').count() * 8).to_string();
    let (address, prefix) = value.split_once('/').unwrap_or((value, &implicit_prefix));
    let mut octets: Vec<_> = address.split('.').collect();
    while octets.len() < 4 {
        octets.push("0");
    }
    format!("{}/{prefix}", octets.join("."))
}

#[cfg(target_os = "linux")]
fn platform_routes() -> Result<Vec<SystemRoute>, NetworkError> {
    let mut routes = Vec::new();
    for (family, flag, default) in [("ipv4", "-4", "0.0.0.0/0"), ("ipv6", "-6", "::/0")] {
        let text = output("ip", &["-j", flag, "route", "show", "table", "all"])?;
        let entries: Vec<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        for entry in entries {
            let destination = entry["dst"].as_str().unwrap_or("default");
            let hops = entry["nexthops"].as_array();
            let next_hops: Vec<_> = hops.map_or_else(|| vec![&entry], |hops| hops.iter().collect());
            for hop in next_hops {
                routes.push(SystemRoute {
                    family,
                    destination: if destination == "default" {
                        default.to_owned()
                    } else if destination.contains('/') {
                        destination.to_owned()
                    } else {
                        format!("{destination}/{}", if family == "ipv4" { 32 } else { 128 })
                    },
                    gateway: hop["gateway"].as_str().map(str::to_owned),
                    interface: hop["dev"].as_str().map(str::to_owned),
                    metric: entry["metric"].as_u64(),
                    kind: entry["type"].as_str().map(str::to_owned),
                    table: entry.get("table").map(|table| {
                        table
                            .as_str()
                            .map_or_else(|| table.to_string(), str::to_owned)
                    }),
                });
            }
        }
    }
    Ok(routes)
}

#[cfg(target_os = "windows")]
fn platform_routes() -> Result<Vec<SystemRoute>, NetworkError> {
    let adapters = crate::windows::adapters()?;
    Ok(crate::windows::routes()?
        .into_iter()
        .map(|route| SystemRoute {
            family: if route.ipv6 { "ipv6" } else { "ipv4" },
            destination: route.destination,
            gateway: Some(route.gateway),
            interface: adapters
                .iter()
                .find(|adapter| adapter.index == route.index || adapter.ipv6_index == route.index)
                .map(|adapter| adapter.name.clone()),
            metric: Some(u64::from(route.metric)),
            kind: None,
            table: None,
        })
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_routes() -> Result<Vec<SystemRoute>, NetworkError> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "routing table inspection is unsupported on this platform",
    )
    .into())
}
