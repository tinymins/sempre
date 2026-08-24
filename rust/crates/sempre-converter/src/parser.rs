mod uri;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Proxy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub format: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub decoded_text: String,
    pub nodes: Vec<Proxy>,
    pub discarded_placeholder_nodes: Vec<Proxy>,
    pub diagnostics: Vec<String>,
}

impl ParseResult {
    pub(crate) fn messages(&self) -> &[String] {
        &self.diagnostics
    }
}

pub fn parse_subscription(text: &str) -> ParseResult {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return result("unknown", vec![], vec!["Response body is empty".into()]);
    }
    let yaml_hint = trimmed.starts_with("proxies:")
        || trimmed.starts_with("port:")
        || trimmed.starts_with('#')
        || trimmed.contains("\nproxies:");
    let decoded =
        decode_subscription(trimmed).filter(|value| contains_proxy_uri(value) && !yaml_hint);
    let mut parsed = if let Some(decoded) = decoded {
        parse_uri_lines("base64", decoded)
    } else if contains_proxy_uri(trimmed) && !yaml_hint {
        parse_uri_lines("uri-list", trimmed.into())
    } else {
        parse_yaml(text, yaml_hint)
    };
    discard_placeholders(&mut parsed);
    parsed
}

fn parse_uri_lines(format: &str, text: String) -> ParseResult {
    let mut nodes = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().map(str::trim).enumerate() {
        if line.is_empty() {
            continue;
        }
        match uri::parse(line) {
            Ok(proxy) => nodes.push(proxy),
            Err(error) => diagnostics.push(format!(
                "Line {} uses an unsupported or invalid proxy URI: {error}",
                index + 1
            )),
        }
    }
    let decoded_text = if format == "base64" {
        text
    } else {
        String::new()
    };
    ParseResult {
        format: format.into(),
        decoded_text,
        nodes,
        discarded_placeholder_nodes: Vec::new(),
        diagnostics,
    }
}

fn parse_yaml(text: &str, yaml_hint: bool) -> ParseResult {
    let decoded: Value = match serde_yaml::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return result(
                if yaml_hint { "yaml" } else { "unknown" },
                vec![],
                vec![format!("YAML parse failed: {error}")],
            );
        }
    };
    let values = decoded
        .get("proxies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut nodes = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, value) in values.into_iter().enumerate() {
        match Proxy::from_value(value) {
            Ok(proxy) if valid_proxy(&proxy) => nodes.push(proxy),
            Ok(_) | Err(_) => diagnostics.push(format!(
                "Proxy {} is invalid: name, type, server, and a valid port are required",
                index + 1
            )),
        }
    }
    if nodes.is_empty() {
        diagnostics.push("YAML response contains no proxy nodes".into());
    }
    result("yaml", nodes, diagnostics)
}

fn valid_proxy(proxy: &Proxy) -> bool {
    !proxy.name.trim().is_empty()
        && !proxy.proxy_type.trim().is_empty()
        && !proxy.server.trim().is_empty()
        && proxy.port > 0
}

fn discard_placeholders(parsed: &mut ParseResult) {
    let mut usable = Vec::with_capacity(parsed.nodes.len());
    for proxy in parsed.nodes.drain(..) {
        if matches!(proxy.server.as_str(), "127.0.0.1" | "::1" | "localhost") && proxy.port <= 1 {
            parsed.discarded_placeholder_nodes.push(proxy);
        } else {
            usable.push(proxy);
        }
    }
    parsed.nodes = usable;
    if !parsed.discarded_placeholder_nodes.is_empty() {
        parsed.diagnostics.push(format!(
            "Discarded {} placeholder node(s) using a loopback address and port 0 or 1",
            parsed.discarded_placeholder_nodes.len()
        ));
    }
}

fn result(format: &str, nodes: Vec<Proxy>, diagnostics: Vec<String>) -> ParseResult {
    ParseResult {
        format: format.into(),
        decoded_text: String::new(),
        nodes,
        discarded_placeholder_nodes: Vec::new(),
        diagnostics,
    }
}

fn contains_proxy_uri(value: &str) -> bool {
    [
        "vless://",
        "vmess://",
        "ss://",
        "trojan://",
        "hysteria2://",
        "hy2://",
        "anytls://",
    ]
    .iter()
    .any(|scheme| value.contains(scheme))
}

fn decode_subscription(value: &str) -> Option<String> {
    let compact: String = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    for engine in [&STANDARD, &URL_SAFE_NO_PAD] {
        let mut padded = compact.clone();
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        if let Ok(bytes) = engine.decode(padded.as_bytes())
            && let Ok(text) = String::from_utf8(bytes)
        {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_subscription;

    #[test]
    fn parses_yaml_and_discards_placeholder() {
        let parsed = parse_subscription(
            "proxies:\n  - { name: edge, type: socks5, server: edge.example.com, port: 1080 }\n  - { name: empty, type: socks5, server: 127.0.0.1, port: 1 }",
        );
        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.discarded_placeholder_nodes.len(), 1);
    }

    #[test]
    fn parses_plain_uri_list() {
        let parsed = parse_subscription(
            "vless://id@example.com:443?security=tls&sni=edge.example.com#Hong%20Kong%20%E9%A6%99%E6%B8%AF",
        );
        assert_eq!(parsed.nodes[0].name, "Hong Kong 香港");
        assert_eq!(parsed.nodes[0].proxy_type, "vless");
    }
}
