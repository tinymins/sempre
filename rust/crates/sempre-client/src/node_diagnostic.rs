use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use reqwest::{Client, Proxy, redirect::Policy};
use sempre_manager::Manager;
use sempre_state::RuntimeState;
use serde::Serialize;
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tokio::{net::TcpStream, process::Child, time::sleep};
use uuid::Uuid;

const START_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_BODY_SIZE: u64 = 2 * 1024 * 1024;

pub(crate) struct DiagnosticCore {
    child: Child,
    directory: TempDir,
    client: Client,
}

#[derive(Serialize)]
pub(crate) struct HttpResult {
    pub(crate) url: String,
    pub(crate) status: u16,
    pub(crate) bytes: usize,
}

#[derive(Serialize)]
pub(crate) struct DnsResult {
    pub(crate) domain: String,
    pub(crate) resolver: &'static str,
    pub(crate) status: i64,
    pub(crate) answers: Vec<String>,
}

impl DiagnosticCore {
    pub(crate) async fn start(manager: &Manager, node: &str) -> Result<Self, String> {
        let document = manager.state().map_err(|error| error.to_string())?;
        if document.runtime.state != RuntimeState::Running {
            return Err("managed core is not running".into());
        }
        let core = required(document.runtime.core.as_deref(), "runtime core")?;
        if !matches!(core, "sing-box" | "mihomo" | "clash-rs") {
            return Err(format!(
                "core {core:?} does not support isolated node diagnostics"
            ));
        }
        let version = required(document.runtime.version.as_deref(), "runtime core version")?;
        let config_path = required(
            document.runtime.runtime_config.as_deref(),
            "runtime configuration",
        )?;
        let layout = manager.store().layout();
        let binary = layout.core_binary(core, document.runtime.repository.as_deref(), version);
        if !binary.is_file() {
            return Err(format!("core binary does not exist: {}", binary.display()));
        }

        let directory = tempfile::Builder::new()
            .prefix("node-test-")
            .tempdir_in(&layout.runtime)
            .map_err(|error| format!("create diagnostic directory: {error}"))?;
        let port = available_port().await?;
        let username = "sempre-node-test";
        let password = Uuid::new_v4().simple().to_string();
        let config = build_config(
            core,
            Path::new(config_path),
            node,
            port,
            username,
            &password,
        )?;
        let extension = if core == "sing-box" { "json" } else { "yaml" };
        let diagnostic_config = directory.path().join(format!("config.{extension}"));
        fs::write(&diagnostic_config, config)
            .map_err(|error| format!("write diagnostic configuration: {error}"))?;

        let adapter = sempre_core::built_in_registry()
            .get(core)
            .map_err(|error| error.to_string())?;
        let spec = adapter.run_spec(
            &binary.to_string_lossy(),
            &diagnostic_config.to_string_lossy(),
            &directory.path().to_string_lossy(),
        );
        let mut command = tokio::process::Command::new(&spec.program);
        command
            .args(&spec.arguments)
            .envs(&spec.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(working_directory) = spec.working_directory {
            command.current_dir(working_directory);
        }
        hide_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("start diagnostic core: {error}"))?;
        wait_until_ready(&mut child, port).await?;

        let proxy = Proxy::all(format!("http://127.0.0.1:{port}"))
            .map_err(|error| format!("configure diagnostic proxy: {error}"))?
            .basic_auth(username, &password);
        let client = Client::builder()
            .proxy(proxy)
            .redirect(Policy::limited(5))
            .timeout(REQUEST_TIMEOUT)
            .user_agent("Sempre node diagnostics")
            .build()
            .map_err(|error| format!("build diagnostic HTTP client: {error}"))?;
        Ok(Self {
            child,
            directory,
            client,
        })
    }

    pub(crate) async fn http_get(&self, url: &str) -> Result<HttpResult, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BODY_SIZE)
        {
            return Err(format!("response body exceeds {MAX_BODY_SIZE} bytes"));
        }
        let mut bytes = 0_usize;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            bytes += chunk.map_err(|error| error.to_string())?.len();
            if bytes as u64 > MAX_BODY_SIZE {
                return Err(format!("response body exceeds {MAX_BODY_SIZE} bytes"));
            }
        }
        Ok(HttpResult {
            url: url.into(),
            status,
            bytes,
        })
    }

    pub(crate) async fn dns_query(&self, domain: &str) -> Result<DnsResult, String> {
        let mut url = url::Url::parse("https://dns.google/resolve")
            .map_err(|error| format!("build DoH URL: {error}"))?;
        url.query_pairs_mut()
            .append_pair("name", domain)
            .append_pair("type", "A");
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("DoH returned HTTP {}", response.status().as_u16()));
        }
        let payload: Value = response.json().await.map_err(|error| error.to_string())?;
        let status = payload.get("Status").and_then(Value::as_i64).unwrap_or(-1);
        let answers = payload
            .get("Answer")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|answer| answer.get("data").and_then(Value::as_str))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if status != 0 {
            return Err(format!("DoH returned DNS status {status}"));
        }
        Ok(DnsResult {
            domain: domain.into(),
            resolver: "dns.google",
            status,
            answers,
        })
    }

    pub(crate) async fn stop(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        drop(self.directory);
    }
}

fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is unavailable"))
}

async fn available_port() -> Result<u16, String> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|error| format!("reserve diagnostic port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("read diagnostic port: {error}"))
}

async fn wait_until_ready(child: &mut Child, port: u16) -> Result<(), String> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let started = Instant::now();
    while started.elapsed() < START_TIMEOUT {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("inspect diagnostic core: {error}"))?
        {
            return Err(format!("diagnostic core exited with {status}"));
        }
        if TcpStream::connect(address).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill().await;
    Err("diagnostic core did not open its HTTP proxy in time".into())
}

fn build_config(
    core: &str,
    source: &Path,
    node: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let text = fs::read_to_string(source)
        .map_err(|error| format!("read runtime configuration: {error}"))?;
    if core == "sing-box" {
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("parse sing-box configuration: {error}"))?;
        let value = sing_box_config(value, node, port, username, password)?;
        serde_json::to_string_pretty(&value)
            .map_err(|error| format!("serialize diagnostic configuration: {error}"))
    } else {
        let value: Value = serde_yaml::from_str(&text)
            .map_err(|error| format!("parse Clash configuration: {error}"))?;
        let value = clash_config(value, node, port, username, password)?;
        serde_yaml::to_string(&value)
            .map_err(|error| format!("serialize diagnostic configuration: {error}"))
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
    root.insert(
        "inbounds".into(),
        json!([{"type":"http","tag":"sempre-node-test-in","listen":"127.0.0.1","listen_port":port,"users":[{"username":username,"password":password}]}]),
    );
    let mut route = Map::new();
    route.insert("auto_detect_interface".into(), Value::Bool(true));
    route.insert("final".into(), Value::String(node.into()));
    root.insert("route".into(), Value::Object(route));
    root.remove("experimental");
    root.remove("endpoints");
    if let Some(dns) = root.get_mut("dns").and_then(Value::as_object_mut) {
        dns.remove("rules");
    }
    Ok(value)
}

fn ensure_outbound(root: &Map<String, Value>, node: &str) -> Result<(), String> {
    let exists = root
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|outbound| outbound.get("tag").and_then(Value::as_str) == Some(node));
    if exists {
        Ok(())
    } else {
        Err(format!(
            "node {node:?} is not present in the runtime configuration"
        ))
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

#[cfg(windows)]
fn hide_window(command: &mut tokio::process::Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_sing_box_to_selected_node() {
        let input = json!({
            "inbounds": [{"type":"tun"}],
            "outbounds": [{"type":"shadowsocks","tag":"node-a"}],
            "route": {"rule_set":[{"tag":"remote"}]},
            "experimental": {"clash_api":{"external_controller":"127.0.0.1:9090"}},
            "dns": {"servers":[{"type":"local","tag":"local"}],"rules":[{"rule_set":"remote"}]}
        });
        let output = sing_box_config(input, "node-a", 19080, "user", "pass").unwrap();
        assert_eq!(output["inbounds"][0]["listen_port"], 19080);
        assert_eq!(output["route"]["final"], "node-a");
        assert!(output.get("experimental").is_none());
        assert!(output["dns"].get("rules").is_none());
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
