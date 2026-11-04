//! Proxies that only carry subscription downloads. Each runs as an isolated core started
//! for the download and stopped right after, so nothing stays running between refreshes.
use std::{
    fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use sempre_converter::{Profile, Source};
use sempre_proxy::{Connection, Engine};
use sempre_subscription::{Fetcher, SourceProxyRoute};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    time::sleep,
};
use uuid::Uuid;

use crate::{Manager, ManagerError, VersionRunner, fetch_proxy_rules::ResourceRules};

const CORE: &str = "sing-box";
const START_TIMEOUT: Duration = Duration::from_secs(15);
const TEST_TIMEOUT: Duration = Duration::from_secs(15);
const TEST_URL: &str = "https://www.gstatic.com/generate_204";
// The generated configuration uses the DNS server format sing-box introduced in 1.12.
const MINIMUM_MINOR: u32 = 12;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FetchProxy {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub custom_node_id: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FetchProxyInput {
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub custom_node_id: Option<String>,
    pub enabled: bool,
}

/// What a listing shows; only the editor reads the credentials in `content`.
#[derive(Clone, Debug, Serialize)]
pub struct FetchProxySummary {
    pub id: String,
    pub name: String,
    pub custom_node_id: Option<String>,
    pub enabled: bool,
    pub protocol: Option<String>,
    pub reference_count: usize,
    pub error: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
pub(super) struct Document {
    #[serde(default)]
    pub(super) proxies: Vec<FetchProxy>,
    #[serde(default)]
    pub(super) rules: ResourceRules,
}

pub(crate) struct SourceProxies {
    pub(crate) fetcher: Fetcher,
    pub(crate) warnings: Vec<String>,
    _running: Vec<Running>,
}

pub(super) struct Running {
    pub(super) route: SourceProxyRoute,
    _child: Child,
    _directory: tempfile::TempDir,
}

impl<R: VersionRunner> Manager<R> {
    pub fn fetch_proxies(&self) -> Result<Vec<FetchProxySummary>, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let document = self.read_fetch_proxies()?;
        Ok(document
            .proxies
            .iter()
            .map(|proxy| {
                let connection = connection(proxy, &catalog.custom_nodes);
                FetchProxySummary {
                    id: proxy.id.clone(),
                    name: proxy.name.clone(),
                    custom_node_id: proxy.custom_node_id.clone(),
                    enabled: proxy.enabled,
                    protocol: connection.as_ref().ok().map(|item| item.protocol.clone()),
                    reference_count: references(&catalog.profiles, &proxy.id)
                        + document.rules.references(&proxy.id),
                    error: connection.err().map(|error| error.to_string()),
                }
            })
            .collect())
    }

    pub fn fetch_proxy(&self, id: &str) -> Result<FetchProxy, ManagerError> {
        self.read_fetch_proxies()?
            .proxies
            .into_iter()
            .find(|proxy| proxy.id == id)
            .ok_or_else(|| not_found(id))
    }

    pub fn create_fetch_proxy(&self, input: FetchProxyInput) -> Result<FetchProxy, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let proxy = self.validated(Uuid::new_v4().to_string(), input)?;
        let mut document = self.read_fetch_proxies()?;
        document.proxies.push(proxy.clone());
        self.write_fetch_proxies(&document)?;
        Ok(proxy)
    }

    pub fn update_fetch_proxy(
        &self,
        id: &str,
        input: FetchProxyInput,
    ) -> Result<FetchProxy, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let proxy = self.validated(id.to_owned(), input)?;
        let mut document = self.read_fetch_proxies()?;
        let slot = document
            .proxies
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| not_found(id))?;
        *slot = proxy.clone();
        self.write_fetch_proxies(&document)?;
        Ok(proxy)
    }

    pub fn remove_fetch_proxy(&self, id: &str) -> Result<(), ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let mut document = self.read_fetch_proxies()?;
        let count =
            references(&self.subscriptions.read()?.profiles, id) + document.rules.references(id);
        if count > 0 {
            return Err(ManagerError::InvalidOperation(format!(
                "this proxy is used by {count} subscription source(s) or proxy rule(s); choose another one first"
            )));
        }
        let before = document.proxies.len();
        document.proxies.retain(|proxy| proxy.id != id);
        if document.proxies.len() == before {
            return Err(not_found(id));
        }
        self.write_fetch_proxies(&document)
    }

    /// Requests a known URL through the proxy and reports how long it took.
    pub async fn test_fetch_proxy(&self, id: &str) -> Result<u128, ManagerError> {
        let proxy = self.fetch_proxy(id)?;
        let running = self.start_fetch_proxy(&proxy).await?;
        let started = Instant::now();
        let route = &running.route;
        let client = sempre_http::client_builder()
            .proxy(
                reqwest::Proxy::all(format!("http://{}", route.address))
                    .map_err(invalid)?
                    .basic_auth(&route.username, &route.password),
            )
            .timeout(TEST_TIMEOUT)
            .build()
            .map_err(invalid)?;
        client
            .get(TEST_URL)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| {
                ManagerError::InvalidOperation(format!("proxy test failed: {error}"))
            })?;
        Ok(started.elapsed().as_millis())
    }

    /// A proxy that cannot start only fails the sources naming it; those never go direct.
    pub(crate) async fn start_source_proxies(&self, sources: &[Source]) -> SourceProxies {
        let mut ids = sources
            .iter()
            .filter(|source| source.enabled)
            .filter_map(|source| source.extra.get("proxy_id")?.as_str())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        let mut warnings = Vec::new();
        let mut running = Vec::new();
        for id in ids {
            let started = match self.fetch_proxy(id) {
                Ok(proxy) if proxy.enabled => self.start_fetch_proxy(&proxy).await,
                Ok(proxy) => Err(ManagerError::InvalidOperation(format!(
                    "proxy {:?} is disabled",
                    proxy.name
                ))),
                Err(error) => Err(error),
            };
            match started {
                Ok(proxy) => running.push(proxy),
                Err(error) => warnings.push(format!("subscription proxy {id}: {error}")),
            }
        }
        let routes = running
            .iter()
            .map(|proxy| SourceProxyRoute {
                id: proxy.route.id.clone(),
                address: proxy.route.address,
                username: proxy.route.username.clone(),
                password: proxy.route.password.clone(),
            })
            .collect::<Vec<_>>();
        let fetcher = match self.fetcher.with_source_proxies(&routes) {
            Ok(fetcher) => fetcher,
            Err(error) => {
                warnings.push(format!("subscription proxies: {error}"));
                self.fetcher.clone()
            }
        };
        SourceProxies {
            fetcher,
            warnings,
            _running: running,
        }
    }

    fn validated(&self, id: String, input: FetchProxyInput) -> Result<FetchProxy, ManagerError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(ManagerError::InvalidOperation(
                "proxy name cannot be empty".into(),
            ));
        }
        let custom_node_id = input.custom_node_id.filter(|node| !node.is_empty());
        let proxy = FetchProxy {
            id,
            name: name.into(),
            content: if custom_node_id.is_some() {
                String::new()
            } else {
                input.content
            },
            custom_node_id,
            enabled: input.enabled,
        };
        connection(&proxy, &self.subscriptions.read()?.custom_nodes)?;
        Ok(proxy)
    }

    pub(super) async fn start_fetch_proxy(
        &self,
        proxy: &FetchProxy,
    ) -> Result<Running, ManagerError> {
        let connection = connection(proxy, &self.subscriptions.read()?.custom_nodes)?;
        let binary = self.fetch_proxy_core()?;
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .and_then(|listener| listener.local_addr())
            .map_err(|error| ManagerError::io("reserve subscription proxy port", error))?
            .port();
        let username = "sempre-fetch".to_owned();
        let password = Uuid::new_v4().simple().to_string();
        let mut config = sempre_proxy::configuration(&connection, port, &username, &password);
        config["inbounds"][0]["listen"] = json!("127.0.0.1");
        let directory = tempfile::Builder::new()
            .prefix("fetch-proxy-")
            .tempdir_in(&self.store.layout().runtime)
            .map_err(|error| ManagerError::io("create subscription proxy directory", error))?;
        let path = directory.path().join("config.json");
        sempre_state::write_atomic(&path, config.to_string().as_bytes(), 0o600)
            .map_err(|error| ManagerError::io("write subscription proxy configuration", error))?;
        let spec = self.registry.get(CORE)?.run_spec(
            &binary.to_string_lossy(),
            &path.to_string_lossy(),
            &directory.path().to_string_lossy(),
        );
        let mut command = Command::new(&spec.program);
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
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .map_err(|error| ManagerError::io("start subscription proxy", error))?;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        wait_until_ready(&mut child, address).await?;
        Ok(Running {
            route: SourceProxyRoute {
                id: proxy.id.clone(),
                address,
                username,
                password,
            },
            _child: child,
            _directory: directory,
        })
    }

    fn fetch_proxy_core(&self) -> Result<PathBuf, ManagerError> {
        let document = self.store.read()?;
        let source = document.cores.get(CORE).map(|core| &core.default);
        let version = source
            .and_then(|source| {
                source
                    .channels
                    .get("stable")
                    .or_else(|| source.installed.keys().next_back())
            })
            .ok_or_else(|| {
                ManagerError::InvalidOperation(
                    "subscription proxies need sing-box to be installed".into(),
                )
            })?;
        let mut parts = version
            .split('.')
            .filter_map(|part| part.parse::<u32>().ok());
        if parts.next() == Some(1) && parts.next().is_some_and(|minor| minor < MINIMUM_MINOR) {
            return Err(ManagerError::InvalidOperation(format!(
                "subscription proxies need sing-box 1.{MINIMUM_MINOR} or newer, found {version}"
            )));
        }
        let binary = self.store.layout().core_binary(CORE, None, version);
        if !binary.is_file() {
            return Err(ManagerError::InvalidOperation(format!(
                "sing-box {version} is not installed"
            )));
        }
        Ok(binary)
    }

    pub(super) fn read_fetch_proxies(&self) -> Result<Document, ManagerError> {
        match fs::read(&self.store.layout().fetch_proxies) {
            Ok(data) => serde_json::from_slice(&data).map_err(invalid),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Document::default()),
            Err(error) => Err(ManagerError::io("read subscription proxies", error)),
        }
    }

    pub(super) fn write_fetch_proxies(&self, document: &Document) -> Result<(), ManagerError> {
        let data = serde_json::to_vec_pretty(document).map_err(invalid)?;
        sempre_state::write_atomic(&self.store.layout().fetch_proxies, &data, 0o600)
            .map_err(|error| ManagerError::io("write subscription proxies", error))
    }
}

async fn wait_until_ready(child: &mut Child, address: SocketAddr) -> Result<(), ManagerError> {
    let started = Instant::now();
    while started.elapsed() < START_TIMEOUT {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ManagerError::io("inspect subscription proxy", error))?
        {
            return Err(ManagerError::InvalidOperation(format!(
                "subscription proxy exited with {status}"
            )));
        }
        if TcpStream::connect(address).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    Err(ManagerError::InvalidOperation(
        "subscription proxy did not start in time".into(),
    ))
}

fn connection(
    proxy: &FetchProxy,
    nodes: &[sempre_converter::CustomNode],
) -> Result<Connection, ManagerError> {
    let content = match &proxy.custom_node_id {
        Some(id) => {
            let node = nodes.iter().find(|node| node.id == *id).ok_or_else(|| {
                ManagerError::InvalidOperation(format!("custom node {id:?} does not exist"))
            })?;
            json!({ "proxies": [node.proxy] }).to_string()
        }
        None => proxy.content.clone(),
    };
    sempre_proxy::parse(&content, Some(Engine::SingBox)).map_err(ManagerError::InvalidOperation)
}

fn references(profiles: &[Profile], id: &str) -> usize {
    profiles
        .iter()
        .flat_map(|profile| &profile.sources)
        .filter(|source| source.extra.get("proxy_id").and_then(Value::as_str) == Some(id))
        .count()
}

fn not_found(id: &str) -> ManagerError {
    ManagerError::InvalidOperation(format!("subscription proxy {id:?} does not exist"))
}

fn invalid(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::InvalidOperation(error.to_string())
}

#[cfg(test)]
#[path = "fetch_proxy_tests.rs"]
mod tests;
