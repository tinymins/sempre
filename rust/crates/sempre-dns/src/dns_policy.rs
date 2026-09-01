use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DnsRewrite {
    pub id: String,
    pub enabled: bool,
    pub domain: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub answer: String,
    pub ttl: u32,
    pub comment: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsQueryEvent {
    pub time: i64,
    pub client: String,
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub decision: String,
    pub answers: Vec<String>,
    pub upstream: String,
    pub latency_ms: u64,
    pub detail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

pub trait DnsRuntimePolicy: Send + Sync {
    fn rewrite(&self, name: &str, record_type: &str) -> Option<DnsRewrite>;
    fn reject_https(&self) -> bool {
        false
    }
    fn record(&self, event: DnsQueryEvent);
}

#[derive(Default)]
pub(crate) struct NoopDnsRuntimePolicy;

impl DnsRuntimePolicy for NoopDnsRuntimePolicy {
    fn rewrite(&self, _name: &str, _record_type: &str) -> Option<DnsRewrite> {
        None
    }

    fn record(&self, _event: DnsQueryEvent) {}
}
