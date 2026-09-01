use super::Controller;
use crate::TransparentError;

impl Controller {
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
                crate::desktop_plan::Platform::Windows,
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
