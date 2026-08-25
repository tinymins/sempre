use std::{net::IpAddr, time::Duration};

use chrono::{DateTime, Utc};
use futures_util::{StreamExt as _, future::join_all};
use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::NetworkError;

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
        id: "domestic-ip",
        name: "Domestic IP",
        region: "domestic",
        category: "ip",
        url: "https://ip.3322.net",
        success: status_2xx_3xx,
        parse: Some(parse_text_ip),
    },
    Probe {
        id: "foreign-ip",
        name: "Foreign IP",
        region: "foreign",
        category: "ip",
        url: "https://api64.ipify.org?format=json",
        success: status_2xx_3xx,
        parse: Some(parse_json_ip),
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
    let results = join_all(PROBES.into_iter().map(|probe| run_probe(&client, probe))).await;
    Ok(NetworkTestReport {
        checked_at: Utc::now(),
        results,
    })
}

async fn run_probe(client: &Client, probe: Probe) -> NetworkTestResult {
    let started = std::time::Instant::now();
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
        detail: None,
    };
    let response = match client.get(probe.url).send().await {
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

fn parse_json_ip(data: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|error| error.to_string())?;
    value
        .get("ip")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "response did not contain an IP address".into())
        .and_then(normalize_ip)
}

fn parse_text_ip(data: &[u8]) -> Result<String, String> {
    String::from_utf8_lossy(data)
        .split(|character: char| {
            !(character.is_ascii_hexdigit() || character == ':' || character == '.')
        })
        .find_map(|candidate| normalize_ip(candidate).ok())
        .ok_or_else(|| "response did not contain an IP address".into())
}

fn normalize_ip(value: &str) -> Result<String, String> {
    value
        .trim()
        .parse::<IpAddr>()
        .map(|address| address.to_string())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_and_json_ip_responses() {
        assert_eq!(
            parse_text_ip(b"current address: 203.0.113.7\n").expect("text IP"),
            "203.0.113.7"
        );
        assert_eq!(
            parse_json_ip(br#"{"ip":"2001:db8::1"}"#).expect("JSON IP"),
            "2001:db8::1"
        );
        assert!(parse_text_ip(b"unavailable").is_err());
    }
}
