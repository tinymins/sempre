use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Overview {
    pub core: String,
    pub version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mode: String,
    pub connections: usize,
    pub download: i64,
    pub upload: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Proxy {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub proxy_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub now: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub udp: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Latency>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Latency {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub delay: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProxyProvider {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub provider_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vehicle_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub updated_at: String,
    pub proxies: Vec<Proxy>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Rule {
    #[serde(rename = "type", default)]
    pub rule_type: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ConnectionSnapshot {
    pub download_total: i64,
    pub upload_total: i64,
    pub connections: Vec<Connection>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Connection {
    pub id: String,
    pub metadata: ConnectionMetadata,
    pub chains: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rule: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rule_payload: String,
    pub download: i64,
    pub upload: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub start: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ConnectionMetadata {
    pub network: String,
    #[serde(rename = "type")]
    pub connection_type: String,
    pub source_ip: String,
    pub destination_ip: String,
    pub source_port: String,
    pub destination_port: String,
    pub host: String,
    pub dns_mode: String,
    pub process: String,
    pub process_path: String,
    pub inbound_user: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct RawConnections {
    #[serde(rename = "downloadTotal", default)]
    download_total: i64,
    #[serde(rename = "uploadTotal", default)]
    upload_total: i64,
    #[serde(default)]
    connections: Vec<RawConnection>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RawConnection {
    #[serde(default)]
    id: String,
    #[serde(default)]
    metadata: RawMetadata,
    #[serde(default)]
    chains: Vec<String>,
    #[serde(default)]
    rule: String,
    #[serde(rename = "rulePayload", default)]
    rule_payload: String,
    #[serde(default)]
    download: i64,
    #[serde(default)]
    upload: i64,
    #[serde(default)]
    start: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RawMetadata {
    #[serde(default)]
    network: String,
    #[serde(rename = "type", default)]
    connection_type: String,
    #[serde(rename = "sourceIP", default)]
    source_ip: String,
    #[serde(rename = "destinationIP", default)]
    destination_ip: String,
    #[serde(rename = "sourcePort", default)]
    source_port: String,
    #[serde(rename = "destinationPort", default)]
    destination_port: String,
    #[serde(default)]
    host: String,
    #[serde(rename = "dnsMode", default)]
    dns_mode: String,
    #[serde(default)]
    process: String,
    #[serde(rename = "processPath", default)]
    process_path: String,
    #[serde(rename = "inboundUser", default)]
    inbound_user: String,
}

impl From<RawConnections> for ConnectionSnapshot {
    fn from(value: RawConnections) -> Self {
        Self {
            download_total: value.download_total,
            upload_total: value.upload_total,
            connections: value
                .connections
                .into_iter()
                .map(Connection::from)
                .collect(),
        }
    }
}

impl From<RawConnection> for Connection {
    fn from(item: RawConnection) -> Self {
        Self {
            id: item.id,
            metadata: ConnectionMetadata {
                network: item.metadata.network,
                connection_type: item.metadata.connection_type,
                source_ip: item.metadata.source_ip,
                destination_ip: item.metadata.destination_ip,
                source_port: item.metadata.source_port,
                destination_port: item.metadata.destination_port,
                host: item.metadata.host,
                dns_mode: item.metadata.dns_mode,
                process: item.metadata.process,
                process_path: item.metadata.process_path,
                inbound_user: item.metadata.inbound_user,
            },
            chains: item.chains,
            rule: item.rule,
            rule_payload: item.rule_payload,
            download: item.download,
            upload: item.upload,
            start: item.start,
        }
    }
}

pub(crate) fn proxy_map(values: Map<String, Value>) -> Vec<Proxy> {
    values
        .into_iter()
        .filter_map(|(name, value)| {
            serde_json::from_value::<Proxy>(value)
                .ok()
                .map(|mut proxy| {
                    if proxy.name.is_empty() {
                        proxy.name = name;
                    }
                    proxy
                })
        })
        .collect()
}

pub(crate) fn providers(values: Map<String, Value>) -> Vec<ProxyProvider> {
    let mut providers = values
        .into_iter()
        .filter_map(|(name, value)| {
            let object = value.as_object()?;
            let proxies = object
                .get("proxies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|value| serde_json::from_value(value.clone()).ok())
                .collect();
            Some(ProxyProvider {
                name: object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&name)
                    .into(),
                provider_type: string(object, "type"),
                vehicle_type: string(object, "vehicleType"),
                updated_at: string(object, "updatedAt"),
                proxies,
            })
        })
        .collect::<Vec<_>>();
    providers.sort_by_key(|provider| provider.name.to_lowercase());
    providers
}

fn string(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(value: &i64) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalizes_named_proxy_maps_and_connection_field_names() {
        let proxies = proxy_map(
            json!({ "Fallback": { "type": "Selector", "now": "edge", "all": ["edge"] } })
                .as_object()
                .expect("proxy map")
                .clone(),
        );
        assert_eq!(proxies[0].name, "Fallback");
        assert_eq!(proxies[0].proxy_type, "Selector");

        let raw: RawConnections = serde_json::from_value(json!({
            "downloadTotal": 10,
            "uploadTotal": 20,
            "connections": [{
                "id": "connection-1", "metadata": { "sourceIP": "127.0.0.1" },
                "chains": ["edge"], "rulePayload": "example.com"
            }]
        }))
        .expect("connections");
        let normalized = ConnectionSnapshot::from(raw);
        assert_eq!(normalized.download_total, 10);
        assert_eq!(normalized.connections[0].metadata.source_ip, "127.0.0.1");
        assert_eq!(normalized.connections[0].rule_payload, "example.com");
    }

    #[test]
    fn preserves_proxy_order_from_the_core() {
        let mut values = Map::new();
        for name in ["🔰 国外流量", "GLOBAL", "⚓️ 其他流量"] {
            values.insert(
                name.into(),
                json!({ "type": "Selector", "now": "edge", "all": ["edge"] }),
            );
        }

        let names = proxy_map(values)
            .into_iter()
            .map(|proxy| proxy.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["🔰 国外流量", "GLOBAL", "⚓️ 其他流量"]);
    }
}
