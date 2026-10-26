use std::os::fd::{AsRawFd, OwnedFd};

use nix::{
    net::if_::if_nametoindex,
    sys::{
        socket::{self, AddressFamily, MsgFlags, SockFlag, SockType, sockopt},
        time::{TimeVal, TimeValLike},
    },
};

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

mod message;
use message::{Current, HEADER, integer};

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
pub(super) async fn policy(
    _: &dyn command::Runner,
    _: bool,
    _: bool,
) -> Result<(), TransparentError> {
    Ok(())
}

pub(super) async fn observe(
    _: &dyn command::Runner,
    candidates: &[Route],
) -> Result<Vec<Route>, TransparentError> {
    let candidates = candidates.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut connection = Connection::open()?;
        let mut installed = Vec::new();
        for route in candidates {
            // Only an owned route with the complete identity would be deleted.
            if let Some(current) = connection.get(&route)?
                && current.operation(&route, false)?.is_some()
            {
                installed.push(route);
            }
        }
        Ok(installed)
    })
    .await
    .map_err(|e| invalid(e.to_string()))?
}

pub(super) async fn preflight(
    _: &dyn command::Runner,
    routes: &[Route],
) -> Result<(), TransparentError> {
    let routes = routes.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut connection = Connection::open()?;
        for route in routes {
            if let Some(current) = connection.get(&route)? {
                current.operation(&route, true)?;
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
            let operation = match connection.get(&route)? {
                Some(current) => current.operation(&route, add)?,
                None => add.then_some(1),
            };
            if let Some(operation) = operation {
                connection.request(&route, operation)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| invalid(e.to_string()))?
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
        message::current(route, &response)
    }

    fn request(&mut self, route: &Route, operation: u8) -> Result<Vec<u8>, TransparentError> {
        self.sequence += 1;
        let message = message::encode(route, operation, self.sequence)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use message::{IFSCOPE, OWNER, address_bytes, addresses, same_gateway, same_mask};

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

        for (prefix, gateway) in [("0.0.0.0/0", "192.168.1.1"), ("::/0", "fe80::1%en0")] {
            let route = Route {
                prefix: prefix.into(),
                endpoint: Endpoint {
                    interface: "en0".into(),
                    index: 7,
                    gateway: gateway.into(),
                },
                fallback: false,
                scoped: true,
            };
            for operation in [1, 2, 3, 4] {
                let encoded = message::encode(&route, operation, 42).expect("scoped message");
                assert_eq!(integer(&encoded, 8).expect("flags") & IFSCOPE, IFSCOPE);
                assert_eq!(u16::from_ne_bytes([encoded[4], encoded[5]]), 7);
            }
            let mut response = message::encode(&route, 1, 42).expect("default response");
            // Darwin embeds interface scope in sockaddr padding as well as the header.
            let scope_offset = HEADER + if prefix.contains(':') { 24 } else { 8 };
            response[scope_offset..scope_offset + 4].copy_from_slice(&7_u32.to_ne_bytes());
            let owned = message::current(&route, &response)
                .expect("decode")
                .expect("scoped default");
            assert_eq!(owned.operation(&route, true).expect("update"), Some(3));
            assert_eq!(owned.operation(&route, false).expect("delete"), Some(2));
            let mut changed = route.clone();
            changed.endpoint.gateway = if prefix.contains(':') {
                "fe80::2%en0"
            } else {
                "192.168.1.2"
            }
            .into();
            assert_eq!(owned.operation(&changed, false).expect("replacement"), None);
            let system_flags = integer(&response, 8).expect("flags") & !OWNER;
            response[8..12].copy_from_slice(&system_flags.to_ne_bytes());
            let borrowed = message::current(&route, &response)
                .expect("decode")
                .expect("system default");
            assert_eq!(borrowed.operation(&route, true).expect("borrow"), None);
            assert_eq!(borrowed.operation(&route, false).expect("preserve"), None);
            assert!(borrowed.operation(&changed, true).is_err());
            let mut unscoped = route.clone();
            unscoped.scoped = false;
            assert!(
                message::current(&unscoped, &response)
                    .expect("scope mismatch")
                    .is_none()
            );
            response[8..12].copy_from_slice(&(system_flags & !IFSCOPE).to_ne_bytes());
            assert!(
                message::current(&route, &response)
                    .expect("unscoped default")
                    .is_none()
            );
            response[8..12].copy_from_slice(&system_flags.to_ne_bytes());
            response[4..6].copy_from_slice(&8_u16.to_ne_bytes());
            assert!(
                message::current(&route, &response)
                    .expect("other interface")
                    .is_none()
            );
            response[4..6].copy_from_slice(&7_u16.to_ne_bytes());
            response[8..12].copy_from_slice(&(system_flags | 0x1000).to_ne_bytes());
            assert!(
                message::current(&route, &response)
                    .expect("decode")
                    .expect("blackhole")
                    .operation(&route, true)
                    .is_err()
            );
        }
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
            scoped: false,
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
