use std::sync::Arc;

use sempre_converter::{CompileRequest, CompileResult, Profile, Target, compile};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, custom_nodes, error::ApiError, fetch};

pub(crate) async fn compile_target(
    state: &Arc<AppState>,
    profile_id: Uuid,
    target: Target,
) -> Result<CompileResult, ApiError> {
    let (revision, request) = compile_request(state, profile_id, target).await?;
    let result = compile(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let diagnostics = serde_json::to_value(&result.diagnostics).map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO artifacts (profile_id, revision, target, content, content_hash, node_count, diagnostics) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (profile_id, revision, target) DO UPDATE SET content = EXCLUDED.content, content_hash = EXCLUDED.content_hash, node_count = EXCLUDED.node_count, diagnostics = EXCLUDED.diagnostics, created_at = NOW()")
        .bind(profile_id)
        .bind(revision)
        .bind(&result.format)
        .bind(&result.content)
        .bind(&result.artifact_hash)
        .bind(i32::try_from(result.node_count).map_err(ApiError::internal)?)
        .bind(diagnostics)
        .execute(&state.pool)
        .await?;
    Ok(result)
}

pub(crate) async fn compile_request(
    state: &Arc<AppState>,
    profile_id: Uuid,
    target: Target,
) -> Result<(i64, CompileRequest), ApiError> {
    let row = sqlx::query("SELECT owner_id, revision, document FROM profiles WHERE id = $1")
        .bind(profile_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    let owner_id: Uuid = row.try_get("owner_id").map_err(ApiError::internal)?;
    let revision: i64 = row.try_get("revision").map_err(ApiError::internal)?;
    let document: Value = row.try_get("document").map_err(ApiError::internal)?;
    let profile: Profile = serde_json::from_value(document).map_err(ApiError::internal)?;
    let snapshots = fetch::load_snapshots(state, profile_id, &profile).await?;
    let custom_nodes =
        custom_nodes::load_selected(state, owner_id, &profile.custom_node_ids).await?;
    Ok((
        revision,
        CompileRequest {
            protocol: 1,
            profile,
            snapshots,
            custom_nodes,
            target,
        },
    ))
}

pub(crate) async fn compile_targets(
    state: &Arc<AppState>,
    profile_id: Uuid,
    targets: Vec<Target>,
) -> Result<Vec<CompileResult>, ApiError> {
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        results.push(compile_target(state, profile_id, target).await?);
    }
    Ok(results)
}
