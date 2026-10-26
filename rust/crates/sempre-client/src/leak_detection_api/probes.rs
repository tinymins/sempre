pub(super) mod control;
mod helper;
mod http;
pub(super) mod types;
mod worker;

use super::{
    model::{Connection, Input, Report},
    platform::ProcessInfo,
};
pub(crate) use helper::main as helper_main;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use types::ProbeResult;

pub(super) enum Event {
    Registered {
        id: String,
        process: ProcessInfo,
        activate: oneshot::Sender<()>,
    },
    Records(Vec<Connection>),
    Result(ProbeResult),
}
pub(super) struct Session {
    pub receiver: mpsc::Receiver<Event>,
    sender: mpsc::Sender<Event>,
    workers: Vec<JoinHandle<()>>,
    pending: Vec<(u32, oneshot::Sender<()>)>,
    pub processes: Vec<ProcessInfo>,
    context: worker::Context,
}
impl Drop for Session {
    fn drop(&mut self) {
        for worker in &self.workers {
            worker.abort();
        }
    }
}
impl Session {
    pub fn new(
        input: Input,
        core: Option<sempre_core_control::Client>,
        proxies: Vec<sempre_core_control::Proxy>,
        ranges: Vec<ipnet::IpNet>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        Self {
            receiver,
            sender,
            workers: vec![],
            pending: vec![],
            processes: vec![],
            context: worker::Context {
                input,
                core,
                proxies,
                ranges,
            },
        }
    }
    pub fn command(&mut self, command: control::Command, report: &mut Report) {
        match command {
            control::Command::Start
                if report.probes.status == "idle" && report.ready && report.topology == "local" =>
            {
                report.probes.status = "running".into();
                report.probes.available = false;
                report.probes.reason = "独立进程探针；与真实应用分开统计".into();
                for id in ["dns", "ipv4", "ipv6"] {
                    report.probes.results.push(ProbeResult {
                        id: id.into(),
                        status: "pending".into(),
                        detail: "等待独立进程注册到采集器".into(),
                        pid: None,
                        addresses: vec![],
                        public_ip: None,
                        elapsed_ms: 0,
                    });
                    self.workers.push(tokio::spawn(worker::run(
                        id.into(),
                        self.context.clone(),
                        self.sender.clone(),
                    )));
                }
            }
            control::Command::Stop => self.stop(report),
            control::Command::Start => (),
        }
    }
    pub fn apply(&mut self, event: Event, report: &mut Report) {
        if report.probes.status != "running" {
            return;
        }
        match event {
            Event::Registered {
                id,
                process,
                activate,
            } => {
                if let Some(result) = report
                    .probes
                    .results
                    .iter_mut()
                    .find(|result| result.id == id)
                {
                    result.pid = Some(process.pid);
                    result.detail = "独立进程已就绪，等待一次真实采样后开始流量".into();
                }
                self.pending.push((process.pid, activate));
                self.processes.push(process);
            }
            Event::Records(records) => super::observe::merge_records(report, records),
            Event::Result(mut result) => {
                if let Some(existing) = report
                    .probes
                    .results
                    .iter_mut()
                    .find(|existing| existing.id == result.id)
                {
                    result.pid = result.pid.or(existing.pid);
                    *existing = result;
                }
                if report
                    .probes
                    .results
                    .iter()
                    .all(|result| !["pending", "running"].contains(&result.status.as_str()))
                {
                    report.probes.status = "completed".into();
                }
            }
        }
    }
    pub fn sampled(&mut self, processes: &[ProcessInfo], report: &mut Report) {
        let mut waiting = Vec::new();
        for (pid, activate) in self.pending.drain(..) {
            if processes
                .iter()
                .any(|process| process.pid == pid && process.app == "probe")
            {
                let _ = activate.send(());
                if let Some(result) = report
                    .probes
                    .results
                    .iter_mut()
                    .find(|result| result.pid == Some(pid))
                {
                    result.status = "running".into();
                    result.detail = "正在独立执行；每项最长 18 秒".into();
                }
            } else {
                waiting.push((pid, activate));
            }
        }
        self.pending = waiting;
    }
    pub fn stop(&mut self, report: &mut Report) {
        for worker in &self.workers {
            worker.abort();
        }
        self.pending.clear();
        self.processes.clear();
        report.probes.stop();
    }
}
