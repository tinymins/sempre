use std::{net::IpAddr, time::Duration};

use serde::Serialize;
use tokio::{net::lookup_host, time::timeout};

const DNS_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DnsAnswer {
    pub address: IpAddr,
    pub fake_ip: bool,
}

pub(crate) struct DnsLookup {
    pub answers: Vec<DnsAnswer>,
    pub error: Option<String>,
}

pub(crate) async fn resolve(host: &str) -> DnsLookup {
    match timeout(DNS_TIMEOUT, lookup_host((host, 0))).await {
        Ok(Ok(addresses)) => {
            let answers = classify(addresses.map(|address| address.ip()));
            let error = answers
                .is_empty()
                .then(|| "DNS lookup returned no addresses".into());
            DnsLookup { answers, error }
        }
        Ok(Err(error)) => DnsLookup {
            answers: Vec::new(),
            error: Some(error.to_string()),
        },
        Err(_) => DnsLookup {
            answers: Vec::new(),
            error: Some("DNS lookup timed out".into()),
        },
    }
}

fn classify(addresses: impl Iterator<Item = IpAddr>) -> Vec<DnsAnswer> {
    let mut addresses = addresses.collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    addresses
        .into_iter()
        .map(|address| DnsAnswer {
            fake_ip: is_managed_fake_ip(address),
            address,
        })
        .collect()
}

fn is_managed_fake_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            octets[0] == 198 && octets[1] & 0xfe == 18
        }
        IpAddr::V6(address) => address.segments()[0] & 0xffc0 == 0xfc00,
    }
}
