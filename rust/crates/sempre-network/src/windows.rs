//! Read-only Windows network queries shared by network discovery and DNS capture.
use crate::NetworkError;
use std::{io, net::IpAddr};
use windows_sys::{
    Win32::{
        Foundation::NO_ERROR,
        NetworkManagement::{
            IpHelper::{
                ConvertInterfaceGuidToLuid, ConvertInterfaceLuidToIndex, FreeMibTable,
                GetBestRoute2, GetIpForwardTable2, GetIpInterfaceEntry, GetIpNetEntry2,
                GetUnicastIpAddressTable, InitializeIpInterfaceEntry, MIB_IPFORWARD_ROW2,
                MIB_IPINTERFACE_ROW, MIB_IPNET_ROW2, MIB_UNICASTIPADDRESS_ROW,
            },
            Ndis::NET_LUID_LH,
        },
        Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_INET},
    },
    core::GUID,
};
#[path = "windows/address.rs"]
mod address;
use address::{decode, encode, gateway};
#[path = "windows/registry.rs"]
mod registry;

pub struct Adapter {
    pub name: String,
    pub guid: String,
    pub index: u32,
    pub ipv6_index: u32,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
}

pub struct Route {
    pub destination: String,
    pub gateway: String,
    pub index: u32,
    pub metric: u32,
    pub ipv6: bool,
}

pub struct DnsInterface {
    pub guid: String,
    pub original: Vec<String>,
    pub automatic: bool,
    pub default_route: bool,
}

fn status(api: &str, code: u32) -> Result<(), NetworkError> {
    if code == NO_ERROR {
        Ok(())
    } else {
        Err(io::Error::other(format!("{api}: Windows error {code}")).into())
    }
}

#[allow(unsafe_code)] // Official API owns the table until it is copied and freed below.
fn unicast() -> Result<Vec<MIB_UNICASTIPADDRESS_ROW>, NetworkError> {
    let mut table = std::ptr::null_mut();
    // SAFETY: table is a valid output pointer; success provides NumEntries contiguous rows.
    status("GetUnicastIpAddressTable", unsafe {
        GetUnicastIpAddressTable(AF_UNSPEC, &raw mut table)
    })?;
    // SAFETY: copy only the API-reported rows while allocation is alive, then free once.
    Ok(unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_UNICASTIPADDRESS_ROW>(),
            (*table).NumEntries as usize,
        )
        .to_vec();
        FreeMibTable(table.cast());
        rows
    })
}

#[allow(unsafe_code)] // GUID/LUID/index conversion uses local initialized output buffers.
pub fn adapters() -> Result<Vec<Adapter>, NetworkError> {
    let unicast = unicast()?;
    ipconfig::get_adapters()
        .map_err(io::Error::other)?
        .into_iter()
        .map(|adapter| {
            let guid = adapter
                .adapter_name()
                .trim_matches(['{', '}'])
                .to_ascii_lowercase();
            let raw = u128::from_str_radix(&guid.replace('-', ""), 16).map_err(io::Error::other)?;
            let guid_value = GUID::from_u128(raw);
            let mut luid = NET_LUID_LH::default();
            let mut index = 0;
            // SAFETY: all input/output pointers refer to initialized, live local values.
            status("ConvertInterfaceGuidToLuid", unsafe {
                ConvertInterfaceGuidToLuid(&raw const guid_value, &raw mut luid)
            })?;
            status("ConvertInterfaceLuidToIndex", unsafe {
                ConvertInterfaceLuidToIndex(&raw const luid, &raw mut index)
            })?;
            let addresses = unicast
                .iter()
                .filter(|row| {
                    row.InterfaceIndex == index || row.InterfaceIndex == adapter.ipv6_if_index()
                })
                .map(|row| {
                    Ok(format!(
                        "{}/{}",
                        decode(&row.Address)?,
                        row.OnLinkPrefixLength
                    ))
                })
                .collect::<Result<Vec<_>, NetworkError>>()?;
            Ok(Adapter {
                name: adapter.friendly_name().into(),
                guid,
                index,
                ipv6_index: adapter.ipv6_if_index(),
                addresses,
                dns: adapter
                    .dns_servers()
                    .iter()
                    .filter(|ip| ip.is_ipv4())
                    .map(ToString::to_string)
                    .collect(),
            })
        })
        .collect()
}

#[allow(unsafe_code)] // The Windows route table is copied before freeing its owning allocation.
pub fn routes() -> Result<Vec<Route>, NetworkError> {
    let mut table = std::ptr::null_mut();
    // SAFETY: valid output pointer receives an allocation containing NumEntries rows.
    status("GetIpForwardTable2", unsafe {
        GetIpForwardTable2(AF_UNSPEC, &raw mut table)
    })?;
    // SAFETY: rows remain valid until FreeMibTable; no borrowed pointers escape.
    let rows = unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
            (*table).NumEntries as usize,
        )
        .to_vec();
        FreeMibTable(table.cast());
        rows
    };
    rows.into_iter()
        .map(|row| {
            let ip = decode(&row.DestinationPrefix.Prefix)?;
            Ok(Route {
                destination: format!("{ip}/{}", row.DestinationPrefix.PrefixLength),
                gateway: gateway(&row.NextHop)?,
                index: row.InterfaceIndex,
                metric: row.Metric,
                ipv6: ip.is_ipv6(),
            })
        })
        .collect()
}

#[allow(unsafe_code)] // Initializes an official row and reads only the requested family/index.
fn interface(index: u32, ipv6: bool) -> Result<MIB_IPINTERFACE_ROW, NetworkError> {
    let mut row = MIB_IPINTERFACE_ROW::default();
    // SAFETY: row is a valid initialized output buffer for both API calls.
    unsafe {
        InitializeIpInterfaceEntry(&raw mut row);
    }
    row.Family = if ipv6 { AF_INET6 } else { AF_INET };
    row.InterfaceIndex = index;
    status("GetIpInterfaceEntry", unsafe {
        GetIpInterfaceEntry(&raw mut row)
    })?;
    Ok(row)
}

pub fn default_route() -> Result<Route, NetworkError> {
    let mut best: Option<(u64, Route)> = None;
    for route in routes()?
        .into_iter()
        .filter(|route| route.destination == "0.0.0.0/0")
    {
        let metric = u64::from(route.metric) + u64::from(interface(route.index, false)?.Metric);
        if best.as_ref().is_none_or(|(current, _)| metric < *current) {
            best = Some((metric, route));
        }
    }
    best.map(|(_, route)| route)
        .ok_or_else(|| io::Error::other("default route has no interface address").into())
}

#[allow(unsafe_code)] // Reads a neighbor row identified by initialized interface/address fields.
pub fn neighbor_mac(index: u32, gateway: &str) -> Option<String> {
    let mut row = MIB_IPNET_ROW2 {
        InterfaceIndex: index,
        Address: address::parse_gateway(gateway).ok()?,
        ..Default::default()
    };
    // SAFETY: row has an initialized IPv4/IPv6 address and interface index.
    if unsafe { GetIpNetEntry2(&raw mut row) } != NO_ERROR || row.PhysicalAddressLength != 6 {
        return None;
    }
    crate::normalize_mac(
        &row.PhysicalAddress[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[allow(unsafe_code)] // GetBestRoute2 receives live initialized address/row output buffers.
pub(crate) fn route_for(ip: IpAddr) -> Result<crate::route_probe::RouteDecision, NetworkError> {
    let destination = encode(ip, 0);
    let mut row = MIB_IPFORWARD_ROW2::default();
    let mut source = SOCKADDR_INET::default();
    // SAFETY: null source/interface request system selection; all output buffers are valid.
    status("GetBestRoute2", unsafe {
        GetBestRoute2(
            std::ptr::null(),
            0,
            std::ptr::null(),
            &raw const destination,
            0,
            &raw mut row,
            &raw mut source,
        )
    })?;
    let next_hop = decode(&row.NextHop)?;
    Ok(crate::route_probe::RouteDecision {
        address: ip,
        interface: decode(&source)?.to_string(),
        gateway: if next_hop.is_unspecified() {
            "On-link".into()
        } else {
            gateway(&row.NextHop)?
        },
    })
}

pub fn dns_interfaces() -> Result<Vec<DnsInterface>, NetworkError> {
    let defaults = routes()?
        .into_iter()
        .filter(|route| route.destination == "0.0.0.0/0")
        .map(|route| route.index)
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for adapter in adapters()? {
        if adapter.dns.is_empty() || !interface(adapter.index, false)?.Connected {
            continue;
        }
        result.push(DnsInterface {
            automatic: registry::automatic_dns(&adapter.guid)?,
            default_route: defaults.contains(&adapter.index),
            guid: adapter.guid,
            original: adapter.dns,
        });
    }
    Ok(result)
}
