use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response, Sse, sse::Event, sse::KeepAlive},
    routing::post,
};
use sempre_network::DiagnosticProgress;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/network/diagnostics", post(run))
}

#[derive(Deserialize)]
struct DiagnosticInput {
    target: String,
}

struct StreamEvent {
    name: &'static str,
    data: Value,
}

async fn run(State(state): State<Arc<AppState>>, Json(input): Json<DiagnosticInput>) -> Response {
    let runtime_running = state
        .manager
        .runtime_status()
        .is_ok_and(|status| status.runtime_state == sempre_state::RuntimeState::Running);
    let (sender, receiver) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let worker_sender = sender.clone();
        let progress_sender = sender.clone();
        let work = sempre_network::run_network_diagnostics_with_progress(
            &input.target,
            runtime_running,
            move |progress| {
                let event = match progress {
                    DiagnosticProgress::Started { layer } => StreamEvent {
                        name: "layer-started",
                        data: json!({ "layer": layer }),
                    },
                    DiagnosticProgress::Completed { layer } => StreamEvent {
                        name: "layer-completed",
                        data: json!({ "layer": layer }),
                    },
                };
                let _ = progress_sender.send(event);
            },
        );
        tokio::select! {
            () = worker_sender.closed() => {}
            result = work => {
                let event = match result {
                    Ok(report) => StreamEvent { name: "result", data: json!(report) },
                    Err(error) => StreamEvent {
                        name: "error",
                        data: json!({ "message": error.to_string() }),
                    },
                };
                let _ = worker_sender.send(event);
            }
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let item = receiver.recv().await?;
        let event = Event::default().event(item.name).json_data(item.data);
        Some((event, receiver))
    });
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}
