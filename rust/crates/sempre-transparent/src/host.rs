use std::{sync::Arc, time::Duration};

use tokio::{net::TcpStream, time::sleep};

use crate::{
    Mode, Plan, TransparentError, command, macos_dns::SystemDns as MacSystemDns, nft, policy,
    system_dns::SystemDns, windows_dns::SystemDns as WindowsSystemDns,
};

const TUN_TIMEOUT: Duration = Duration::from_secs(20);
const LISTENER_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct Controller {
    runner: Arc<dyn command::Runner>,
    system_dns: SystemDns,
    macos_dns: MacSystemDns,
    windows_dns: WindowsSystemDns,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            runner: Arc::new(command::SystemRunner),
            system_dns: SystemDns::disabled(),
            macos_dns: MacSystemDns::new(false, std::path::PathBuf::new()),
            windows_dns: WindowsSystemDns::new(false, std::path::PathBuf::new()),
        }
    }
}

impl Controller {
    pub fn new(layout: &sempre_state::Layout) -> Self {
        Self {
            runner: Arc::new(command::SystemRunner),
            system_dns: SystemDns::new(
                cfg!(target_os = "linux") && layout.mode == sempre_state::Mode::System,
                layout.home.join("system-dns"),
                "/etc/resolv.conf".into(),
            ),
            macos_dns: MacSystemDns::new(
                cfg!(target_os = "macos") && layout.mode == sempre_state::Mode::System,
                layout.home.join("system-dns").join("macos"),
            ),
            windows_dns: WindowsSystemDns::new(
                cfg!(target_os = "windows") && layout.mode == sempre_state::Mode::System,
                layout.home.join("system-dns").join("windows"),
            ),
        }
    }

    pub async fn prepare(
        &self,
        core: &str,
        core_version: &str,
        profile: &sempre_converter::Profile,
        runtime_config: &std::path::Path,
    ) -> Result<Plan, TransparentError> {
        if cfg!(target_os = "macos") {
            if crate::system_dns_intent(profile).is_none() {
                return Ok(Plan::default());
            }
            let upstreams = self
                .macos_dns
                .discover_upstreams(self.runner.as_ref())
                .await?;
            return crate::desktop_plan::prepare(
                crate::desktop_plan::Platform::Macos,
                core,
                core_version,
                profile,
                runtime_config,
                upstreams,
            );
        }
        if cfg!(target_os = "windows") {
            if crate::system_dns_intent(profile).is_none() {
                return Ok(Plan::default());
            }
            let upstreams = self
                .windows_dns
                .discover_upstreams(self.runner.as_ref())
                .await?;
            return crate::desktop_plan::prepare(
                crate::desktop_plan::Platform::Windows,
                core,
                core_version,
                profile,
                runtime_config,
                upstreams,
            );
        }
        if !cfg!(target_os = "linux") {
            return Ok(Plan::default());
        }
        let inventory = sempre_network::inventory()?;
        crate::prepare_with_inventory_authorized(
            core,
            profile,
            runtime_config,
            &inventory,
            self.system_dns.allowed(),
        )
    }

    pub async fn apply(&self, plan: &Plan) -> Result<(), TransparentError> {
        if cfg!(target_os = "macos") {
            return self.apply_macos(plan).await;
        }
        if cfg!(target_os = "windows") {
            return self.apply_windows(plan).await;
        }
        if !cfg!(target_os = "linux") || !plan.active() {
            return Ok(());
        }
        self.require_root().await?;
        self.system_dns.restore()?;
        if plan.enabled() {
            policy::check_collisions(self.runner.as_ref()).await?;
            nft::check_collisions(self.runner.as_ref()).await?;
            self.cleanup_network().await?;
            self.wait_ready(plan).await?;
            if plan.mode == Mode::TProxy {
                self.check_forwarding(plan).await?;
                if let Err(error) = policy::apply(self.runner.as_ref()).await {
                    let _ = self.cleanup_network().await;
                    return Err(error);
                }
                if let Err(error) = nft::apply(self.runner.as_ref(), plan).await {
                    let _ = self.cleanup_network().await;
                    return Err(error);
                }
            }
        }
        if let Some(system_dns) = &plan.system_dns {
            self.wait_system_dns(system_dns).await?;
            self.system_dns.apply()?;
        }
        if let Err(error) = self.verify(plan).await {
            let _ = self.cleanup_owned().await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn verify(&self, plan: &Plan) -> Result<(), TransparentError> {
        if cfg!(target_os = "macos") {
            if plan.system_dns.is_some() {
                self.macos_dns.verify(self.runner.as_ref()).await?;
            }
            return Ok(());
        }
        if cfg!(target_os = "windows") {
            if plan.system_dns.is_some() {
                self.windows_dns.verify(self.runner.as_ref()).await?;
            }
            return Ok(());
        }
        if !cfg!(target_os = "linux") || !plan.active() {
            return Ok(());
        }
        if plan.enabled() {
            self.wait_ready(plan).await?;
            if plan.mode == Mode::TProxy {
                nft::verify(self.runner.as_ref()).await?;
                policy::verify(self.runner.as_ref()).await?;
            }
        }
        if plan.system_dns.is_some() {
            self.system_dns.verify()?;
        }
        Ok(())
    }

    pub async fn cleanup(&self) -> Result<(), TransparentError> {
        if cfg!(target_os = "macos") {
            if self.is_root().await? {
                return self.macos_dns.restore(self.runner.as_ref()).await;
            }
            return Ok(());
        }
        if cfg!(target_os = "windows") {
            if self.is_root().await? {
                return self.windows_dns.restore(self.runner.as_ref()).await;
            }
            return Ok(());
        }
        if !cfg!(target_os = "linux") || !self.is_root().await? {
            return Ok(());
        }
        self.cleanup_owned().await
    }

    async fn cleanup_owned(&self) -> Result<(), TransparentError> {
        let dns_result = self.system_dns.restore();
        let network_result = self.cleanup_network().await;
        dns_result.and(network_result)
    }

    pub async fn recover_stale_system_dns(&self) -> Result<(), TransparentError> {
        if cfg!(target_os = "macos") && self.macos_dns.allowed() && self.is_root().await? {
            self.macos_dns.restore(self.runner.as_ref()).await?;
        }
        if cfg!(target_os = "windows") && self.windows_dns.allowed() && self.is_root().await? {
            self.windows_dns.restore(self.runner.as_ref()).await?;
        }
        Ok(())
    }

    async fn apply_macos(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !plan.active() {
            return Ok(());
        }
        self.require_root().await?;
        if plan.system_dns.is_some() && self.macos_dns.verify(self.runner.as_ref()).await.is_ok() {
            return Ok(());
        }
        self.macos_dns.restore(self.runner.as_ref()).await?;
        if let Some(system_dns) = &plan.system_dns {
            self.wait_system_dns(system_dns).await?;
            if let Err(error) = self
                .macos_dns
                .apply(self.runner.as_ref(), &system_dns.original_upstreams)
                .await
            {
                let _ = self.macos_dns.restore(self.runner.as_ref()).await;
                return Err(error);
            }
        }
        if let Err(error) = self.verify(plan).await {
            let _ = self.macos_dns.restore(self.runner.as_ref()).await;
            return Err(error);
        }
        Ok(())
    }

    async fn apply_windows(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !plan.active() {
            return Ok(());
        }
        self.require_root().await?;
        if plan.system_dns.is_some() && self.windows_dns.verify(self.runner.as_ref()).await.is_ok()
        {
            return Ok(());
        }
        self.windows_dns.restore(self.runner.as_ref()).await?;
        if let Some(system_dns) = &plan.system_dns {
            self.wait_system_dns(system_dns).await?;
            if let Err(error) = self
                .windows_dns
                .apply(self.runner.as_ref(), &system_dns.original_upstreams)
                .await
            {
                let _ = self.windows_dns.restore(self.runner.as_ref()).await;
                return Err(error);
            }
        }
        if let Err(error) = self.verify(plan).await {
            let _ = self.windows_dns.restore(self.runner.as_ref()).await;
            return Err(error);
        }
        Ok(())
    }

    async fn cleanup_network(&self) -> Result<(), TransparentError> {
        let nft_result = nft::delete_owned(self.runner.as_ref()).await;
        let policy_result = policy::delete(self.runner.as_ref()).await;
        nft_result.and(policy_result)
    }

    async fn wait_system_dns(&self, plan: &crate::SystemDnsPlan) -> Result<(), TransparentError> {
        let started = std::time::Instant::now();
        for host in &plan.listen_hosts {
            let host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
            loop {
                match TcpStream::connect((host, plan.listen_port)).await {
                    Ok(_) => break,
                    Err(_) if started.elapsed() < LISTENER_TIMEOUT => sleep(POLL_INTERVAL).await,
                    Err(error) => {
                        return Err(TransparentError::Invalid(format!(
                            "system DNS listener {host}:{} did not become ready: {error}",
                            plan.listen_port
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    async fn wait_ready(&self, plan: &Plan) -> Result<(), TransparentError> {
        let timeout = if plan.mode == Mode::Tun {
            TUN_TIMEOUT
        } else {
            LISTENER_TIMEOUT
        };
        let started = std::time::Instant::now();
        loop {
            let result = if plan.mode == Mode::Tun {
                tun_ready(plan)
            } else {
                listeners_ready(plan).await
            };
            if result.is_ok() {
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err(TransparentError::Invalid(format!(
                    "transparent proxy did not become ready after {timeout:?}: {}",
                    result.expect_err("readiness failed")
                )));
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn require_root(&self) -> Result<(), TransparentError> {
        if self.is_root().await? {
            Ok(())
        } else {
            Err(TransparentError::Invalid(
                "system network integration requires administrator privileges".into(),
            ))
        }
    }

    async fn is_root(&self) -> Result<bool, TransparentError> {
        if cfg!(target_os = "windows") {
            let script = "$p=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()); if($p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){exit 0}else{exit 1}";
            let output = self
                .runner
                .run(
                    "powershell.exe",
                    &["-NoProfile", "-NonInteractive", "-Command", script],
                    None,
                )
                .await?;
            return Ok(output.success);
        }
        if cfg!(target_os = "macos") {
            let output = command::require_success(
                "/usr/bin/id",
                self.runner.run("/usr/bin/id", &["-u"], None).await?,
            )?;
            return Ok(output.stdout.trim() == "0");
        }
        let status = tokio::fs::read_to_string("/proc/self/status")
            .await
            .map_err(|source| TransparentError::Io {
                context: "read effective Linux user ID".into(),
                source,
            })?;
        let uid = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|value| value.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| {
                TransparentError::Invalid("Linux process status has no effective user ID".into())
            })?;
        Ok(uid == 0)
    }

    async fn check_forwarding(&self, plan: &Plan) -> Result<(), TransparentError> {
        if plan.lan_interfaces.is_empty() {
            return Ok(());
        }
        let output = command::require_success(
            "sysctl",
            self.runner
                .run("sysctl", &["-n", "net.ipv4.ip_forward"], None)
                .await?,
        )?;
        if output.stdout.trim() == "1" {
            Ok(())
        } else {
            Err(TransparentError::Invalid(
                "net.ipv4.ip_forward is disabled; enable forwarding before using Sempre as a LAN gateway".into(),
            ))
        }
    }
}

fn tun_ready(plan: &Plan) -> Result<(), TransparentError> {
    let inventory = sempre_network::inventory()?;
    let interface = inventory
        .interfaces
        .iter()
        .find(|interface| interface.name == plan.tun_interface)
        .ok_or_else(|| {
            TransparentError::Invalid(format!(
                "TUN interface {} is unavailable",
                plan.tun_interface
            ))
        })?;
    if plan.tun_address.is_empty()
        || interface
            .addresses
            .iter()
            .any(|address| address == &plan.tun_address)
    {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "TUN interface {} does not have address {}",
            plan.tun_interface, plan.tun_address
        )))
    }
}

async fn listeners_ready(plan: &Plan) -> Result<(), TransparentError> {
    for port in [plan.tproxy_port, plan.dns_port] {
        TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|error| {
                TransparentError::Invalid(format!("TCP port {port} is not listening: {error}"))
            })?;
    }
    Ok(())
}
