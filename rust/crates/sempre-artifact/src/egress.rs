use std::net::SocketAddr;

use reqwest::{ClientBuilder, Proxy};

/// How a client reaches the network, as chosen by the proxy rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Egress {
    /// Follows the environment's proxy settings.
    System,
    /// Connects directly and ignores the environment.
    Direct,
    HttpProxy {
        address: SocketAddr,
        username: String,
        password: String,
    },
}

impl Egress {
    pub fn apply(&self, builder: ClientBuilder) -> reqwest::Result<ClientBuilder> {
        Ok(match self {
            Self::System => builder,
            Self::Direct => builder.no_proxy(),
            Self::HttpProxy {
                address,
                username,
                password,
            } => builder
                .proxy(Proxy::all(format!("http://{address}"))?.basic_auth(username, password)),
        })
    }
}
