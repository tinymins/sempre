//! The rules choosing how Sempre reaches the network for its own downloads: rule sets,
//! `GeoIP` lists, remote subscriptions, updates, UI archives, and cores. Same semantics as the
//! server's relay policy; a chosen proxy that is unavailable fails instead of going direct.
use std::time::{Duration, Instant};

use sempre_artifact::{Downloader, Egress, GithubClient};
use sempre_subscription::{ResourceRoute, ResourceRoutes, SourceProxyRoute};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

use crate::{
    Manager, ManagerError, VersionRunner,
    fetch_proxy::{Document, FetchProxy, Running},
};

const USER_AGENT: &str = concat!("Sempre/", env!("CARGO_PKG_VERSION"));
const PROBE_LIMIT: usize = 32 << 20;
const PROBE_TIMEOUT: Duration = Duration::from_mins(2);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceOutbound {
    #[default]
    System,
    Direct,
    Proxy {
        proxy_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceHostRule {
    pub host: String,
    pub outbound: ResourceOutbound,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceRules {
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub default_outbound: ResourceOutbound,
    #[serde(default)]
    pub host_rules: Vec<ResourceHostRule>,
}

impl ResourceRules {
    pub(crate) fn references(&self, id: &str) -> usize {
        self.outbounds()
            .filter(|outbound| matches!(outbound, ResourceOutbound::Proxy { proxy_id } if proxy_id == id))
            .count()
    }

    fn outbounds(&self) -> impl Iterator<Item = &ResourceOutbound> {
        std::iter::once(&self.default_outbound)
            .chain(self.host_rules.iter().map(|rule| &rule.outbound))
    }

    fn all_system(&self) -> bool {
        self.outbounds()
            .all(|outbound| *outbound == ResourceOutbound::System)
    }

    fn outbound_for(&self, host: &str) -> (Option<&str>, &ResourceOutbound) {
        let host = host.to_ascii_lowercase();
        self.host_rules
            .iter()
            .find(|rule| rule.host == host)
            .map_or((None, &self.default_outbound), |rule| {
                (Some(rule.host.as_str()), &rule.outbound)
            })
    }
}

/// Keeps the proxies these routes use running; dropping it stops them.
pub(crate) struct ActiveRoutes {
    pub(crate) routes: Option<ResourceRoutes>,
    _running: Vec<Running>,
}

impl ActiveRoutes {
    pub(crate) fn apply(
        &self,
        fetcher: &sempre_subscription::Fetcher,
    ) -> sempre_subscription::Fetcher {
        match &self.routes {
            Some(routes) => fetcher.with_resource_routes(routes.clone()),
            None => fetcher.clone(),
        }
    }
}

/// How to reach one URL; a proxy keeps running until this is dropped.
pub struct ResourceLease {
    pub egress: Egress,
    _running: Option<Running>,
}

impl ResourceLease {
    /// The rules left this request to the caller's usual path.
    pub fn is_system(&self) -> bool {
        self.egress == Egress::System
    }

    /// `None` leaves this request to the manager's usual client.
    pub(crate) fn github_client(&self) -> Result<Option<GithubClient>, ManagerError> {
        if self.is_system() {
            return Ok(None);
        }
        Ok(Some(GithubClient::with_egress(
            crate::USER_AGENT,
            &self.egress,
        )?))
    }

    /// `None` leaves this request to the manager's usual downloader.
    pub(crate) fn downloader(&self) -> Result<Option<Downloader>, ManagerError> {
        if self.is_system() {
            return Ok(None);
        }
        Ok(Some(Downloader::with_egress(
            crate::USER_AGENT,
            &self.egress,
        )?))
    }
}

impl<R: VersionRunner> Manager<R> {
    pub fn resource_rules(&self) -> Result<ResourceRules, ManagerError> {
        Ok(self.read_fetch_proxies()?.rules)
    }

    pub fn update_resource_rules(
        &self,
        mut rules: ResourceRules,
    ) -> Result<ResourceRules, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let mut document = self.read_fetch_proxies()?;
        if rules.revision != document.rules.revision {
            return Err(ManagerError::InvalidOperation(
                "proxy rules changed elsewhere; reload them first".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for rule in &mut rules.host_rules {
            rule.host = sempre_proxy::normalize_host(&rule.host)
                .map_err(|error| ManagerError::InvalidOperation(error.into()))?;
            if !seen.insert(rule.host.clone()) {
                return Err(ManagerError::InvalidOperation(format!(
                    "more than one rule for {}",
                    rule.host
                )));
            }
        }
        for outbound in rules.outbounds() {
            if let ResourceOutbound::Proxy { proxy_id } = outbound
                && !document
                    .proxies
                    .iter()
                    .any(|proxy| proxy.id == *proxy_id && proxy.enabled)
            {
                return Err(ManagerError::InvalidOperation(format!(
                    "proxy {proxy_id:?} does not exist or is disabled"
                )));
            }
        }
        rules.revision += 1;
        document.rules = rules;
        self.write_fetch_proxies(&document)?;
        Ok(document.rules)
    }

    /// Starts the proxies the rules use. Without rules nothing runs and callers keep their
    /// usual path, so this is free to call before any download.
    /// Downloads one resource through the proxy rules, choosing again at each redirect.
    pub(crate) async fn fetch_through_rules(
        &self,
        url: &str,
        user_agent: &str,
        limit: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>, ManagerError> {
        let active = self.start_resource_routes().await?;
        let routes = match &active.routes {
            Some(routes) => routes.clone(),
            None => ResourceRoutes::new(ResourceRoute::System, Vec::new())?,
        };
        let system = sempre_http::client_builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| ManagerError::InvalidOperation(error.to_string()))?;
        Ok(routes.fetch(url, user_agent, limit, &system).await?)
    }

    pub(crate) async fn start_resource_routes(&self) -> Result<ActiveRoutes, ManagerError> {
        let document = self.read_fetch_proxies()?;
        let rules = &document.rules;
        if rules.all_system() {
            return Ok(ActiveRoutes {
                routes: None,
                _running: Vec::new(),
            });
        }
        let mut running = Vec::new();
        let default = self
            .start_route(&document, &rules.default_outbound, &mut running)
            .await;
        let mut hosts = Vec::new();
        for rule in &rules.host_rules {
            let route = self
                .start_route(&document, &rule.outbound, &mut running)
                .await;
            hosts.push((rule.host.clone(), route));
        }
        Ok(ActiveRoutes {
            routes: Some(ResourceRoutes::new(default, hosts)?),
            _running: running,
        })
    }

    async fn start_route(
        &self,
        document: &Document,
        outbound: &ResourceOutbound,
        running: &mut Vec<Running>,
    ) -> ResourceRoute {
        let proxy_id = match outbound {
            ResourceOutbound::System => return ResourceRoute::System,
            ResourceOutbound::Direct => return ResourceRoute::Direct,
            ResourceOutbound::Proxy { proxy_id } => proxy_id,
        };
        if let Some(active) = running.iter().find(|item| item.route.id == *proxy_id) {
            return ResourceRoute::Proxy(copy_route(&active.route));
        }
        match self.start_named_proxy(document, proxy_id).await {
            Ok(proxy) => {
                let route = copy_route(&proxy.route);
                running.push(proxy);
                ResourceRoute::Proxy(route)
            }
            Err(error) => ResourceRoute::Unavailable(error.to_string()),
        }
    }

    async fn start_named_proxy(
        &self,
        document: &Document,
        id: &str,
    ) -> Result<Running, ManagerError> {
        let proxy: &FetchProxy = document
            .proxies
            .iter()
            .find(|proxy| proxy.id == id)
            .ok_or_else(|| {
                ManagerError::InvalidOperation(format!("subscription proxy {id:?} does not exist"))
            })?;
        if !proxy.enabled {
            return Err(ManagerError::InvalidOperation(format!(
                "proxy {:?} is disabled",
                proxy.name
            )));
        }
        self.start_fetch_proxy(proxy).await
    }

    /// For clients that follow redirects themselves: the starting host chooses the outbound
    /// for the whole request. Fails rather than going direct when that proxy is unavailable.
    pub async fn resource_egress(&self, url: &str) -> Result<ResourceLease, ManagerError> {
        let parsed = Url::parse(url)
            .map_err(|_| ManagerError::InvalidOperation(format!("invalid URL {url:?}")))?;
        let document = self.read_fetch_proxies()?;
        let (_, outbound) = document
            .rules
            .outbound_for(parsed.host_str().unwrap_or_default());
        Ok(match outbound {
            ResourceOutbound::System => ResourceLease {
                egress: Egress::System,
                _running: None,
            },
            ResourceOutbound::Direct => ResourceLease {
                egress: Egress::Direct,
                _running: None,
            },
            ResourceOutbound::Proxy { proxy_id } => {
                let proxy = self.start_named_proxy(&document, proxy_id).await?;
                ResourceLease {
                    egress: Egress::HttpProxy {
                        address: proxy.route.address,
                        username: proxy.route.username.clone(),
                        password: proxy.route.password.clone(),
                    },
                    _running: Some(proxy),
                }
            }
        })
    }

    pub(crate) async fn render_remote(
        &self,
        profile: &sempre_converter::Profile,
        target: &sempre_converter::Target,
    ) -> Result<sempre_subscription::RemoteResult, ManagerError> {
        let active = self.start_resource_routes().await?;
        let result = match &active.routes {
            Some(routes) => {
                self.remote
                    .with_resource_routes(routes.clone())
                    .render(profile, target)
                    .await
            }
            None => self.remote.render(profile, target).await,
        };
        Ok(result?)
    }

    /// Requests a URL with the saved rules and reports what happened.
    pub async fn test_resource_rules(&self, url: &str) -> Result<Value, ManagerError> {
        let parsed = Url::parse(url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
            .ok_or_else(|| ManagerError::InvalidOperation("invalid URL".into()))?;
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let document = self.read_fetch_proxies()?;
        let (matched, outbound) = document.rules.outbound_for(&host);
        let outbound_name = match outbound {
            ResourceOutbound::Proxy { proxy_id } => document
                .proxies
                .iter()
                .find(|proxy| proxy.id == *proxy_id)
                .map_or_else(|| proxy_id.clone(), |proxy| proxy.name.clone()),
            ResourceOutbound::System => "system".into(),
            ResourceOutbound::Direct => "direct".into(),
        };
        let mut active = self.start_resource_routes().await?;
        let routes = match active.routes.take() {
            Some(routes) => routes,
            None => ResourceRoutes::new(ResourceRoute::System, Vec::new())?,
        };
        let system = sempre_http::client_builder()
            .timeout(PROBE_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| ManagerError::InvalidOperation(error.to_string()))?;
        let started = Instant::now();
        let result = routes.probe(url, USER_AGENT, PROBE_LIMIT, &system).await;
        let elapsed_ms = started.elapsed().as_millis();
        Ok(match result {
            Ok((status, bytes)) => json!({
                "ok": (200..300).contains(&status), "host": host, "matched_host": matched,
                "outbound": outbound, "outbound_name": outbound_name,
                "http_status": status, "elapsed_ms": elapsed_ms, "body_bytes": bytes,
            }),
            Err(error) => json!({
                "ok": false, "host": host, "matched_host": matched,
                "outbound": outbound, "outbound_name": outbound_name,
                "http_status": null, "elapsed_ms": elapsed_ms, "body_bytes": 0,
                "error": error.to_string(),
            }),
        })
    }
}

fn copy_route(route: &SourceProxyRoute) -> SourceProxyRoute {
    SourceProxyRoute {
        id: route.id.clone(),
        address: route.address,
        username: route.username.clone(),
        password: route.password.clone(),
    }
}

#[cfg(test)]
#[path = "fetch_proxy_rules_tests.rs"]
mod tests;
