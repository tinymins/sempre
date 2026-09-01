use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use tokio::{net::UdpSocket, sync::watch, task::JoinHandle};

use crate::{Config, GatewayError, LeaseView};

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

pub(crate) struct DhcpServer {
    state: Arc<Mutex<LeaseState>>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

#[derive(Clone, Debug)]
struct Lease {
    mac: String,
    ip: Ipv4Addr,
    hostname: String,
    expires_at: DateTime<Utc>,
}

struct LeaseState {
    config: Config,
    leases: HashMap<String, Lease>,
    server_id: Ipv4Addr,
}

impl DhcpServer {
    pub(crate) async fn start(config: Config) -> Result<Self, GatewayError> {
        let state = Arc::new(Mutex::new(LeaseState::new(config)?));
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DHCP_SERVER_PORT))
            .await
            .map_err(|error| GatewayError::io("listen DHCP", error))?;
        socket
            .set_broadcast(true)
            .map_err(|error| GatewayError::io("enable DHCP broadcast", error))?;
        let (shutdown, mut receiver) = watch::channel(false);
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let mut buffer = [0_u8; 1500];
            loop {
                tokio::select! {
                    changed = receiver.changed() => {
                        if changed.is_err() || *receiver.borrow() { return; }
                    }
                    result = socket.recv_from(&mut buffer) => {
                        let Ok((count, _)) = result else { return; };
                        let response = task_state.lock().ok()
                            .and_then(|mut state| state.response(&buffer[..count]));
                        if let Some(response) = response {
                            let destination = SocketAddrV4::new(Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT);
                            let _ = socket.send_to(&response, destination).await;
                        }
                    }
                }
            }
        });
        Ok(Self {
            state,
            shutdown,
            task,
        })
    }

    pub(crate) async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }

    pub(crate) fn leases(&self) -> Vec<LeaseView> {
        self.state
            .lock()
            .map_or_else(|_| Vec::new(), |state| state.lease_views())
    }

    pub(crate) fn revoke(&self, mac: &str) -> Result<(), GatewayError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| GatewayError::invalid("DHCP lease state is unavailable"))?;
        if state.leases.remove(&mac.to_ascii_lowercase()).is_none() {
            return Err(GatewayError::invalid(format!("lease for {mac} not found")));
        }
        Ok(())
    }
}

impl LeaseState {
    fn new(config: Config) -> Result<Self, GatewayError> {
        let server_id = config
            .lan
            .gateway_cidr
            .split_once('/')
            .and_then(|(address, _)| address.parse().ok())
            .ok_or_else(|| GatewayError::invalid("invalid LAN gateway CIDR"))?;
        Ok(Self {
            config,
            leases: HashMap::new(),
            server_id,
        })
    }

    fn response(&mut self, packet: &[u8]) -> Option<Vec<u8>> {
        if packet.len() < 240 || packet[0] != 1 || packet[236..240] != MAGIC_COOKIE {
            return None;
        }
        let options = parse_options(&packet[240..]);
        let message_type = options.get(&53).and_then(|value| value.first()).copied()?;
        if !matches!(message_type, 1 | 3) {
            return None;
        }
        let hardware_length = usize::from(packet[2]);
        if hardware_length == 0 || hardware_length > 16 || 28 + hardware_length > packet.len() {
            return None;
        }
        let mac = format_mac(&packet[28..28 + hardware_length]);
        let hostname = options.get(&12).map_or_else(String::new, |value| {
            String::from_utf8_lossy(value).into_owned()
        });
        let ip = self.allocate(&mac, &hostname)?;
        let mut reply = vec![0_u8; 240];
        reply.copy_from_slice(&packet[..240]);
        reply[0] = 2;
        reply[16..20].copy_from_slice(&ip.octets());
        reply[20..24].copy_from_slice(&self.server_id.octets());
        reply[236..240].copy_from_slice(&MAGIC_COOKIE);
        append_option(&mut reply, 53, &[if message_type == 1 { 2 } else { 5 }]);
        append_option(&mut reply, 54, &self.server_id.octets());
        append_option(&mut reply, 1, &subnet_mask(&self.config.lan.gateway_cidr));
        append_option(&mut reply, 3, &self.server_id.octets());
        append_option(&mut reply, 6, &self.server_id.octets());
        if !self.config.dhcp.domain.is_empty() {
            append_option(&mut reply, 15, self.config.dhcp.domain.as_bytes());
        }
        let seconds = u32::try_from(self.lease_duration().as_secs()).unwrap_or(u32::MAX);
        append_option(&mut reply, 51, &seconds.to_be_bytes());
        reply.push(255);
        Some(reply)
    }

    fn allocate(&mut self, mac: &str, hostname: &str) -> Option<Ipv4Addr> {
        if let Some(reservation) = self
            .config
            .dhcp
            .reservations
            .iter()
            .find(|reservation| reservation.mac.eq_ignore_ascii_case(mac))
        {
            return reservation.ip.parse().ok();
        }
        let key = mac.to_ascii_lowercase();
        let now = Utc::now();
        if let Some(lease) = self.leases.get(&key)
            && lease.expires_at > now
        {
            return Some(lease.ip);
        }
        let start: Ipv4Addr = self.config.dhcp.range_start.parse().ok()?;
        let end: Ipv4Addr = self.config.dhcp.range_end.parse().ok()?;
        let duration = chrono::Duration::from_std(self.lease_duration()).ok()?;
        for value in u32::from(start)..=u32::from(end) {
            let ip = Ipv4Addr::from(value);
            if self.ip_in_use(ip, now) {
                continue;
            }
            self.leases.insert(
                key.clone(),
                Lease {
                    mac: key,
                    ip,
                    hostname: hostname.into(),
                    expires_at: now + duration,
                },
            );
            return Some(ip);
        }
        None
    }

    fn ip_in_use(&self, ip: Ipv4Addr, now: DateTime<Utc>) -> bool {
        self.leases
            .values()
            .any(|lease| lease.ip == ip && lease.expires_at > now)
            || self
                .config
                .dhcp
                .reservations
                .iter()
                .any(|reservation| reservation.ip.parse::<Ipv4Addr>().ok() == Some(ip))
    }

    fn lease_duration(&self) -> std::time::Duration {
        humantime::parse_duration(&self.config.dhcp.lease_time)
            .ok()
            .filter(|duration| !duration.is_zero())
            .unwrap_or(std::time::Duration::from_hours(12))
    }

    fn lease_views(&self) -> Vec<LeaseView> {
        let mut result = self
            .config
            .dhcp
            .reservations
            .iter()
            .map(|reservation| LeaseView {
                mac: reservation.mac.to_ascii_lowercase(),
                ip: reservation.ip.clone(),
                hostname: reservation.hostname.clone(),
                expires_at: None,
                reserved: true,
            })
            .collect::<Vec<_>>();
        let now = Utc::now();
        result.extend(
            self.leases
                .values()
                .filter(|lease| lease.expires_at > now)
                .map(|lease| LeaseView {
                    mac: lease.mac.clone(),
                    ip: lease.ip.to_string(),
                    hostname: lease.hostname.clone(),
                    expires_at: Some(lease.expires_at),
                    reserved: false,
                }),
        );
        result
    }
}

fn parse_options(data: &[u8]) -> HashMap<u8, Vec<u8>> {
    let mut result = HashMap::new();
    let mut offset = 0;
    while offset < data.len() {
        let code = data[offset];
        offset += 1;
        if code == 255 {
            break;
        }
        if code == 0 || offset >= data.len() {
            continue;
        }
        let length = usize::from(data[offset]);
        offset += 1;
        let Some(value) = data.get(offset..offset + length) else {
            break;
        };
        result.insert(code, value.to_vec());
        offset += length;
    }
    result
}

fn append_option(packet: &mut Vec<u8>, code: u8, value: &[u8]) {
    packet.push(code);
    packet.push(u8::try_from(value.len()).unwrap_or(u8::MAX));
    packet.extend_from_slice(&value[..value.len().min(usize::from(u8::MAX))]);
}

fn format_mac(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn subnet_mask(cidr: &str) -> [u8; 4] {
    let prefix = cidr
        .split_once('/')
        .and_then(|(_, prefix)| prefix.parse::<u8>().ok())
        .filter(|prefix| *prefix <= 32)
        .unwrap_or(24);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    mask.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_allocates_and_request_renews_the_same_address() {
        let mut state = LeaseState::new(Config::default()).expect("state");
        let packet = request_packet(1);
        let offer = state.response(&packet).expect("offer");
        assert_eq!(&offer[16..20], &[10, 10, 10, 100]);
        let request = state.response(&request_packet(3)).expect("ack");
        assert_eq!(&request[16..20], &[10, 10, 10, 100]);
        assert_eq!(state.lease_views().len(), 1);
    }

    fn request_packet(message_type: u8) -> Vec<u8> {
        let mut packet = vec![0_u8; 240];
        packet[0] = 1;
        packet[1] = 1;
        packet[2] = 6;
        packet[28..34].copy_from_slice(&[0, 1, 2, 3, 4, 5]);
        packet[236..240].copy_from_slice(&MAGIC_COOKIE);
        append_option(&mut packet, 53, &[message_type]);
        packet.push(255);
        packet
    }
}
