use crate::{Manager, VersionRunner};
use ipnet::IpNet;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    sync::Mutex,
    time::{Duration, Instant},
};
use sysinfo::Networks;

#[derive(Debug, Serialize)]
pub struct FakeIpRangeCheck {
    pub conflicts: Vec<String>,
    pub recommendation: Option<RangeRecommendation>,
}
#[derive(Debug, Serialize)]
pub struct RangeRecommendation {
    pub range: String,
    pub private_network_fallback: bool,
}
#[derive(Default)]
pub(crate) struct RouteCache(Mutex<Option<RouteSnapshot>>);
struct RouteSnapshot {
    sampled_at: Instant,
    runtime_identity: String,
    routes: Result<Vec<IpNet>, String>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn check_fakeip_range(&self, range: IpNet) -> Result<FakeIpRangeCheck, String> {
        Ok(check_range(range, &self.fakeip_system_routes()?))
    }
    pub(crate) fn fakeip_system_routes(&self) -> Result<Vec<IpNet>, String> {
        if !cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        )) {
            return Err("system route inspection is unavailable".into());
        }
        let document = self.store.read().map_err(|error| error.to_string())?;
        let identity = format!(
            "{:?}|{:?}",
            document.runtime.pid, document.runtime.runtime_config_hash
        );
        let mut cache = self.fakeip_route_cache.0.lock().expect("route cache lock");
        if let Some(snapshot) = cache.as_ref()
            && snapshot.runtime_identity == identity
            && snapshot.sampled_at.elapsed() < Duration::from_secs(10)
        {
            return snapshot.routes.clone();
        }
        let routes = (|| {
            let mut interfaces = Vec::new();
            if document
                .runtime
                .pid
                .is_some_and(crate::runtime::process_alive)
                && let Some(path) = document.runtime.runtime_config.as_deref()
            {
                let data = fs::read(path).map_err(|error| error.to_string())?;
                let config =
                    serde_yaml::from_slice::<Value>(&data).map_err(|error| error.to_string())?;
                interfaces = managed_interfaces(&config, &Networks::new_with_refreshed_list());
            }
            sempre_network::route_prefixes_excluding(&interfaces)
                .map(|routes| {
                    routes
                        .iter()
                        .filter_map(|route| route.parse::<IpNet>().ok())
                        .filter(|route| route.prefix_len() > 0)
                        .collect()
                })
                .map_err(|error| error.to_string())
        })();
        *cache = Some(RouteSnapshot {
            sampled_at: Instant::now(),
            runtime_identity: identity,
            routes: routes.clone(),
        });
        routes
    }
}

fn managed_interfaces(config: &Value, networks: &Networks) -> Vec<String> {
    let mut names = Vec::new();
    let mut addresses = Vec::new();
    if let Some(tun) = config.get("tun")
        && tun.get("enable").and_then(Value::as_bool) == Some(true)
        && let Some(name) = tun.get("device").and_then(Value::as_str)
        && !name.is_empty()
    {
        names.push(name.to_owned());
    }
    for inbound in config
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if inbound.get("type").and_then(Value::as_str) != Some("tun")
            && inbound.get("protocol").and_then(Value::as_str) != Some("tun")
        {
            continue;
        }
        let tun = inbound.get("settings").unwrap_or(inbound);
        for key in ["interface_name", "name"] {
            if let Some(name) = tun
                .get(key)
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
            {
                names.push(name.to_owned());
            }
        }
        for key in ["address", "gateway"] {
            for address in tun.get(key).and_then(Value::as_array).into_iter().flatten() {
                if let Some(address) = address
                    .as_str()
                    .and_then(|value| value.parse::<IpNet>().ok())
                {
                    addresses.push(address.addr());
                }
            }
        }
    }
    let mut identifiers = names.clone();
    for (name, data) in networks {
        if names.contains(name)
            || data
                .ip_networks()
                .iter()
                .any(|network| addresses.contains(&network.addr))
        {
            identifiers.push(name.clone());
            identifiers.extend(
                data.ip_networks()
                    .iter()
                    .map(|network| network.addr.to_string()),
            );
        }
    }
    identifiers
}

fn overlaps(left: IpNet, right: IpNet) -> bool {
    left.contains(&right.network()) || right.contains(&left.network())
}

pub(crate) fn check_range(range: IpNet, routes: &[IpNet]) -> FakeIpRangeCheck {
    let conflicts = routes
        .iter()
        .filter(|route| overlaps(range, **route))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let recommendation = if conflicts.is_empty() {
        None
    } else {
        let pools: &[(&str, u8, u8, bool)] = if range.addr().is_ipv4() {
            &[
                ("198.18.0.0/15", 15, 24, false),
                ("172.16.0.0/12", 16, 24, true),
                ("10.0.0.0/8", 16, 24, true),
                ("192.168.0.0/16", 16, 24, true),
            ]
        } else {
            &[("fd00::/8", 18, 64, false)]
        };
        pools.iter().find_map(|(pool, minimum, maximum, fallback)| {
            let pool = pool.parse::<IpNet>().expect("constant CIDR");
            pool.subnets(*minimum)
                .ok()?
                .filter_map(|subnet| largest_free_subnet(subnet, routes, *maximum))
                .reduce(|left, right| {
                    if right.prefix_len() < left.prefix_len() {
                        right
                    } else {
                        left
                    }
                })
                .map(|net| RangeRecommendation {
                    range: net.to_string(),
                    private_network_fallback: *fallback,
                })
        })
    };
    FakeIpRangeCheck {
        conflicts,
        recommendation,
    }
}

fn largest_free_subnet(pool: IpNet, routes: &[IpNet], maximum: u8) -> Option<IpNet> {
    let conflicts = routes
        .iter()
        .filter(|route| overlaps(pool, **route))
        .copied()
        .collect::<Vec<_>>();
    if conflicts.is_empty() {
        return Some(pool);
    }
    if pool.prefix_len() >= maximum
        || conflicts
            .iter()
            .any(|route| route.contains(&pool.network()) && route.prefix_len() <= pool.prefix_len())
    {
        return None;
    }
    pool.subnets(pool.prefix_len() + 1)
        .ok()?
        .filter_map(|child| largest_free_subnet(child, &conflicts, maximum))
        .reduce(|left, right| {
            if right.prefix_len() < left.prefix_len() {
                right
            } else {
                left
            }
        })
}
