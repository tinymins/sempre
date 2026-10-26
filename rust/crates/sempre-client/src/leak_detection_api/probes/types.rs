use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeResult {
    pub id: String,
    pub status: String,
    pub detail: String,
    pub pid: Option<u32>,
    pub addresses: Vec<String>,
    pub public_ip: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeState {
    pub available: bool,
    pub reason: String,
    pub status: String,
    pub results: Vec<ProbeResult>,
}

impl ProbeState {
    pub fn new(local: bool) -> Self {
        Self {
            available: false,
            reason: if local {
                "等待采集器就绪"
            } else {
                "网关自身探针不能代表客户端；请使用客户端浏览器探针"
            }
            .into(),
            status: if local { "idle" } else { "unavailable" }.into(),
            results: vec![],
        }
    }

    pub fn stop(&mut self) {
        for result in &mut self.results {
            if ["pending", "running"].contains(&result.status.as_str()) {
                result.status = "stopped".into();
                result.detail = "探针已停止；未完成的项目不作通过判断".into();
            }
        }
        if self.status == "running" {
            self.status = "stopped".into();
        }
        self.available = false;
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum Message {
    Ready,
    Connected {
        local: std::net::SocketAddr,
        remote: std::net::SocketAddr,
        phase: String,
    },
    Result {
        addresses: Vec<String>,
        public_ip: Option<String>,
        detail: String,
    },
    Error {
        detail: String,
    },
}
