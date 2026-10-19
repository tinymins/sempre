mod backend;
mod model;
mod platform;

use std::{fs, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use crate::{TransparentError, command};
pub use model::Intent as CnRouteIntent;
pub fn normalize_cn_prefixes(prefixes: &[String]) -> Result<Vec<String>, TransparentError> {
    model::normalize(prefixes)
}
use backend::{Backend, Native};
use model::{Journal, Route, difference, invalid};

pub(crate) async fn outbound_interfaces(
    runner: &dyn command::Runner,
) -> Result<[Option<String>; 2], TransparentError> {
    platform::outbound_interfaces(runner).await
}

pub(crate) struct Controller<B = Native> {
    backend: B,
    path: Option<PathBuf>,
    gate: Mutex<()>,
}

impl<B: Backend> Controller<B> {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        Self {
            backend: B::default(),
            path,
            gate: Mutex::new(()),
        }
    }

    pub(crate) async fn apply(
        self: &Arc<Self>,
        runner: Arc<dyn command::Runner>,
        intent: &CnRouteIntent,
    ) -> Result<(), TransparentError> {
        let controller = Arc::clone(self);
        let intent = intent.clone();
        // Dropping a caller must not cancel a native batch while cleanup runs.
        tokio::spawn(async move {
            let _gate = controller.gate.lock().await;
            let (uplinks, tun) = controller
                .backend
                .discover(runner.as_ref(), &intent)
                .await?;
            let previous = controller.read()?;
            let desired = model::plan(&intent, &uplinks, &tun)?;
            if previous.routes == desired {
                return Ok(());
            }
            controller.replace(runner.as_ref(), desired).await
        })
        .await
        .map_err(|e| invalid(e.to_string()))?
    }

    async fn replace(
        &self,
        runner: &dyn command::Runner,
        desired: Vec<Route>,
    ) -> Result<(), TransparentError> {
        let previous = self.read()?;
        let additions = difference(&desired, &previous.routes);
        let removals = difference(&previous.routes, &desired);
        // Write ahead: an interrupted batch can always be recovered. Backends only
        // delete routes bearing our ownership marker and the complete identity.
        let mut pending = previous;
        pending.routes.extend(additions.clone());
        pending.policy = true;
        self.write(&pending)?;
        let operation = async {
            self.backend.preflight(runner, &additions).await?;
            self.change(runner, &additions, false, true).await?;
            self.change(runner, &additions, true, true).await?;
            self.backend.policy(runner, true).await?;
            self.change(runner, &removals, true, false).await?;
            self.change(runner, &removals, false, false).await?;
            self.write(&Journal {
                routes: desired,
                policy: true,
            })
        }
        .await;
        if let Err(error) = operation {
            // Disconnect TUN first, including on partial installation failure.
            if let Err(cleanup) = self.cleanup_locked(runner).await {
                return Err(invalid(format!(
                    "{error}; CN route cleanup failed: {cleanup}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    async fn change(
        &self,
        runner: &dyn command::Runner,
        routes: &[Route],
        fallback: bool,
        add: bool,
    ) -> Result<(), TransparentError> {
        let routes = routes
            .iter()
            .filter(|r| r.fallback == fallback)
            .cloned()
            .collect::<Vec<_>>();
        for chunk in routes.chunks(self.backend.batch_size()) {
            self.backend.change(runner, chunk, add).await?;
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    pub(crate) async fn cleanup(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        let _gate = self.gate.lock().await;
        self.cleanup_locked(runner).await
    }

    async fn cleanup_locked(&self, runner: &dyn command::Runner) -> Result<(), TransparentError> {
        let journal = self.read()?;
        if journal.policy {
            self.backend.policy(runner, false).await?;
        }
        self.change(runner, &journal.routes, true, false).await?;
        self.change(runner, &journal.routes, false, false).await?;
        if let Some(path) = &self.path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(invalid(format!("remove CN route journal: {error}"))),
            }
        }
        Ok(())
    }

    fn read(&self) -> Result<Journal, TransparentError> {
        let Some(path) = &self.path else {
            return Ok(Journal::default());
        };
        match fs::read(path) {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|e| invalid(format!("read CN route journal: {e}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Journal::default()),
            Err(error) => Err(invalid(format!("read CN route journal: {error}"))),
        }
    }

    fn write(&self, journal: &Journal) -> Result<(), TransparentError> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| invalid("CN routing requires a persistent journal"))?;
        let bytes = serde_json::to_vec(journal).map_err(|e| invalid(e.to_string()))?;
        sempre_state::write_atomic(path, &bytes, 0o600)
            .map_err(|e| invalid(format!("write CN route journal: {e}")))
    }
}

#[cfg(test)]
mod tests;
