use crate::{
    DnsError,
    dns::DnsServer,
    dns_policy::DnsRuntimePolicy,
    model::{DnsConfig, validate},
};

pub struct DnsService {
    server: DnsServer,
}

impl DnsService {
    pub async fn start(config: DnsConfig) -> Result<Self, DnsError> {
        Self::start_with_policy(
            config,
            std::sync::Arc::new(crate::dns_policy::NoopDnsRuntimePolicy),
        )
        .await
    }

    pub async fn start_with_policy(
        config: DnsConfig,
        policy: std::sync::Arc<dyn DnsRuntimePolicy>,
    ) -> Result<Self, DnsError> {
        let mut errors = Vec::new();
        validate(&config, &mut errors);
        if !config.enabled {
            errors.push("DNS service is disabled".into());
        }
        if !errors.is_empty() {
            return Err(DnsError::invalid(errors.join("; ")));
        }
        Ok(Self {
            server: DnsServer::start_with_policy(config, policy).await?,
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
        let config = DnsConfig {
            enabled: true,
            listen_hosts: vec!["127.0.0.1".into()],
            listen_port: std::net::TcpListener::bind("127.0.0.1:0")
                .expect("port probe")
                .local_addr()
                .expect("port")
                .port(),
            ..DnsConfig::default()
        };
        let service = DnsService::start(config).await.expect("start DNS");
        service.stop().await;

        let invalid = DnsConfig {
            enabled: true,
            listen_hosts: vec!["not-an-address".into()],
            ..DnsConfig::default()
        };
        assert!(DnsService::start(invalid).await.is_err());
    }
}
