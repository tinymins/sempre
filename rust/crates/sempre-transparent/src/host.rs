use std::{sync::Arc, time::Duration};

use tokio::{net::TcpStream, time::sleep};

use crate::{Mode, Plan, TransparentError, command, nft, policy};

const TUN_TIMEOUT: Duration = Duration::from_secs(20);
const LISTENER_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct Controller {
    runner: Arc<dyn command::Runner>,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(command::SystemRunner),
        }
    }

    pub async fn apply(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !cfg!(target_os = "linux") || !plan.enabled() {
            return Ok(());
        }
        self.require_root().await?;
        policy::check_collisions(self.runner.as_ref()).await?;
        nft::check_collisions(self.runner.as_ref()).await?;
        self.cleanup_owned().await?;
        self.wait_ready(plan).await?;
        if plan.mode == Mode::Tun {
            return Ok(());
        }
        self.check_forwarding(plan).await?;
        if let Err(error) = policy::apply(self.runner.as_ref()).await {
            let _ = self.cleanup_owned().await;
            return Err(error);
        }
        if let Err(error) = nft::apply(self.runner.as_ref(), plan).await {
            let _ = self.cleanup_owned().await;
            return Err(error);
        }
        if let Err(error) = self.verify(plan).await {
            let _ = self.cleanup_owned().await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn verify(&self, plan: &Plan) -> Result<(), TransparentError> {
        if !cfg!(target_os = "linux") || !plan.enabled() {
            return Ok(());
        }
        self.wait_ready(plan).await?;
        if plan.mode == Mode::TProxy {
            nft::verify(self.runner.as_ref()).await?;
            policy::verify(self.runner.as_ref()).await?;
        }
        Ok(())
    }

    pub async fn cleanup(&self) -> Result<(), TransparentError> {
        if !cfg!(target_os = "linux") || !self.is_root().await? {
            return Ok(());
        }
        self.cleanup_owned().await
    }

    async fn cleanup_owned(&self) -> Result<(), TransparentError> {
        let nft_result = nft::delete_owned(self.runner.as_ref()).await;
        let policy_result = policy::delete(self.runner.as_ref()).await;
        nft_result.and(policy_result)
    }

    async fn wait_ready(&self, plan: &Plan) -> Result<(), TransparentError> {
        let timeout = if plan.mode == Mode::Tun {
            TUN_TIMEOUT
        } else {
            LISTENER_TIMEOUT
        };
        let started = std::time::Instant::now();
        loop {
            let result = if plan.mode == Mode::Tun {
                tun_ready(plan)
            } else {
                listeners_ready(plan).await
            };
            if result.is_ok() {
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err(TransparentError::Invalid(format!(
                    "transparent proxy did not become ready after {timeout:?}: {}",
                    result.expect_err("readiness failed")
                )));
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn require_root(&self) -> Result<(), TransparentError> {
        if self.is_root().await? {
            Ok(())
        } else {
            Err(TransparentError::Invalid(
                "Linux transparent proxy mode requires root privileges".into(),
            ))
        }
    }

    async fn is_root(&self) -> Result<bool, TransparentError> {
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

    async fn check_forwarding(&self, plan: &Plan) -> Result<(), TransparentError> {
        if plan.lan_interfaces.is_empty() {
            return Ok(());
        }
        let output = command::require_success(
            "sysctl",
            self.runner
                .run("sysctl", &["-n", "net.ipv4.ip_forward"], None)
                .await?,
        )?;
        if output.stdout.trim() == "1" {
            Ok(())
        } else {
            Err(TransparentError::Invalid(
                "net.ipv4.ip_forward is disabled; enable forwarding before using Sempre as a LAN gateway".into(),
            ))
        }
    }
}

fn tun_ready(plan: &Plan) -> Result<(), TransparentError> {
    let inventory = sempre_network::inventory()?;
    let interface = inventory
        .interfaces
        .iter()
        .find(|interface| interface.name == plan.tun_interface)
        .ok_or_else(|| {
            TransparentError::Invalid(format!(
                "TUN interface {} is unavailable",
                plan.tun_interface
            ))
        })?;
    if plan.tun_address.is_empty()
        || interface
            .addresses
            .iter()
            .any(|address| address == &plan.tun_address)
    {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "TUN interface {} does not have address {}",
            plan.tun_interface, plan.tun_address
        )))
    }
}

async fn listeners_ready(plan: &Plan) -> Result<(), TransparentError> {
    for port in [plan.tproxy_port, plan.dns_port] {
        TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|error| {
                TransparentError::Invalid(format!("TCP port {port} is not listening: {error}"))
            })?;
    }
    Ok(())
}
