//! Pure configuration and control protocol shared with the core-free server.
use sempre_converter::{CompileRequest, Profile, Proxy, Target, compile, parse_subscription};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    SingBox,
    Xray,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    pub engine: Engine,
    pub outbound: Value,
    pub protocol: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub id: Uuid,
    pub revision: i64,
    pub connection: Connection,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    pub revision: i64,
    pub port: u16,
    pub username: String,
    pub password: String,
}

/// Parse exactly one URI, Clash node, or native outbound, without network access.
pub fn parse(content: &str, engine: Option<Engine>) -> Result<Connection, String> {
    if content.len() > 65_536 {
        return Err("proxy configuration exceeds 64 KiB".into());
    }
    if let Ok(value) = sempre_converter::parse_jsonc_value(content)
        && value.is_object()
        && (value.get("protocol").is_some() || value.get("server_port").is_some())
    {
        let native = if value.get("protocol").is_some() {
            Engine::Xray
        } else {
            Engine::SingBox
        };
        if engine.as_ref().is_some_and(|selected| selected != &native) {
            return Err("native outbound does not match the selected engine".into());
        }
        return native_connection(value, native);
    }
    let parsed = parse_subscription(content);
    if parsed.nodes.len() != 1 {
        return Err("provide exactly one proxy URI, node, or native outbound".into());
    }
    let node = &parsed.nodes[0];
    match engine {
        Some(engine) => convert(node, engine),
        None => convert(node, Engine::SingBox).or_else(|_| convert(node, Engine::Xray)),
    }
}

fn convert(node: &Proxy, engine: Engine) -> Result<Connection, String> {
    let target = Target::parse(match engine {
        Engine::SingBox => "sing-box-v14",
        Engine::Xray => "xray",
    })
    .map_err(|error| error.to_string())?;
    let result = compile(&CompileRequest {
        protocol: 1,
        profile: Profile {
            manual_servers: vec![node.as_value()],
            ..Profile::default()
        },
        snapshots: vec![],
        custom_nodes: vec![],
        target,
    })
    .map_err(|_| "proxy cannot be represented by the selected engine".to_owned())?;
    let diff = result
        .field_diffs
        .first()
        .ok_or("proxy was not converted")?;
    if !diff.dropped.is_empty() {
        return Err(format!("unsupported fields: {}", diff.dropped.join(", ")));
    }
    let outbound = diff
        .outbound
        .clone()
        .ok_or("protocol is unsupported by this engine")?;
    native_connection(outbound, engine)
}

fn native_connection(mut outbound: Value, engine: Engine) -> Result<Connection, String> {
    let protocol = outbound
        .get(match engine {
            Engine::SingBox => "type",
            Engine::Xray => "protocol",
        })
        .and_then(Value::as_str)
        .ok_or("outbound protocol required")?
        .to_owned();
    let allowed = match engine {
        Engine::SingBox => [
            "http",
            "socks",
            "shadowsocks",
            "vmess",
            "vless",
            "trojan",
            "hysteria2",
            "tuic",
            "anytls",
        ]
        .as_slice(),
        Engine::Xray => ["http", "socks", "shadowsocks", "vmess", "vless", "trojan"].as_slice(),
    };
    if !allowed.contains(&protocol.as_str()) {
        return Err("unsupported subscription proxy protocol".into());
    }
    if engine == Engine::Xray
        && outbound
            .pointer("/streamSettings/tlsSettings/allowInsecure")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Err("Xray 26.3.27 does not support allowInsecure; use a valid certificate or native certificate pinning".into());
    }
    // A managed connection must be self-contained. No chained outbounds or host access.
    validate_fields(&outbound, "outbound")?;
    outbound["tag"] = json!("upstream");
    Ok(Connection {
        engine,
        outbound,
        protocol,
    })
}

fn validate_fields(value: &Value, parent: &str) -> Result<(), String> {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let lower = key.to_ascii_lowercase();
                if lower.contains("path")
                    || lower.contains("file")
                    || matches!(
                        lower.as_str(),
                        "detour"
                            | "dialerproxy"
                            | "proxysettings"
                            | "bind_interface"
                            | "bind_ipv4_address"
                            | "bind_ipv6_address"
                            | "routing_mark"
                            | "sockopt"
                            | "plugin"
                            | "plugin_opts"
                            | "external"
                            | "certificate"
                            | "key"
                            | "inbounds"
                            | "outbounds"
                            | "dns"
                            | "keylog"
                    )
                {
                    // WebSocket/HTTP request paths are transport data, not filesystem paths.
                    if lower != "path"
                        || !value.is_string()
                        || ![
                            "transport",
                            "wsSettings",
                            "httpSettings",
                            "xhttpSettings",
                            "httpupgradeSettings",
                        ]
                        .contains(&parent)
                    {
                        return Err(format!("field {key} is not supported in managed proxies"));
                    }
                }
                validate_fields(value, key)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_fields(item, parent)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn configuration(connection: &Connection, port: u16, username: &str, password: &str) -> Value {
    match connection.engine {
        Engine::SingBox => json!({
            "log":{"level":"error"},
            "inbounds":[{"type":"http","tag":"fetch","listen":"0.0.0.0","listen_port":port,
                "users":[{"username":username,"password":password}]}],
            "outbounds":[connection.outbound],
            "dns":{"servers":[{"type":"local","tag":"bootstrap"}],"final":"bootstrap"},
            "route":{"final":"upstream","default_domain_resolver":"bootstrap"}
        }),
        Engine::Xray => json!({
            "log":{"loglevel":"error"},
            "inbounds":[{"tag":"fetch","listen":"0.0.0.0","port":port,"protocol":"http",
                "settings":{"accounts":[{"user":username,"pass":password}]}}],
            "outbounds":[connection.outbound],
            "routing":{"rules":[{"type":"field","inboundTag":["fetch"],"outboundTag":"upstream"}]}
        }),
    }
}
