use std::{sync::Arc, sync::atomic::AtomicU64, sync::atomic::Ordering, time::Duration};

use axum::{
    Router,
    extract::{Query, State},
    response::{IntoResponse, Response, Sse, sse::Event, sse::KeepAlive},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{api::AppState, runtime_control_api};

const ALL_TOPICS: [&str; 4] = ["traffic", "memory", "connections", "logs"];

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/runtime/events", get(events))
}

#[derive(Default, Deserialize)]
struct EventQuery {
    #[serde(default)]
    topics: String,
}

#[derive(Serialize)]
struct EventPayload {
    topic: &'static str,
    timestamp: DateTime<Utc>,
    sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct StreamMessage {
    topic: &'static str,
    data: Option<Value>,
    error: Option<String>,
}

async fn events(State(state): State<Arc<AppState>>, Query(query): Query<EventQuery>) -> Response {
    let client = match runtime_control_api::client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_control_api::runtime_error(&error),
    };
    let (sender, receiver) = mpsc::channel(64);
    for topic in selected_topics(&query.topics) {
        tokio::spawn(stream_topic(client.clone(), topic, sender.clone()));
    }
    drop(sender);
    let sequence = Arc::new(AtomicU64::new(0));
    let stream = futures_util::stream::unfold(
        (receiver, sequence),
        |(mut receiver, sequence)| async move {
            let message = receiver.recv().await?;
            let payload = EventPayload {
                topic: message.topic,
                timestamp: Utc::now(),
                sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
                data: message.data,
                error: message.error,
            };
            let event = Event::default().event(message.topic).json_data(payload);
            Some((event, (receiver, sequence)))
        },
    );
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

fn selected_topics(requested: &str) -> Vec<&'static str> {
    let mut selected = Vec::new();
    for candidate in requested.split(',').map(str::trim) {
        if let Some(topic) = ALL_TOPICS.iter().copied().find(|topic| *topic == candidate)
            && !selected.contains(&topic)
        {
            selected.push(topic);
        }
    }
    if selected.is_empty() {
        ALL_TOPICS.into()
    } else {
        selected
    }
}

async fn stream_topic(
    client: sempre_core_control::Client,
    topic: &'static str,
    sender: mpsc::Sender<StreamMessage>,
) {
    loop {
        let connection = tokio::select! {
            () = sender.closed() => return,
            result = client.stream(topic) => result,
        };
        let error = match connection {
            Ok(mut stream) => loop {
                let event = tokio::select! {
                    () = sender.closed() => return,
                    result = stream.next_json() => result,
                };
                match event {
                    Ok(data) => {
                        if sender
                            .send(StreamMessage {
                                topic,
                                data: Some(data),
                                error: None,
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => break error,
                }
            },
            Err(error) => error,
        };
        if sender
            .send(StreamMessage {
                topic,
                data: None,
                error: Some(error.to_string()),
            })
            .await
            .is_err()
        {
            return;
        }
        tokio::select! {
            () = sender.closed() => return,
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_topics_are_filtered_deduplicated_and_defaulted() {
        assert_eq!(
            selected_topics("logs,unknown, traffic,logs"),
            ["logs", "traffic"]
        );
        assert_eq!(selected_topics("unknown"), ALL_TOPICS);
        assert_eq!(selected_topics(""), ALL_TOPICS);
    }
}
