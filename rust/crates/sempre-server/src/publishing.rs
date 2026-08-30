use std::sync::Arc;

use sempre_converter::{CompileRequest, CompileResult, Diagnostic, Profile, Target, compile};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, custom_nodes, error::ApiError, fetch};

pub(crate) struct CompileBatch {
    pub(crate) results: Vec<CompileResult>,
    pub(crate) partial: bool,
}

struct PreparedCompile {
    revision: i64,
    request: CompileRequest,
    diagnostics: Vec<Diagnostic>,
}

pub(crate) async fn compile_target(
    state: &Arc<AppState>,
    profile_id: Uuid,
    target: Target,
) -> Result<CompileResult, ApiError> {
    let prepared = prepare_compile(state, profile_id).await?;
    let result = compile_prepared(&prepared, target)?;
    store_results(
        state,
        profile_id,
        prepared.revision,
        std::slice::from_ref(&result),
    )
    .await?;
    Ok(result)
}

fn compile_prepared(
    prepared: &PreparedCompile,
    mut target: Target,
) -> Result<CompileResult, ApiError> {
    target.standalone = true;
    let mut request = prepared.request.clone();
    request.target = target;
    let mut result = compile(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    result
        .diagnostics
        .splice(0..0, prepared.diagnostics.clone());
    Ok(result)
}

pub(crate) async fn compile_request(
    state: &Arc<AppState>,
    profile_id: Uuid,
    mut target: Target,
) -> Result<(i64, CompileRequest), ApiError> {
    target.standalone = true;
    let prepared = prepare_compile(state, profile_id).await?;
    let mut request = prepared.request;
    request.target = target;
    Ok((prepared.revision, request))
}

async fn store_results(
    state: &AppState,
    profile_id: Uuid,
    revision: i64,
    results: &[CompileResult],
) -> Result<(), ApiError> {
    let mut transaction = state.pool.begin().await?;
    for result in results {
        let diagnostics = serde_json::to_value(&result.diagnostics).map_err(ApiError::internal)?;
        sqlx::query("INSERT INTO artifacts (profile_id, revision, target, content, content_hash, node_count, diagnostics) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (profile_id, revision, target) DO UPDATE SET content = EXCLUDED.content, content_hash = EXCLUDED.content_hash, node_count = EXCLUDED.node_count, diagnostics = EXCLUDED.diagnostics, created_at = NOW()")
        .bind(profile_id)
        .bind(revision)
        .bind(&result.format)
        .bind(&result.content)
        .bind(&result.artifact_hash)
        .bind(i32::try_from(result.node_count).map_err(ApiError::internal)?)
        .bind(diagnostics)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn prepare_compile(
    state: &Arc<AppState>,
    profile_id: Uuid,
) -> Result<PreparedCompile, ApiError> {
    let row = sqlx::query("SELECT owner_id, revision, document FROM profiles WHERE id = $1")
        .bind(profile_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    let owner_id: Uuid = row.try_get("owner_id").map_err(ApiError::internal)?;
    let revision: i64 = row.try_get("revision").map_err(ApiError::internal)?;
    let document: Value = row.try_get("document").map_err(ApiError::internal)?;
    let mut profile: Profile = serde_json::from_value(document).map_err(ApiError::internal)?;
    let had_remote_sources = profile
        .sources
        .iter()
        .any(|source| source.enabled && source.kind == "url");
    let loaded = fetch::load_snapshots(state, profile_id, &profile).await?;
    let available = loaded
        .snapshots
        .iter()
        .map(|snapshot| snapshot.source_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for source in &mut profile.sources {
        if source.enabled && !available.contains(source.id.as_str()) {
            source.enabled = false;
        }
    }
    let custom_nodes =
        custom_nodes::load_selected(state, owner_id, &profile.custom_node_ids).await?;
    if had_remote_sources
        && loaded.snapshots.is_empty()
        && profile.manual_servers.is_empty()
        && custom_nodes.is_empty()
    {
        return Err(ApiError::unavailable(
            "all enabled subscription sources are unavailable; keeping last published artifacts",
        ));
    }
    Ok(PreparedCompile {
        revision,
        request: CompileRequest {
            protocol: 1,
            profile,
            snapshots: loaded.snapshots,
            custom_nodes,
            target: Target::parse("sing-box-v13").expect("built-in target"),
        },
        diagnostics: loaded.diagnostics,
    })
}

pub(crate) async fn compile_targets(
    state: &Arc<AppState>,
    profile_id: Uuid,
    targets: Vec<Target>,
) -> Result<CompileBatch, ApiError> {
    let prepared = prepare_compile(state, profile_id).await?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        results.push(compile_prepared(&prepared, target)?);
    }
    store_results(state, profile_id, prepared.revision, &results).await?;
    Ok(CompileBatch {
        results,
        partial: !prepared.diagnostics.is_empty(),
    })
}
