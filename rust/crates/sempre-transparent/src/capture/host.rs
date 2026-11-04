use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};

use super::{
    CaptureIntent, MARK, ROUTE_TABLE, RULE_PRIORITY, Ruleset, TABLE, discovery, geoip, script,
};
use crate::{TransparentError, command};

const LISTENER_ATTEMPTS: u32 = 20;
const UCI_SECTION: &str = "firewall.sempre_capture";

pub struct CaptureController {
    runner: Arc<dyn command::Runner>,
    sys: PathBuf,
    proc_net: PathBuf,
    active_script: PathBuf,
    status: PathBuf,
    cn_list: PathBuf,
    include: PathBuf,
    enabled: bool,
}

impl CaptureController {
    pub(crate) fn new(layout: &sempre_state::Layout) -> Self {
        Self::with_runner(
            layout,
            Arc::new(command::SystemRunner),
            "/sys/class/net",
            "/proc/net",
        )
    }

    pub(crate) fn with_runner(
        layout: &sempre_state::Layout,
        runner: Arc<dyn command::Runner>,
        sys: impl Into<PathBuf>,
        proc_net: impl Into<PathBuf>,
    ) -> Self {
        Self {
            runner,
            sys: sys.into(),
            proc_net: proc_net.into(),
            active_script: layout.runtime.join("capture.nft"),
            status: layout.runtime.join("capture-status.json"),
            cn_list: layout.capture_cn_list.clone(),
            include: layout.home.join("firewall.include"),
            enabled: layout.openwrt,
        }
    }

    /// Replaces the rules once the core listens, then keeps them across core restarts.
    pub async fn apply(&self, intent: &CaptureIntent) -> Result<(), TransparentError> {
        if !self.enabled {
            return Ok(());
        }
        self.wait_for_listener(intent.tproxy_port).await?;
        let (mut rules, reason) = self.build(intent).await?;
        let mut reason = reason.to_owned();
        let mut text = script(&rules);
        if rules.offload.is_some() && self.check(&text).await.is_err() {
            rules.offload = None;
            reason = "kernel-unavailable".into();
            text = script(&rules);
        }
        self.check(&text).await?;
        self.ensure_routes().await?;
        self.load(&text).await?;
        write(&self.active_script, text.as_bytes(), 0o600)?;
        let status = json!({
            "active": true,
            "tproxy_port": rules.tproxy_port,
            "dns_port": rules.dns_port,
            "lan_interfaces": rules.lan_interfaces,
            "cn_prefixes": rules.cn_prefixes.len(),
            "bypass_macs": rules.bypass_macs.len(),
            "offload": rules.offload.is_some(),
            "offload_reason": reason,
        });
        write(&self.status, status.to_string().as_bytes(), 0o600)?;
        self.register_include().await
    }

    /// Restores rules that a firewall reload removed. Does nothing after a stop.
    pub async fn reapply(&self) -> Result<bool, TransparentError> {
        let Ok(text) = fs::read_to_string(&self.active_script) else {
            return Ok(false);
        };
        self.ensure_routes().await?;
        self.load(&text).await?;
        Ok(true)
    }

    pub async fn withdraw(&self) -> Result<(), TransparentError> {
        if !self.enabled {
            return Ok(());
        }
        let _ = fs::remove_file(&self.active_script);
        let _ = fs::remove_file(&self.status);
        let delete = format!("add table inet {TABLE}\ndelete table inet {TABLE}\n");
        let table = self.load(&delete).await;
        let priority = RULE_PRIORITY.to_string();
        let route = ROUTE_TABLE.to_string();
        let _ = self
            .runner
            .run(
                "ip",
                &[
                    "-4", "rule", "del", "priority", &priority, "fwmark", MARK, "table", &route,
                ],
                None,
            )
            .await;
        let _ = self
            .runner
            .run(
                "ip",
                &[
                    "-4", "route", "del", "local", "default", "dev", "lo", "table", &route,
                ],
                None,
            )
            .await;
        table
    }

    pub fn status(&self) -> Value {
        fs::read(&self.status)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_else(|| json!({ "active": false }))
    }

    /// Age of the installed list, or `None` when no list has been fetched yet.
    pub fn cn_list_age(&self) -> Option<Duration> {
        fs::metadata(&self.cn_list)
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(|modified| modified.elapsed().unwrap_or_default())
    }

    pub fn read_cn_list(&self) -> Option<String> {
        fs::read_to_string(&self.cn_list).ok()
    }

    pub fn install_cn_list(&self, text: &str) -> Result<(), TransparentError> {
        if super::parse_cidrs(text)?.is_empty() {
            return Err(TransparentError::Invalid("GeoIP list is empty".into()));
        }
        write(&self.cn_list, text.as_bytes(), 0o644)
    }

    async fn build(
        &self,
        intent: &CaptureIntent,
    ) -> Result<(Ruleset, &'static str), TransparentError> {
        let settings = &intent.settings;
        discovery::validate_macs(&settings.bypass_macs)?;
        discovery::validate_names(&settings.lan_interfaces)?;
        discovery::validate_names(&settings.offload_wan_interfaces)?;
        let addresses = command::require_success(
            "ip",
            self.runner
                .run("ip", &["-o", "-4", "addr", "show", "scope", "global"], None)
                .await?,
        )?;
        let local_nets = discovery::bridge_addresses(&addresses.stdout)
            .into_iter()
            .map(|(_, address)| address)
            .collect::<Vec<_>>();
        let lan_interfaces = discovery::resolve_lan(self.runner.as_ref(), settings).await?;
        discovery::validate_names(&lan_interfaces)?;
        if local_nets.is_empty() || lan_interfaces.is_empty() {
            return Err(TransparentError::Invalid(
                "gateway capture found no LAN networks".into(),
            ));
        }
        let (offload, reason) =
            discovery::offload(self.runner.as_ref(), &self.sys, settings, &lan_interfaces).await;
        let gateways = local_nets
            .iter()
            .filter_map(|net| net.split('/').next().map(str::to_owned))
            .collect();
        Ok((
            Ruleset {
                tproxy_port: intent.tproxy_port,
                dns_port: intent.dns_port,
                lan_interfaces,
                local_nets,
                gateways,
                bypass_macs: settings.bypass_macs.clone(),
                cn_prefixes: geoip::load(&self.cn_list)?,
                offload,
            },
            reason,
        ))
    }

    async fn wait_for_listener(&self, port: u16) -> Result<(), TransparentError> {
        for attempt in 0..LISTENER_ATTEMPTS {
            if self.listening(port) {
                return Ok(());
            }
            if attempt + 1 < LISTENER_ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        Err(TransparentError::Invalid(format!(
            "core is not listening on TProxy port {port}; capture rules left unchanged"
        )))
    }

    fn listening(&self, port: u16) -> bool {
        ["tcp", "tcp6"].iter().any(|name| {
            fs::read_to_string(self.proc_net.join(name))
                .is_ok_and(|table| table_has_listener(&table, port))
        })
    }

    async fn check(&self, text: &str) -> Result<(), TransparentError> {
        command::require_success(
            "nft",
            self.runner
                .run("nft", &["-c", "-f", "-"], Some(text.as_bytes()))
                .await?,
        )?;
        Ok(())
    }

    async fn load(&self, text: &str) -> Result<(), TransparentError> {
        command::require_success(
            "nft",
            self.runner
                .run("nft", &["-f", "-"], Some(text.as_bytes()))
                .await?,
        )?;
        Ok(())
    }

    async fn ensure_routes(&self) -> Result<(), TransparentError> {
        let route = ROUTE_TABLE.to_string();
        let priority = RULE_PRIORITY.to_string();
        command::require_success(
            "ip",
            self.runner
                .run(
                    "ip",
                    &[
                        "-4", "route", "replace", "local", "default", "dev", "lo", "table", &route,
                    ],
                    None,
                )
                .await?,
        )?;
        let rules = command::require_success(
            "ip",
            self.runner.run("ip", &["-4", "rule", "show"], None).await?,
        )?;
        let prefix = format!("{priority}:");
        match rules.stdout.lines().find(|line| line.starts_with(&prefix)) {
            Some(line) if line.contains(&format!("fwmark {MARK}")) => Ok(()),
            Some(line) => Err(TransparentError::Invalid(format!(
                "policy rule priority {priority} is already in use: {line}"
            ))),
            None => {
                command::require_success(
                    "ip",
                    self.runner
                        .run(
                            "ip",
                            &[
                                "-4", "rule", "add", "priority", &priority, "fwmark", MARK,
                                "table", &route,
                            ],
                            None,
                        )
                        .await?,
                )?;
                Ok(())
            }
        }
    }

    /// `fw4` runs this script after each reload; the script itself finds nothing to do
    /// once capture has been withdrawn.
    async fn register_include(&self) -> Result<(), TransparentError> {
        let executable = std::env::current_exe().map_err(|source| TransparentError::Io {
            context: "locate the Sempre executable".into(),
            source,
        })?;
        let body = format!(
            "#!/bin/sh\nexec {} --system capture reapply\n",
            executable.display()
        );
        write(&self.include, body.as_bytes(), 0o755)?;
        let path = self.include.display().to_string();
        let current = self
            .runner
            .run("uci", &["-q", "get", &format!("{UCI_SECTION}.path")], None)
            .await?;
        if current.success && current.stdout.trim() == path {
            return Ok(());
        }
        for setting in [
            format!("{UCI_SECTION}=include"),
            format!("{UCI_SECTION}.type=script"),
            format!("{UCI_SECTION}.path={path}"),
            format!("{UCI_SECTION}.fw4_compatible=1"),
        ] {
            command::require_success(
                "uci",
                self.runner.run("uci", &["set", &setting], None).await?,
            )?;
        }
        command::require_success(
            "uci",
            self.runner
                .run("uci", &["commit", "firewall"], None)
                .await?,
        )?;
        Ok(())
    }
}

fn write(path: &Path, data: &[u8], mode: u32) -> Result<(), TransparentError> {
    sempre_state::write_atomic(path, data, mode).map_err(|source| TransparentError::Io {
        context: format!("write {}", path.display()),
        source,
    })
}

/// `/proc/net/tcp` state `0A` is LISTEN; the local port is the hex after the colon.
pub(super) fn table_has_listener(table: &str, port: u16) -> bool {
    let wanted = format!("{port:04X}");
    table.lines().skip(1).any(|line| {
        let mut fields = line.split_whitespace();
        let local = fields.nth(1);
        let state = fields.nth(1);
        state == Some("0A")
            && local
                .and_then(|local| local.rsplit(':').next())
                .is_some_and(|value| value.eq_ignore_ascii_case(&wanted))
    })
}
