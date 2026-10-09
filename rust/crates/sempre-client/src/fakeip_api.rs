use std::{fs, sync::Arc};

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::Networks;

use crate::api::{AppState, api_error};

#[derive(Deserialize)]
pub(crate) struct CheckInput {
    range: String,
}

#[derive(Debug, Serialize)]
struct RangeCheck {
    conflicts: Vec<String>,
    recommendation: Option<RangeRecommendation>,
}

#[derive(Debug, Serialize)]
struct RangeRecommendation {
    range: String,
    private_network_fallback: bool,
}

pub(crate) async fn check(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CheckInput>,
) -> Response {
    let Ok(range) = input.range.trim().parse::<IpNet>() else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_FAKE_IP_RANGE",
            "Invalid FakeIP CIDR",
        );
    };
    match tokio::task::spawn_blocking(move || inspect(&state, range)).await {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FAKE_IP_CHECK_FAILED",
            error,
        ),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FAKE_IP_CHECK_FAILED",
            error.to_string(),
        ),
    }
}

fn inspect(state: &AppState, range: IpNet) -> Result<RangeCheck, String> {
    if !cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) {
        return Err("system route inspection is unavailable".into());
    }
    let mut interfaces = Vec::new();
    let status = state
        .manager
        .runtime_status()
        .map_err(|error| error.to_string())?;
    if status.pid != 0 {
        let document = state.manager.state().map_err(|error| error.to_string())?;
        if let Some(path) = document.runtime.runtime_config {
            let data = fs::read(path).map_err(|error| error.to_string())?;
            let config =
                serde_yaml::from_slice::<Value>(&data).map_err(|error| error.to_string())?;
            interfaces = managed_interfaces(&config, &Networks::new_with_refreshed_list());
        }
    }
    let routes = sempre_network::route_prefixes_excluding(&interfaces)
        .map_err(|error| error.to_string())?
        .iter()
        .filter_map(|route| route.parse::<IpNet>().ok())
        .filter(|route| route.prefix_len() > 0)
        .collect::<Vec<_>>();
    Ok(check_range(range, &routes))
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

fn check_range(range: IpNet, routes: &[IpNet]) -> RangeCheck {
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
    RangeCheck {
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
