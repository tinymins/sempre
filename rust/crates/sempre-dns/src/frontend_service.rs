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
        validate_start(&config)?;
        Ok(Self {
            server: DnsServer::start_with_policy(config, policy).await?,
        })
    }

    pub async fn start_with_policy_and_optional_loopback_port(
        config: DnsConfig,
        policy: std::sync::Arc<dyn DnsRuntimePolicy>,
        optional_port: u16,
    ) -> Result<(Self, Option<String>), DnsError> {
        validate_start(&config)?;
        let (server, optional_error) = DnsServer::start_with_policy_and_optional_loopback_port(
            config,
            policy,
            Some(optional_port),
        )
        .await?;
        Ok((Self { server }, optional_error))
    }

    pub async fn stop(self) {
        self.server.stop().await;
    }

    pub fn update(&self, config: DnsConfig) -> Result<(), DnsError> {
        validate_start(&config)?;
        self.server.update(config)
    }
}

fn validate_start(config: &DnsConfig) -> Result<(), DnsError> {
    let mut errors = Vec::new();
    validate(config, &mut errors);
    if !config.enabled {
        errors.push("DNS service is disabled".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DnsError::invalid(errors.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, UdpSocket};

    async fn shared_port() -> u16 {
        for _ in 0..32 {
            let udp = UdpSocket::bind("127.0.0.1:0").await.expect("UDP port");
            let address = udp.local_addr().expect("frontend address");
            if TcpListener::bind(address).await.is_ok() {
                return address.port();
            }
        }
        panic!("no shared UDP/TCP frontend port available");
    }

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

    #[tokio::test]
    async fn optional_loopback_port_listens_without_replacing_the_primary_port() {
        let primary = shared_port().await;
        let optional = shared_port().await;
        let config = DnsConfig {
            enabled: true,
            listen_port: primary,
            ..DnsConfig::default()
        };

        let (service, error) = DnsService::start_with_policy_and_optional_loopback_port(
            config,
            std::sync::Arc::new(crate::dns_policy::NoopDnsRuntimePolicy),
            optional,
        )
        .await
        .expect("start DNS");

        assert!(error.is_none());
        assert_eq!(
            TcpListener::bind(("127.0.0.1", primary))
                .await
                .expect_err("primary listener")
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        assert_eq!(
            TcpListener::bind(("127.0.0.1", optional))
                .await
                .expect_err("optional listener")
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        service.stop().await;
    }

    #[tokio::test]
    async fn occupied_optional_loopback_port_does_not_block_the_primary_listener() {
        let primary = shared_port().await;
        let occupied = UdpSocket::bind("127.0.0.1:0").await.expect("occupied port");
        let optional = occupied.local_addr().expect("occupied address").port();
        let config = DnsConfig {
            enabled: true,
            listen_port: primary,
            ..DnsConfig::default()
        };

        let (service, error) = DnsService::start_with_policy_and_optional_loopback_port(
            config,
            std::sync::Arc::new(crate::dns_policy::NoopDnsRuntimePolicy),
            optional,
        )
        .await
        .expect("start primary DNS");

        assert!(
            error.is_some_and(
                |error| error.contains(&format!("listen DNS UDP 127.0.0.1:{optional}"))
            )
        );
        assert_eq!(
            TcpListener::bind(("127.0.0.1", primary))
                .await
                .expect_err("primary listener")
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        TcpListener::bind(("127.0.0.1", optional))
            .await
            .expect("optional TCP listener was rolled back");
        service.stop().await;
    }
}
