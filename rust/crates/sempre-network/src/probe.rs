use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use futures_util::{StreamExt as _, future::join_all};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::NetworkError;

const TIMEOUT: Duration = Duration::from_secs(15);
const IP_METADATA_TIMEOUT: Duration = Duration::from_secs(5);
const IP_METADATA_SUCCESS_TTL: Duration = Duration::from_hours(24);
const IP_METADATA_FAILURE_TTL: Duration = Duration::from_mins(5);
const IP_METADATA_CACHE_CAPACITY: usize = 256;
const BODY_LIMIT: usize = 1 << 20;
const IP_METADATA_ENDPOINT: &str = "https://api.ip.sb/geoip";

type IpMetadataSlot = Arc<AsyncMutex<Option<CachedIpMetadata>>>;

struct IpMetadataCacheEntry {
    slot: IpMetadataSlot,
    last_used: Instant,
}

struct CachedIpMetadata {
    metadata: Option<IpMetadata>,
    expires_at: Instant,
}

static IP_METADATA_CACHE: OnceLock<Mutex<HashMap<IpAddr, IpMetadataCacheEntry>>> = OnceLock::new();

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IpMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn_organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
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

async fn lookup_ip_metadata(client: &Client, ip: &str) -> Result<IpMetadata, String> {
    let address = ip.parse::<IpAddr>().map_err(|error| error.to_string())?;
    let slot = ip_metadata_cache_slot(address);
    let mut cached = slot.lock().await;
    if let Some(entry) = cached
        .as_ref()
        .filter(|entry| entry.expires_at > Instant::now())
    {
        return entry
            .metadata
            .clone()
            .ok_or_else(|| "cached IP metadata lookup failure".into());
    }
    let result = fetch_ip_metadata(client, address).await;
    let ttl = if result.is_ok() {
        IP_METADATA_SUCCESS_TTL
    } else {
        IP_METADATA_FAILURE_TTL
    };
    *cached = Some(CachedIpMetadata {
        metadata: result.clone().ok(),
        expires_at: Instant::now() + ttl,
    });
    result
}

fn ip_metadata_cache_slot(address: IpAddr) -> IpMetadataSlot {
    let now = Instant::now();
    let cache = IP_METADATA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(entry) = cache.get_mut(&address) {
        entry.last_used = now;
        return Arc::clone(&entry.slot);
    }
    if cache.len() >= IP_METADATA_CACHE_CAPACITY
        && let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(address, _)| *address)
    {
        cache.remove(&oldest);
    }
    let slot = Arc::new(AsyncMutex::new(None));
    cache.insert(
        address,
        IpMetadataCacheEntry {
            slot: Arc::clone(&slot),
            last_used: now,
        },
    );
    slot
}

async fn fetch_ip_metadata(client: &Client, address: IpAddr) -> Result<IpMetadata, String> {
    let response = client
        .get(format!("{IP_METADATA_ENDPOINT}/{address}"))
        .timeout(IP_METADATA_TIMEOUT)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    parse_ip_metadata(&limited_body(response).await?)
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

fn parse_ip_metadata(data: &[u8]) -> Result<IpMetadata, String> {
    let metadata: IpMetadata = serde_json::from_slice(data).map_err(|error| error.to_string())?;
    if metadata.country.is_none()
        && metadata.region.is_none()
        && metadata.city.is_none()
        && metadata.asn.is_none()
        && metadata.asn_organization.is_none()
        && metadata.isp.is_none()
        && metadata.organization.is_none()
    {
        return Err("response did not contain IP metadata".into());
    }
    Ok(metadata)
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

    #[test]
    fn parses_optional_ip_metadata() {
        let metadata = parse_ip_metadata(
            br#"{"country_code":"CN","country":"China","region":"Zhejiang","city":"Hangzhou","asn":4134,"asn_organization":"CHINANET-BACKBONE","isp":"China Telecom"}"#,
        )
        .expect("IP metadata");
        assert_eq!(metadata.country_code.as_deref(), Some("CN"));
        assert_eq!(metadata.city.as_deref(), Some("Hangzhou"));
        assert_eq!(metadata.asn, Some(4134));
        assert_eq!(metadata.isp.as_deref(), Some("China Telecom"));
        assert!(parse_ip_metadata(br#"{"ip":"203.0.113.7"}"#).is_err());
    }

    #[tokio::test]
    async fn metadata_cache_reuses_the_same_ip_without_a_request() {
        let address = "192.0.2.77".parse::<IpAddr>().expect("test IP");
        let slot = ip_metadata_cache_slot(address);
        slot.lock().await.replace(CachedIpMetadata {
            metadata: Some(IpMetadata {
                country_code: Some("US".into()),
                country: Some("United States".into()),
                region: None,
                city: None,
                asn: Some(64500),
                asn_organization: Some("Example Network".into()),
                isp: None,
                organization: None,
            }),
            expires_at: Instant::now() + IP_METADATA_SUCCESS_TTL,
        });

        let metadata = lookup_ip_metadata(&Client::new(), &address.to_string())
            .await
            .expect("cached metadata");
        assert_eq!(metadata.asn, Some(64500));
        assert!(Arc::ptr_eq(&slot, &ip_metadata_cache_slot(address)));
    }
}
