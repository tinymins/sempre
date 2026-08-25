use serde::Serialize;

use crate::NetworkError;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Interface {
    pub name: String,
    pub index: u32,
    pub kind: String,
    pub up: bool,
    pub default_route: bool,
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Inventory {
    pub supported: bool,
    pub interfaces: Vec<Interface>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default_interface: String,
    pub recommended_lan_interfaces: Vec<String>,
    pub local_prefixes: Vec<String>,
    pub vpn_prefixes: Vec<String>,
    pub occupied_prefixes: Vec<String>,
}

pub fn inventory() -> Result<Inventory, NetworkError> {
    platform::inventory()
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{Inventory, NetworkError};

    #[allow(clippy::unnecessary_wraps)]
    pub fn inventory() -> Result<Inventory, NetworkError> {
        Ok(Inventory::default())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{collections::HashMap, fs, net::IpAddr, path::Path};

    use sysinfo::{InterfaceOperationalState, Networks};

    use super::{Interface, Inventory, NetworkError};

    pub fn inventory() -> Result<Inventory, NetworkError> {
        let networks = Networks::new_with_refreshed_list();
        let routes = routes()?;
        let default_interface = routes
            .iter()
            .find(|route| route.prefix == "0.0.0.0/0")
            .or_else(|| routes.iter().find(|route| route.prefix == "::/0"))
            .map_or_else(String::new, |route| route.interface.clone());
        let mut kinds = HashMap::new();
        let mut interfaces = Vec::new();
        let mut local_prefixes = Vec::new();
        let mut occupied_prefixes = Vec::new();
        for (position, (name, data)) in networks.iter().enumerate() {
            let kind = interface_kind(name);
            kinds.insert(name.clone(), kind.clone());
            let mut addresses = data
                .ip_networks()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            addresses.sort();
            for network in data.ip_networks() {
                let prefix = masked_prefix(network.addr, network.prefix);
                local_prefixes.push(prefix.clone());
                occupied_prefixes.push(prefix);
            }
            interfaces.push(Interface {
                name: name.clone(),
                index: interface_index(name).unwrap_or_else(|| {
                    u32::try_from(position)
                        .unwrap_or(u32::MAX - 1)
                        .saturating_add(1)
                }),
                kind,
                up: matches!(data.operational_state(), InterfaceOperationalState::Up),
                default_route: *name == default_interface,
                addresses,
            });
        }
        let mut vpn_prefixes = Vec::new();
        for route in routes
            .iter()
            .filter(|route| route.prefix != "0.0.0.0/0" && route.prefix != "::/0")
        {
            occupied_prefixes.push(route.prefix.clone());
            if route.local {
                local_prefixes.push(route.prefix.clone());
            }
            if kinds
                .get(&route.interface)
                .is_some_and(|kind| kind == "vpn")
            {
                vpn_prefixes.push(route.prefix.clone());
            }
        }
        interfaces.sort_by(|left, right| left.name.cmp(&right.name));
        let mut recommended_lan_interfaces = interfaces
            .iter()
            .filter(|interface| {
                interface.up
                    && !interface.default_route
                    && matches!(interface.kind.as_str(), "bridge" | "physical")
                    && interface
                        .addresses
                        .iter()
                        .any(|value| private_address(value))
            })
            .map(|interface| interface.name.clone())
            .collect::<Vec<_>>();
        recommended_lan_interfaces.sort_by_key(|name| (!name.starts_with("br"), name.clone()));
        normalize(&mut local_prefixes);
        normalize(&mut vpn_prefixes);
        normalize(&mut occupied_prefixes);
        Ok(Inventory {
            supported: true,
            interfaces,
            default_interface,
            recommended_lan_interfaces,
            local_prefixes,
            vpn_prefixes,
            occupied_prefixes,
        })
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Route {
        interface: String,
        prefix: String,
        local: bool,
    }

    fn routes() -> Result<Vec<Route>, NetworkError> {
        let mut routes = parse_ipv4_routes(&fs::read_to_string("/proc/net/route")?);
        if let Ok(data) = fs::read_to_string("/proc/net/ipv6_route") {
            routes.extend(parse_ipv6_routes(&data));
        }
        Ok(routes)
    }

    fn parse_ipv4_routes(data: &str) -> Vec<Route> {
        data.lines()
            .skip(1)
            .filter_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                let interface = (*fields.first()?).to_owned();
                let destination = ipv4_hex(fields.get(1)?)?;
                let gateway = ipv4_hex(fields.get(2)?)?;
                let mask = ipv4_hex(fields.get(7)?)?;
                let prefix = mask
                    .octets()
                    .iter()
                    .map(|byte| byte.count_ones())
                    .sum::<u32>();
                Some(Route {
                    interface,
                    prefix: format!("{destination}/{prefix}"),
                    local: gateway.is_unspecified(),
                })
            })
            .collect()
    }

    fn parse_ipv6_routes(data: &str) -> Vec<Route> {
        data.lines()
            .filter_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                let destination = ipv6_hex(fields.first()?)?;
                let prefix = u8::from_str_radix(fields.get(1)?, 16).ok()?;
                let gateway = ipv6_hex(fields.get(4)?)?;
                Some(Route {
                    interface: (*fields.get(9)?).to_owned(),
                    prefix: format!("{destination}/{prefix}"),
                    local: gateway.is_unspecified(),
                })
            })
            .collect()
    }

    fn ipv4_hex(value: &str) -> Option<std::net::Ipv4Addr> {
        let value = u32::from_str_radix(value, 16).ok()?;
        Some(std::net::Ipv4Addr::from(value.to_le_bytes()))
    }

    fn ipv6_hex(value: &str) -> Option<std::net::Ipv6Addr> {
        if value.len() != 32 {
            return None;
        }
        let value = u128::from_str_radix(value, 16).ok()?;
        Some(std::net::Ipv6Addr::from(value))
    }

    fn masked_prefix(address: IpAddr, prefix: u8) -> String {
        match address {
            IpAddr::V4(address) => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                format!(
                    "{}/{prefix}",
                    std::net::Ipv4Addr::from(u32::from(address) & mask)
                )
            }
            IpAddr::V6(address) => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                format!(
                    "{}/{prefix}",
                    std::net::Ipv6Addr::from(u128::from(address) & mask)
                )
            }
        }
    }

    fn private_address(value: &str) -> bool {
        value
            .split_once('/')
            .and_then(|(address, _)| address.parse::<IpAddr>().ok())
            .is_some_and(|address| match address {
                IpAddr::V4(address) => address.is_private(),
                IpAddr::V6(address) => address.is_unique_local(),
            })
    }

    fn interface_index(name: &str) -> Option<u32> {
        fs::read_to_string(Path::new("/sys/class/net").join(name).join("ifindex"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    fn interface_kind(name: &str) -> String {
        let root = Path::new("/sys/class/net").join(name);
        if root.join("bridge").is_dir() {
            return "bridge".into();
        }
        if ["tun", "tap", "wg", "tailscale", "zt", "ppp"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return "vpn".into();
        }
        if fs::read_link(&root).is_ok_and(|target| target.to_string_lossy().contains("/virtual/")) {
            return "virtual".into();
        }
        "physical".into()
    }

    fn normalize(values: &mut Vec<String>) {
        values.sort();
        values.dedup();
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_linux_ipv4_routes() {
            let routes = parse_ipv4_routes(
                "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0 00000000 0101A8C0 0003 0 0 100 00000000 0 0 0\nbr0 0002A8C0 00000000 0001 0 0 0 00FFFFFF 0 0 0\n",
            );
            assert_eq!(routes[0].prefix, "0.0.0.0/0");
            assert_eq!(routes[0].interface, "eth0");
            assert_eq!(routes[1].prefix, "192.168.2.0/24");
            assert!(routes[1].local);
        }

        #[test]
        fn masks_interface_addresses() {
            assert_eq!(
                masked_prefix("192.168.2.15".parse().expect("address"), 24),
                "192.168.2.0/24"
            );
            assert_eq!(
                masked_prefix("fd00::1234".parse().expect("address"), 64),
                "fd00::/64"
            );
        }
    }
}
