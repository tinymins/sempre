use std::{collections::BTreeSet, fs, path::Path};

use ipnet::IpNet;
use serde_json::Value;

#[derive(Debug, Default, Eq, PartialEq)]
struct RouteRefinement {
    additions: Vec<IpNet>,
    unresolved: Vec<IpNet>,
}

pub(crate) fn adapt_runtime_config(path: &Path) -> Vec<String> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) => {
            return vec![format!(
                "inspect FakeIP routes: read runtime config: {error}"
            )];
        }
    };
    let mut document = match serde_json::from_slice::<Value>(&data) {
        Ok(document) => document,
        Err(error) => {
            return vec![format!(
                "inspect FakeIP routes: decode runtime config: {error}"
            )];
        }
    };
    let Some(route_addresses) = tun_route_addresses(&document) else {
        return Vec::new();
    };
    let fakeip_ranges = runtime_fakeip_ranges(&document);
    if fakeip_ranges.is_empty() {
        return Vec::new();
    }
    let configured = route_addresses
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|value| value.parse::<IpNet>().ok())
        .collect::<BTreeSet<_>>();
    let owned = fakeip_ranges
        .iter()
        .filter(|range| configured.contains(range))
        .copied()
        .collect::<Vec<_>>();
    if owned.is_empty() {
        return Vec::new();
    }
    let system_routes = match sempre_network::route_prefixes() {
        Ok(routes) => routes
            .into_iter()
            .filter_map(|route| route.parse::<IpNet>().ok())
            .collect::<Vec<_>>(),
        Err(error) => {
            return vec![format!(
                "FakeIP route conflict detection unavailable; keeping configured routes: {error}"
            )];
        }
    };
    let refinement = refine_routes(&owned, &system_routes);
    let mut diagnostics = Vec::new();
    if !refinement.additions.is_empty() {
        let additions = refinement
            .additions
            .iter()
            .filter(|route| !configured.contains(route))
            .copied()
            .collect::<Vec<_>>();
        if !additions.is_empty() {
            if let Some(addresses) = tun_route_addresses_mut(&mut document) {
                addresses.extend(
                    additions
                        .iter()
                        .map(|route| Value::String(route.to_string())),
                );
            }
            match serde_json::to_vec_pretty(&document)
                .map_err(|error| error.to_string())
                .and_then(|data| {
                    sempre_state::write_atomic(path, &data, 0o600)
                        .map_err(|error| error.to_string())
                }) {
                Ok(()) => diagnostics.push(format!(
                    "refined FakeIP routes around system conflicts: {}",
                    join_routes(&additions)
                )),
                Err(error) => diagnostics.push(format!(
                    "could not refine FakeIP routes; keeping configured routes: {error}"
                )),
            }
        }
    }
    if !refinement.unresolved.is_empty() {
        diagnostics.push(format!(
            "system host routes cannot be out-ranked by subnet refinement: {}",
            join_routes(&refinement.unresolved)
        ));
    }
    diagnostics
}

fn tun_route_addresses(document: &Value) -> Option<&Vec<Value>> {
    document
        .get("inbounds")?
        .as_array()?
        .iter()
        .find(|inbound| {
            inbound.get("type").and_then(Value::as_str) == Some("tun")
                && inbound.get("tag").and_then(Value::as_str) == Some("tun-in")
        })?
        .get("route_address")?
        .as_array()
}

fn tun_route_addresses_mut(document: &mut Value) -> Option<&mut Vec<Value>> {
    document
        .get_mut("inbounds")?
        .as_array_mut()?
        .iter_mut()
        .find(|inbound| {
            inbound.get("type").and_then(Value::as_str) == Some("tun")
                && inbound.get("tag").and_then(Value::as_str) == Some("tun-in")
        })?
        .get_mut("route_address")?
        .as_array_mut()
}

pub(crate) fn runtime_fakeip_ranges(document: &Value) -> Vec<IpNet> {
    let mut ranges = BTreeSet::new();
    if let Some(fakeip) = document.pointer("/dns/fakeip")
        && fakeip.get("enabled").and_then(Value::as_bool) != Some(false)
    {
        collect_ranges(fakeip, &mut ranges);
    }
    if let Some(servers) = document.pointer("/dns/servers").and_then(Value::as_array) {
        for server in servers.iter().filter(|server| {
            server.get("type").and_then(Value::as_str) == Some("fakeip")
                || server.get("address").and_then(Value::as_str) == Some("fakeip")
        }) {
            collect_ranges(server, &mut ranges);
        }
    }
    if document
        .pointer("/dns/enhanced-mode")
        .and_then(Value::as_str)
        == Some("fake-ip")
    {
        for key in ["fake-ip-range", "fake-ip-range6"] {
            if let Some(range) = document["dns"][key]
                .as_str()
                .and_then(|range| range.parse().ok())
            {
                ranges.insert(range);
            }
        }
    }
    ranges.into_iter().collect()
}

fn collect_ranges(value: &Value, ranges: &mut BTreeSet<IpNet>) {
    for key in ["inet4_range", "inet6_range"] {
        if let Some(range) = value
            .get(key)
            .and_then(Value::as_str)
            .and_then(|range| range.parse().ok())
        {
            ranges.insert(range);
        }
    }
}

fn refine_routes(fakeip_ranges: &[IpNet], system_routes: &[IpNet]) -> RouteRefinement {
    let mut additions = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for fakeip in fakeip_ranges {
        for route in system_routes {
            if matches!(fakeip, IpNet::V4(_)) != matches!(route, IpNet::V4(_))
                || route.prefix_len() < fakeip.prefix_len()
                || !fakeip.contains(&route.network())
            {
                continue;
            }
            let maximum = if matches!(route, IpNet::V4(_)) {
                32
            } else {
                128
            };
            if route.prefix_len() == maximum {
                unresolved.insert(*route);
                continue;
            }
            if let Ok(children) = route.subnets(route.prefix_len() + 1) {
                additions.extend(children);
            }
        }
    }
    RouteRefinement {
        additions: additions.into_iter().collect(),
        unresolved: unresolved.into_iter().collect(),
    }
}

fn join_routes(routes: &[IpNet]) -> String {
    routes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn networks(values: &[&str]) -> Vec<IpNet> {
        values
            .iter()
            .map(|value| value.parse().expect("network"))
            .collect()
    }

    #[test]
    fn refines_only_the_conflicting_part_of_a_fakeip_range() {
        let result = refine_routes(
            &networks(&["198.18.0.0/15", "fc00::/18"]),
            &networks(&[
                "10.0.0.0/8",
                "198.0.0.0/8",
                "198.18.0.0/16",
                "198.18.64.0/18",
            ]),
        );

        assert_eq!(
            result.additions,
            networks(&[
                "198.18.0.0/17",
                "198.18.64.0/19",
                "198.18.96.0/19",
                "198.18.128.0/17",
            ])
        );
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn reports_host_routes_that_cannot_be_out_ranked() {
        let result = refine_routes(
            &networks(&["198.18.0.0/15", "fc00::/18"]),
            &networks(&["198.18.0.67/32", "fc00::10/128"]),
        );

        assert!(result.additions.is_empty());
        assert_eq!(
            result.unresolved,
            networks(&["198.18.0.67/32", "fc00::10/128"])
        );
    }
}
