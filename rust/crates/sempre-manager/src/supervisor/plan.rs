use std::fs;

use sempre_core::CoreRef;
use sempre_state::DesiredState;
use sha2::{Digest, Sha256};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

use super::{RuntimePlan, path_text};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(super) async fn resolve_runtime_plan(&self) -> Result<RuntimePlan, ManagerError> {
        let document = self.store.read()?;
        let deployment = document
            .active
            .clone()
            .ok_or_else(|| ManagerError::RuntimeNotReady("no active core deployment".into()))?;
        if document.desired_state == DesiredState::Stopped {
            return Err(ManagerError::RuntimeNotReady(
                "managed core is stopped".into(),
            ));
        }
        self.validate_cn_core(&deployment.core)?;
        let reference = CoreRef {
            core: deployment.core.clone(),
            repository: deployment.repository.clone(),
            reference: deployment.reference.clone(),
        };
        self.startup_tasks.runtime_log("network", "");
        self.ensure_local_proxy_ports_available(&document, &deployment)?;
        let dns_frontend = self
            .prepare_dns_frontend_plan(&document, &deployment, &reference)
            .await?;
        self.dns_frontend.prepare(dns_frontend.as_ref()).await?;
        let adapter = self.registry.get(&deployment.core)?;
        let binary = self.store.layout().core_binary(
            &deployment.core,
            deployment.repository.as_deref(),
            &deployment.version,
        );
        let config = self
            .store
            .layout()
            .config(&deployment.core, &deployment.config_hash);
        if !binary.is_file() || !config.is_file() {
            return Err(ManagerError::RuntimeNotReady(
                "active core binary or configuration is unavailable".into(),
            ));
        }
        let data = self.store.layout().runtime.join(&deployment.core);
        fs::create_dir_all(&data)
            .map_err(|error| ManagerError::io("create core runtime directory", error))?;
        let control_directory = data.join("control");
        if control_directory.exists() {
            fs::remove_dir_all(&control_directory)
                .map_err(|error| ManagerError::io("reset core control directory", error))?;
        }
        let runtime = adapter.prepare_runtime(&config, &control_directory)?;
        if deployment.core == "sing-box" {
            self.adapt_fakeip_runtime(&runtime.config);
        }
        let dns_rule_files = if deployment.core == "sing-box" {
            crate::dns_rule_files::prepare(&runtime.config, &config, &self.dns_settings.read())?
        } else {
            None
        };
        let (cn_source, rules) = self
            .prepare_cn_rules(&deployment.core, &binary, &runtime.config)
            .await?;
        let mut transparent = self
            .prepare_dns_transparent_plan(&document, &deployment, &reference, &runtime.config)
            .await?;
        self.configure_cn_runtime(
            &binary,
            &runtime.config,
            cn_source.as_ref(),
            &mut transparent,
        )
        .await?;
        self.startup_tasks.runtime_log("validating", "");
        self.validate_config_path(&reference, &deployment.version, &runtime.config)
            .await?;
        self.startup_tasks
            .capture_config(&deployment.config_hash, &config);
        let runtime_data = fs::read(&runtime.config)
            .map_err(|error| ManagerError::io("read runtime configuration", error))?;
        let runtime_config_hash = format!("{:x}", Sha256::digest(runtime_data));
        let managed_system_dns = transparent
            .system_dns
            .as_ref()
            .is_some_and(|system_dns| system_dns.managed_frontend);
        if managed_system_dns != dns_frontend.is_some() {
            return Err(ManagerError::InvalidOperation(
                "runtime and daemon DNS frontend plans do not match".into(),
            ));
        }
        let binary = path_text(&binary)?;
        let runtime_config = path_text(&runtime.config)?;
        Ok(RuntimePlan {
            spec: adapter.run_spec(binary, runtime_config, path_text(&data)?),
            deployment,
            runtime_config: runtime.config,
            runtime_config_hash,
            control: runtime.control,
            transparent,
            cn_source,
            dns_frontend,
            dns_rule_files,
            rules,
        })
    }

    fn adapt_fakeip_runtime(&self, config: &std::path::Path) {
        for message in crate::fakeip_routes::adapt_runtime_config(config) {
            self.log_supervisor(&message);
            self.startup_tasks.runtime_log("network", &message);
        }
    }
}
