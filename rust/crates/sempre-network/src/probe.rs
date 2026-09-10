use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::{StreamExt as _, future::join_all};
use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::{
    DOMESTIC_IP_PROBE, DnsAnswer, FOREIGN_IP_PROBE, IpMetadata, NetworkError, dns_probe,
    lookup_ip_metadata,
};

const TIMEOUT: Duration = Duration::from_secs(15);
const BODY_LIMIT: usize = 1 << 20;

#[derive(Clone, Debug, Serialize)]
pub struct NetworkTestReport {
    pub checked_at: DateTime<Utc>,
    pub results: Vec<NetworkTestResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NetworkTestResult {
    pub id: &'static str,
    pub name: &'static str,
    pub region: &'static str,
    pub category: &'static str,
    pub url: &'static str,
    pub ok: bool,
    pub latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_metadata: Option<IpMetadata>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns_answers: Vec<DnsAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Copy)]
struct Probe {
    id: &'static str,
    name: &'static str,
    region: &'static str,
    category: &'static str,
    url: &'static str,
    success: fn(StatusCode) -> bool,
    parse: Option<ParseResponse>,
}

type ParseResponse = fn(&[u8]) -> Result<String, String>;

const PROBES: [Probe; 7] = [
    Probe {
        id: DOMESTIC_IP_PROBE.id,
        name: DOMESTIC_IP_PROBE.name,
        region: DOMESTIC_IP_PROBE.region,
        category: "ip",
        url: DOMESTIC_IP_PROBE.url,
        success: status_2xx_3xx,
        parse: Some(parse_domestic_ip),
    },
    Probe {
        id: FOREIGN_IP_PROBE.id,
        name: FOREIGN_IP_PROBE.name,
        region: FOREIGN_IP_PROBE.region,
        category: "ip",
        url: FOREIGN_IP_PROBE.url,
        success: status_2xx_3xx,
        parse: Some(parse_foreign_ip),
    },
    Probe {
        id: "baidu",
        name: "Baidu",
        region: "domestic",
        category: "reachability",
        url: "https://www.baidu.com/",
        success: status_2xx_3xx,
        parse: None,
    },
    Probe {
        id: "google",
        name: "Google",
        region: "foreign",
        category: "reachability",
        url: "https://www.google.com/generate_204",
        success: status_204,
        parse: None,
    },
    Probe {
        id: "openai",
        name: "OpenAI",
        region: "foreign",
        category: "reachability",
        url: "https://api.openai.com/v1/models",
        success: status_401,
        parse: None,
    },
    Probe {
        id: "youtube",
        name: "YouTube",
        region: "foreign",
        category: "reachability",
        url: "https://www.youtube.com/generate_204",
        success: status_204,
        parse: None,
    },
    Probe {
        id: "github",
        name: "GitHub",
        region: "foreign",
        category: "reachability",
        url: "https://api.github.com/rate_limit",
        success: status_200,
        parse: None,
    },
];

pub async fn run_network_test() -> Result<NetworkTestReport, NetworkError> {
    let client = Client::builder()
        .no_proxy()
        .timeout(TIMEOUT)
        .user_agent(concat!("Sempre/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut results = join_all(PROBES.into_iter().map(|probe| run_probe(&client, probe))).await;
    enrich_ip_metadata(&client, &mut results).await;
    Ok(NetworkTestReport {
        checked_at: Utc::now(),
        results,
    })
}

async fn run_probe(client: &Client, probe: Probe) -> NetworkTestResult {
    let started = std::time::Instant::now();
    let url = reqwest::Url::parse(probe.url).expect("built-in probe URL");
    let host = url.host_str().expect("built-in probe host");
    let (dns, response) = tokio::join!(dns_probe::resolve(host), client.get(probe.url).send());
    let mut result = NetworkTestResult {
        id: probe.id,
        name: probe.name,
        region: probe.region,
        category: probe.category,
        url: probe.url,
        ok: false,
        latency_ms: 0,
        http_status: None,
        ip: None,
        ip_metadata: None,
        dns_answers: dns.answers,
        dns_error: dns.error,
        detail: None,
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            result.latency_ms = started.elapsed().as_millis();
            result.detail = Some(error.to_string());
            return result;
        }
    };
    result.latency_ms = started.elapsed().as_millis();
    result.http_status = Some(response.status().as_u16());
    if !(probe.success)(response.status()) {
        result.detail = Some(format!("HTTP {}", response.status().as_u16()));
        return result;
    }
    let body = match limited_body(response).await {
        Ok(body) => body,
        Err(error) => {
            result.detail = Some(error);
            return result;
        }
    };
    if let Some(parse) = probe.parse {
        match parse(&body) {
            Ok(ip) => result.ip = Some(ip),
            Err(error) => {
                result.detail = Some(error);
                return result;
            }
        }
    }
    result.ok = true;
    result
}

async fn enrich_ip_metadata(client: &Client, results: &mut [NetworkTestResult]) {
    let lookups = results.iter().enumerate().filter_map(|(index, result)| {
        result
            .ip
            .as_ref()
            .filter(|_| result.ok && result.category == "ip")
            .map(|ip| async move { (index, lookup_ip_metadata(client, ip).await) })
    });
    for (index, metadata) in join_all(lookups).await {
        if let Ok(metadata) = metadata {
            results[index].ip_metadata = Some(metadata);
        }
    }
}

async fn limited_body(response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if body.len().saturating_add(chunk.len()) > BODY_LIMIT {
            return Err("response exceeds 1 MiB".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn status_2xx_3xx(status: StatusCode) -> bool {
    status.is_success() || status.is_redirection()
}

fn status_200(status: StatusCode) -> bool {
    status == StatusCode::OK
}

fn status_204(status: StatusCode) -> bool {
    status == StatusCode::NO_CONTENT
}

fn status_401(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED
}

fn parse_domestic_ip(data: &[u8]) -> Result<String, String> {
    DOMESTIC_IP_PROBE.parse_response(data)
}

fn parse_foreign_ip(data: &[u8]) -> Result<String, String> {
    FOREIGN_IP_PROBE.parse_response(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_order_prioritizes_public_ips() {
        assert_eq!(
            PROBES.map(|probe| probe.id),
            [
                "domestic-ip",
                "foreign-ip",
                "baidu",
                "google",
                "openai",
                "youtube",
                "github",
            ]
        );
    }
}
