#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
use windows as native;

use super::model::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

pub(super) async fn outbound_interfaces(
    runner: &dyn command::Runner,
) -> Result<[Option<String>; 2], TransparentError> {
    let mut result = [None, None];
    for uplink in native::uplinks(runner).await? {
        let index = usize::from(uplink.gateway.contains(':'));
        result[index] = Some(uplink.interface);
    }
    Ok(result)
}

pub(super) async fn discover(
    runner: &dyn command::Runner,
    addresses: &[String],
) -> Result<(Vec<Endpoint>, Endpoint), TransparentError> {
    let wanted = addresses
        .iter()
        .filter_map(|s| s.parse::<ipnet::IpNet>().ok())
        .map(|n| n.addr())
        .collect::<Vec<_>>();
    #[cfg(target_os = "windows")]
    let tun = native::discover_tun(runner, &wanted).await?;
    #[cfg(not(target_os = "windows"))]
    let tun = discover_tun(runner, &wanted).await?;
    let uplinks = native::uplinks(runner).await?;

    Ok((uplinks, tun))
}

#[cfg(not(target_os = "windows"))]
async fn discover_tun(
    runner: &dyn command::Runner,
    wanted: &[std::net::IpAddr],
) -> Result<Endpoint, TransparentError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
    let names = loop {
        let networks = sysinfo::Networks::new_with_refreshed_list();
        let names = networks
            .iter()
            .filter(|(_, data)| data.ip_networks().iter().any(|n| wanted.contains(&n.addr)))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }
        break names;
    };
    if names.len() != 1 {
        return Err(invalid(
            "managed TUN is not ready or its address conflicts with another interface",
        ));
    }
    native::endpoint(runner, &names[0]).await
}

pub(super) async fn observe(
    runner: &dyn command::Runner,
    candidates: &[Route],
) -> Result<Vec<Route>, TransparentError> {
    native::observe(runner, candidates).await
}

pub(super) async fn preflight(
    runner: &dyn command::Runner,
    routes: &[Route],
) -> Result<(), TransparentError> {
    native::preflight(runner, routes).await
}

pub(super) async fn change(
    runner: &dyn command::Runner,
    routes: &[Route],
    add: bool,
) -> Result<(), TransparentError> {
    native::change(runner, routes, add).await
}

pub(super) async fn policy(
    runner: &dyn command::Runner,
    add: bool,
    capture: bool,
) -> Result<(), TransparentError> {
    native::policy(runner, add, capture).await
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod native {
    use super::{Endpoint, Route, TransparentError, command, invalid};
    pub async fn endpoint(_: &dyn command::Runner, _: &str) -> Result<Endpoint, TransparentError> {
        Err(invalid("CN routing is unsupported on this platform"))
    }
    pub async fn uplinks(_: &dyn command::Runner) -> Result<Vec<Endpoint>, TransparentError> {
        Err(invalid("CN routing is unsupported on this platform"))
    }
    pub async fn observe(
        _: &dyn command::Runner,
        _: &[Route],
    ) -> Result<Vec<Route>, TransparentError> {
        Err(invalid("unsupported CN routing platform"))
    }
    pub async fn preflight(_: &dyn command::Runner, _: &[Route]) -> Result<(), TransparentError> {
        Err(invalid("unsupported CN routing platform"))
    }
    pub async fn change(
        _: &dyn command::Runner,
        _: &[Route],
        _: bool,
    ) -> Result<(), TransparentError> {
        Err(invalid("unsupported CN routing platform"))
    }
    pub async fn policy(_: &dyn command::Runner, _: bool, _: bool) -> Result<(), TransparentError> {
        Ok(())
    }
}
