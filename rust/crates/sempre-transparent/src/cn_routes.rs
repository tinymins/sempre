mod backend;
mod model;
mod platform;

use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::{Mutex, watch};

use crate::{TransparentError, command};
pub use model::Intent as CnRouteIntent;
pub fn normalize_cn_prefixes(prefixes: &[String]) -> Result<Vec<String>, TransparentError> {
    model::normalize(prefixes)
}
use backend::{Backend, Native};
use model::{Journal, Route, invalid};

/// Native route changes completed by the current reconciliation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CnRouteProgress {
    pub done: usize,
    pub total: usize,
}

pub(crate) async fn outbound_interfaces(
    runner: &dyn command::Runner,
) -> Result<[Option<String>; 2], TransparentError> {
    platform::outbound_interfaces(runner).await
}

pub(crate) struct Controller<B = Native> {
    backend: B,
    path: Option<PathBuf>,
    gate: Mutex<()>,
    // The journal records ownership; it describes the OS table only after this
    // process has reconciled against it.
    verified: AtomicBool,
    progress: watch::Sender<CnRouteProgress>,
}

impl<B: Backend> Controller<B> {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        Self {
            backend: B::default(),
            path,
            gate: Mutex::new(()),
            verified: AtomicBool::new(false),
            progress: watch::Sender::new(CnRouteProgress::default()),
        }
    }

    pub(crate) fn progress(&self) -> watch::Receiver<CnRouteProgress> {
        self.progress.subscribe()
    }

    pub(crate) async fn apply(
        self: &Arc<Self>,
        runner: Arc<dyn command::Runner>,
        intent: &CnRouteIntent,
    ) -> Result<(), TransparentError> {
        // Keep ownership in the caller: cancellation stops the batch and releases
        // the gate; the write-ahead journal retains every possibly installed route.
        let _gate = self.gate.lock().await;
        let (uplinks, tun) = self.backend.discover(runner.as_ref(), intent).await?;
        let desired = model::plan(intent, &uplinks, &tun)?;
        if self.verified.load(Ordering::SeqCst) && self.read()?.routes == desired {
            return Ok(());
        }
        let capture = desired.iter().any(|r| r.fallback && r.scoped);
        if let Err(error) = self.reconcile(runner.as_ref(), desired, Some(capture)).await {
            // Disconnect TUN first, including on partial installation failure.
            if let Err(cleanup) = self.reconcile(runner.as_ref(), Vec::new(), None).await {
                return Err(invalid(format!(
                    "{error}; CN route cleanup failed: {cleanup}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn cleanup(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        let _gate = self.gate.lock().await;
        self.reconcile(runner, Vec::new(), None).await
    }

    /// Converges owned routes on `target`; `policy` is the capture mode for a
    /// non-empty target. Every completed batch is checkpointed, so an interrupted
    /// run resumes with only the remaining work.
    async fn reconcile(
        &self,
        runner: &dyn command::Runner,
        target: Vec<Route>,
        policy: Option<bool>,
    ) -> Result<(), TransparentError> {
        self.verified.store(false, Ordering::SeqCst);
        let mut journal = self.read()?;
        let mut candidates = journal.routes.clone();
        candidates.extend(model::difference(&target, &journal.routes));
        let installed = if candidates.is_empty() {
            Vec::new()
        } else {
            self.backend.observe(runner, &candidates).await?
        };
        let additions = model::difference(&target, &installed);
        let removals = model::difference(&installed, &target);
        let mut progress = Progress::new(&self.progress, additions.len() + removals.len());
        journal.routes = installed;
        journal.routes.extend(additions.iter().cloned());
        if let Some(capture) = policy {
            // Write ahead: an interrupted batch can always be recovered. Backends only
            // delete routes bearing our ownership marker and the complete identity.
            journal.policy = true;
            self.write(&journal)?;
            self.backend.preflight(runner, &additions).await?;
            for fallback in [false, true] {
                self.change(runner, &mut journal, &additions, fallback, true, &mut progress)
                    .await?;
            }
            self.backend.policy(runner, true, capture).await?;
        } else if journal.policy {
            self.backend.policy(runner, false, false).await?;
            journal.policy = false;
        }
        for fallback in [true, false] {
            self.change(runner, &mut journal, &removals, fallback, false, &mut progress)
                .await?;
        }
        journal.routes = target;
        self.store(&journal)?;
        self.verified.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn change(
        &self,
        runner: &dyn command::Runner,
        journal: &mut Journal,
        routes: &[Route],
        fallback: bool,
        add: bool,
        progress: &mut Progress<'_>,
    ) -> Result<(), TransparentError> {
        let routes = routes
            .iter()
            .filter(|r| r.fallback == fallback)
            .cloned()
            .collect::<Vec<_>>();
        for chunk in routes.chunks(self.backend.batch_size()) {
            self.backend.change(runner, chunk, add).await?;
            if !add {
                let removed = chunk.iter().collect::<HashSet<_>>();
                journal.routes.retain(|route| !removed.contains(route));
                self.store(journal)?;
            }
            progress.advance(chunk.len());
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    fn store(&self, journal: &Journal) -> Result<(), TransparentError> {
        if !journal.routes.is_empty() || journal.policy {
            return self.write(journal);
        }
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

struct Progress<'a> {
    sender: &'a watch::Sender<CnRouteProgress>,
    state: CnRouteProgress,
}

impl<'a> Progress<'a> {
    fn new(sender: &'a watch::Sender<CnRouteProgress>, total: usize) -> Self {
        let state = CnRouteProgress { done: 0, total };
        sender.send_replace(state);
        Self { sender, state }
    }

    fn advance(&mut self, count: usize) {
        self.state.done += count;
        self.sender.send_replace(self.state);
    }
}

#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod tests;
