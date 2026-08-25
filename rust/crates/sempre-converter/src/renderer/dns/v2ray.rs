use std::net::IpAddr;

use serde_json::{Value, json};

use crate::Proxy;

use super::SharedDns;

pub(super) fn render(proxies: &[Proxy], shared: &SharedDns) -> Value {
    let bootstrap_domains = proxies
        .iter()
        .filter(|proxy| proxy.server.parse::<IpAddr>().is_err())
        .map(|proxy| format!("full:{}", proxy.server))
        .collect::<Vec<_>>();
    json!({
        "hosts": {
            (&shared.remote_server_name): shared.remote_dns,
            (&shared.bootstrap_server_name): shared.bootstrap_dns
        },
        "servers": [
            { "address": shared.bootstrap_dns, "port": 53, "domains": bootstrap_domains, "skipFallback": true, "tag": "bootstrap-dns" },
            { "address": shared.local_dns, "port": shared.local_port, "domains": ["geosite:cn"], "expectedIPs": ["geoip:cn"], "skipFallback": true, "tag": "local-dns" },
            { "address": format!("https://{}:443/dns-query", shared.remote_server_name), "tag": "remote-dns", "finalQuery": true }
        ],
        "queryStrategy": if shared.prefer_ipv4() { "UseIPv4" } else { "UseIP" },
        "disableFallbackIfMatch": true, "tag": "remote-dns"
    })
}
