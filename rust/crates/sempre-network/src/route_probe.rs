use std::{net::IpAddr, process::Command};

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RouteDecision {
    pub address: IpAddr,
    pub interface: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gateway: String,
}

pub fn inspect(addresses: &[IpAddr]) -> (Vec<RouteDecision>, Vec<RouteDecision>) {
    let routes = addresses
        .iter()
        .filter_map(|address| route_for(*address))
        .collect();
    let fake_ip_samples = ["198.18.0.1", "198.19.0.1"]
        .into_iter()
        .filter_map(|address| address.parse().ok())
        .filter_map(route_for)
        .collect();
    (routes, fake_ip_samples)
}

#[cfg(target_os = "macos")]
fn route_for(address: IpAddr) -> Option<RouteDecision> {
    let output = Command::new("/sbin/route")
        .args(["-n", "get", &address.to_string()])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_macos_route(address, &String::from_utf8_lossy(&output.stdout)))
        .flatten()
}

#[cfg(target_os = "linux")]
fn route_for(address: IpAddr) -> Option<RouteDecision> {
    let family = if address.is_ipv4() { "-4" } else { "-6" };
    let output = Command::new("ip")
        .args([family, "route", "get", &address.to_string()])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_linux_route(address, &String::from_utf8_lossy(&output.stdout)))
        .flatten()
}

#[cfg(target_os = "windows")]
fn route_for(address: IpAddr) -> Option<RouteDecision> {
    let IpAddr::V4(address) = address else {
        return None;
    };
    let output = Command::new("route.exe")
        .args(["PRINT", "-4"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_windows_route(address, &String::from_utf8_lossy(&output.stdout)))
        .flatten()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn route_for(_address: IpAddr) -> Option<RouteDecision> {
    None
}

fn field(output: &str, key: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (candidate, value) = line.trim().split_once(':')?;
        (candidate == key).then(|| value.trim().to_owned())
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_route(address: IpAddr, output: &str) -> Option<RouteDecision> {
    Some(RouteDecision {
        address,
        interface: field(output, "interface")?,
        gateway: field(output, "gateway").unwrap_or_default(),
    })
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_route(address: IpAddr, output: &str) -> Option<RouteDecision> {
    let fields = output.split_whitespace().collect::<Vec<_>>();
    let value_after = |key| {
        fields
            .iter()
            .position(|value| *value == key)
            .and_then(|index| fields.get(index + 1))
            .map(|value| (*value).to_owned())
    };
    Some(RouteDecision {
        address,
        interface: value_after("dev")?,
        gateway: value_after("via").unwrap_or_default(),
    })
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows_route(address: std::net::Ipv4Addr, output: &str) -> Option<RouteDecision> {
    let target = u32::from(address);
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 5 {
                return None;
            }
            let destination = fields[0].parse::<std::net::Ipv4Addr>().ok()?;
            let mask = fields[1].parse::<std::net::Ipv4Addr>().ok()?;
            let mask = u32::from(mask);
            (target & mask == u32::from(destination) & mask).then_some((
                mask.count_ones(),
                fields[4].parse::<u32>().ok()?,
                RouteDecision {
                    address: IpAddr::V4(address),
                    interface: fields[3].to_owned(),
                    gateway: fields[2].to_owned(),
                },
            ))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)))
        .map(|(_, _, route)| route)
}

pub fn fake_ip_routes_conflict(routes: &[RouteDecision]) -> bool {
    routes.len() > 1
        && routes.windows(2).any(|pair| {
            pair[0].interface != pair[1].interface || pair[0].gateway != pair[1].gateway
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_platform_route_outputs() {
        let target = "198.18.0.67".parse().expect("target");
        assert_eq!(
            parse_macos_route(
                target,
                "route to: 198.18.0.67\ngateway: 10.251.1.1\ninterface: utun4\n"
            ),
            Some(RouteDecision {
                address: target,
                interface: "utun4".into(),
                gateway: "10.251.1.1".into(),
            })
        );
        assert_eq!(
            parse_linux_route(target, "198.18.0.67 via 10.0.0.1 dev tun0 src 10.0.0.2"),
            Some(RouteDecision {
                address: target,
                interface: "tun0".into(),
                gateway: "10.0.0.1".into(),
            })
        );
        let windows =
            "0.0.0.0 0.0.0.0 10.0.0.1 10.0.0.2 25\n198.18.0.0 255.255.0.0 10.251.1.1 10.251.1.1 5";
        assert_eq!(
            parse_windows_route("198.18.0.67".parse().expect("IPv4"), windows)
                .expect("Windows route")
                .gateway,
            "10.251.1.1"
        );
    }

    #[test]
    fn detects_split_fake_ip_route_ownership() {
        let routes = vec![
            RouteDecision {
                address: "198.18.0.1".parse().expect("address"),
                interface: "utun4".into(),
                gateway: "10.251.1.1".into(),
            },
            RouteDecision {
                address: "198.19.0.1".parse().expect("address"),
                interface: "utun5".into(),
                gateway: "172.19.0.1".into(),
            },
        ];
        assert!(fake_ip_routes_conflict(&routes));
    }
}
