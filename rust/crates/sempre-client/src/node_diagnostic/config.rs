use std::{fs, path::Path};

use serde_json::{Map, Value, json};

pub(super) struct DiagnosticConfig {
    pub(super) text: String,
}

pub(super) fn build_config(
    core: &str,
    source: &Path,
    node: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<DiagnosticConfig, String> {
    let text = fs::read_to_string(source)
        .map_err(|error| format!("read runtime configuration: {error}"))?;
    if core == "sing-box" {
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("parse sing-box configuration: {error}"))?;
        let value = sing_box_config(value, node, port, username, password)?;
        let text = serde_json::to_string_pretty(&value)
            .map_err(|error| format!("serialize diagnostic configuration: {error}"))?;
        Ok(DiagnosticConfig { text })
    } else {
        let value: Value = serde_yaml::from_str(&text)
            .map_err(|error| format!("parse Clash configuration: {error}"))?;
        let value = clash_config(value, node, port, username, password)?;
        let text = serde_yaml::to_string(&value)
            .map_err(|error| format!("serialize diagnostic configuration: {error}"))?;
        Ok(DiagnosticConfig { text })
    }
}

fn sing_box_config(
    mut value: Value,
    node: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<Value, String> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| "sing-box configuration must be an object".to_string())?;
    ensure_outbound(root, node)?;
    let default_domain_resolver = root
        .get("route")
        .and_then(Value::as_object)
        .and_then(|route| route.get("default_domain_resolver"))
        .cloned();
    root.insert(
        "inbounds".into(),
        json!([{"type":"http","tag":"sempre-node-test-in","listen":"127.0.0.1","listen_port":port,"users":[{"username":username,"password":password}]}]),
    );
    let mut route = Map::new();
    route.insert("auto_detect_interface".into(), Value::Bool(true));
    route.insert("final".into(), Value::String(node.into()));
    if let Some(resolver) = default_domain_resolver {
        route.insert("default_domain_resolver".into(), resolver);
    }
    root.insert("route".into(), Value::Object(route));
    root.remove("experimental");
    remove_endpoints_and_dependent_dns(root);
    if let Some(dns) = root.get_mut("dns").and_then(Value::as_object_mut) {
        dns.remove("rules");
    }
    Ok(value)
}

fn ensure_outbound(root: &Map<String, Value>, node: &str) -> Result<(), String> {
    let outbound_exists = root
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|outbound| outbound.get("tag").and_then(Value::as_str) == Some(node));
    if outbound_exists {
        return Ok(());
    }
    let endpoint_exists = root
        .get("endpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|endpoint| endpoint.get("tag").and_then(Value::as_str) == Some(node));
    if endpoint_exists {
        return Err("WireGuard diagnostics must reuse the endpoint in the managed core".into());
    }
    Err(format!(
        "node {node:?} is not present in the runtime configuration"
    ))
}

fn remove_endpoints_and_dependent_dns(root: &mut Map<String, Value>) {
    let removed = root
        .get("endpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|endpoint| endpoint.get("tag").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    root.remove("endpoints");
    if let Some(servers) = root
        .get_mut("dns")
        .and_then(Value::as_object_mut)
        .and_then(|dns| dns.get_mut("servers"))
        .and_then(Value::as_array_mut)
    {
        servers.retain(|server| {
            server
                .get("detour")
                .and_then(Value::as_str)
                .is_none_or(|detour| !removed.iter().any(|tag| tag == detour))
        });
    }
}

fn clash_config(
    mut value: Value,
    node: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<Value, String> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| "Clash configuration must be an object".to_string())?;
    for key in [
        "socks-port",
        "mixed-port",
        "redir-port",
        "tproxy-port",
        "tun",
        "listeners",
        "tunnels",
        "external-controller",
        "external-controller-tls",
        "external-controller-unix",
        "external-controller-pipe",
        "external-ui",
        "external-ui-name",
        "external-ui-url",
        "secret",
        "rule-providers",
        "ebpf",
    ] {
        root.remove(key);
    }
    root.insert("port".into(), json!(port));
    root.insert("bind-address".into(), json!("127.0.0.1"));
    root.insert("allow-lan".into(), Value::Bool(false));
    root.insert(
        "authentication".into(),
        json!([format!("{username}:{password}")]),
    );
    root.insert("mode".into(), json!("rule"));
    if let Some(dns) = root.get_mut("dns").and_then(Value::as_object_mut) {
        dns.remove("listen");
    }
    let groups = root
        .entry("proxy-groups")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "Clash proxy-groups must be an array".to_string())?;
    groups.insert(
        0,
        json!({"name":"__sempre_node_test__","type":"select","proxies":[node]}),
    );
    root.insert("rules".into(), json!(["MATCH,__sempre_node_test__"]));
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_sing_box_to_selected_node() {
        let input = json!({
            "inbounds": [{"type":"tun"}],
            "outbounds": [{"type":"shadowsocks","tag":"node-a"}],
            "endpoints": [{"type":"wireguard","tag":"home-wg","peers":[{"allowed_ips":["10.8.28.0/24"]}]}],
            "route": {"rule_set":[{"tag":"remote"}]},
            "experimental": {"clash_api":{"external_controller":"127.0.0.1:9090"}},
            "dns": {"servers":[
                {"type":"local","tag":"local"},
                {"type":"udp","tag":"home-wg-dns","detour":"home-wg"}
            ],"rules":[{"rule_set":"remote"}]}
        });
        let output = sing_box_config(input, "node-a", 19080, "user", "pass").unwrap();
        assert_eq!(output["inbounds"][0]["listen_port"], 19080);
        assert_eq!(output["route"]["final"], "node-a");
        assert!(output.get("experimental").is_none());
        assert!(output.get("endpoints").is_none());
        assert_eq!(output["dns"]["servers"].as_array().unwrap().len(), 1);
        assert!(output["dns"].get("rules").is_none());
    }

    #[test]
    fn refuses_to_start_a_second_wireguard_endpoint() {
        let input = json!({
            "inbounds": [{"type":"tun"}],
            "outbounds": [{"type":"direct","tag":"direct"}],
            "endpoints": [
                {"type":"wireguard","tag":"home-wg","peers":[{"allowed_ips":["10.8.28.0/24"]}]},
                {"type":"wireguard","tag":"other-wg","peers":[{"allowed_ips":["10.9.0.0/24"]}]}
            ],
            "route": {
                "default_domain_resolver":{"server":"bootstrap","strategy":"ipv4_only"},
                "rules": [{"outbound":"direct"}]
            },
            "dns": {"servers":[
                {"type":"tls","tag":"bootstrap"},
                {"type":"udp","tag":"home-wg-dns","detour":"home-wg"},
                {"type":"udp","tag":"other-wg-dns","detour":"other-wg"}
            ]},
            "experimental": {"clash_api":{"external_controller":"127.0.0.1:9090"}}
        });
        let error = sing_box_config(input, "home-wg", 19080, "user", "pass").unwrap_err();
        assert_eq!(
            error,
            "WireGuard diagnostics must reuse the endpoint in the managed core"
        );
    }

    #[test]
    fn isolates_clash_to_selected_node() {
        let input = json!({
            "mixed-port": 7890,
            "external-controller": "127.0.0.1:9090",
            "tun": {"enable":true},
            "proxies": [{"name":"node,a","type":"ss"}],
            "proxy-groups": []
        });
        let output = clash_config(input, "node,a", 19080, "user", "pass").unwrap();
        assert_eq!(output["port"], 19080);
        assert!(output.get("mixed-port").is_none());
        assert!(output.get("tun").is_none());
        assert_eq!(output["proxy-groups"][0]["proxies"][0], "node,a");
        assert_eq!(output["rules"][0], "MATCH,__sempre_node_test__");
    }
}
