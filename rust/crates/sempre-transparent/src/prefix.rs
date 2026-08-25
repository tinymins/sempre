use std::net::{IpAddr, Ipv4Addr};

use ipnet::IpNet;

use crate::TransparentError;

pub(crate) fn resolve_tun_address(
    explicit: &str,
    occupied: &[String],
) -> Result<String, TransparentError> {
    let occupied = parse_all(occupied);
    if !explicit.is_empty() {
        let prefix = parse(explicit)?;
        if !matches!(prefix, IpNet::V4(value) if value.prefix_len() == 30) {
            return Err(TransparentError::Invalid(format!(
                "TUN address {explicit:?} must be an IPv4 /30 prefix"
            )));
        }
        if occupied.iter().any(|current| overlaps(&prefix, current)) {
            return Err(TransparentError::Invalid(format!(
                "TUN address {explicit} conflicts with an existing address or route"
            )));
        }
        return Ok(explicit.into());
    }
    for (base, bits) in [
        (Ipv4Addr::new(172, 19, 0, 0), 16),
        (Ipv4Addr::new(172, 20, 0, 0), 14),
        (Ipv4Addr::new(172, 24, 0, 0), 13),
        (Ipv4Addr::new(198, 18, 0, 0), 15),
    ] {
        let pool = ipnet::Ipv4Net::new(base, bits).expect("valid built-in TUN pool");
        let start = u32::from(pool.network());
        let end = u32::from(pool.broadcast());
        for candidate in (start..=end).step_by(4) {
            let network =
                ipnet::Ipv4Net::new(Ipv4Addr::from(candidate), 30).expect("aligned /30 candidate");
            let candidate = IpNet::V4(network);
            if !occupied.iter().any(|current| overlaps(&candidate, current)) {
                return Ok(format!(
                    "{}/30",
                    Ipv4Addr::from(u32::from(network.network()) + 1)
                ));
            }
        }
    }
    Err(TransparentError::Invalid(
        "no non-conflicting IPv4 /30 is available for the TUN".into(),
    ))
}

pub(crate) fn normalized(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut prefixes = values
        .into_iter()
        .filter_map(|value| value.parse::<IpNet>().ok())
        .filter(|value| value.prefix_len() != 0)
        .collect::<Vec<_>>();
    prefixes.sort_by(|left, right| {
        left.addr()
            .is_ipv6()
            .cmp(&right.addr().is_ipv6())
            .then_with(|| address_bytes(left.addr()).cmp(&address_bytes(right.addr())))
            .then_with(|| left.prefix_len().cmp(&right.prefix_len()))
    });
    let mut result: Vec<IpNet> = Vec::new();
    for prefix in prefixes {
        if result.iter().any(|existing| contains(existing, &prefix)) {
            continue;
        }
        result.push(prefix.trunc());
    }
    result.into_iter().map(|value| value.to_string()).collect()
}

pub(crate) fn filter_overlaps(values: Vec<String>, blocked: &[String]) -> Vec<String> {
    let blocked = parse_all(blocked);
    normalized(values)
        .into_iter()
        .filter(|value| {
            value
                .parse::<IpNet>()
                .is_ok_and(|value| !blocked.iter().any(|item| overlaps(&value, item)))
        })
        .collect()
}

pub(crate) fn host_prefix(value: &str) -> Option<String> {
    value.trim().parse::<IpAddr>().ok().map(|address| {
        let bits = if address.is_ipv4() { 32 } else { 128 };
        format!("{address}/{bits}")
    })
}

fn parse(value: &str) -> Result<IpNet, TransparentError> {
    value.parse().map_err(|_| {
        TransparentError::Invalid(format!("invalid transparent proxy prefix {value:?}"))
    })
}

fn parse_all(values: &[String]) -> Vec<IpNet> {
    values
        .iter()
        .filter_map(|value| value.parse::<IpNet>().ok())
        .map(|value| value.trunc())
        .collect()
}

fn overlaps(left: &IpNet, right: &IpNet) -> bool {
    left.addr().is_ipv4() == right.addr().is_ipv4()
        && (left.contains(&right.network()) || right.contains(&left.network()))
}

fn contains(left: &IpNet, right: &IpNet) -> bool {
    left.addr().is_ipv4() == right.addr().is_ipv4() && left.contains(&right.network())
}

fn address_bytes(address: IpAddr) -> Vec<u8> {
    match address {
        IpAddr::V4(value) => value.octets().to_vec(),
        IpAddr::V6(value) => value.octets().to_vec(),
    }
}
