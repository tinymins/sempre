use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
};

use reqwest::{Client, ClientBuilder, Proxy};
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
    let proxy = current(manager)?;
    Downloader::new_via_http_proxy(user_agent, proxy.address, &proxy.username, &proxy.password)
        .map_err(|error| error.to_string())
}

pub(crate) fn client(manager: &Manager, builder: ClientBuilder) -> Result<Client, String> {
    let runtime = current(manager)?;
    let proxy = Proxy::all(format!("http://{}", runtime.address))
        .map_err(|error| format!("configure update proxy: {error}"))?
        .basic_auth(&runtime.username, &runtime.password);
    builder
        .proxy(proxy)
        .build()
        .map_err(|error| format!("build proxied update client: {error}"))
}

pub(crate) async fn proxy_first<T, P, D, DF, F>(
    operation: &str,
    proxy: Result<P, String>,
    before_direct: F,
    direct: D,
) -> Result<T, String>
where
    P: Future<Output = Result<T, String>>,
    D: FnOnce() -> DF,
    DF: Future<Output = Result<T, String>>,
    F: FnOnce(bool) -> Result<(), String>,
{
    let (attempted, proxy_error) = match proxy {
        Ok(attempt) => match attempt.await {
            Ok(value) => return Ok(value),
            Err(error) => (true, error),
        },
        Err(error) => (false, error),
    };
    before_direct(attempted)?;
    direct().await.map_err(|direct_error| {
        format!(
            "{operation} through the running core failed: {proxy_error}; direct fallback failed: {direct_error}"
        )
    })
}

fn current(manager: &Manager) -> Result<RuntimeProxy, String> {
    let document = manager
        .state()
        .map_err(|error| format!("read runtime state: {error}"))?;
    let catalog = manager
        .subscriptions()
        .read()
        .map_err(|error| format!("read subscription profiles: {error}"))?;
    resolve(&document, &catalog)
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
