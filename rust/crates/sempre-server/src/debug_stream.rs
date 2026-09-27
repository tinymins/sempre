use std::{convert::Infallible, future::Future, ops::Deref, time::Instant};

use axum::response::{
    IntoResponse as _, Response,
    sse::{Event, KeepAlive, Sse},
};
use futures_util::stream;
use serde_json::Value;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

struct StreamMessage {
    name: &'static str,
    data: Value,
}

#[derive(Clone, Default)]
pub(crate) struct StageLog {
    events: Vec<Value>,
    sender: Option<UnboundedSender<StreamMessage>>,
}

impl StageLog {
    fn streaming(sender: UnboundedSender<StreamMessage>) -> Self {
        Self {
            events: Vec::new(),
            sender: Some(sender),
        }
    }

    pub(crate) fn push(&mut self, event: Value) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(StreamMessage {
                name: "stage",
                data: event.clone(),
            });
        }
        self.events.push(event);
    }

    pub(crate) fn into_events(self) -> Vec<Value> {
        self.events
    }
}

impl Deref for StageLog {
    type Target = [Value];

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

pub(crate) fn response<F, Fut>(work: F) -> Response
where
    F: FnOnce(StageLog) -> Fut + Send + 'static,
    Fut: Future<Output = Value> + Send + 'static,
{
    let (sender, receiver) = unbounded_channel();
    let stream = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|message: StreamMessage| {
            let event = Event::default()
                .event(message.name)
                .data(message.data.to_string());
            (Ok::<_, Infallible>(event), receiver)
        })
    });
    let response = Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response();
    let worker_sender = sender.clone();
    tokio::spawn(async move {
        let started = Instant::now();
        let work = work(StageLog::streaming(worker_sender.clone()));
        tokio::select! {
            () = worker_sender.closed() => {},
            result = work => {
                let mut result = result;
                let ok = result.get("ok").and_then(Value::as_bool) == Some(true);
                if let Some(object) = result.as_object_mut() {
                    object.insert("elapsedMs".into(), serde_json::json!(started.elapsed().as_millis()));
                }
                let _ = worker_sender.send(StreamMessage {
                    name: if ok { "result" } else { "error" },
                    data: result,
                });
            }
        }
    });
    response
}
