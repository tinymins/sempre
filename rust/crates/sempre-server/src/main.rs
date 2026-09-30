mod auth;
mod config;
mod debug_stream;
mod diagnostic_projection;
mod editor_migration;
mod error;
mod export_target;
mod fetch;
mod maintenance;
mod openwrt_export;
mod source_cache;
mod subscription_compile;
mod subscription_editor;
mod subscription_rules;
mod subscription_selected_nodes;
mod subscription_source_debug;
mod subscription_sources;
mod subscription_stats;
mod subscription_validation;
mod subscriptions;
mod toolbox_account;
mod toolbox_admin;
mod toolbox_network;
mod toolbox_nodes;
mod toolbox_overview;
mod toolbox_public;
mod toolbox_rules;
mod trusted_proxy;

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{any, get, post},
};
use sqlx::PgPool;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::error::ApiError;

#[derive(Clone)]
pub(crate) struct AppState {
    pool: PgPool,
    config: Config,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("sempre_server=info,tower_http=info")),
        )
        .init();
    match std::env::args().nth(1).as_deref() {
        Some("migrate") => {
            let database_url = std::env::var("DATABASE_URL")?;
            let pool = PgPool::connect(&database_url).await?;
            sqlx::migrate!("./toolbox-migrations").run(&pool).await?;
            editor_migration::run(&pool).await?;
            info!("Toolbox schema initialized");
            return Ok(());
        }
        None => {}
        Some(_) => return Err("usage: sempre-server [migrate]".into()),
    }
    let config = Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;
    let schema_ready: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.proxy_subscribes') IS NOT NULL AND to_regclass('public.proxy_custom_nodes') IS NOT NULL AND to_regclass('public.proxy_subscribe_custom_nodes') IS NOT NULL AND to_regclass('public.proxy_access_logs') IS NOT NULL AND to_regclass('public.subscription_source_snapshots') IS NOT NULL AND to_regclass('public.source_debug_cache') IS NOT NULL AND to_regclass('public.subscription_artifacts') IS NOT NULL AND to_regclass('public.invitation_codes') IS NOT NULL AND to_regclass('public.user_avatars') IS NOT NULL AND EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='sessions' AND column_name='id') AND EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='subscription_artifacts' AND column_name='last_success_at')",
    )
    .fetch_one(&pool)
    .await?;
    if !schema_ready {
        return Err(
            "Toolbox schema missing or incompatible; run sempre-server migrate on a new database"
                .into(),
        );
    }
    let editor_ready: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='proxy_subscribes' AND column_name='editor_version')")
        .fetch_one(&pool).await?;
    if !editor_ready {
        return Err("shared editor migration required; run sempre-server migrate".into());
    }
    let pending: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM proxy_subscribes WHERE editor_version=0)")
            .fetch_one(&pool)
            .await?;
    if pending {
        return Err("shared editor migration incomplete; run sempre-server migrate".into());
    }
    let address = config.bind_address;
    let web_root = config.web_root.clone();
    let state = Arc::new(AppState { pool, config });
    tokio::spawn(maintenance::run(state.clone()));
    let protected = subscriptions::router()
        .merge(subscription_compile::router())
        .merge(subscription_stats::router())
        .merge(subscription_source_debug::router())
        .merge(toolbox_nodes::router())
        .merge(toolbox_account::router())
        .merge(toolbox_admin::router())
        .merge(toolbox_overview::router())
        .merge(toolbox_network::router())
        .merge(auth::protected_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/targets", get(targets))
        .route("/api/v1/auth/config", get(auth::public_config))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .merge(toolbox_public::router())
        .merge(toolbox_rules::router())
        .merge(toolbox_account::public_router())
        .merge(protected)
        .route("/api/{*path}", any(api_not_found))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(TraceLayer::new_for_http())
        .fallback_service(
            ServeDir::new(&web_root).fallback(ServeFile::new(web_root.join("index.html"))),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "Sempre multi-user server listening");
    let result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await;
    result?;
    Ok(())
}

async fn targets() -> axum::Json<Vec<sempre_converter::Target>> {
    axum::Json(export_target::available())
}

async fn health() -> Result<&'static str, ApiError> {
    Ok("ok")
}

async fn api_not_found() -> ApiError {
    ApiError::not_found("API route")
}

async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = interrupt => {}, () = terminate => {} }
}
