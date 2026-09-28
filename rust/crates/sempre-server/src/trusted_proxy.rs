use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;

use crate::AppState;

pub(crate) fn client_ip(state: &AppState, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    let trusted = &state.config.trusted_proxy_ips;
    let mut current = peer.ip();
    if !trusted.contains(&current) {
        return current;
    }
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        let Ok(forwarded) = forwarded.to_str() else {
            return current;
        };
        for hop in forwarded.rsplit(',') {
            if !trusted.contains(&current) {
                break;
            }
            let Ok(ip) = hop.trim().parse() else {
                return current;
            };
            current = ip;
        }
        return current;
    }
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(current)
}
