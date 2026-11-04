use std::{collections::HashMap, net::SocketAddr};

use reqwest::Proxy;

use super::{Fetcher, client};
use crate::SubscriptionError;

/// A running local HTTP proxy that carries the downloads of the sources naming it.
pub struct SourceProxyRoute {
    pub id: String,
    pub address: SocketAddr,
    pub username: String,
    pub password: String,
}

impl Fetcher {
    /// Routes sources by their `proxy_id`. A source naming a proxy that is not among the
    /// routes fails instead of connecting directly.
    pub fn with_source_proxies(
        &self,
        routes: &[SourceProxyRoute],
    ) -> Result<Self, SubscriptionError> {
        let mut source_proxies = HashMap::new();
        for route in routes {
            if !route.address.ip().is_loopback() {
                return Err(SubscriptionError::Invalid(
                    "subscription proxy must be loopback".into(),
                ));
            }
            let proxy = Proxy::all(format!("http://{}", route.address))
                .map_err(|error| SubscriptionError::Fetch(error.to_string()))?
                .basic_auth(&route.username, &route.password);
            source_proxies.insert(route.id.clone(), client(Some(proxy))?);
        }
        Ok(Self {
            store: self.store.clone(),
            standard: self.standard.clone(),
            source_proxies,
            resource_routes: self.resource_routes.clone(),
        })
    }
}
