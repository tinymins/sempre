use std::net::IpAddr;

use ipnet::IpNet;

use super::{Endpoint, Route, TransparentError, invalid};

pub(super) const HEADER: usize = 92; // Darwin rt_msghdr.
pub(super) const OWNER: i32 = 0x8000; // RTF_PROTO1.
pub(super) const IFSCOPE: i32 = 0x0100_0000;

pub(super) fn current(route: &Route, response: &[u8]) -> Result<Option<Current>, TransparentError> {
    let addresses = addresses(response)?;
    let net: IpNet = route
        .prefix
        .parse()
        .map_err(|_| invalid("invalid route prefix"))?;
    let flags = integer(response, 8)?;
    let index = u32::from(u16::from_ne_bytes([response[4], response[5]]));
    if (flags & IFSCOPE != 0) != route.scoped || (route.scoped && index != route.endpoint.index) {
        return Ok(None);
    }
    let host = net.prefix_len() == if net.addr().is_ipv4() { 32 } else { 128 };
    if (flags & 4 != 0) != host
        || addresses[0]
            .as_ref()
            .is_none_or(|value| !same_gateway(value, &address_bytes(net.network(), 0)))
        || (!host
            && match &addresses[2] {
                Some(value) => !same_mask(value, &address_bytes(net.netmask(), 0)),
                None => net.prefix_len() != 0,
            })
    {
        return Ok(None);
    }
    Ok(Some(Current {
        flags,
        index,
        gateway: addresses[1].clone().unwrap_or_default(),
    }))
}

pub(super) struct Current {
    pub(super) flags: i32,
    pub(super) index: u32,
    gateway: Vec<u8>,
}

impl Current {
    fn matches(&self, route: &Route) -> Result<bool, TransparentError> {
        Ok((self.flags & IFSCOPE != 0) == route.scoped
            && self.index == route.endpoint.index
            && same_gateway(&self.gateway, &gateway(route)?))
    }

    pub(super) fn operation(
        &self,
        route: &Route,
        add: bool,
    ) -> Result<Option<u8>, TransparentError> {
        if !add {
            return Ok((self.flags & OWNER != 0 && self.matches(route)?).then_some(2));
        }
        if self.flags & OWNER != 0 {
            return Ok(Some(3));
        }
        // An existing physical scoped route is borrowed, never claimed or removed.
        if route.scoped
            && self.flags & 3 == 3
            && self.flags & (8 | 0x1000) == 0
            && self.matches(route)?
        {
            return Ok(None);
        }
        Err(invalid(format!(
            "route {} is already owned by another service",
            route.prefix
        )))
    }
}

pub(super) fn encode(
    route: &Route,
    operation: u8,
    sequence: i32,
) -> Result<Vec<u8>, TransparentError> {
    let net: IpNet = route
        .prefix
        .parse()
        .map_err(|_| invalid("invalid route prefix"))?;
    let mut message = vec![0_u8; HEADER];
    message[2] = 5;
    message[3] = operation;
    if operation != 4 || route.scoped {
        message[4..6].copy_from_slice(
            &u16::try_from(route.endpoint.index)
                .map_err(|_| invalid("interface index exceeds Darwin route ABI"))?
                .to_ne_bytes(),
        );
    }
    let mut flags: i32 = 1 | 0x800 | OWNER;
    if route.scoped {
        flags |= IFSCOPE;
    }
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
    message[20..24].copy_from_slice(&sequence.to_ne_bytes());
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
    let length = u16::try_from(message.len()).map_err(|_| invalid("route message too large"))?;
    message[..2].copy_from_slice(&length.to_ne_bytes());
    Ok(message)
}

pub(super) fn integer(data: &[u8], offset: usize) -> Result<i32, TransparentError> {
    Ok(i32::from_ne_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(|| invalid("short Darwin route header"))?
            .try_into()
            .map_err(|_| invalid("invalid route integer"))?,
    ))
}

pub(super) fn address_bytes(address: IpAddr, scope: u32) -> Vec<u8> {
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

pub(super) fn addresses(message: &[u8]) -> Result<[Option<Vec<u8>>; 8], TransparentError> {
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

pub(super) fn same_mask(actual: &[u8], expected: &[u8]) -> bool {
    let start = if expected[1] == 2 { 4 } else { 8 };
    let end = if expected[1] == 2 { 8 } else { 24 };
    (start..end).all(|i| actual.get(i).copied().unwrap_or(0) == expected[i])
}

pub(super) fn same_gateway(actual: &[u8], expected: &[u8]) -> bool {
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
            normalize(actual)
                .zip(normalize(expected))
                .is_some_and(|(a, b)| a == b)
        }
        _ => false,
    }
}
