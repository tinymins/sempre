use std::{fs, future::Future, io, path::Path, time::Duration};

use sempre_core::{CommandSpec, CoreRef};
use sempre_state::{Deployment, Document};
use sempre_supervisor::ManagedProcess;

use crate::{
    Manager, ManagerError, ValidationRunner, VersionRunner, lifecycle::resolve_installed_version,
};

const STOP_GRACE: Duration = Duration::from_secs(10);

struct DirectPlan {
    label: String,
    spec: CommandSpec,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn run_direct(
        &self,
        reference: Option<&str>,
        started: impl FnOnce(&str),
    ) -> Result<(), ManagerError> {
        self.run_direct_until(reference, started, direct_shutdown())
            .await
    }

    async fn run_direct_until<F>(
        &self,
        reference: Option<&str>,
        started: impl FnOnce(&str),
        shutdown: F,
    ) -> Result<(), ManagerError>
    where
        F: Future<Output = Result<(), io::Error>>,
    {
        let _instance = self.store.acquire_instance()?;
        let plan = self.direct_plan(reference).await?;
        started(&plan.label);
        let mut process = ManagedProcess::spawn_foreground(&plan.spec)?;
        tokio::pin!(shutdown);
        tokio::select! {
            result = process.wait() => {
                let status = result?;
                if status.success() {
                    Ok(())
                } else {
                    Err(ManagerError::DirectExit {
                        reference: plan.label,
                        status: status.to_string(),
                    })
                }
            }
            signal = &mut shutdown => {
                let signal = signal.map_err(|error| ManagerError::io("wait for interrupt", error));
                let terminated = process.terminate(STOP_GRACE).await;
                signal?;
                terminated?;
                Ok(())
            }
        }
    }

    async fn direct_plan(&self, value: Option<&str>) -> Result<DirectPlan, ManagerError> {
        let document = self.store.read()?;
        let deployment = match value.filter(|value| !value.trim().is_empty()) {
            Some(value) => self.direct_deployment(&document, value)?,
            None => document.active.ok_or(ManagerError::NoSelectedCore)?,
        };
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
                "foreground core binary or configuration is unavailable".into(),
            ));
        }
        let reference = CoreRef {
            core: deployment.core.clone(),
            repository: deployment.repository.clone(),
            reference: deployment.reference.clone(),
        };
        self.validate_config_path(&reference, &deployment.version, &config)
            .await?;
        let data = self.store.layout().runtime.join(&deployment.core);
        fs::create_dir_all(&data)
            .map_err(|error| ManagerError::io("create foreground core data directory", error))?;
        Ok(DirectPlan {
            label: format!("{reference} -> {}", deployment.version),
            spec: adapter.run_spec(path_text(&binary)?, path_text(&config)?, path_text(&data)?),
        })
    }

    fn direct_deployment(
        &self,
        document: &Document,
        value: &str,
    ) -> Result<Deployment, ManagerError> {
        let reference = self.normalized_reference(value)?;
        let version = resolve_installed_version(document, &reference)?;
        let config_hash = document
            .configs
            .get(&reference.core)
            .filter(|hash| !hash.is_empty())
            .cloned()
            .ok_or(ManagerError::NoConfiguration)?;
        Ok(Deployment {
            core: reference.core,
            repository: reference.repository,
            reference: reference.reference,
            version,
            config_hash,
        })
    }
}

fn path_text(path: &Path) -> Result<&str, ManagerError> {
    path.to_str()
        .ok_or_else(|| ManagerError::NonUnicodePath(path.to_path_buf()))
}

async fn direct_shutdown() -> io::Result<()> {
    let interrupt = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = interrupt => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    interrupt.await
}
