use std::net::{Ipv4Addr, SocketAddr};

use sempre_artifact::Downloader;
use sempre_manager::Manager;
use sempre_state::{Document, RuntimeState};
use sempre_subscription::Catalog;

struct RuntimeProxy {
    address: SocketAddr,
    username: String,
    password: String,
}

pub(crate) fn downloader(manager: &Manager, user_agent: &str) -> Result<Downloader, String> {
    let document = manager
        .state()
        .map_err(|error| format!("read runtime state: {error}"))?;
    let catalog = manager
        .subscriptions()
        .read()
        .map_err(|error| format!("read subscription profiles: {error}"))?;
    let proxy = resolve(&document, &catalog)?;
    Downloader::new_via_http_proxy(user_agent, proxy.address, &proxy.username, &proxy.password)
        .map_err(|error| error.to_string())
}

fn resolve(document: &Document, catalog: &Catalog) -> Result<RuntimeProxy, String> {
    if document.runtime.state != RuntimeState::Running {
        return Err("managed core is not running".into());
    }
    let profile_id = document
        .active_profile_id
        .as_deref()
        .ok_or_else(|| "active subscription profile is unavailable".to_string())?;
    let profile = catalog
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("active subscription profile {profile_id:?} was not found"))?;
    let port = profile.local_proxy.http_port;
    if port == 0 {
        return Err("core HTTP proxy port is unavailable".into());
    }
    Ok(RuntimeProxy {
        address: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        username: profile.local_proxy.username.clone(),
        password: profile.local_proxy.password.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_running_active_profile_proxy() {
        let catalog = Catalog::default();
        let profile = &catalog.profiles[0];
        let document = Document {
            runtime: sempre_state::Runtime {
                state: RuntimeState::Running,
                ..sempre_state::Runtime::default()
            },
            active_profile_id: Some(profile.id.clone()),
            ..Document::default()
        };

        let proxy = resolve(&document, &catalog).expect("proxy");

        assert_eq!(
            proxy.address,
            SocketAddr::from((Ipv4Addr::LOCALHOST, profile.local_proxy.http_port))
        );
        assert_eq!(proxy.username, profile.local_proxy.username);
        assert_eq!(proxy.password, profile.local_proxy.password);
    }

    #[test]
    fn refuses_proxy_fallback_while_core_is_stopped() {
        let catalog = Catalog::default();
        let document = Document {
            active_profile_id: Some(catalog.profiles[0].id.clone()),
            ..Document::default()
        };

        assert_eq!(
            resolve(&document, &catalog).err().as_deref(),
            Some("managed core is not running")
        );
    }
}
