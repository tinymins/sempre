use std::{collections::HashSet, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use sempre_converter::{CompileResult, Target};
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, auth::CurrentUser, error::ApiError, profiles, publishing};

const MIN_INTERVAL_MINUTES: i32 = 5;
const MAX_INTERVAL_MINUTES: i32 = 43_200;
const SCHEDULER_POLL_SECONDS: u64 = 15;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/v1/profiles/{id}/refresh",
        get(settings).put(update_settings).post(refresh_now),
    )
}

#[derive(Debug, Serialize)]
struct RefreshSettings {
    enabled: bool,
    interval_minutes: i32,
    targets: Vec<String>,
    next_refresh_at: Option<DateTime<Utc>>,
    last_refresh_at: Option<DateTime<Utc>>,
    last_refresh_status: String,
    last_refresh_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefreshSettingsInput {
    enabled: bool,
    interval_minutes: i32,
    targets: Vec<String>,
}

#[derive(Debug)]
struct DueRefresh {
    profile_id: Uuid,
    targets: Vec<String>,
}

async fn settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RefreshSettings>, ApiError> {
    profiles::require_read(&state, id, &user).await?;
    load_settings(&state, id).await.map(Json)
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RefreshSettingsInput>,
) -> Result<Json<RefreshSettings>, ApiError> {
    profiles::require_write(&state, id, &user).await?;
    validate_interval(input.interval_minutes)?;
    let targets = parse_targets(input.targets)?;
    let names: Vec<String> = targets.into_iter().map(|target| target.format).collect();
    sqlx::query("UPDATE profiles SET refresh_enabled = $1, refresh_interval_minutes = $2, publish_targets = $3, next_refresh_at = CASE WHEN $1 THEN NOW() + ($2 * INTERVAL '1 minute') ELSE NULL END WHERE id = $4")
        .bind(input.enabled)
        .bind(input.interval_minutes)
        .bind(names)
        .bind(id)
        .execute(&state.pool)
        .await?;
    load_settings(&state, id).await.map(Json)
}

async fn refresh_now(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CompileResult>>, ApiError> {
    profiles::require_write(&state, id, &user).await?;
    let target_names = load_settings(&state, id).await?.targets;
    let targets = parse_targets(target_names)?;
    mark_running(&state, id).await?;
    match publishing::compile_targets(&state, id, targets).await {
        Ok(batch) => {
            mark_finished(&state, id, batch.partial, None).await?;
            Ok(Json(batch.results))
        }
        Err(error) => {
            mark_finished(&state, id, false, Some(error.message())).await?;
            Err(error)
        }
    }
}

pub(crate) async fn run(state: Arc<AppState>) {
    loop {
        match claim_due(&state).await {
            Ok(refreshes) => {
                for refresh in refreshes {
                    let result = match parse_targets(refresh.targets) {
                        Ok(targets) => {
                            publishing::compile_targets(&state, refresh.profile_id, targets)
                                .await
                                .map(|batch| batch.partial)
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = mark_finished(
                        &state,
                        refresh.profile_id,
                        result.as_ref().copied().unwrap_or(false),
                        result.as_ref().err().map(ApiError::message),
                    )
                    .await
                    {
                        tracing::error!(profile_id = %refresh.profile_id, error = %error.message(), "failed to record scheduled refresh result");
                    }
                }
            }
            Err(error) => {
                tracing::error!(error = %error.message(), "failed to claim scheduled refreshes");
            }
        }
        tokio::time::sleep(Duration::from_secs(SCHEDULER_POLL_SECONDS)).await;
    }
}

async fn claim_due(state: &AppState) -> Result<Vec<DueRefresh>, ApiError> {
    let rows = sqlx::query("WITH due AS (SELECT id FROM profiles WHERE refresh_enabled = TRUE AND (next_refresh_at IS NULL OR next_refresh_at <= NOW()) ORDER BY next_refresh_at NULLS FIRST FOR UPDATE SKIP LOCKED LIMIT 10) UPDATE profiles p SET next_refresh_at = NOW() + (p.refresh_interval_minutes * INTERVAL '1 minute'), last_refresh_at = NOW(), last_refresh_status = 'running', last_refresh_error = NULL FROM due WHERE p.id = due.id RETURNING p.id, p.publish_targets")
        .fetch_all(&state.pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(DueRefresh {
                profile_id: row.try_get("id").map_err(ApiError::internal)?,
                targets: row.try_get("publish_targets").map_err(ApiError::internal)?,
            })
        })
        .collect()
}

async fn mark_running(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    sqlx::query("UPDATE profiles SET last_refresh_at = NOW(), last_refresh_status = 'running', last_refresh_error = NULL WHERE id = $1")
        .bind(id).execute(&state.pool).await?;
    Ok(())
}

async fn mark_finished(
    state: &AppState,
    id: Uuid,
    partial: bool,
    error: Option<&str>,
) -> Result<(), ApiError> {
    let has_artifacts: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM artifacts WHERE profile_id = $1)")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let status = refresh_status(partial, error.is_some(), has_artifacts);
    sqlx::query(
        "UPDATE profiles SET last_refresh_status = $1, last_refresh_error = $2 WHERE id = $3",
    )
    .bind(status)
    .bind(error.map(|value| value.chars().take(2000).collect::<String>()))
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

fn refresh_status(partial: bool, failed: bool, has_artifacts: bool) -> &'static str {
    if failed {
        if has_artifacts { "stale" } else { "failed" }
    } else if partial {
        "partial"
    } else {
        "success"
    }
}

async fn load_settings(state: &AppState, id: Uuid) -> Result<RefreshSettings, ApiError> {
    let row = sqlx::query("SELECT refresh_enabled, refresh_interval_minutes, publish_targets, next_refresh_at, last_refresh_at, last_refresh_status, last_refresh_error FROM profiles WHERE id = $1")
        .bind(id).fetch_optional(&state.pool).await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    Ok(RefreshSettings {
        enabled: row.try_get("refresh_enabled").map_err(ApiError::internal)?,
        interval_minutes: row
            .try_get("refresh_interval_minutes")
            .map_err(ApiError::internal)?,
        targets: row.try_get("publish_targets").map_err(ApiError::internal)?,
        next_refresh_at: row.try_get("next_refresh_at").map_err(ApiError::internal)?,
        last_refresh_at: row.try_get("last_refresh_at").map_err(ApiError::internal)?,
        last_refresh_status: row
            .try_get("last_refresh_status")
            .map_err(ApiError::internal)?,
        last_refresh_error: row
            .try_get("last_refresh_error")
            .map_err(ApiError::internal)?,
    })
}

fn validate_interval(value: i32) -> Result<(), ApiError> {
    if (MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&value) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "refresh interval must be between 5 and 43200 minutes",
        ))
    }
}

fn parse_targets(values: Vec<String>) -> Result<Vec<Target>, ApiError> {
    if values.is_empty() || values.len() > 16 {
        return Err(ApiError::bad_request(
            "between 1 and 16 publish targets are required",
        ));
    }
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| {
            let target =
                Target::parse(&value).map_err(|error| ApiError::bad_request(error.to_string()))?;
            if !seen.insert(target.format.clone()) {
                return Err(ApiError::bad_request("publish targets must be unique"));
            }
            Ok(target)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_targets, refresh_status, validate_interval};

    #[test]
    fn refresh_settings_reject_busy_intervals_and_duplicate_targets() {
        assert!(validate_interval(4).is_err());
        assert!(validate_interval(5).is_ok());
        assert!(parse_targets(vec!["sing-box-v13".into(), "sing-box-v13".into()]).is_err());
        assert!(parse_targets(vec!["sing-box-v13".into(), "clash-meta".into()]).is_ok());
    }

    #[test]
    fn refresh_outcomes_distinguish_partial_and_preserved_artifacts() {
        assert_eq!(refresh_status(false, false, false), "success");
        assert_eq!(refresh_status(true, false, true), "partial");
        assert_eq!(refresh_status(false, true, true), "stale");
        assert_eq!(refresh_status(false, true, false), "failed");
    }
}
