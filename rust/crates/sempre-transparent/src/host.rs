use std::sync::Arc;

use crate::{
    Mode, Plan, TransparentError, command, macos_dns::SystemDns as MacSystemDns, nft, policy,
    system_dns::SystemDns, windows_dns::SystemDns as WindowsSystemDns,
};

mod dns;

pub struct Controller {
    runner: Arc<dyn command::Runner>,
    system_dns: SystemDns,
    macos_dns: MacSystemDns,
    windows_dns: WindowsSystemDns,
    cn_routes: Arc<crate::cn_routes::Controller>,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            runner: Arc::new(command::SystemRunner),
            system_dns: SystemDns::disabled(),
            macos_dns: MacSystemDns::new(std::path::PathBuf::new()),
            windows_dns: WindowsSystemDns::new(std::path::PathBuf::new()),
            cn_routes: Arc::new(crate::cn_routes::Controller::new(None)),
        }
    }
}

impl Controller {
    pub fn new(layout: &sempre_state::Layout) -> Self {
        Self {
            runner: Arc::new(command::SystemRunner),
            cn_routes: Arc::new(crate::cn_routes::Controller::new(Some(
                layout.home.join("cn-routes.json"),
            ))),
            system_dns: SystemDns::new(layout.home.join("system-dns"), "/etc/resolv.conf".into()),
            macos_dns: MacSystemDns::new(layout.home.join("system-dns").join("macos")),
            windows_dns: WindowsSystemDns::new(layout.home.join("system-dns").join("windows")),
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
            if crate::system_dns_intent(profile).is_none()
                && !sempre_converter::intranet_proxy_enabled(profile)
            {
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
            if crate::system_dns_intent(profile).is_none()
                && !sempre_converter::intranet_proxy_enabled(profile)
            {
                return Ok(Plan::default());
            }
            let upstreams = self
                .windows_dns
                .discover_upstreams(self.runner.as_ref())
                .await?;
            return crate::desktop_plan::prepare(
                crate::desktop_plan::windows_platform(),
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
        let upstreams = if sempre_converter::intranet_proxy_enabled(profile) {
            self.system_dns.discover_upstreams()?
        } else {
            Vec::new()
        };
        let mut plan =
            crate::prepare_with_upstreams(core, profile, runtime_config, &inventory, &upstreams)?;
        if let Some(system_dns) = plan
            .system_dns
            .as_mut()
            .filter(|system_dns| system_dns.managed_frontend)
        {
            system_dns.original_upstreams = self.system_dns.discover_upstreams()?;
        }
        Ok(plan)
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
        self.system_dns.restore()?;
        if plan.enabled() {
            policy::check_collisions(self.runner.as_ref()).await?;
            nft::check_collisions(self.runner.as_ref()).await?;
            self.cleanup_network().await?;
            if plan.mode == Mode::TProxy {
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
        if let Some(system_dns) = &plan.system_dns
            && system_dns.takeover_host
        {
            self.system_dns.apply()?;
        }

        Ok(())
    }

    pub async fn cleanup(&self) -> Result<(), TransparentError> {
        self.cn_routes.cleanup(self.runner.as_ref()).await?;
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

    pub fn cn_route_progress(&self) -> tokio::sync::watch::Receiver<crate::CnRouteProgress> {
        self.cn_routes.progress()
    }

    pub async fn cleanup_runtime_network(&self) -> Result<(), TransparentError> {
        self.cn_routes.cleanup(self.runner.as_ref()).await?;
        if !cfg!(target_os = "linux") || !self.is_root().await? {
            return Ok(());
        }
        self.cleanup_network().await
    }

    async fn cleanup_owned(&self) -> Result<(), TransparentError> {
        let dns_result = self.system_dns.restore();
        let network_result = self.cleanup_network().await;
        dns_result.and(network_result)
    }

    pub async fn recover_stale_system_dns(&self) -> Result<(), TransparentError> {
        self.cn_routes.cleanup(self.runner.as_ref()).await?;
        if cfg!(target_os = "macos") && self.is_root().await? {
            self.macos_dns.restore(self.runner.as_ref()).await?;
        }
        if cfg!(target_os = "windows") && self.is_root().await? {
            self.windows_dns.restore(self.runner.as_ref()).await?;
        }
        Ok(())
    }

    async fn apply_macos(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !plan.active() {
            return Ok(());
        }
        if let Some(system_dns) = &plan.system_dns
            && self
                .macos_dns
                .verify(self.runner.as_ref(), system_dns.listen_port)
                .await
                .is_ok()
        {
            return Ok(());
        }
        self.macos_dns.restore(self.runner.as_ref()).await?;
        if let Some(system_dns) = &plan.system_dns
            && let Err(error) = self
                .macos_dns
                .apply(
                    self.runner.as_ref(),
                    &system_dns.original_upstreams,
                    system_dns.listen_port,
                )
                .await
        {
            let _ = self.macos_dns.restore(self.runner.as_ref()).await;
            return Err(error);
        }

        Ok(())
    }

    async fn apply_windows(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !plan.active() {
            return Ok(());
        }
        if cfg!(target_arch = "x86_64") {
            self.windows_dns.restore(self.runner.as_ref()).await?;
            return Ok(());
        }
        if plan.system_dns.is_some() && self.windows_dns.verify(self.runner.as_ref()).await.is_ok()
        {
            return Ok(());
        }
        self.windows_dns.restore(self.runner.as_ref()).await?;
        if let Some(system_dns) = &plan.system_dns
            && let Err(error) = self
                .windows_dns
                .apply(self.runner.as_ref(), &system_dns.original_upstreams)
                .await
        {
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

    async fn is_root(&self) -> Result<bool, TransparentError> {
        if cfg!(target_os = "windows") {
            #[cfg(target_os = "windows")]
            return windows_security::is_administrator();
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
}

#[cfg(target_os = "windows")]
mod windows_security;
