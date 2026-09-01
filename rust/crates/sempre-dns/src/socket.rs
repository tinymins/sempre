use tokio::net::{TcpListener, UdpSocket};

use crate::DnsError;

#[cfg(target_os = "linux")]
pub(crate) fn upstream_socket(
    mark: Option<u32>,
) -> std::future::Ready<Result<UdpSocket, DnsError>> {
    std::future::ready(linux_upstream_socket(mark))
}

#[cfg(target_os = "linux")]
fn linux_upstream_socket(mark: Option<u32>) -> Result<UdpSocket, DnsError> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|error| DnsError::io("create DNS upstream socket", error))?;
    if let Some(mark) = mark {
        socket
            .set_mark(mark)
            .map_err(|error| DnsError::io("mark DNS upstream socket", error))?;
    }
    socket
        .set_nonblocking(true)
        .map_err(|error| DnsError::io("configure DNS upstream socket", error))?;
    socket
        .bind(&std::net::SocketAddr::from(([0, 0, 0, 0], 0)).into())
        .map_err(|error| DnsError::io("bind DNS upstream socket", error))?;
    UdpSocket::from_std(socket.into())
        .map_err(|error| DnsError::io("open DNS upstream socket", error))
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn upstream_socket(mark: Option<u32>) -> Result<UdpSocket, DnsError> {
    let _ = mark;
    UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|error| DnsError::io("bind DNS upstream socket", error))
}

pub(crate) async fn bind_udp(address: &str, mark: Option<u32>) -> Result<UdpSocket, DnsError> {
    #[cfg(target_os = "linux")]
    if let Some(mark) = mark {
        return marked_socket(address, mark, socket2::Type::DGRAM, socket2::Protocol::UDP)
            .and_then(|socket| {
                UdpSocket::from_std(socket.into())
                    .map_err(|error| DnsError::io(format!("listen DNS UDP {address}"), error))
            });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = mark;
    UdpSocket::bind(address)
        .await
        .map_err(|error| DnsError::io(format!("listen DNS UDP {address}"), error))
}

pub(crate) async fn bind_tcp(address: &str, mark: Option<u32>) -> Result<TcpListener, DnsError> {
    #[cfg(target_os = "linux")]
    if let Some(mark) = mark {
        return marked_socket(address, mark, socket2::Type::STREAM, socket2::Protocol::TCP)
            .and_then(|socket| {
                socket
                    .listen(128)
                    .map_err(|error| DnsError::io(format!("listen DNS TCP {address}"), error))?;
                TcpListener::from_std(socket.into())
                    .map_err(|error| DnsError::io(format!("listen DNS TCP {address}"), error))
            });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = mark;
    TcpListener::bind(address)
        .await
        .map_err(|error| DnsError::io(format!("listen DNS TCP {address}"), error))
}

#[cfg(target_os = "linux")]
fn marked_socket(
    address: &str,
    mark: u32,
    kind: socket2::Type,
    protocol: socket2::Protocol,
) -> Result<socket2::Socket, DnsError> {
    let address = address
        .parse::<std::net::SocketAddr>()
        .map_err(|_| DnsError::invalid(format!("invalid DNS listen address {address:?}")))?;
    let socket = socket2::Socket::new(socket2::Domain::for_address(address), kind, Some(protocol))
        .map_err(|error| DnsError::io(format!("create DNS listener {address}"), error))?;
    socket
        .set_mark(mark)
        .map_err(|error| DnsError::io(format!("mark DNS listener {address}"), error))?;
    socket
        .set_nonblocking(true)
        .map_err(|error| DnsError::io(format!("configure DNS listener {address}"), error))?;
    socket
        .bind(&address.into())
        .map_err(|error| DnsError::io(format!("bind DNS listener {address}"), error))?;
    Ok(socket)
}
