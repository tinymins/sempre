use std::{
    net::IpAddr,
    os::fd::{AsRawFd, OwnedFd},
};

use ipnet::IpNet;
use nix::{
    net::if_::if_nametoindex,
    sys::{
        socket::{self, AddressFamily, MsgFlags, SockFlag, SockType, sockopt},
        time::{TimeVal, TimeValLike},
    },
};

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

const HEADER: usize = 92; // Darwin rt_msghdr: 36-byte header and 56-byte rt_metrics.
const OWNER: i32 = 0x8000; // RTF_PROTO1, checked together with full route identity.

#[allow(clippy::unused_async)] // Shared asynchronous platform contract.
pub(super) async fn endpoint(
    _: &dyn command::Runner,
    name: &str,
) -> Result<Endpoint, TransparentError> {
    let index = if_nametoindex(name).map_err(|e| invalid(e.to_string()))?;
    Ok(Endpoint {
        interface: name.into(),
        index,
        gateway: String::new(),
    })
}

pub(super) async fn uplinks(
    runner: &dyn command::Runner,
) -> Result<Vec<Endpoint>, TransparentError> {
    let mut result = Vec::new();
    for family in ["-inet", "-inet6"] {
        let output = runner
            .run("/sbin/route", &["-n", "get", family, "default"], None)
            .await?;
        if family == "-inet6" && output.stderr.contains("not in table") {
            continue;
        }
        if !output.success {
            command::require_success("route", output)?;
            continue;
        }
        let name = field(&output.stdout, "interface")
            .ok_or_else(|| invalid("default route has no interface"))?;
        let gateway = field(&output.stdout, "gateway")
            .ok_or_else(|| invalid("default route has no gateway"))?;
        let mut e = endpoint(runner, name).await?;
        e.gateway = gateway.into();
        result.push(e);
    }
    Ok(result)
}

fn field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
}

#[allow(clippy::unused_async)] // Policy rules are required only on Linux.
pub(super) async fn policy(_: &dyn command::Runner, _: bool) -> Result<(), TransparentError> {
    Ok(())
}

pub(super) async fn preflight(
    _: &dyn command::Runner,
    routes: &[Route],
) -> Result<(), TransparentError> {
    let routes = routes.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut connection = Connection::open()?;
        for route in routes {
            if let Some(current) = connection.get(&route)?
                && current.flags & OWNER == 0
            {
                return Err(invalid(format!(
                    "route {} is already owned by another service",
                    route.prefix
                )));
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| invalid(e.to_string()))?
}

pub(super) async fn change(
    _: &dyn command::Runner,
    routes: &[Route],
    add: bool,
) -> Result<(), TransparentError> {
    let routes = routes.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut connection = Connection::open()?;
        for route in routes {
            let current = connection.get(&route)?;
            if add {
                if let Some(current) = &current
                    && current.flags & OWNER == 0
                {
                    return Err(invalid(format!("route {} ownership changed", route.prefix)));
                }
                let operation = if current.is_some() { 3 } else { 1 };
                connection.request(&route, operation)?;
            } else if let Some(current) = current
                && current.flags & OWNER != 0
                && current.index == route.endpoint.index
                && same_gateway(&current.gateway, &gateway(&route)?)
            {
                // Never delete a replacement installed by another owner or uplink.
                connection.request(&route, 2)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| invalid(e.to_string()))?
}

struct Current {
    flags: i32,
    index: u32,
    gateway: Vec<u8>,
}

struct Connection {
    socket: OwnedFd,
    sequence: i32,
}

impl Connection {
    fn open() -> Result<Self, TransparentError> {
        let socket = socket::socket(AddressFamily::Route, SockType::Raw, SockFlag::empty(), None)
            .map_err(|e| invalid(e.to_string()))?;
        socket::setsockopt(&socket, sockopt::ReceiveTimeout, &TimeVal::seconds(3))
            .map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            socket,
            sequence: 0,
        })
    }

    fn get(&mut self, route: &Route) -> Result<Option<Current>, TransparentError> {
        let response = match self.request(route, 4) {
            Ok(response) => response,
            Err(TransparentError::Io { source, .. }) if source.raw_os_error() == Some(3) => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let addresses = addresses(&response)?;
        let net: IpNet = route
            .prefix
            .parse()
            .map_err(|_| invalid("invalid route prefix"))?;
        let mask = address_bytes(net.netmask(), 0);
        let destination = address_bytes(net.network(), 0);
        // GET is longest-prefix lookup. Only an exact destination AND mask is ours.
        if addresses[0].as_deref() != Some(destination.as_slice()) {
            return Ok(None);
        }
        if net.prefix_len() != if net.addr().is_ipv4() { 32 } else { 128 }
            && addresses[2]
                .as_ref()
                .is_none_or(|value| !same_mask(value, &mask))
        {
            return Ok(None);
        }
        Ok(Some(Current {
            flags: integer(&response, 8)?,
            index: u32::from(u16::from_ne_bytes([response[4], response[5]])),
            gateway: addresses[1].clone().unwrap_or_default(),
        }))
    }

    fn request(&mut self, route: &Route, operation: u8) -> Result<Vec<u8>, TransparentError> {
        self.sequence += 1;
        let net: IpNet = route
            .prefix
            .parse()
            .map_err(|_| invalid("invalid route prefix"))?;
        let mut message = vec![0_u8; HEADER];
        message[2] = 5;
        message[3] = operation;
        if operation != 4 {
            message[4..6].copy_from_slice(
                &u16::try_from(route.endpoint.index)
                    .map_err(|_| invalid("interface index exceeds Darwin route ABI"))?
                    .to_ne_bytes(),
            );
        }
        let mut flags: i32 = 1 | 0x800 | OWNER;
        if !route.endpoint.gateway.is_empty() {
            flags |= 2;
        }
        if net.prefix_len() == if net.addr().is_ipv4() { 32 } else { 128 } {
            flags |= 4;
        }
        message[8..12].copy_from_slice(&flags.to_ne_bytes());
        let fields: i32 = if operation == 4 { 0x11 } else { 0x17 };
        message[12..16].copy_from_slice(&fields.to_ne_bytes());
        message[16..20].copy_from_slice(&std::process::id().to_ne_bytes());
        message[20..24].copy_from_slice(&self.sequence.to_ne_bytes());
        message.extend(address_bytes(net.network(), 0));
        if operation == 4 {
            // A GET carrying NETMASK can return the default instead of a host route.
            // Query longest-prefix first, then verify the returned destination/mask.
            message.extend([8, 18, 0, 0, 0, 0, 0, 0]);
        } else {
            message.extend(gateway(route)?);
            message.extend(mask_bytes(net));
            message.extend(link_address(&route.endpoint)?);
        }
        let length =
            u16::try_from(message.len()).map_err(|_| invalid("route message too large"))?;
        message[..2].copy_from_slice(&length.to_ne_bytes());
        socket::send(self.socket.as_raw_fd(), &message, MsgFlags::empty()).map_err(|error| {
            TransparentError::Io {
                context: "send Darwin route message".into(),
                source: error.into(),
            }
        })?;
        let mut response = vec![0_u8; 4096];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err(invalid("Darwin route response timed out"));
            }
            let count = socket::recv(self.socket.as_raw_fd(), &mut response, MsgFlags::empty())
                .map_err(|e| invalid(e.to_string()))?;
            if count < HEADER
                || integer(&response, 16)? != i32::try_from(std::process::id()).unwrap_or_default()
                || integer(&response, 20)? != self.sequence
            {
                continue;
            }
            let errno = integer(&response, 24)?;
            if errno != 0 {
                return Err(TransparentError::Io {
                    context: format!("Darwin route {}", route.prefix),
                    source: std::io::Error::from_raw_os_error(errno),
                });
            }
            return Ok(response[..count].to_vec());
        }
    }
}

fn integer(data: &[u8], offset: usize) -> Result<i32, TransparentError> {
    Ok(i32::from_ne_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(|| invalid("short Darwin route header"))?
            .try_into()
            .map_err(|_| invalid("invalid route integer"))?,
    ))
}

fn address_bytes(address: IpAddr, scope: u32) -> Vec<u8> {
    match address {
        IpAddr::V4(address) => {
            let mut bytes = vec![0; 16];
            bytes[0] = 16;
            bytes[1] = 2;
            bytes[4..8].copy_from_slice(&address.octets());
            bytes
        }
        IpAddr::V6(address) => {
            let mut bytes = vec![0; 28];
            bytes[0] = 28;
            bytes[1] = 30;
            bytes[8..24].copy_from_slice(&address.octets());
            bytes[24..28].copy_from_slice(&scope.to_ne_bytes());
            bytes
        }
    }
}

fn mask_bytes(net: IpNet) -> Vec<u8> {
    let mut bytes = address_bytes(net.netmask(), 0);
    bytes[0] = 0;
    bytes[1] = 0;
    let length = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |last| last + 1);
    bytes[0] = u8::try_from(length).expect("IP sockaddr length");
    bytes.truncate(if length == 0 { 4 } else { (length + 3) & !3 });
    bytes
}

fn link_address(endpoint: &Endpoint) -> Result<Vec<u8>, TransparentError> {
    let name = endpoint.interface.as_bytes();
    let length = 8 + name.len();
    let mut bytes = vec![0; (length + 3) & !3];
    bytes[0] = u8::try_from(length).map_err(|_| invalid("interface name too long"))?;
    bytes[1] = 18;
    bytes[2..4].copy_from_slice(
        &u16::try_from(endpoint.index)
            .map_err(|_| invalid("interface index too large"))?
            .to_ne_bytes(),
    );
    bytes[5] = u8::try_from(name.len()).map_err(|_| invalid("interface name too long"))?;
    bytes[8..length].copy_from_slice(name);
    Ok(bytes)
}

fn gateway(route: &Route) -> Result<Vec<u8>, TransparentError> {
    if route.endpoint.gateway.is_empty() {
        return link_address(&route.endpoint);
    }
    let address: IpAddr = route
        .endpoint
        .gateway
        .split('%')
        .next()
        .unwrap_or_default()
        .parse()
        .map_err(|_| invalid("invalid gateway"))?;
    let scope = if matches!(address, IpAddr::V6(ip) if ip.is_unicast_link_local()) {
        route.endpoint.index
    } else {
        0
    };
    Ok(address_bytes(address, scope))
}

fn addresses(message: &[u8]) -> Result<[Option<Vec<u8>>; 8], TransparentError> {
    let flags = integer(message, 12)?;
    let mut result = [const { None }; 8];
    let mut offset = HEADER;
    for (index, slot) in result.iter_mut().enumerate() {
        if flags & (1 << index) == 0 {
            continue;
        }
        let length = usize::from(
            *message
                .get(offset)
                .ok_or_else(|| invalid("short route sockaddr"))?,
        );
        let padded = if length == 0 { 4 } else { (length + 3) & !3 };
        let bytes = message
            .get(offset..offset + padded)
            .ok_or_else(|| invalid("invalid route sockaddr length"))?;
        *slot = Some(bytes.to_vec());
        offset += padded;
    }
    Ok(result)
}

fn same_mask(actual: &[u8], expected: &[u8]) -> bool {
    let start = if expected[1] == 2 { 4 } else { 8 };
    (start..expected.len()).all(|i| actual.get(i).copied().unwrap_or(0) == expected[i])
}

fn same_gateway(actual: &[u8], expected: &[u8]) -> bool {
    if actual.get(1) != expected.get(1) {
        return false;
    }
    match expected.get(1) {
        Some(18) => actual.get(2..4) == expected.get(2..4),
        Some(2) => actual.get(4..8) == expected.get(4..8),
        Some(30) => {
            let normalize = |bytes: &[u8]| {
                let mut address = bytes.get(8..24)?.to_vec();
                if address[0] == 0xfe && address[1] & 0xc0 == 0x80 {
                    address[2] = 0;
                    address[3] = 0;
                }
                Some(address)
            };
            normalize(actual) == normalize(expected)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_masks_and_scoped_gateways_decode() {
        let mask = address_bytes("255.255.0.0".parse().expect("mask"), 0);
        assert!(same_mask(&[6, 0, 0, 0, 255, 255, 0, 0], &mask));
        assert!(!same_mask(&[5, 0, 0, 0, 255, 0, 0, 0], &mask));
        let expected = address_bytes("fe80::1".parse().expect("gateway"), 1);
        let mut actual = expected.clone();
        actual[10..12].copy_from_slice(&1_u16.to_be_bytes());
        assert!(same_gateway(&actual, &expected));
        let mut message = vec![0; HEADER];
        message[12..16].copy_from_slice(&7_i32.to_ne_bytes());
        message.extend(address_bytes("223.5.5.0".parse().expect("IP"), 0));
        message.extend(address_bytes("192.168.1.1".parse().expect("gateway"), 0));
        message.extend([6, 0, 0, 0, 255, 255, 0, 0]);
        let decoded = addresses(&message).expect("sockaddrs");
        assert!(same_mask(decoded[2].as_ref().expect("mask"), &mask));
        assert!(addresses(&message[..HEADER + 2]).is_err());
    }

    #[test]
    #[ignore = "read-only native routing socket acceptance check"]
    fn native_get_loopback_route_is_read_only() {
        let route = Route {
            prefix: "127.0.0.1/32".into(),
            endpoint: Endpoint {
                interface: "lo0".into(),
                index: if_nametoindex("lo0").expect("loopback"),
                gateway: String::new(),
            },
            fallback: false,
        };
        let current = Connection::open()
            .expect("route socket")
            .get(&route)
            .expect("RTM_GET")
            .expect("exact loopback route");
        assert_eq!(current.index, route.endpoint.index);
        assert_eq!(current.flags & OWNER, 0);
    }
}
