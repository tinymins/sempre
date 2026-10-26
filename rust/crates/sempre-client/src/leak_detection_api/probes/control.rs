use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
pub(crate) enum Command {
    Start,
    Stop,
}
struct Active {
    id: String,
    local: bool,
    ready: bool,
    used: bool,
    deadline: Instant,
    sender: mpsc::Sender<Command>,
}
static ACTIVE: OnceLock<Mutex<Option<Active>>> = OnceLock::new();
fn active() -> &'static Mutex<Option<Active>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}
pub(crate) struct Guard {
    id: String,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut slot) = active().lock()
            && slot.as_ref().is_some_and(|active| active.id == self.id)
        {
            *slot = None;
        }
    }
}
pub(crate) fn register(
    id: &str,
    local: bool,
    duration: u64,
    sender: mpsc::Sender<Command>,
) -> Guard {
    if let Ok(mut slot) = active().lock() {
        *slot = Some(Active {
            id: id.into(),
            local,
            ready: false,
            used: false,
            deadline: Instant::now() + Duration::from_secs(duration),
            sender,
        });
    }
    Guard { id: id.into() }
}
pub(crate) fn ready(id: &str) {
    if let Ok(mut slot) = active().lock()
        && let Some(active) = slot.as_mut()
        && active.id == id
    {
        active.ready = true;
    }
}
fn dispatch(id: &str, start: bool) -> Response {
    let Ok(mut slot) = active().lock() else {
        return super::super::error(StatusCode::SERVICE_UNAVAILABLE, "观测控制暂不可用");
    };
    let Some(active) = slot.as_mut().filter(|active| active.id == id) else {
        return super::super::error(StatusCode::CONFLICT, "观测已结束或任务 ID 不匹配");
    };
    if !active.local {
        return super::super::error(
            StatusCode::FORBIDDEN,
            "网关自身探针不能代表客户端；请使用客户端浏览器探针",
        );
    }
    if start
        && (!active.ready
            || active.used
            || active.deadline.saturating_duration_since(Instant::now()) < Duration::from_secs(20))
    {
        return super::super::error(
            StatusCode::CONFLICT,
            "采集器须就绪、探针尚未运行，且观测剩余时间至少 20 秒",
        );
    }
    if active
        .sender
        .try_send(if start { Command::Start } else { Command::Stop })
        .is_err()
    {
        return super::super::error(StatusCode::CONFLICT, "探针控制正在处理或观测已结束");
    }
    if start {
        active.used = true;
    }
    (StatusCode::ACCEPTED, Json(json!({"accepted":true}))).into_response()
}
pub(crate) async fn start(Path(id): Path<String>) -> Response {
    dispatch(&id, true)
}
pub(crate) async fn stop(Path(id): Path<String>) -> Response {
    dispatch(&id, false)
}
