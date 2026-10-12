mod runtime;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use runtime::Runtime;
use sempre_proxy::{Apply, InstanceStatus};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use subtle::ConstantTimeEq as _;
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

struct StateData {
    token: String,
    runtime: Runtime,
}
type Shared = Arc<StateData>;
type Error = (StatusCode, Json<serde_json::Value>);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let token = std::env::var("SEMPRE_PROXY_TOKEN")?;
    if token.len() < 32 {
        return Err("SEMPRE_PROXY_TOKEN must contain at least 32 characters".into());
    }
    let directory = PathBuf::from(
        std::env::var("SEMPRE_PROXY_RUNTIME_DIR").unwrap_or_else(|_| "/tmp/sempre-proxy".into()),
    );
    // Configurations are ephemeral; desired and last working configurations live in PostgreSQL.
    if directory.exists() {
        tokio::fs::remove_dir_all(&directory).await?;
    }
    tokio::fs::create_dir_all(&directory).await?;
    let probe_url =
        std::env::var("SEMPRE_PROXY_PROBE_URL").unwrap_or_else(|_| "https://www.baidu.com/".into());
    let state = Arc::new(StateData {
        token,
        runtime: Runtime {
            instances: Mutex::new(HashMap::new()),
            updates: Mutex::new(HashMap::new()),
            draining: Arc::new(Semaphore::new(16)),
            directory,
            probe_url,
        },
    });
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/instances/{id}", get(status).put(apply).delete(remove))
        .with_state(state);
    let bind = std::env::var("SEMPRE_PROXY_BIND").unwrap_or_else(|_| "0.0.0.0:8790".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "subscription proxy runtime listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn authorize(state: &StateData, headers: &HeaderMap) -> Result<(), Error> {
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !bool::from(supplied.as_bytes().ct_eq(state.token.as_bytes())) {
        return Err(error(StatusCode::UNAUTHORIZED, "authentication required"));
    }
    Ok(())
}

async fn apply(
    State(state): State<Shared>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<Apply>,
) -> Result<Json<InstanceStatus>, Error> {
    authorize(&state, &headers)?;
    if id != input.id || input.revision < 1 {
        return Err(error(StatusCode::BAD_REQUEST, "invalid instance identity"));
    }
    // Revalidate raw outbounds even when called by an authenticated control client.
    sempre_proxy::parse(
        &input.connection.outbound.to_string(),
        Some(input.connection.engine.clone()),
    )
    .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid managed outbound"))?;
    state
        .runtime
        .apply(input)
        .await
        .map(Json)
        .map_err(|message| error(StatusCode::SERVICE_UNAVAILABLE, message))
}

async fn status(
    State(state): State<Shared>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<InstanceStatus>, Error> {
    authorize(&state, &headers)?;
    let mut instances = state.runtime.instances.lock().await;
    let instance = instances
        .get_mut(&id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "proxy is stopped"))?;
    if instance
        .child
        .try_wait()
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "status check failed"))?
        .is_some()
    {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "proxy process exited",
        ));
    }
    Ok(Json(instance.status.clone()))
}

async fn remove(
    State(state): State<Shared>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, Error> {
    authorize(&state, &headers)?;
    state.runtime.remove(id).await;
    Ok(StatusCode::NO_CONTENT)
}

fn error(status: StatusCode, message: &str) -> Error {
    (
        status,
        Json(serde_json::json!({"error":{"message":message}})),
    )
}
