use sempre_proxy::{Apply, Engine, InstanceStatus, configuration};
use std::sync::Arc;
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt as _;
use tokio::{
    process::{Child, Command},
    sync::{Mutex, Semaphore},
};
use uuid::Uuid;

pub struct Runtime {
    pub updates: Mutex<HashMap<Uuid, Retry>>,
    pub draining: Arc<Semaphore>,
    pub instances: Mutex<HashMap<Uuid, Instance>>,
    pub directory: PathBuf,
    pub probe_url: String,
}

pub struct Retry {
    revision: i64,
    attempts: u32,
    after: Instant,
    message: &'static str,
}

pub struct Instance {
    pub child: Child,
    pub status: InstanceStatus,
    pub directory: PathBuf,
}

impl Runtime {
    pub async fn apply(&self, input: Apply) -> Result<InstanceStatus, &'static str> {
        let mut updates = tokio::time::timeout(Duration::from_secs(5), self.updates.lock())
            .await
            .map_err(|_| "proxy runtime is busy; retry shortly")?;
        if let Some(retry) = updates.get(&input.id)
            && retry.revision == input.revision
            && retry.after > Instant::now()
        {
            return Err(retry.message);
        }
        let id = input.id;
        let revision = input.revision;
        let result = self.apply_inner(input).await;
        match &result {
            Ok(_) => {
                updates.remove(&id);
            }
            Err(message) => {
                let attempts = updates
                    .get(&id)
                    .filter(|retry| retry.revision == revision)
                    .map_or(0, |retry| retry.attempts);
                let delay = (30_u64 * (1_u64 << attempts.min(4))).min(300);
                updates.insert(
                    id,
                    Retry {
                        revision,
                        attempts: attempts + 1,
                        after: Instant::now() + Duration::from_secs(delay),
                        message,
                    },
                );
            }
        }
        result
    }

    async fn apply_inner(&self, input: Apply) -> Result<InstanceStatus, &'static str> {
        let mut instances = self.instances.lock().await;
        if let Some(existing) = instances.get_mut(&input.id)
            && existing.status.revision == input.revision
            && existing
                .child
                .try_wait()
                .map_err(|_| "process status failed")?
                .is_none()
        {
            return Ok(existing.status.clone());
        }
        if !instances.contains_key(&input.id) && instances.len() >= 64 {
            return Err("runtime proxy limit reached");
        }
        // Reserve drain capacity before releasing the instance lock.
        let retiring = if instances.contains_key(&input.id) {
            Some(
                self.draining
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| "previous proxy requests are still draining")?,
            )
        } else {
            None
        };
        drop(instances);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|_| "port allocation failed")?;
        let port = listener
            .local_addr()
            .map_err(|_| "port allocation failed")?
            .port();
        let status = InstanceStatus {
            revision: input.revision,
            port,
            username: Uuid::new_v4().simple().to_string(),
            password: Uuid::new_v4().simple().to_string(),
        };
        let directory = self.directory.join(Uuid::new_v4().to_string());
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|_| "config directory failed")?;
        let result = self
            .start(&input, status, directory.clone(), listener)
            .await;
        let instance = match result {
            Ok(instance) => instance,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(directory).await;
                return Err(error);
            }
        };
        let status = instance.status.clone();
        if let Some(mut previous) = self.instances.lock().await.insert(input.id, instance) {
            tokio::spawn(async move {
                let _permit = retiring;
                tokio::time::sleep(Duration::from_mins(2)).await;
                let _ = previous.child.kill().await;
                let _ = tokio::fs::remove_dir_all(previous.directory).await;
            });
        }
        Ok(status)
    }

    async fn start(
        &self,
        input: &Apply,
        status: InstanceStatus,
        directory: PathBuf,
        listener: tokio::net::TcpListener,
    ) -> Result<Instance, &'static str> {
        let path = directory.join("config.json");
        let config = configuration(
            &input.connection,
            status.port,
            &status.username,
            &status.password,
        );
        let bytes =
            serde_json::to_vec(&config).map_err(|_| "configuration serialization failed")?;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&path)
            .await
            .map_err(|_| "config write failed")?;
        file.write_all(&bytes)
            .await
            .map_err(|_| "config write failed")?;
        drop(file);
        let mut check = command(&input.connection.engine);
        match input.connection.engine {
            Engine::SingBox => {
                check.args(["check", "-c"]);
            }
            Engine::Xray => {
                check.args(["run", "-test", "-config"]);
            }
        }
        check
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let result = tokio::time::timeout(Duration::from_secs(10), check.status())
            .await
            .map_err(|_| "engine configuration check timed out")?
            .map_err(|_| "engine unavailable")?;
        if !result.success() {
            return Err("engine rejected configuration");
        }
        drop(listener);
        let mut cmd = command(&input.connection.engine);
        match input.connection.engine {
            Engine::SingBox => {
                cmd.args(["run", "-c"]);
            }
            Engine::Xray => {
                cmd.args(["run", "-config"]);
            }
        }
        let mut child = cmd
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "engine launch failed")?;
        let mut ready = false;
        for _ in 0..50 {
            if child
                .try_wait()
                .map_err(|_| "process status failed")?
                .is_some()
            {
                return Err("engine exited during startup");
            }
            if tokio::net::TcpStream::connect(("127.0.0.1", status.port))
                .await
                .is_ok()
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !ready {
            return Err("proxy listener did not start");
        }
        self.probe(&status).await?;
        Ok(Instance {
            child,
            status,
            directory,
        })
    }

    async fn probe(&self, status: &InstanceStatus) -> Result<(), &'static str> {
        let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{}", status.port))
            .map_err(|_| "proxy client failed")?
            .basic_auth(&status.username, &status.password);
        let client = reqwest::Client::builder()
            .proxy(proxy)
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "proxy client failed")?;
        let response = client
            .get(&self.probe_url)
            .send()
            .await
            .map_err(|_| "upstream connectivity check failed")?;
        if !response.status().is_success() {
            return Err("upstream connectivity check returned an error");
        }
        Ok(())
    }

    pub async fn remove(&self, id: Uuid) {
        if let Some(mut instance) = self.instances.lock().await.remove(&id) {
            let _ = instance.child.kill().await;
            let _ = tokio::fs::remove_dir_all(instance.directory).await;
        }
    }
}

fn command(engine: &Engine) -> Command {
    Command::new(match engine {
        Engine::SingBox => {
            std::env::var("SEMPRE_SING_BOX_BIN").unwrap_or_else(|_| "sing-box".into())
        }
        Engine::Xray => std::env::var("SEMPRE_XRAY_BIN").unwrap_or_else(|_| "xray".into()),
    })
}
