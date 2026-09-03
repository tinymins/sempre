use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, get_sockets_info};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::packet::Packet;

pub struct Owners {
    parent: u32,
    processes: System,
}

impl Owners {
    pub fn new(parent: u32) -> Self {
        Self {
            parent,
            processes: System::new(),
        }
    }

    // Query the current endpoint owner, rather than caching reusable UDP source ports.
    // Unknown ownership fails open so an upstream query cannot recurse through DNS.
    pub fn bypass(&mut self, packet: &Packet<'_>) -> bool {
        let family = if packet.source.is_ipv4() {
            AddressFamilyFlags::IPV4
        } else {
            AddressFamilyFlags::IPV6
        };
        let protocol = if packet.tcp {
            ProtocolFlags::TCP
        } else {
            ProtocolFlags::UDP
        };
        let Ok(sockets) = get_sockets_info(family, protocol) else {
            return true;
        };
        let mut found = false;
        for socket in sockets {
            if socket.local_port() != packet.source.port()
                || (!socket.local_addr().is_unspecified()
                    && socket.local_addr() != packet.source.ip())
            {
                continue;
            }
            if let ProtocolSocketInfo::Tcp(tcp) = &socket.protocol_socket_info
                && (tcp.remote_addr != packet.destination.ip()
                    || tcp.remote_port != packet.destination.port())
            {
                continue;
            }
            for pid in socket.associated_pids {
                found = true;
                if self.owned(pid) {
                    return true;
                }
            }
        }
        !found
    }

    fn owned(&mut self, pid: u32) -> bool {
        if pid == self.parent || pid == std::process::id() {
            return true;
        }
        let pid = Pid::from_u32(pid);
        self.processes.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        self.processes
            .process(pid)
            .and_then(sysinfo::Process::parent)
            .is_some_and(|parent| parent.as_u32() == self.parent)
    }
}
