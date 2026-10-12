use crate::{AppState, error::ApiError, proxy_crypto};
use reqwest::Method;
use sempre_proxy::{Apply, Connection, InstanceStatus};
use serde::{Deserialize, Serialize};
use sqlx::{Row as _, postgres::PgRow};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct ProxyConfig {
    pub url: url::Url,
    pub token: String,
    pub key: [u8; 32],
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Stored {
    pub content: String,
    pub connection: Connection,
}

pub(crate) struct Route {
    pub url: Option<String>,
    pub identity: String,
    pub name: String,
}

pub(crate) fn config(state: &AppState) -> Result<&ProxyConfig, ApiError> {
    state
        .config
        .proxy
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("subscription proxy runtime is not configured"))
}

pub(crate) fn decode(state: &AppState, id: Uuid, cipher: &str) -> Result<Stored, ApiError> {
    serde_json::from_slice(&proxy_crypto::decrypt(&config(state)?.key, id, cipher)?)
        .map_err(ApiError::internal)
}

fn request(
    state: &AppState,
    id: Uuid,
    method: Method,
) -> Result<reqwest::RequestBuilder, ApiError> {
    let config = config(state)?;
    let url = config
        .url
        .join(&format!("instances/{id}"))
        .map_err(ApiError::internal)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(ApiError::internal)?;
    Ok(client.request(method, url).bearer_auth(&config.token))
}

async fn status(state: &AppState, id: Uuid) -> Result<InstanceStatus, ApiError> {
    let response = request(state, id, Method::GET)?
        .send()
        .await
        .map_err(|_| ApiError::unavailable("subscription proxy runtime is unreachable"))?;
    if !response.status().is_success() {
        return Err(ApiError::unavailable("subscription proxy is not running"));
    }
    response
        .json()
        .await
        .map_err(|_| ApiError::unavailable("invalid proxy runtime response"))
}

async fn apply(
    state: &AppState,
    id: Uuid,
    revision: i64,
    cipher: &str,
) -> Result<InstanceStatus, ApiError> {
    let stored = decode(state, id, cipher)?;
    let response = request(state, id, Method::PUT)?
        .json(&Apply {
            id,
            revision,
            connection: stored.connection,
        })
        .send()
        .await
        .map_err(|_| ApiError::unavailable("subscription proxy runtime is unreachable"))?;
    if !response.status().is_success() {
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let message = body
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("subscription proxy startup failed");
        return Err(ApiError::unavailable(message));
    }
    response
        .json()
        .await
        .map_err(|_| ApiError::unavailable("invalid proxy runtime response"))
}

pub(crate) async fn stop(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    let response = request(state, id, Method::DELETE)?
        .send()
        .await
        .map_err(|_| ApiError::unavailable("subscription proxy runtime is unreachable"))?;
    if !response.status().is_success() {
        return Err(ApiError::unavailable("proxy stop failed"));
    }
    Ok(())
}

pub(crate) async fn reconcile(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    // Serialize desired/active configuration changes with editor writes.
    let mut tx = state.pool.begin().await?;
    let Some(row) =
        sqlx::query("SELECT * FROM subscription_proxies WHERE id=$1 FOR UPDATE SKIP LOCKED")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok(());
    };
    let enabled: bool = row.try_get("enabled")?;
    if !enabled {
        stop(state, id).await?;
        sqlx::query("UPDATE subscription_proxies SET status='stopped',last_error=NULL WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(());
    }
    let revision: i64 = row.try_get("revision")?;
    let active: Option<i64> = row.try_get("active_revision")?;
    let current = status(state, id).await.ok();
    if current.is_none()
        && let (Some(active), Some(cipher)) = (
            active,
            row.try_get::<Option<String>, _>("active_configuration")?,
        )
    {
        // On runtime restart, restore the last working version before trying a pending edit.
        if active != revision {
            let _ = apply(state, id, active, &cipher).await;
        }
    }
    let cipher: String = row.try_get("configuration")?;
    let result = apply(state, id, revision, &cipher).await;
    match result {
        Ok(_) => {
            sqlx::query("UPDATE subscription_proxies SET active_revision=revision,active_configuration=configuration,status='running',last_error=NULL,checked_at=NOW() WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
        }
        Err(error) => {
            let still_running = status(state, id).await.is_ok();
            sqlx::query("UPDATE subscription_proxies SET status=$2,last_error=$3,checked_at=NOW() WHERE id=$1")
                .bind(id).bind(if still_running { "update-failed" } else { "failed" }).bind(error.message()).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn run(state: Arc<AppState>) {
    if state.config.proxy.is_none() {
        return;
    }
    loop {
        match sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM subscription_proxies ORDER BY created_at",
        )
        .fetch_all(&state.pool)
        .await
        {
            Ok(ids) => {
                for id in ids {
                    if let Err(error) = reconcile(&state, id).await {
                        tracing::warn!(message = error.message(), "proxy reconciliation failed");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "proxy configuration load failed"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

pub(crate) async fn visible_row(state: &AppState, id: Uuid, user: Uuid) -> Result<PgRow, ApiError> {
    sqlx::query("SELECT * FROM subscription_proxies WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text))")
        .bind(id).bind(user).fetch_optional(&state.pool).await?.ok_or_else(|| ApiError::not_found("subscription proxy"))
}

pub(crate) async fn resolve(
    state: &AppState,
    user: Uuid,
    id: Option<Uuid>,
) -> Result<Route, ApiError> {
    let Some(id) = id else {
        return Ok(Route {
            url: None,
            identity: format!("{user}:system"),
            name: "system".into(),
        });
    };
    let row = visible_row(state, id, user).await?;
    if !row.try_get::<bool, _>("enabled")? {
        return Err(ApiError::unavailable(
            "selected subscription proxy is disabled",
        ));
    }
    let current = status(state, id).await?;
    let mut url = config(state)?.url.clone();
    // The control API may use TLS; managed data listeners always speak HTTP.
    url.set_scheme("http")
        .map_err(|()| ApiError::unavailable("invalid runtime address"))?;
    url.set_path("");
    url.set_query(None);
    url.set_port(Some(current.port))
        .map_err(|()| ApiError::unavailable("invalid runtime address"))?;
    url.set_username(&current.username)
        .map_err(|()| ApiError::unavailable("invalid runtime address"))?;
    url.set_password(Some(&current.password))
        .map_err(|()| ApiError::unavailable("invalid runtime address"))?;
    Ok(Route {
        url: Some(url.to_string()),
        identity: format!("{user}:{id}:{}", current.revision),
        name: row.try_get("name")?,
    })
}

pub(crate) async fn owner(
    state: &AppState,
    scope: Option<Uuid>,
    viewer: Uuid,
) -> Result<Uuid, ApiError> {
    match scope {
        Some(id) => Ok(
            sqlx::query_scalar("SELECT user_id FROM proxy_subscribes WHERE id=$1")
                .bind(id)
                .fetch_one(&state.pool)
                .await?,
        ),
        None => Ok(viewer),
    }
}

pub(crate) async fn ensure_schema(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let ready: bool =
        sqlx::query_scalar("SELECT to_regclass('public.subscription_proxies') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !ready {
        return Err("subscription proxy schema required; run sempre-server migrate".into());
    }
    Ok(())
}
