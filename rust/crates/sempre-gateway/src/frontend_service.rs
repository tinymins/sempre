use crate::{
    GatewayError,
    dns::DnsServer,
    model::{DnsConfig, validate_dns},
};

pub struct DnsService {
    server: DnsServer,
}

impl DnsService {
    pub async fn start(config: DnsConfig) -> Result<Self, GatewayError> {
        let mut errors = Vec::new();
        validate_dns(&config, &mut errors);
        if !config.enabled {
            errors.push("DNS service is disabled".into());
        }
        if !errors.is_empty() {
            return Err(GatewayError::invalid(errors.join("; ")));
        }
        Ok(Self {
            server: DnsServer::start(config).await?,
        })
    }

    pub async fn stop(self) {
        self.server.stop().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validates_before_binding_and_stops_cleanly() {
        let mut config = DnsConfig::default();
        config.enabled = true;
        config.listen_hosts = vec!["127.0.0.1".into()];
        config.listen_port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("port probe")
            .local_addr()
            .expect("port")
            .port();
        let service = DnsService::start(config).await.expect("start DNS");
        service.stop().await;

        let mut invalid = DnsConfig::default();
        invalid.enabled = true;
        invalid.listen_hosts = vec!["not-an-address".into()];
        assert!(DnsService::start(invalid).await.is_err());
    }
}
