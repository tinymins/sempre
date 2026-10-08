use std::net::{Ipv4Addr, TcpListener, UdpSocket};

use sempre_state::{Deployment, Document};

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn ensure_local_proxy_ports_available(
        &self,
        document: &Document,
        deployment: &Deployment,
    ) -> Result<(), ManagerError> {
        let adapter = self.registry.get(&deployment.core)?;
        if !adapter
            .capabilities(Some(&deployment.version), &self.target)
            .features
            .iter()
            .any(|feature| feature == sempre_core::features::LOCAL_PROXY)
        {
            return Ok(());
        }
        let Some(profile_id) = document.active_profile_id.as_deref() else {
            return Ok(());
        };
        let catalog = self.subscriptions.read()?;
        let profile = crate::subscription::find_profile(&catalog, profile_id)?;
        ensure_local_proxy_available(
            profile.local_proxy.socks_port,
            profile.local_proxy.http_port,
        )
    }
}

pub(crate) fn ensure_dns_available(port: u16) -> Result<(), ManagerError> {
    let address = (Ipv4Addr::LOCALHOST, port);
    let _tcp = TcpListener::bind(address)
        .map_err(|error| unavailable("core DNS", port, "TCP", "Core DNS port", &error))?;
    let _udp = UdpSocket::bind(address)
        .map_err(|error| unavailable("core DNS", port, "UDP", "Core DNS port", &error))?;
    Ok(())
}

fn ensure_local_proxy_available(socks_port: u16, http_port: u16) -> Result<(), ManagerError> {
    if socks_port == http_port {
        return Err(ManagerError::InvalidOperation(format!(
            "local SOCKS and HTTP ports both use {socks_port}; change Subscriptions > Runtime > Local SOCKS port or Local HTTP port, then restart the core"
        )));
    }
    let _socks = bind_tcp(socks_port, "local SOCKS", "Local SOCKS port")?;
    let _http = bind_tcp(http_port, "local HTTP", "Local HTTP port")?;
    Ok(())
}

fn bind_tcp(port: u16, label: &str, setting: &str) -> Result<TcpListener, ManagerError> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|error| unavailable(label, port, "TCP", setting, &error))
}

fn unavailable(
    label: &str,
    port: u16,
    protocol: &str,
    setting: &str,
    error: &std::io::Error,
) -> ManagerError {
    ManagerError::InvalidOperation(format!(
        "{label} port {port} is unavailable for {protocol} on 127.0.0.1 ({error}); change Subscriptions > Runtime > {setting}, then restart the core"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessRunner;
    use sempre_state::{Layout, Store};

    #[test]
    fn manager_checks_active_profile_ports_before_startup() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::with_runner(Store::new(Layout::at(root.path())), ProcessRunner)
            .expect("manager");
        let document = manager.store.read().expect("document");
        let profile_id = document
            .active_profile_id
            .as_deref()
            .expect("active profile");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("TCP listener");
        let port = listener.local_addr().expect("TCP address").port();
        manager
            .subscriptions
            .update(|catalog| {
                let profile = catalog
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == profile_id)
                    .expect("profile");
                profile.local_proxy.socks_port = port;
                Ok(())
            })
            .expect("update profile");
        let deployment = Deployment {
            core: "sing-box".into(),
            repository: None,
            reference: "stable".into(),
            version: "1.13.18".into(),
            config_hash: "config".into(),
        };

        let error = manager
            .ensure_local_proxy_ports_available(&document, &deployment)
            .expect_err("occupied active profile port");

        assert!(
            error
                .to_string()
                .contains(&format!("local SOCKS port {port}"))
        );
    }
}
