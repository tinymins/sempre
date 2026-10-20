use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, SOCKADDR_IN, SOCKADDR_IN6,
    SOCKADDR_IN6_0, SOCKADDR_INET,
};

use crate::NetworkError;

fn invalid(message: impl Into<String>) -> NetworkError {
    std::io::Error::other(message.into()).into()
}

#[allow(unsafe_code)] // Reads only the union member selected by the address family.
pub(super) fn decode(address: &SOCKADDR_INET) -> Result<IpAddr, NetworkError> {
    // SAFETY: the family discriminator selects the initialized union member.
    unsafe {
        match address.si_family {
            AF_INET => Ok(Ipv4Addr::from(address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()).into()),
            AF_INET6 => Ok(Ipv6Addr::from(address.Ipv6.sin6_addr.u.Byte).into()),
            family => Err(invalid(format!(
                "unsupported Windows address family {family}"
            ))),
        }
    }
}

pub(super) fn encode(ip: IpAddr, scope: u32) -> SOCKADDR_INET {
    match ip {
        IpAddr::V4(ip) => SOCKADDR_INET {
            Ipv4: SOCKADDR_IN {
                sin_family: AF_INET,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: u32::from_ne_bytes(ip.octets()),
                    },
                },
                ..Default::default()
            },
        },
        IpAddr::V6(ip) => SOCKADDR_INET {
            Ipv6: SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 { Byte: ip.octets() },
                },
                Anonymous: SOCKADDR_IN6_0 {
                    sin6_scope_id: scope,
                },
                ..Default::default()
            },
        },
    }
}

#[allow(unsafe_code)] // Reads only the union member selected by the address family.
pub(super) fn gateway(address: &SOCKADDR_INET) -> Result<String, NetworkError> {
    let ip = decode(address)?;
    // SAFETY: IPv6 decoding above confirms the initialized IPv6 member.
    if ip.is_ipv6() {
        let scope = unsafe { address.Ipv6.Anonymous.sin6_scope_id };
        if scope != 0 {
            return Ok(format!("{ip}%{scope}"));
        }
    }
    Ok(ip.to_string())
}

pub(super) fn parse_gateway(value: &str) -> Result<SOCKADDR_INET, NetworkError> {
    let (ip, scope) = value
        .split_once('%')
        .map_or((value, Ok(0)), |(ip, scope)| (ip, scope.parse::<u32>()));
    let ip = ip
        .parse()
        .map_err(|e| invalid(format!("invalid route gateway {value}: {e}")))?;
    let scope = scope.map_err(|e| invalid(format!("invalid route gateway scope {value}: {e}")))?;
    Ok(encode(ip, scope))
}
