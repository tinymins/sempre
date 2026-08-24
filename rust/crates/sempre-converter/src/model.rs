use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Target;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileRequest {
    #[serde(default = "protocol_version")]
    pub protocol: u32,
    pub profile: Profile,
    #[serde(default)]
    pub snapshots: Vec<SourceSnapshot>,
    #[serde(default)]
    pub custom_nodes: Vec<CustomNode>,
    pub target: Target,
}

const fn protocol_version() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub custom_node_ids: Vec<String>,
    #[serde(default)]
    pub manual_servers: Vec<Value>,
    #[serde(default)]
    pub groups: Vec<ProxyGroup>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub rule_providers: Vec<RuleProvider>,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default)]
    pub dns: Value,
    #[serde(default)]
    pub private_access: Value,
    #[serde(default)]
    pub core_overrides: HashMap<String, Value>,
    #[serde(default)]
    pub local_proxy: LocalProxy,
    #[serde(default)]
    pub transparent_proxy: TransparentProxy,
    #[serde(default)]
    pub management_api: ManagementApi,
}

fn default_log_level() -> String {
    "info".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub remark: String,
    #[serde(default)]
    pub prefix: String,
}

impl Source {
    pub(crate) fn label(&self) -> String {
        if !self.remark.trim().is_empty() {
            self.remark.clone()
        } else if !self.url.trim().is_empty() {
            self.url.clone()
        } else {
            self.id.clone()
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub source_id: String,
    pub content: String,
    #[serde(default)]
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomNode {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub proxy: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyGroup {
    pub name: String,
    #[serde(rename = "type", default = "default_group_type")]
    pub group_type: String,
    #[serde(default)]
    pub proxies: Vec<String>,
    #[serde(default)]
    pub include_all: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub interval: u64,
    #[serde(default)]
    pub tolerance: u64,
    #[serde(default)]
    pub default: String,
}

fn default_group_type() -> String {
    "select".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleProvider {
    pub tag: String,
    pub url: String,
    #[serde(default)]
    pub outbound: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub behavior: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProxy {
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

impl Default for LocalProxy {
    fn default() -> Self {
        Self {
            socks_port: default_socks_port(),
            http_port: default_http_port(),
            username: String::new(),
            password: String::new(),
        }
    }
}

const fn default_socks_port() -> u16 {
    1080
}
const fn default_http_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransparentProxy {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub tun: Value,
    #[serde(default)]
    pub tproxy: Value,
    #[serde(default)]
    pub ebpf: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagementApi {
    #[serde(default)]
    pub external_controller: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub external_ui: String,
    #[serde(default)]
    pub allow_origins: Vec<String>,
    #[serde(default)]
    pub allow_private_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub server: String,
    pub port: u16,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Proxy {
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn as_value(&self) -> Value {
        serde_json::to_value(self).expect("Proxy serialization is infallible")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDiff {
    pub node: String,
    pub represented: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbound: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub protocol: u32,
    pub format: String,
    pub version: String,
    pub platform: String,
    pub content: String,
    pub artifact_hash: String,
    pub node_count: usize,
    pub field_diffs: Vec<FieldDiff>,
    pub node_origins: HashMap<String, String>,
    pub diagnostics: Vec<Diagnostic>,
    pub runtime_validated: bool,
}
