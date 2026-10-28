use serde::Serialize;
use sysinfo::Networks;

#[derive(Clone, Debug, Serialize)]
pub struct InterfaceAddress {
    pub interface: String,
    pub address: String,
}

pub fn interface_addresses() -> Vec<InterfaceAddress> {
    let networks = Networks::new_with_refreshed_list();
    let mut addresses = Vec::new();
    for (name, data) in &networks {
        for network in data.ip_networks() {
            if !network.addr.is_unspecified()
                && !network.addr.is_multicast()
                && !matches!(network.addr, std::net::IpAddr::V6(ip) if ip.is_unicast_link_local())
            {
                addresses.push(InterfaceAddress {
                    interface: name.clone(),
                    address: network.addr.to_string(),
                });
            }
        }
    }
    addresses.sort_by(|left, right| {
        (&left.interface, &left.address).cmp(&(&right.interface, &right.address))
    });
    addresses
        .dedup_by(|left, right| left.interface == right.interface && left.address == right.address);
    addresses
}
