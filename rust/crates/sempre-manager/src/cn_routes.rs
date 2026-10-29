use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use sempre_transparent::{CnRouteIntent, normalize_cn_prefixes};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner, supervisor::RuntimePlan};

pub(crate) struct Source {
    pub path: PathBuf,
    pub binary: bool,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(crate) async fn prepare_cn_rules(
        &self,
        core: &str,
        binary: &Path,
        config: &Path,
    ) -> Result<(Option<Source>, crate::rule_bootstrap::RuleBootstrap), ManagerError> {
        if core != "sing-box" {
            if self.network_settings.read().cn_direct_routes {
                self.log_supervisor(&format!(
                    "CN route operation unavailable: adapter for {core} is not implemented"
                ));
            }
            return Ok((None, crate::rule_bootstrap::RuleBootstrap::default()));
        }
        let source = self.prepare_cn_source(binary, config).await;
        let rules = crate::rule_bootstrap::RuleBootstrap::prepare(&self.fetcher, config)?;
        match source {
            Ok(source) => Ok((source, rules)),
            Err(error @ (ManagerError::Io { .. } | ManagerError::Subscription(_))) => {
                self.log_supervisor(&format!("CN route source operation failed: {error}"));
                Ok((None, rules))
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) async fn configure_cn_runtime(
        &self,
        binary: &Path,
        runtime: &Path,
        source: Option<&Source>,
        transparent: &mut sempre_transparent::Plan,
    ) -> Result<(), ManagerError> {
        if let Some(source) = source {
            let interfaces = match self.transparent.cn_outbound_interfaces().await {
                Ok(interfaces) => interfaces,
                Err(error) => {
                    self.log_supervisor(&format!("read CN egress interfaces failed: {error}"));
                    [None, None]
                }
            };
            configure(
                binary,
                runtime,
                source,
                transparent,
                interfaces,
                self.network_settings.read().cn_routing_mode == crate::CnRoutingMode::DirectRoute,
                {
                    let settings = self.network_settings.read();
                    settings.intranet_routing_enabled
                        && settings.intranet_routing_mode == crate::IntranetRoutingMode::Proxy
                },
            )
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn prepare_cn_source(
        &self,
        binary: &Path,
        config: &Path,
    ) -> Result<Option<Source>, ManagerError> {
        if !self.network_settings.read().cn_direct_routes {
            return Ok(None);
        }
        let mut document: Value = serde_json::from_slice(
            &fs::read(config).map_err(|e| ManagerError::io("read CN runtime configuration", e))?,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let definitions = document
            .pointer_mut("/route/rule_set")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("CN pre-routing requires geoip-cn"))?;
        let definition = definitions
            .iter_mut()
            .find(|r| r["tag"] == "geoip-cn")
            .ok_or_else(|| invalid("CN pre-routing requires geoip-cn"))?;
        let format = definition["format"].as_str().unwrap_or("binary").to_owned();
        if definition["type"] == "remote" {
            let url = definition["url"]
                .as_str()
                .ok_or_else(|| invalid("CN rules have no URL"))?
                .to_owned();
            let snapshot = if let Some(snapshot) = self.fetcher.cached_rule_set(&url, &format)? {
                snapshot
            } else {
                let candidate = self.fetcher.fetch_rule_set(&url).await?;
                read_prefixes(
                    binary,
                    &Source {
                        path: candidate.path.clone(),
                        binary: format == "binary",
                    },
                )
                .await?;
                self.fetcher.accept_rule_set(&url, &format, &candidate)?;
                self.fetcher
                    .cached_rule_set(&url, &format)?
                    .ok_or_else(|| invalid("CN cache publication failed"))?
            };
            // RuleBootstrap retains the remote definition for periodic refresh.
            return Ok(Some(Source {
                path: snapshot.path,
                binary: format == "binary",
            }));
        }
        if definition["type"] != "local" {
            return Err(invalid(
                "CN pre-routing requires a local or remote IP-only rule set",
            ));
        }
        let mut path = PathBuf::from(
            definition["path"]
                .as_str()
                .ok_or_else(|| invalid("CN rule file has no path"))?,
        );
        if path.is_relative() {
            path = config
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| invalid("runtime has no working directory"))?
                .join(path);
        }
        Ok(Some(Source {
            path,
            binary: format == "binary",
        }))
    }

    pub(crate) async fn monitor_cn_routes(&self, plan: &RuntimePlan) {
        let (Some(source), Some(initial)) = (&plan.cn_source, &plan.transparent.cn_routes) else {
            std::future::pending::<()>().await;
            return;
        };
        let binary = self.store.layout().core_binary(
            &plan.deployment.core,
            plan.deployment.repository.as_deref(),
            &plan.deployment.version,
        );
        let mut events = match sempre_transparent::RouteEvents::open() {
            Ok(events) => Some(events),
            Err(error) => {
                self.log_supervisor(&format!(
                    "CN route events unavailable; following rule updates only: {error}"
                ));
                None
            }
        };
        let mut intent = initial.clone();
        let mut hash = String::new();
        loop {
            // Event driven: an idle host never wakes this loop.
            let changed = tokio::select! {
                result = route_changed(&mut events) => result,
                () = self.cn_rules_changed.notified() => Ok(()),
            };
            if let Err(error) = changed {
                self.log_supervisor(&format!(
                    "CN route events failed; following rule updates only: {error}"
                ));
                events = None;
                continue;
            }
            let result = async {
                let _gate = self.runtime_rules_gate.lock().await;
                let content = fs::read(&source.path)
                    .map_err(|e| ManagerError::io("read live CN snapshot", e))?;
                let current = format!("{:x}", Sha256::digest(&content));
                if current != hash {
                    intent.prefixes = read_prefixes(&binary, source).await?;
                    hash = current;
                }
                let started = std::time::Instant::now();
                let report = self.transparent.apply_cn_routes(&intent).await?;
                if report.added + report.removed > 0 {
                    self.log_supervisor(&format!(
                        "CN route drift repaired: {} restored, {} removed of {} expected ({} ms)",
                        report.added,
                        report.removed,
                        report.expected,
                        started.elapsed().as_millis()
                    ));
                }
                Ok::<_, ManagerError>(())
            }
            .await;
            if let Err(error) = result {
                self.log_supervisor(&format!("CN routing operation failed: {error}"));
            }
        }
    }
}

async fn route_changed(
    events: &mut Option<sempre_transparent::RouteEvents>,
) -> Result<(), sempre_transparent::TransparentError> {
    match events {
        Some(events) => events.changed().await,
        None => std::future::pending().await,
    }
}

pub(crate) async fn configure(
    binary: &Path,
    runtime: &Path,
    source: &Source,
    transparent: &mut sempre_transparent::Plan,
    outbound_interfaces: [Option<String>; 2],
    direct: bool,
    intranet_proxy: bool,
) -> Result<(), ManagerError> {
    let prefixes = read_prefixes(binary, source).await?;
    let mut document: Value = serde_json::from_slice(
        &fs::read(runtime).map_err(|e| ManagerError::io("read CN runtime", e))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    let fakeip_prefixes = fakeip_capture_prefixes(&document);
    let tun = document["inbounds"]
        .as_array_mut()
        .and_then(|a| {
            a.iter_mut()
                .find(|r| r["tag"] == "tun-in" && r["type"] == "tun")
        })
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("CN pre-routing requires the managed sing-box TUN"))?;
    let mut capture_prefixes = tun
        .get("route_address")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let intranet_exclusions = intranet_proxy.then(|| {
        tun.get("route_exclude_address")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    });
    for prefix in fakeip_prefixes {
        if !capture_prefixes.contains(&prefix) {
            capture_prefixes.push(prefix);
        }
    }
    tun.insert("auto_route".into(), json!(false));
    tun.insert("auto_redirect".into(), json!(false));
    tun.insert("strict_route".into(), json!(false));
    for field in [
        "route_address",
        "route_exclude_address",
        "route_address_set",
        "route_exclude_address_set",
        "dns_address",
        "dns_mode",
    ] {
        tun.remove(field);
    }
    let addresses = tun
        .get_mut("address")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("TUN has no addresses"))?;
    if !addresses
        .iter()
        .any(|a| a.as_str().is_some_and(|a| a.contains(':')))
    {
        addresses.push(json!("fdfe:dcba:9876::1/126"));
    }
    let tun_addresses = addresses
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if !direct {
        sempre_converter::apply_cn_proxy(&mut document)?;
    }
    // Bound core sockets must use the original physical default, never TUN.
    if let Some(interface) = outbound_interfaces.iter().flatten().next() {
        document["route"]["auto_detect_interface"] = json!(false);
        document["route"]["default_interface"] = json!(interface);
    }
    if cfg!(target_os = "linux") {
        document["route"]["default_mark"] = json!(sempre_transparent::BYPASS_MARK);
    }
    sempre_state::write_atomic(
        runtime,
        &serde_json::to_vec(&document).map_err(|error| invalid(error.to_string()))?,
        0o600,
    )
    .map_err(|e| ManagerError::io("write CN runtime", e))?;
    transparent.cn_routes = Some(CnRouteIntent {
        direct,
        prefixes,
        tun_addresses,
        capture_prefixes,
        intranet_exclusions,
        outbound_interfaces,
    });
    Ok(())
}

fn fakeip_capture_prefixes(document: &Value) -> Vec<String> {
    let servers = document["dns"]["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|server| server["type"] == "fakeip");
    let legacy = document
        .pointer("/dns/fakeip")
        .filter(|config| config["enabled"] == true);
    let mut prefixes = Vec::new();
    for config in servers.chain(legacy) {
        for field in ["inet4_range", "inet6_range"] {
            let Some(prefix) = config[field]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let prefix = prefix.to_owned();
            if !prefixes.contains(&prefix) {
                prefixes.push(prefix);
            }
        }
    }
    prefixes
}

pub(crate) async fn read_prefixes(
    binary: &Path,
    source: &Source,
) -> Result<Vec<String>, ManagerError> {
    let bytes = if source.binary {
        let temporary =
            tempfile::tempdir().map_err(|e| ManagerError::io("create CN decode directory", e))?;
        let output = temporary.path().join("cn.json");
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::process::Command::new(binary)
                .args(["rule-set", "decompile", "--output"])
                .arg(&output)
                .arg(&source.path)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| invalid("CN rule decompile timed out"))?
        .map_err(|e| ManagerError::io("decompile CN rule set", e))?;
        if !result.status.success() {
            return Err(invalid(format!(
                "CN rule decompile failed: {}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
        fs::read(output).map_err(|e| ManagerError::io("read decoded CN rules", e))?
    } else {
        fs::read(&source.path).map_err(|e| ManagerError::io("read CN rules", e))?
    };
    parse_prefixes(&bytes)
}

fn parse_prefixes(bytes: &[u8]) -> Result<Vec<String>, ManagerError> {
    let document: Value =
        serde_json::from_slice(bytes).map_err(|error| invalid(error.to_string()))?;
    let rules = document["rules"]
        .as_array()
        .ok_or_else(|| invalid("CN rule set has no rules"))?;
    let mut prefixes = Vec::new();
    for rule in rules {
        let fields = rule
            .as_object()
            .ok_or_else(|| invalid("CN rule is not an object"))?;
        if fields.keys().any(|k| k != "ip_cidr") {
            return Err(invalid(
                "CN rule set must contain only destination IP CIDRs",
            ));
        }
        match &rule["ip_cidr"] {
            Value::String(prefix) => prefixes.push(prefix.clone()),
            Value::Array(values) => {
                for value in values {
                    prefixes.push(
                        value
                            .as_str()
                            .ok_or_else(|| invalid("CN CIDR is not a string"))?
                            .into(),
                    );
                }
            }
            _ => return Err(invalid("CN rule has no IP CIDRs")),
        }
    }
    normalize_cn_prefixes(&prefixes).map_err(Into::into)
}

fn invalid(message: impl Into<String>) -> ManagerError {
    ManagerError::InvalidOperation(message.into())
}
