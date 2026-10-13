use super::Controller;
use crate::TransparentError;

impl Controller {
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "windows")),
        allow(clippy::unused_async)
    )] // Shared asynchronous platform contract; Linux has no DNS cache to flush.
    pub async fn flush_cn_dns(&self) -> Result<(), TransparentError> {
        #[cfg(target_os = "macos")]
        {
            crate::command::require_success(
                "dscacheutil",
                self.runner
                    .run("/usr/bin/dscacheutil", &["-flushcache"], None)
                    .await?,
            )?;
            crate::command::require_success(
                "mDNSResponder",
                self.runner
                    .run("/usr/bin/killall", &["-HUP", "mDNSResponder"], None)
                    .await?,
            )?;
        }
        #[cfg(target_os = "windows")]
        crate::command::require_success(
            "Clear-DnsClientCache",
            self.runner
                .run(
                    "powershell.exe",
                    &[
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "Clear-DnsClientCache",
                    ],
                    None,
                )
                .await?,
        )?;
        Ok(())
    }
    pub async fn cn_outbound_interfaces(&self) -> Result<[Option<String>; 2], TransparentError> {
        crate::cn_routes::outbound_interfaces(self.runner.as_ref()).await
    }
    pub async fn apply_cn_routes(
        &self,
        intent: &crate::CnRouteIntent,
    ) -> Result<(), TransparentError> {
        self.cn_routes
            .apply(std::sync::Arc::clone(&self.runner), intent)
            .await
    }

    pub async fn prepare_managed_dns_frontend(
        &self,
        core: &str,
        profile: &sempre_converter::Profile,
    ) -> Result<Option<crate::SystemDnsPlan>, TransparentError> {
        let Some(mut system_dns) = crate::system_dns_intent(profile)
            .filter(|system_dns| core == "sing-box" && system_dns.managed_frontend)
        else {
            return Ok(None);
        };
        if cfg!(target_os = "macos") {
            let upstreams = self
                .macos_dns
                .discover_upstreams(self.runner.as_ref())
                .await?;
            return Ok(crate::desktop_plan::managed_frontend_plan(
                crate::desktop_plan::Platform::Macos,
                core,
                profile,
                upstreams,
            ));
        }
        if cfg!(target_os = "windows") {
            let upstreams = self
                .windows_dns
                .discover_upstreams(self.runner.as_ref())
                .await?;
            return Ok(crate::desktop_plan::managed_frontend_plan(
                crate::desktop_plan::windows_platform(),
                core,
                profile,
                upstreams,
            ));
        }
        if !cfg!(target_os = "linux") {
            return Ok(None);
        }
        if !self.system_dns.allowed() {
            return Err(TransparentError::Invalid(
                "managed DNS frontend requires system mode".into(),
            ));
        }
        system_dns.original_upstreams = self.system_dns.discover_upstreams()?;
        Ok(Some(system_dns))
    }

    pub async fn cleanup_system_dns(&self) -> Result<(), TransparentError> {
        if !self.is_root().await? {
            return Ok(());
        }
        if cfg!(target_os = "macos") {
            return self.macos_dns.restore(self.runner.as_ref()).await;
        }
        if cfg!(target_os = "windows") {
            return self.windows_dns.restore(self.runner.as_ref()).await;
        }
        if cfg!(target_os = "linux") {
            return self.system_dns.restore();
        }
        Ok(())
    }
}
