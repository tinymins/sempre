use serde::{Deserialize, Serialize};

use super::platform::{DnsObservation, ProcessInfo};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    pub duration_seconds: u64,
    pub target: String,
    pub topology: String,
    pub client_ip: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Connection {
    pub id: String,
    #[serde(skip)]
    pub socket_key: String,
    pub pid: Option<u32>,
    pub app: String,
    pub protocol: String,
    pub source: String,
    pub destination: String,
    pub host: String,
    pub ip_version: &'static str,
    pub verdict: &'static str,
    pub rule: String,
    pub chains: Vec<String>,
    pub evidence: Vec<String>,
    pub physical_egress: bool,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Summary {
    pub proxy_route: usize,
    pub direct_route: usize,
    pub unattributed: usize,
    pub ipv4: usize,
    pub ipv6: usize,
    pub physical_egress: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Report {
    pub id: String,
    pub started_at: String,
    pub elapsed_seconds: u64,
    pub target: String,
    pub topology: String,
    pub platform: String,
    pub processes: Vec<ProcessInfo>,
    pub connections: Vec<Connection>,
    pub dns: Vec<DnsObservation>,
    pub gaps: Vec<String>,
    pub summary: Summary,
    pub completed: bool,
    pub capture: super::platform::CaptureSnapshot,
}

impl Report {
    pub fn summarize(&mut self) {
        self.summary = Summary::default();
        for connection in &self.connections {
            if connection.physical_egress {
                self.summary.physical_egress += 1;
            }
            match connection.verdict {
                "proxy-route" => self.summary.proxy_route += 1,
                "direct-route" => self.summary.direct_route += 1,
                _ => self.summary.unattributed += 1,
            }
            match connection.ip_version {
                "ipv4" => self.summary.ipv4 += 1,
                "ipv6" => self.summary.ipv6 += 1,
                _ => (),
            }
        }
    }

    pub fn gap(&mut self, gap: impl Into<String>) {
        let gap = gap.into();
        if !self.gaps.contains(&gap) && self.gaps.len() < 64 {
            self.gaps.push(gap);
        }
    }
}
