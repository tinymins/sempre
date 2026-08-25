use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use sempre_artifact::Downloader;
use sempre_core::CommandSpec;
use sempre_state::Layout;
use sempre_supervisor::ManagedProcess;
use tokio::{
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
    time::{sleep, timeout},
};

use crate::{
    BinaryStatus, Config, Instance, InstanceStatus, Status, TunnelError,
    package::{binary_status, install},
    store::Store,
};

const STOP_GRACE: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_mins(1);
const LOG_LIMIT: usize = 256 << 10;

struct Worker {
    signature: String,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct Runtime {
    running: bool,
    workers: BTreeMap<String, Worker>,
    statuses: BTreeMap<String, InstanceStatus>,
}

pub struct Controller {
    layout: Layout,
    store: Store,
    downloader: Downloader,
    operation: AsyncMutex<()>,
    installing: AsyncMutex<()>,
    runtime: Mutex<Runtime>,
}

impl Controller {
    pub fn new(layout: Layout) -> Result<Self, TunnelError> {
        let store = Store::new(layout.tunnels.clone());
        store.initialize()?;
        Ok(Self {
            layout,
            store,
            downloader: Downloader::new(concat!("Sempre/", env!("CARGO_PKG_VERSION")))?,
            operation: AsyncMutex::new(()),
            installing: AsyncMutex::new(()),
            runtime: Mutex::new(Runtime::default()),
        })
    }

    pub fn read(&self) -> Result<Config, TunnelError> {
        self.store.read()
    }

    pub async fn update(self: &Arc<Self>, mut config: Config) -> Result<Config, TunnelError> {
        let _operation = self.operation.lock().await;
        config.normalize();
        config.validate()?;
        self.store.write(&config)?;
        self.reconcile_locked(&config).await?;
        Ok(config)
    }

    pub async fn action(self: &Arc<Self>, id: &str, action: &str) -> Result<Status, TunnelError> {
        let _operation = self.operation.lock().await;
        let mut config = self.store.read()?;
        let instance = config
            .instances
            .iter_mut()
            .find(|instance| instance.id == id)
            .ok_or_else(|| TunnelError::invalid(format!("tunnel instance {id:?} was not found")))?;
        match action {
            "start" => instance.desired_state = "running".into(),
            "stop" => instance.desired_state = "stopped".into(),
            "restart" if instance.desired_state == "running" => {
                self.stop_worker(id).await?;
            }
            "restart" => {
                return Err(TunnelError::invalid(format!(
                    "stopped tunnel instance {id:?} cannot be restarted"
                )));
            }
            _ => {
                return Err(TunnelError::invalid(format!(
                    "unsupported tunnel action {action:?}"
                )));
            }
        }
        if action != "restart" {
            self.store.write(&config)?;
        }
        self.reconcile_locked(&config).await?;
        Ok(self.status_with_config(config))
    }

    pub fn status(&self) -> Result<Status, TunnelError> {
        Ok(self.status_with_config(self.store.read()?))
    }

    pub async fn install(&self) -> Result<BinaryStatus, TunnelError> {
        let _installing = self.installing.lock().await;
        install(&self.layout, &self.downloader).await?;
        Ok(binary_status(&self.layout))
    }

    pub fn log(&self, id: &str) -> Result<String, TunnelError> {
        validate_id(id)?;
        let stdout = read_tail(&self.log_path(id, false), LOG_LIMIT / 2)?;
        let stderr = read_tail(&self.log_path(id, true), LOG_LIMIT / 2)?;
        Ok(match (stdout.is_empty(), stderr.is_empty()) {
            (false, false) => format!("{stdout}\n{stderr}"),
            (false, true) => stdout,
            (true, _) => stderr,
        })
    }

    pub async fn run(
        self: Arc<Self>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), TunnelError> {
        {
            self.runtime.lock().expect("tunnel runtime lock").running = true;
        }
        let config = self.store.read()?;
        self.reconcile(&config).await?;
        while !*shutdown.borrow() && shutdown.changed().await.is_ok() {}
        self.stop_all().await
    }

    async fn reconcile(self: &Arc<Self>, config: &Config) -> Result<(), TunnelError> {
        let _operation = self.operation.lock().await;
        self.reconcile_locked(config).await
    }

    async fn reconcile_locked(self: &Arc<Self>, config: &Config) -> Result<(), TunnelError> {
        let desired: BTreeMap<_, _> = config
            .instances
            .iter()
            .filter(|instance| instance.desired_state == "running")
            .map(|instance| (instance.id.clone(), instance.clone()))
            .collect();
        let mut stopped = Vec::new();
        {
            let mut runtime = self.runtime.lock().expect("tunnel runtime lock");
            if !runtime.running {
                return Ok(());
            }
            let identifiers: Vec<_> = runtime.workers.keys().cloned().collect();
            for id in identifiers {
                let keep = desired.get(&id).is_some_and(|instance| {
                    runtime.workers.get(&id).is_some_and(|worker| {
                        !worker.task.is_finished() && worker.signature == signature(instance)
                    })
                });
                if !keep && let Some(worker) = runtime.workers.remove(&id) {
                    let _ = worker.stop.send(true);
                    stopped.push(worker.task);
                    runtime.statuses.insert(
                        id.clone(),
                        self.instance_status(&id, "stopping", 0, String::new()),
                    );
                }
            }
        }
        wait_workers(stopped).await?;
        let mut runtime = self.runtime.lock().expect("tunnel runtime lock");
        if !runtime.running {
            return Ok(());
        }
        for (id, instance) in desired {
            if runtime.workers.contains_key(&id) {
                continue;
            }
            let (stop, receiver) = watch::channel(false);
            let controller = Arc::clone(self);
            let worker_instance = instance.clone();
            let task = tokio::spawn(async move {
                controller.run_worker(worker_instance, receiver).await;
            });
            runtime.statuses.insert(
                id.clone(),
                self.instance_status(&id, "starting", 0, String::new()),
            );
            runtime.workers.insert(
                id,
                Worker {
                    signature: signature(&instance),
                    stop,
                    task,
                },
            );
        }
        for instance in &config.instances {
            if instance.desired_state == "stopped" {
                runtime.statuses.insert(
                    instance.id.clone(),
                    self.instance_status(&instance.id, "stopped", 0, String::new()),
                );
            }
        }
        Ok(())
    }

    async fn run_worker(self: Arc<Self>, instance: Instance, mut stop: watch::Receiver<bool>) {
        let mut restarts = 0;
        let mut backoff = Duration::from_secs(1);
        loop {
            self.set_status(self.instance_status(
                &instance.id,
                "installing",
                restarts,
                String::new(),
            ));
            let binary = match self.install_binary().await {
                Ok(binary) => binary,
                Err(error) => {
                    if !self
                        .retry(
                            &instance.id,
                            &mut stop,
                            &error.to_string(),
                            &mut restarts,
                            backoff,
                        )
                        .await
                    {
                        return;
                    }
                    backoff = next_backoff(backoff);
                    continue;
                }
            };
            let failure = match self
                .run_process(&instance, &binary, &mut stop, restarts)
                .await
            {
                WorkerCycle::Stopped => return,
                WorkerCycle::Failed(failure) => failure,
            };
            if !self
                .retry(&instance.id, &mut stop, &failure, &mut restarts, backoff)
                .await
            {
                return;
            }
            backoff = next_backoff(backoff);
        }
    }

    async fn install_binary(&self) -> Result<PathBuf, TunnelError> {
        let _installing = self.installing.lock().await;
        install(&self.layout, &self.downloader)
            .await
            .map(|result| result.0)
    }

    async fn run_process(
        &self,
        instance: &Instance,
        binary: &Path,
        stop: &mut watch::Receiver<bool>,
        restarts: u32,
    ) -> WorkerCycle {
        let spec = CommandSpec {
            program: binary.to_path_buf(),
            arguments: instance.arguments(),
            working_directory: binary.parent().map(PathBuf::from),
            ..CommandSpec::default()
        };
        self.set_status(self.instance_status(&instance.id, "starting", restarts, String::new()));
        let mut process = match ManagedProcess::spawn(
            &spec,
            self.log_path(&instance.id, false),
            self.log_path(&instance.id, true),
        ) {
            Ok(process) => process,
            Err(error) => return WorkerCycle::Failed(error.to_string()),
        };
        let startup = tokio::select! {
            result = process.wait() => Some(result),
            () = sleep(Duration::from_millis(500)) => None,
            _ = stop.changed() => {
                let _ = process.terminate(STOP_GRACE).await;
                self.set_status(self.instance_status(&instance.id, "stopped", restarts, String::new()));
                return WorkerCycle::Stopped;
            }
        };
        if let Some(result) = startup {
            return WorkerCycle::Failed(process_failure(result));
        }
        self.set_status(InstanceStatus {
            started_at: Some(Utc::now()),
            ..self.instance_status(&instance.id, "running", restarts, String::new())
        });
        tokio::select! {
            result = process.wait() => WorkerCycle::Failed(process_failure(result)),
            _ = stop.changed() => {
                let _ = process.terminate(STOP_GRACE).await;
                self.set_status(self.instance_status(&instance.id, "stopped", restarts, String::new()));
                WorkerCycle::Stopped
            }
        }
    }

    async fn retry(
        &self,
        id: &str,
        stop: &mut watch::Receiver<bool>,
        failure: &str,
        restarts: &mut u32,
        backoff: Duration,
    ) -> bool {
        *restarts += 1;
        self.set_status(self.instance_status(id, "restarting", *restarts, failure.into()));
        tokio::select! {
            () = sleep(backoff) => true,
            _ = stop.changed() => {
                self.set_status(self.instance_status(id, "stopped", *restarts, String::new()));
                false
            }
        }
    }

    async fn stop_worker(&self, id: &str) -> Result<(), TunnelError> {
        let worker = self
            .runtime
            .lock()
            .expect("tunnel runtime lock")
            .workers
            .remove(id);
        if let Some(worker) = worker {
            let _ = worker.stop.send(true);
            wait_workers(vec![worker.task]).await?;
        }
        Ok(())
    }

    async fn stop_all(&self) -> Result<(), TunnelError> {
        let workers = {
            let mut runtime = self.runtime.lock().expect("tunnel runtime lock");
            runtime.running = false;
            runtime
                .workers
                .split_off(&String::new())
                .into_values()
                .map(|worker| {
                    let _ = worker.stop.send(true);
                    worker.task
                })
                .collect()
        };
        wait_workers(workers).await
    }

    fn status_with_config(&self, config: Config) -> Status {
        let runtime = self.runtime.lock().expect("tunnel runtime lock");
        let instances = config
            .instances
            .iter()
            .map(|instance| {
                runtime
                    .statuses
                    .get(&instance.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        self.instance_status(&instance.id, "stopped", 0, String::new())
                    })
            })
            .collect();
        Status {
            forwards: config.forward_endpoints(),
            config,
            binary: binary_status(&self.layout),
            instances,
        }
    }

    fn instance_status(
        &self,
        id: &str,
        state: &str,
        restart_count: u32,
        last_error: String,
    ) -> InstanceStatus {
        InstanceStatus {
            id: id.into(),
            state: state.into(),
            restart_count,
            started_at: None,
            last_error,
            log_path: self.log_path(id, false).to_string_lossy().into_owned(),
        }
    }

    fn set_status(&self, status: InstanceStatus) {
        self.runtime
            .lock()
            .expect("tunnel runtime lock")
            .statuses
            .insert(status.id.clone(), status);
    }

    fn log_path(&self, id: &str, error: bool) -> PathBuf {
        let suffix = if error { ".error.log" } else { ".log" };
        self.layout.tunnel_logs.join(format!("{id}{suffix}"))
    }
}

fn signature(instance: &Instance) -> String {
    serde_json::to_string(instance).expect("tunnel instance serialization is infallible")
}

enum WorkerCycle {
    Stopped,
    Failed(String),
}

fn process_failure(
    result: Result<std::process::ExitStatus, sempre_supervisor::SupervisorError>,
) -> String {
    match result {
        Ok(status) if status.success() => "wstunnel exited unexpectedly".into(),
        Ok(status) => format!("wstunnel exited with {status}"),
        Err(error) => error.to_string(),
    }
}

async fn wait_workers(workers: Vec<JoinHandle<()>>) -> Result<(), TunnelError> {
    for mut worker in workers {
        if let Ok(result) = timeout(Duration::from_secs(12), &mut worker).await {
            result.map_err(TunnelError::Worker)?;
        } else {
            worker.abort();
            let _ = worker.await;
        }
    }
    Ok(())
}

fn read_tail(path: &PathBuf, limit: usize) -> Result<String, TunnelError> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(TunnelError::io("read tunnel log", error)),
    };
    let start = data.len().saturating_sub(limit);
    Ok(String::from_utf8_lossy(&data[start..]).into_owned())
}

fn validate_id(id: &str) -> Result<(), TunnelError> {
    let valid = !id.is_empty()
        && id.len() <= 63
        && id.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (index > 0 && byte == b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(TunnelError::invalid("invalid tunnel instance ID"))
    }
}

fn next_backoff(value: Duration) -> Duration {
    value.saturating_mul(2).min(MAX_BACKOFF)
}
