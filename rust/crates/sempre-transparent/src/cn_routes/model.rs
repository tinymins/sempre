use std::{collections::HashSet, net::IpAddr};

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use crate::TransparentError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intent {
    pub direct: bool,
    pub prefixes: Vec<String>,
    pub tun_addresses: Vec<String>,
    pub capture_prefixes: Vec<String>,
    pub outbound_interfaces: [Option<String>; 2],
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Endpoint {
    pub interface: String,
    pub index: u32,
    pub gateway: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Route {
    pub prefix: String,
    pub endpoint: Endpoint,
    pub fallback: bool,
    pub scoped: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Journal {
    pub routes: Vec<Route>,
    pub policy: bool,
}

pub(crate) fn invalid(message: impl Into<String>) -> TransparentError {
    TransparentError::Invalid(message.into())
}

pub(crate) fn normalize(prefixes: &[String]) -> Result<Vec<String>, TransparentError> {
    let mut nets = prefixes
        .iter()
        .map(|s| {
            s.parse::<IpNet>()
                .map(|net| net.trunc())
                .map_err(|_| invalid(format!("invalid CN prefix {s:?}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    nets = IpNet::aggregate(&nets);

    Ok(nets.iter().map(ToString::to_string).collect())
}

pub(crate) fn plan(
    intent: &Intent,
    uplinks: &[Endpoint],
    tun: &Endpoint,
) -> Result<Vec<Route>, TransparentError> {
    let prefixes = normalize(&intent.prefixes)?;
    let mut routes = Vec::new();
    for uplink in uplinks {
        let gateway = uplink
            .gateway
            .split('%')
            .next()
            .unwrap_or_default()
            .parse::<IpAddr>()
            .map_err(|_| invalid("uplink has no IP gateway"))?;
        let v4 = gateway.is_ipv4();
        if cfg!(target_os = "macos") {
            routes.push(Route {
                prefix: if v4 { "0.0.0.0/0" } else { "::/0" }.into(),
                endpoint: uplink.clone(),
                fallback: false,
                scoped: true,
            });
        }
        let family = prefixes
            .iter()
            .filter(|prefix| prefix.contains(':') != v4)
            .collect::<Vec<_>>();

        if intent.direct {
            routes.extend(family.into_iter().map(|prefix| Route {
                prefix: prefix.clone(),
                endpoint: uplink.clone(),
                fallback: false,
                scoped: false,
            }));
        }
        // Keep the physical default intact. Longer CN prefixes win over these.
        for prefix in if v4 {
            ["0.0.0.0/1", "128.0.0.0/1"]
        } else {
            ["::/1", "8000::/1"]
        } {
            routes.push(Route {
                prefix: prefix.into(),
                endpoint: tun.clone(),
                fallback: true,
                scoped: false,
            });
        }
    }
    for prefix in &intent.capture_prefixes {
        let capture = prefix
            .parse::<IpNet>()
            .map_err(|_| invalid("invalid private capture prefix"))?;

        // FakeIP capture also works without a physical uplink of that family.
        if intent
            .tun_addresses
            .iter()
            .filter_map(|s| s.parse::<IpNet>().ok())
            .any(|address| address.addr().is_ipv6() == capture.addr().is_ipv6())
        {
            routes.push(Route {
                prefix: capture.trunc().to_string(),
                endpoint: tun.clone(),
                fallback: true,
                scoped: false,
            });
        }
    }

    Ok(routes)
}

pub(crate) fn difference(left: &[Route], right: &[Route]) -> Vec<Route> {
    let present = right.iter().collect::<HashSet<_>>();
    left.iter()
        .filter(|route| !present.contains(route))
        .cloned()
        .collect()
}
