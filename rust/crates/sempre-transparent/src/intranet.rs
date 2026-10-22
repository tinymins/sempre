use ipnet::IpNet;
use sempre_converter::Profile;
use serde_json::{Value, json};

use crate::{TransparentError, prefix};

pub(crate) struct Capture {
    pub exclusions: Vec<String>,
    pub prefixes: Vec<String>,
}

pub(crate) fn resolve(
    profile: &Profile,
    config: &Value,
    upstreams: &[String],
) -> Result<Capture, TransparentError> {
    let mut exclusions = profile.transparent_proxy.route_exclusions.clone();
    exclusions.extend(
        upstreams
            .iter()
            .filter_map(|value| prefix::host_prefix(value)),
    );
    let mut connected = Vec::new();
    let tun_addresses = config["inbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|inbound| inbound["type"] == "tun")
        .flat_map(|inbound| inbound["address"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter_map(|value| value.parse::<IpNet>().ok())
        .map(|network| network.addr())
        .collect::<Vec<_>>();
    let mut managed_interfaces = Vec::new();
    for (name, interface) in &sysinfo::Networks::new_with_refreshed_list() {
        if interface
            .ip_networks()
            .iter()
            .any(|network| tun_addresses.contains(&network.addr))
        {
            managed_interfaces.push(name.clone());
        }
        for network in interface.ip_networks() {
            exclusions.extend(prefix::host_prefix(&network.addr.to_string()));
            connected.push(format!("{}/{}", network.addr, network.prefix));
            if let Ok(IpNet::V4(network)) = connected
                .last()
                .expect("connected network")
                .parse::<IpNet>()
                && network.prefix_len() < 31
            {
                exclusions.extend(prefix::host_prefix(&network.broadcast().to_string()));
            }
        }
    }
    for route in sempre_network::route_table()? {
        if cfg!(target_os = "macos")
            && profile.transparent_proxy.auto_exclude_vpn_routes
            && let Some(name) = &route.interface
            && !managed_interfaces.contains(name)
            && ["utun", "tun", "tap", "wg", "tailscale", "zt", "ppp"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            exclusions.push(route.destination.clone());
        }
        if let Some(gateway) = route.gateway {
            exclusions.extend(prefix::host_prefix(
                gateway.split('%').next().unwrap_or_default(),
            ));
        }
    }
    collect_servers(config, &mut exclusions);
    // TUN interface addresses are control-plane endpoints, never proxy destinations.
    for inbound in config["inbounds"].as_array().into_iter().flatten() {
        for address in inbound["address"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            exclusions.push(address.to_owned());
        }
    }
    let exclusions = prefix::normalized(exclusions);
    let ranges = sempre_converter::INTRANET_CAPTURE_PREFIXES
        .iter()
        .filter_map(|value| value.parse::<IpNet>().ok())
        .collect::<Vec<_>>();
    let mut captures = ranges.clone();
    // Connected subnet routes outrank the broad RFC1918/ULA routes. Split them
    // once so capture wins without replacing the host's existing connected route.
    for network in connected
        .iter()
        .filter_map(|value| value.parse::<IpNet>().ok())
    {
        if ranges.iter().any(|range| range.contains(&network.trunc()))
            && let Ok(subnets) = network.trunc().subnets(network.prefix_len() + 1)
        {
            captures.extend(subnets);
        }
    }
    let prefixes = subtract(captures, &exclusions);
    Ok(Capture {
        exclusions,
        prefixes,
    })
}

pub(crate) fn apply(config: &mut Value, capture: &Capture, full_route: bool) {
    for tun in config["inbounds"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter(|inbound| inbound["type"] == "tun")
    {
        let mut exclusions = capture.exclusions.clone();
        exclusions.extend(
            tun["address"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        let mut addresses = tun["route_address"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|value| value.parse::<IpNet>().ok())
            .collect::<Vec<_>>();
        if addresses.is_empty() && full_route {
            addresses.extend(
                ["0.0.0.0/0", "::/0"].map(|value| value.parse::<IpNet>().expect("default prefix")),
            );
        }
        addresses.extend(
            capture
                .prefixes
                .iter()
                .filter_map(|value| value.parse::<IpNet>().ok()),
        );
        tun["route_address"] = json!(subtract(addresses, &exclusions));
        let mut excluded = tun["route_exclude_address"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        excluded.extend(exclusions);
        tun["route_exclude_address"] = json!(prefix::normalized(excluded));
    }
}

pub(crate) fn apply_mihomo(config: &mut Value, capture: &Capture) {
    let mut addresses = config["tun"]["route-address"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|value| value.parse::<IpNet>().ok())
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        addresses.extend(
            ["0.0.0.0/0", "::/0"].map(|value| value.parse::<IpNet>().expect("default prefix")),
        );
    }
    addresses.extend(
        capture
            .prefixes
            .iter()
            .filter_map(|value| value.parse::<IpNet>().ok()),
    );
    config["tun"]["route-address"] = json!(subtract(addresses, &capture.exclusions));
}

pub(crate) fn subtract(mut networks: Vec<IpNet>, exclusions: &[String]) -> Vec<String> {
    for excluded in exclusions
        .iter()
        .filter_map(|value| value.parse::<IpNet>().ok())
    {
        networks = networks
            .into_iter()
            .flat_map(|network| subtract_one(network, excluded))
            .collect();
    }
    // Do not aggregate: more-specific connected captures must remain more specific.
    networks.sort();
    networks.dedup();
    networks
        .into_iter()
        .map(|network| network.trunc().to_string())
        .collect()
}

fn subtract_one(network: IpNet, excluded: IpNet) -> Vec<IpNet> {
    if excluded.contains(&network) {
        return Vec::new();
    }
    if !network.contains(&excluded) {
        return vec![network];
    }
    network
        .subnets(network.prefix_len() + 1)
        .expect("non-host network can split")
        .flat_map(|child| subtract_one(child, excluded))
        .collect()
}

fn collect_servers(value: &Value, prefixes: &mut Vec<String>) {
    for key in ["outbounds", "proxies", "endpoints"] {
        for server in value[key].as_array().into_iter().flatten() {
            server_addresses(server, prefixes);
        }
    }
    for server in value["dns"]["servers"].as_array().into_iter().flatten() {
        if server.get("detour").is_none_or(|detour| {
            detour == "direct"
                || value["outbounds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|outbound| outbound["type"] == "direct" && &outbound["tag"] == detour)
        }) {
            server_addresses(server, prefixes);
        }
    }
    for key in [
        "nameserver",
        "default-nameserver",
        "direct-nameserver",
        "fallback",
    ] {
        for server in value["dns"][key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            prefixes.extend(server_prefix(server));
        }
    }
}

fn server_addresses(value: &Value, prefixes: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "server" | "address")
                    && let Some(value) = value.as_str()
                {
                    prefixes.extend(server_prefix(value));
                } else if matches!(key.as_str(), "settings" | "servers" | "vnext" | "peers") {
                    server_addresses(value, prefixes);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                server_addresses(value, prefixes);
            }
        }
        _ => {}
    }
}

fn server_prefix(value: &str) -> Option<String> {
    let authority = value
        .split_once("://")
        .map_or(value, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let address = authority.parse::<std::net::SocketAddr>().map_or_else(
        |_| authority.trim_matches(['[', ']']).to_owned(),
        |address| address.ip().to_string(),
    );
    prefix::host_prefix(&address)
}
