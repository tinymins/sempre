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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_and_deduplicates_managed_fake_ip_answers() {
        let answers = classify(
            [
                "198.19.255.255",
                "203.0.113.8",
                "198.18.0.1",
                "fc3f::1",
                "fc40::1",
                "198.18.0.1",
            ]
            .into_iter()
            .map(|address| address.parse::<IpAddr>().expect("test IP")),
        );

        assert_eq!(answers.len(), 5);
        assert!(
            answers
                .iter()
                .find(|item| item.address.to_string() == "198.18.0.1")
                .expect("IPv4 fake IP")
                .fake_ip
        );
        assert!(
            answers
                .iter()
                .find(|item| item.address.to_string() == "198.19.255.255")
                .expect("IPv4 fake boundary")
                .fake_ip
        );
        assert!(
            answers
                .iter()
                .find(|item| item.address.to_string() == "fc3f::1")
                .expect("IPv6 fake boundary")
                .fake_ip
        );
        assert!(
            !answers
                .iter()
                .find(|item| item.address.to_string() == "203.0.113.8")
                .expect("real IPv4")
                .fake_ip
        );
        assert!(
            !answers
                .iter()
                .find(|item| item.address.to_string() == "fc40::1")
                .expect("real IPv6")
                .fake_ip
        );
    }
}
