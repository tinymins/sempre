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
use sempre_network::{IpMetadata, PublicIpProbe, lookup_ip_metadata};
use sempre_state::RuntimeState;
use serde::Serialize;
use serde_json::Value;
use tempfile::TempDir;
use tokio::{io::AsyncReadExt, net::TcpStream, process::Child, time::sleep};
use uuid::Uuid;

mod config;

use config::build_config;

const START_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_BODY_SIZE: u64 = 2 * 1024 * 1024;

pub(crate) struct DiagnosticCore {
    child: Child,
    directory: TempDir,
    client: Client,
    private_probe_url: Option<String>,
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

#[derive(Serialize)]
pub(crate) struct PublicIpResult {
    pub(crate) url: &'static str,
    pub(crate) status: u16,
    pub(crate) ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metadata: Option<IpMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metadata_error: Option<String>,
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
        fs::write(&diagnostic_config, config.text)
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
            .stderr(Stdio::piped())
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
            private_probe_url: config.private_probe_url,
        })
    }

    pub(crate) fn private_probe_url(&self) -> Option<&str> {
        self.private_probe_url.as_deref()
    }

    pub(crate) async fn latency(&self, url: &str) -> Result<u64, String> {
        let started = Instant::now();
        self.client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        Ok(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX))
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

    pub(crate) async fn public_ip(&self, probe: PublicIpProbe) -> Result<PublicIpResult, String> {
        let mut failures = Vec::new();
        for url in probe.urls() {
            match self.public_ip_at(probe, url).await {
                Ok(result) => return Ok(result),
                Err(error) => failures.push(format!("{url}: {error}")),
            }
        }
        Err(failures.join("; "))
    }

    async fn public_ip_at(
        &self,
        probe: PublicIpProbe,
        url: &'static str,
    ) -> Result<PublicIpResult, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() && !status.is_redirection() {
            return Err(format!("HTTP {}", status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BODY_SIZE)
        {
            return Err(format!("response body exceeds {MAX_BODY_SIZE} bytes"));
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            body.extend_from_slice(&chunk.map_err(|error| error.to_string())?);
            if body.len() as u64 > MAX_BODY_SIZE {
                return Err(format!("response body exceeds {MAX_BODY_SIZE} bytes"));
            }
        }
        let ip = probe.parse_response(&body)?;
        let (metadata, metadata_error) = match lookup_ip_metadata(&self.client, &ip).await {
            Ok(metadata) => (Some(metadata), None),
            Err(error) => (None, Some(error)),
        };
        Ok(PublicIpResult {
            url,
            status: status.as_u16(),
            ip,
            metadata,
            metadata_error,
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
            return Err(child_exit_error(child, status.to_string()).await);
        }
        if TcpStream::connect(address).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    let stderr = child_stderr(child).await;
    Err(with_stderr(
        "diagnostic core did not open its HTTP proxy in time".into(),
        &stderr,
    ))
}

async fn child_exit_error(child: &mut Child, status: String) -> String {
    let stderr = child_stderr(child).await;
    with_stderr(format!("diagnostic core exited with {status}"), &stderr)
}

async fn child_stderr(child: &mut Child) -> String {
    let Some(mut stderr) = child.stderr.take() else {
        return String::new();
    };
    let mut output = String::new();
    let _ = stderr.read_to_string(&mut output).await;
    output
}

fn with_stderr(message: String, stderr: &str) -> String {
    let detail = stderr.trim();
    if detail.is_empty() {
        return message;
    }
    let detail = detail
        .chars()
        .rev()
        .take(4000)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{message}: {detail}")
}

#[cfg(windows)]
fn hide_window(command: &mut tokio::process::Command) {
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut tokio::process::Command) {}
