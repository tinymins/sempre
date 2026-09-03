use tokio::net::{TcpListener, UdpSocket};

use crate::DnsError;

pub(crate) fn upstream_socket(
    address: std::net::SocketAddr,
    mark: Option<u32>,
) -> Result<UdpSocket, DnsError> {
    let socket = outbound_socket(address, mark, socket2::Type::DGRAM, socket2::Protocol::UDP)?;
    let bind = std::net::SocketAddr::new(
        if address.is_ipv4() {
            std::net::Ipv4Addr::UNSPECIFIED.into()
        } else {
            std::net::Ipv6Addr::UNSPECIFIED.into()
        },
        0,
    );
    socket
        .bind(&bind.into())
        .map_err(|error| DnsError::io("bind DNS upstream socket", error))?;
    UdpSocket::from_std(socket.into())
        .map_err(|error| DnsError::io("open DNS upstream socket", error))
}

pub(crate) async fn upstream_tcp(
    address: std::net::SocketAddr,
    mark: Option<u32>,
) -> Result<tokio::net::TcpStream, DnsError> {
    let socket = outbound_socket(address, mark, socket2::Type::STREAM, socket2::Protocol::TCP)?;
    let stream = tokio::net::TcpSocket::from_std_stream(socket.into())
        .connect(address)
        .await
        .map_err(|error| DnsError::io("connect DNS upstream socket", error))?;
    stream
        .set_nodelay(true)
        .map_err(|error| DnsError::io("configure DNS upstream TCP", error))?;
    Ok(stream)
}

fn outbound_socket(
    address: std::net::SocketAddr,
    mark: Option<u32>,
    kind: socket2::Type,
    protocol: socket2::Protocol,
) -> Result<socket2::Socket, DnsError> {
    let socket = socket2::Socket::new(socket2::Domain::for_address(address), kind, Some(protocol))
        .map_err(|error| DnsError::io("create DNS upstream socket", error))?;
    #[cfg(target_os = "linux")]
    if let Some(mark) = mark {
        socket
            .set_mark(mark)
            .map_err(|error| DnsError::io("mark DNS upstream socket", error))?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = mark;
    socket
        .set_nonblocking(true)
        .map_err(|error| DnsError::io("configure DNS upstream socket", error))?;
    Ok(socket)
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
