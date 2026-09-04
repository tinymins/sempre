use std::net::{Ipv4Addr, TcpListener, UdpSocket};

use crate::ManagerError;

pub(crate) fn ensure_available(port: u16) -> Result<(), ManagerError> {
    let address = (Ipv4Addr::LOCALHOST, port);
    let _tcp = TcpListener::bind(address).map_err(|error| unavailable(port, "TCP", &error))?;
    let _udp = UdpSocket::bind(address).map_err(|error| unavailable(port, "UDP", &error))?;
    Ok(())
}

fn unavailable(port: u16, protocol: &str, error: &std::io::Error) -> ManagerError {
    ManagerError::InvalidOperation(format!(
        "core DNS port {port} is unavailable for {protocol} on 127.0.0.1 ({error}); change Subscriptions > Runtime > Core DNS port, then restart the core"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_tcp_conflicts_with_actionable_guidance() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("TCP listener");
        let port = listener.local_addr().expect("TCP address").port();

        let error = ensure_available(port).expect_err("occupied TCP port");

        let message = error.to_string();
        assert!(message.contains(&format!("core DNS port {port}")));
        assert!(message.contains("Subscriptions > Runtime > Core DNS port"));
    }

    #[test]
    fn reports_udp_conflicts_with_actionable_guidance() {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("UDP socket");
        let port = socket.local_addr().expect("UDP address").port();

        let error = ensure_available(port).expect_err("occupied UDP port");

        assert!(error.to_string().contains("unavailable for UDP"));
    }
}
