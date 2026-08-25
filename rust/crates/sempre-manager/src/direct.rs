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

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use chrono::Utc;
    use sempre_core::Adapter;
    use sempre_state::{Installation, Layout, Selection, Store};

    use super::*;

    #[derive(Clone, Copy)]
    struct FakeRunner;

    impl VersionRunner for FakeRunner {
        fn version<'a>(
            &'a self,
            _: &'a dyn Adapter,
            _: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
            Box::pin(async { Ok("1.2.3".into()) })
        }
    }

    impl ValidationRunner for FakeRunner {
        fn validate<'a>(
            &'a self,
            _: &'a dyn Adapter,
            _: &'a Path,
            _: &'a Path,
            _: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn direct_plan_uses_active_or_explicit_installed_core_without_state_changes() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(temporary.path());
        let store = Store::new(layout.clone());
        let manager = Manager::with_runner(store, FakeRunner).expect("manager");
        let digest = "a".repeat(64);
        manager
            .store()
            .update(|document| {
                let source = &mut document.core_mut("sing-box").default;
                source.channels.insert("stable".into(), "1.2.3".into());
                source.installed.insert(
                    "1.2.3".into(),
                    Installation {
                        explicit: true,
                        digest,
                        source: "test".into(),
                        installed_at: Utc::now(),
                    },
                );
                document.selected = Some(Selection {
                    core: "sing-box".into(),
                    repository: None,
                    reference: "stable".into(),
                });
                document.configs.insert("sing-box".into(), "b".repeat(64));
                document.active = Some(Deployment {
                    core: "sing-box".into(),
                    repository: None,
                    reference: "stable".into(),
                    version: "1.2.3".into(),
                    config_hash: "b".repeat(64),
                });
                Ok(())
            })
            .expect("runtime state");
        let binary = layout.core_binary("sing-box", None, "1.2.3");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("binary parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            fs::write(
                &binary,
                b"#!/bin/sh\ntrap 'exit 0' TERM\nwhile :; do sleep 1; done\n",
            )
            .expect("binary");
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
                .expect("binary permissions");
        }
        #[cfg(not(unix))]
        fs::write(&binary, b"core").expect("binary");
        let config = layout.config("sing-box", &"b".repeat(64));
        fs::create_dir_all(config.parent().expect("config parent")).expect("config parent");
        fs::write(&config, b"{}").expect("config");
        let before = manager.state().expect("state before");

        let active = manager.direct_plan(None).await.expect("active plan");
        let explicit = manager
            .direct_plan(Some("sing-box@stable"))
            .await
            .expect("explicit plan");

        assert_eq!(active.label, "sing-box@stable -> 1.2.3");
        assert_eq!(active.spec, explicit.spec);
        assert_eq!(manager.state().expect("state after"), before);

        #[cfg(unix)]
        {
            manager
                .run_direct_until(None, |_| {}, async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok::<(), io::Error>(())
                })
                .await
                .expect("foreground lifecycle");
            assert_eq!(manager.state().expect("state after run"), before);
        }
    }
}
