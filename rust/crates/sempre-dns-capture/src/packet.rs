use std::net::{IpAddr, SocketAddr};

use etherparse::{InternetSlice, PacketBuilder, SlicedPacket, TransportSlice};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Flow {
    pub client: SocketAddr,
    pub server: IpAddr,
}

pub struct Packet<'a> {
    pub source: SocketAddr,
    pub destination: SocketAddr,
    pub tcp: bool,
    pub syn: bool,
    pub payload: &'a [u8],
    offset: usize,
}

impl<'a> Packet<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let parsed = SlicedPacket::from_ip(data)?;
        let (source, destination) = match parsed.ip {
            Some(InternetSlice::Ipv4(ip, _)) => {
                (ip.source_addr().into(), ip.destination_addr().into())
            }
            Some(InternetSlice::Ipv6(ip, _)) => {
                (ip.source_addr().into(), ip.destination_addr().into())
            }
            _ => return Err("DNS packet has no IP header".into()),
        };
        let (source_port, destination_port, tcp, syn, header_length) = match parsed.transport {
            Some(TransportSlice::Tcp(tcp)) => (
                tcp.source_port(),
                tcp.destination_port(),
                true,
                tcp.syn() && !tcp.ack(),
                tcp.slice().len(),
            ),
            Some(TransportSlice::Udp(udp)) => {
                (udp.source_port(), udp.destination_port(), false, false, 8)
            }
            _ => return Err("DNS packet has no transport header".into()),
        };
        Ok(Self {
            source: SocketAddr::new(source, source_port),
            destination: SocketAddr::new(destination, destination_port),
            tcp,
            syn,
            payload: parsed.payload,
            offset: data.len() - parsed.payload.len() - header_length,
        })
    }

    pub fn flow(&self, response: bool) -> Flow {
        if response {
            Flow {
                client: SocketAddr::new(self.source.ip(), self.destination.port()),
                server: self.destination.ip(),
            }
        } else {
            Flow {
                client: self.source,
                server: self.destination.ip(),
            }
        }
    }

    pub fn udp_response(&self, response: &[u8]) -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        match (self.destination.ip(), self.source.ip()) {
            (IpAddr::V4(source), IpAddr::V4(destination)) => {
                PacketBuilder::ipv4(source.octets(), destination.octets(), 64)
                    .udp(self.destination.port(), self.source.port())
                    .write(&mut data, response)?;
            }
            (IpAddr::V6(source), IpAddr::V6(destination)) => {
                PacketBuilder::ipv6(source.octets(), destination.octets(), 64)
                    .udp(self.destination.port(), self.source.port())
                    .write(&mut data, response)?;
            }
            _ => return Err("DNS packet changed address family".into()),
        }
        Ok(data)
    }

    pub fn reflect(&self, data: &mut [u8], proxy_port: u16, response: bool) {
        let (source, destination, length) = if self.source.is_ipv4() {
            (12, 16, 4)
        } else {
            (8, 24, 16)
        };
        for index in 0..length {
            data.swap(source + index, destination + index);
        }
        let offset = self.offset + if response { 0 } else { 2 };
        data[offset..offset + 2]
            .copy_from_slice(&if response { 53 } else { proxy_port }.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::{Flow, Packet};
    use etherparse::PacketBuilder;

    #[test]
    fn udp_reply_preserves_client_and_original_dns_identity() {
        let mut data = Vec::new();
        PacketBuilder::ipv6([1; 16], [2; 16], 64)
            .udp(50123, 53)
            .write(&mut data, b"query")
            .unwrap();
        let query = Packet::parse(&data).unwrap();
        assert!(!query.tcp && !query.syn);
        let response = query.udp_response(b"answer").unwrap();
        let response = Packet::parse(&response).unwrap();
        assert_eq!(response.source, query.destination);
        assert_eq!(response.destination, query.source);
        assert_eq!(response.payload, b"answer");
    }

    #[test]
    fn tcp_reflection_tracks_full_client_and_server_addresses() {
        let mut data = Vec::new();
        PacketBuilder::ipv4([10, 0, 0, 2], [8, 8, 8, 8], 64)
            .tcp(50123, 53, 10, 8192)
            .syn()
            .write(&mut data, b"")
            .unwrap();
        let query = Packet::parse(&data).unwrap();
        assert!(query.tcp && query.syn);
        let flow = query.flow(false);
        let mut reflected = data.clone();
        query.reflect(&mut reflected, 45054, false);
        let redirected = Packet::parse(&reflected).unwrap();
        assert_eq!(redirected.source.ip(), flow.server);
        assert_eq!(redirected.destination.ip(), flow.client.ip());
        assert_eq!(redirected.destination.port(), 45054);
        assert_eq!(
            flow,
            Flow {
                client: "10.0.0.2:50123".parse().unwrap(),
                server: "8.8.8.8".parse().unwrap()
            }
        );
    }
}
