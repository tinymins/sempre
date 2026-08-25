use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::RngCore as _;
use sempre_converter::{CustomNode, Profile};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub const CATALOG_SCHEMA: u32 = 1;
pub const MAX_SOURCE_SIZE: usize = 32 << 20;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalog {
    pub schema: u32,
    pub updated_at: DateTime<Utc>,
    pub profiles: Vec<Profile>,
    pub custom_nodes: Vec<CustomNode>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            schema: CATALOG_SCHEMA,
            updated_at: Utc::now(),
            profiles: vec![new_profile("")],
            custom_nodes: Vec::new(),
        }
    }
}

pub fn new_profile(name: &str) -> Profile {
    let mut profile = Profile {
        id: Uuid::new_v4().to_string(),
        revision: 1,
        name: name.trim().into(),
        ..Profile::default()
    };
    profile.editor.servers = "[]".into();
    profile.local_proxy.socks_port = 1080;
    profile.local_proxy.http_port = 1081;
    profile.local_proxy.username = "sempre".into();
    profile.local_proxy.password = random_secret();
    profile.management_api.external_controller = "0.0.0.0:9090".into();
    profile.management_api.secret = random_secret();
    profile.transparent_proxy.mode = "tun-router".into();
    profile.transparent_proxy.capture_host = false;
    profile.transparent_proxy.interface_mode = "all".into();
    profile.transparent_proxy.auto_exclude_local_routes = true;
    profile.transparent_proxy.auto_exclude_vpn_routes = true;
    profile.transparent_proxy.tun.interface_name = "sempre-tun".into();
    profile.transparent_proxy.tproxy.listen_port = 7893;
    profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    profile.transparent_proxy.ebpf.wan_interface = "auto".into();
    profile.extra.insert("mode".into(), json!("local"));
    for key in [
        "use_system_groups",
        "use_system_rules",
        "use_system_filters",
        "use_system_dns",
        "use_system_custom_config",
    ] {
        profile.extra.insert(key.into(), json!(true));
    }
    profile
}

fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
