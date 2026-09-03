use tokio_rustls::rustls::pki_types::ServerName;
use url::Url;

use crate::DnsError;

pub fn default_upstreams() -> Vec<String> {
    ["223.6.6.6", "223.5.5.5"]
        .map(|ip| format!("tls://{ip}:853?server_name=dns.alidns.com"))
        .into()
}

pub fn validate_upstream(value: &str) -> Result<(), DnsError> {
    Endpoint::parse(value).map(|_| ())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Protocol {
    Udp,
    Tcp,
    Tls,
}

pub(crate) struct Endpoint {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub server_name: ServerName<'static>,
}

impl Endpoint {
    pub fn parse(value: &str) -> Result<Self, DnsError> {
        let invalid = || {
            DnsError::invalid(format!(
                "invalid DNS upstream {value:?}; use tls://, tcp://, udp:// or host:port"
            ))
        };
        let value = value.trim();
        let explicit = value.contains("://");
        let url = Url::parse(&if explicit {
            value.into()
        } else {
            format!("udp://{value}")
        })
        .map_err(|_| invalid())?;
        let protocol = match url.scheme() {
            "udp" => Protocol::Udp,
            "tcp" => Protocol::Tcp,
            "tls" => Protocol::Tls,
            _ => return Err(invalid()),
        };
        if !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
            || (!explicit && url.port().is_none())
        {
            return Err(invalid());
        }
        let host = url
            .host_str()
            .ok_or_else(invalid)?
            .trim_matches(['[', ']'])
            .to_owned();
        let port = url
            .port()
            .unwrap_or(if protocol == Protocol::Tls { 853 } else { 53 });
        if port == 0 {
            return Err(invalid());
        }
        let mut server_name = None;
        for (key, value) in url.query_pairs() {
            if protocol != Protocol::Tls || key != "server_name" || server_name.is_some() {
                return Err(invalid());
            }
            server_name = Some(value.into_owned());
        }
        let server_name = ServerName::try_from(server_name.unwrap_or_else(|| host.clone()))
            .map_err(|_| invalid())?;
        Ok(Self {
            protocol,
            host,
            port,
            server_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, Protocol, default_upstreams, validate_upstream};

    #[test]
    fn protocols_ports_and_tls_identity() {
        for (value, protocol, port) in [
            ("udp://223.5.5.5", Protocol::Udp, 53),
            ("tcp://[::1]:1053", Protocol::Tcp, 1053),
            ("tls://dns.alidns.com", Protocol::Tls, 853),
            ("127.0.0.1:1053", Protocol::Udp, 1053),
        ] {
            let endpoint = Endpoint::parse(value).expect("endpoint");
            assert_eq!(endpoint.protocol, protocol);
            assert_eq!(endpoint.port, port);
        }
        for value in default_upstreams() {
            let endpoint = Endpoint::parse(&value).expect("default endpoint");
            assert_eq!(endpoint.protocol, Protocol::Tls);
            assert!(endpoint.host.parse::<std::net::IpAddr>().is_ok());
            assert_eq!(endpoint.server_name.to_str(), "dns.alidns.com");
        }
    }

    #[test]
    fn invalid_options_are_not_silently_ignored() {
        for value in [
            "",
            "host",
            "https://dns.example/dns-query",
            "udp://1.1.1.1:0",
            "tcp://user@1.1.1.1",
            "tls://dns.example/path",
            "tls://dns.example#x",
            "udp://1.1.1.1?server_name=dns.example",
            "tls://dns.example?insecure=true",
            "tls://1.1.1.1?server_name=",
            "tls://1.1.1.1?server_name=a&server_name=b",
        ] {
            assert!(validate_upstream(value).is_err(), "{value}");
        }
    }
}
