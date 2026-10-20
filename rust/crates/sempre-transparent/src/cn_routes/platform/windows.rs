use std::{collections::HashMap, net::IpAddr, time::Duration};

use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, NO_ERROR},
    NetworkManagement::{
        IpHelper::{
            ConvertInterfaceLuidToAlias, CreateIpForwardEntry2, DeleteIpForwardEntry2,
            FreeMibTable, GetIpForwardEntry2, GetIpForwardTable2, GetIpInterfaceEntry,
            GetUnicastIpAddressTable, InitializeIpForwardEntry, InitializeIpInterfaceEntry,
            MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW, MIB_UNICASTIPADDRESS_ROW,
        },
        Ndis::NET_LUID_LH,
    },
    Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, MIB_IPPROTO_NETMGMT},
};

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

#[path = "windows/address.rs"]
mod address;
use address::{decode, encode, gateway, prefix};

const METRIC: u32 = 53717;

fn status(api: &str, code: u32, context: impl std::fmt::Display) -> Result<(), TransparentError> {
    if code == NO_ERROR {
        Ok(())
    } else {
        Err(invalid(format!("{api} ({context}): Windows error {code}")))
    }
}

#[allow(unsafe_code)] // Native API buffers are checked and freed within this function.
fn endpoint(luid: &NET_LUID_LH, index: u32, gateway: String) -> Result<Endpoint, TransparentError> {
    let mut alias = [0_u16; 257];
    // SAFETY: luid and alias are valid buffers for the duration of the call.
    status(
        "ConvertInterfaceLuidToAlias",
        unsafe { ConvertInterfaceLuidToAlias(luid, alias.as_mut_ptr(), alias.len()) },
        index,
    )?;
    let length = alias.iter().position(|c| *c == 0).unwrap_or(alias.len());
    Ok(Endpoint {
        interface: String::from_utf16_lossy(&alias[..length]),
        index,
        gateway,
    })
}

#[allow(unsafe_code)] // Native API buffers are checked and freed within this function.
fn tun(addresses: &[IpAddr]) -> Result<Vec<Endpoint>, TransparentError> {
    let mut table = std::ptr::null_mut();
    // SAFETY: table receives a Windows-owned allocation, freed below after copying rows.
    status(
        "GetUnicastIpAddressTable",
        unsafe { GetUnicastIpAddressTable(AF_UNSPEC, &raw mut table) },
        "TUN discovery",
    )?;
    // SAFETY: success returns a non-null table with NumEntries contiguous rows.
    // Copy them while the allocation is live, then release that allocation exactly once.
    let rows = unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_UNICASTIPADDRESS_ROW>(),
            (*table).NumEntries as usize,
        )
        .to_vec();
        FreeMibTable(table.cast());
        rows
    };
    let mut interfaces = HashMap::new();
    for row in rows {
        if addresses.contains(&decode(&row.Address)?) {
            interfaces
                .entry(row.InterfaceIndex)
                .or_insert(row.InterfaceLuid);
        }
    }
    interfaces
        .into_iter()
        .map(|(index, luid)| endpoint(&luid, index, String::new()))
        .collect()
}

pub(super) async fn discover_tun(
    _: &dyn command::Runner,
    addresses: &[IpAddr],
) -> Result<Endpoint, TransparentError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let mut interfaces = tun(addresses)?;
        if interfaces.len() == 1 {
            return Ok(interfaces.remove(0));
        }
        if interfaces.len() > 1 || tokio::time::Instant::now() >= deadline {
            return Err(invalid(
                "managed TUN is not ready or its address conflicts with another interface",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[allow(unsafe_code)] // Native API buffers are checked and freed within this function.
fn routes() -> Result<Vec<MIB_IPFORWARD_ROW2>, TransparentError> {
    let mut table = std::ptr::null_mut();
    // SAFETY: the returned table is read within its API-reported length and freed once.
    status(
        "GetIpForwardTable2",
        unsafe { GetIpForwardTable2(AF_UNSPEC, &raw mut table) },
        "route enumeration",
    )?;
    // SAFETY: success returns a non-null table with NumEntries contiguous rows.
    // Copy them while the allocation is live, then release that allocation exactly once.
    Ok(unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
            (*table).NumEntries as usize,
        )
        .to_vec();
        FreeMibTable(table.cast());
        rows
    })
}

#[allow(clippy::unused_async)] // Shared asynchronous platform contract.
#[allow(unsafe_code)] // Calls use initialized official Windows binding rows.
pub(super) async fn uplinks(_: &dyn command::Runner) -> Result<Vec<Endpoint>, TransparentError> {
    let mut selected: [Option<(u64, MIB_IPFORWARD_ROW2)>; 2] = [None, None];
    for row in routes()? {
        if row.DestinationPrefix.PrefixLength != 0 {
            continue;
        }
        // SAFETY: family is the common discriminator of SOCKADDR_INET.
        let family = unsafe { row.DestinationPrefix.Prefix.si_family };
        let slot = match family {
            AF_INET => 0,
            AF_INET6 => 1,
            _ => continue,
        };
        let mut interface = MIB_IPINTERFACE_ROW::default();
        // SAFETY: InitializeIpInterfaceEntry initializes the provided valid row.
        unsafe {
            InitializeIpInterfaceEntry(&raw mut interface);
        }
        interface.Family = family;
        interface.InterfaceIndex = row.InterfaceIndex;
        // SAFETY: family and index identify the interface to read into this valid row.
        status(
            "GetIpInterfaceEntry",
            unsafe { GetIpInterfaceEntry(&raw mut interface) },
            row.InterfaceIndex,
        )?;
        let metric = u64::from(row.Metric) + u64::from(interface.Metric);
        if selected[slot]
            .as_ref()
            .is_none_or(|(current, _)| metric < *current)
        {
            selected[slot] = Some((metric, row));
        }
    }
    selected
        .into_iter()
        .flatten()
        .map(|(_, row)| {
            endpoint(
                &row.InterfaceLuid,
                row.InterfaceIndex,
                gateway(&row.NextHop)?,
            )
        })
        .collect()
}

#[allow(clippy::unused_async)] // Shared asynchronous platform contract.
pub(super) async fn preflight(
    _: &dyn command::Runner,
    desired: &[Route],
) -> Result<(), TransparentError> {
    let prefixes = desired
        .iter()
        .map(|route| route.prefix.as_str())
        .collect::<std::collections::HashSet<_>>();
    for existing in routes()? {
        let destination = prefix(&existing.DestinationPrefix)?;
        if prefixes.contains(destination.as_str()) && existing.Metric != METRIC {
            return Err(invalid(format!(
                "Route prefix is already owned: {destination}"
            )));
        }
    }
    Ok(())
}

#[allow(clippy::unused_async)] // Bounded batches complete synchronously under the controller gate.
#[allow(unsafe_code)] // Calls use initialized official Windows binding rows.
pub(super) async fn change(
    _: &dyn command::Runner,
    routes: &[Route],
    add: bool,
) -> Result<(), TransparentError> {
    for route in routes {
        let network = route
            .prefix
            .parse::<ipnet::IpNet>()
            .map_err(|e| invalid(e.to_string()))?;
        let mut key = MIB_IPFORWARD_ROW2::default();
        // SAFETY: initializes the valid row before its identifying fields are assigned.
        unsafe {
            InitializeIpForwardEntry(&raw mut key);
        }
        key.InterfaceIndex = route.endpoint.index;
        key.DestinationPrefix.Prefix = encode(network.addr(), 0);
        key.DestinationPrefix.PrefixLength = network.prefix_len();
        key.NextHop = if route.endpoint.gateway.is_empty() {
            encode(
                if network.addr().is_ipv4() {
                    IpAddr::from([0, 0, 0, 0])
                } else {
                    IpAddr::from([0_u16; 8])
                },
                0,
            )
        } else {
            address::parse_gateway(&route.endpoint.gateway)?
        };
        let context = format!("{} on interface {}", route.prefix, route.endpoint.index);
        let mut existing = key;
        // SAFETY: key contains initialized destination, next hop and interface index.
        let found = unsafe { GetIpForwardEntry2(&raw mut existing) };
        if found == NO_ERROR {
            if existing.Metric != METRIC || existing.Protocol != MIB_IPPROTO_NETMGMT {
                return Err(invalid(format!("Route ownership changed: {context}")));
            }
            if !add {
                // SAFETY: existing is an API-returned row whose ownership was checked.
                let deleted = unsafe { DeleteIpForwardEntry2(&raw const existing) };
                if deleted != ERROR_NOT_FOUND {
                    status("DeleteIpForwardEntry2", deleted, &context)?;
                }
            }
        } else {
            if found != ERROR_NOT_FOUND {
                status("GetIpForwardEntry2", found, &context)?;
            }
            if add {
                // InitializeIpForwardEntry leaves an invalid site prefix and enables
                // special-purpose flags; these are ordinary managed forwarding routes.
                key.SitePrefixLength = 0;
                key.Loopback = false;
                key.AutoconfigureAddress = false;
                key.Publish = false;
                key.Immortal = false;
                key.Metric = METRIC;
                key.Protocol = MIB_IPPROTO_NETMGMT;
                key.ValidLifetime = u32::MAX;
                key.PreferredLifetime = u32::MAX;
                // SAFETY: key is initialized and populated with the requested owned route.
                status(
                    "CreateIpForwardEntry2",
                    unsafe { CreateIpForwardEntry2(&raw const key) },
                    &context,
                )?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::unused_async)] // Policy rules are required only on Linux.
pub(super) async fn policy(_: &dyn command::Runner, _: bool) -> Result<(), TransparentError> {
    Ok(())
}
