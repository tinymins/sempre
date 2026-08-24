use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{CompileRequest, CompileResult, Profile, Target, compile};

use crate::{
    AppState,
    auth::{AuthUser, CurrentUser, random_token, token_hash},
    custom_nodes,
    error::ApiError,
    fetch,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/profiles", get(list).post(create))
        .route(
            "/api/v1/profiles/{id}",
            get(get_profile).put(update).delete(remove),
        )
        .route("/api/v1/profiles/{id}/compile", post(compile_profile))
        .route(
            "/api/v1/profiles/{id}/shares",
            get(list_shares).post(create_share),
        )
        .route(
            "/api/v1/profiles/{id}/members",
            get(list_members).put(upsert_member),
        )
        .route(
            "/api/v1/profiles/{id}/members/{user_id}",
            delete(remove_member),
        )
        .route("/api/v1/shares/{id}", delete(revoke_share))
}

#[derive(Debug, Serialize)]
struct ProfileOutput {
    id: Uuid,
    owner_id: Uuid,
    revision: i64,
    name: String,
    document: Value,
    role: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct ProfileInput {
    name: String,
    document: Value,
}

#[derive(Debug, Deserialize)]
struct CompileInput {
    target: Target,
}

#[derive(Debug, Serialize)]
struct ShareOutput {
    id: Uuid,
    token_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct MemberInput {
    email: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct MemberOutput {
    user_id: Uuid,
    email: String,
    role: String,
}

async fn list(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<ProfileOutput>>, ApiError> {
    let rows = sqlx::query("SELECT p.id, p.owner_id, p.revision, p.name, p.document, p.updated_at, CASE WHEN p.owner_id = $1 THEN 'owner' ELSE pm.role END AS role FROM profiles p LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $1 WHERE p.owner_id = $1 OR pm.user_id = $1 ORDER BY p.updated_at DESC").bind(user.id).fetch_all(&state.pool).await?;
    rows.iter()
        .map(profile_output)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn create(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ProfileInput>,
) -> Result<(StatusCode, Json<ProfileOutput>), ApiError> {
    let name = profile_name(&input.name)?;
    let id = Uuid::new_v4();
    let mut document: Profile = serde_json::from_value(input.document)
        .map_err(|error| ApiError::bad_request(format!("invalid profile document: {error}")))?;
    document.id = id.to_string();
    document.revision = 1;
    document.name.clone_from(&name);
    let value = serde_json::to_value(document).map_err(ApiError::internal)?;
    let row = sqlx::query("INSERT INTO profiles (id, owner_id, name, document) VALUES ($1, $2, $3, $4) RETURNING id, owner_id, revision, name, document, updated_at, 'owner' AS role").bind(id).bind(user.id).bind(&name).bind(value).fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(profile_output(&row)?)))
}

async fn get_profile(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ProfileOutput>, ApiError> {
    let row = profile_row(&state, id, &user)
        .await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    profile_output(&row).map(Json)
}

async fn update(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ProfileInput>,
) -> Result<Json<ProfileOutput>, ApiError> {
    require_write(&state, id, &user).await?;
    let expected = revision_header(&headers)?;
    let name = profile_name(&input.name)?;
    let mut document: Profile = serde_json::from_value(input.document)
        .map_err(|error| ApiError::bad_request(format!("invalid profile document: {error}")))?;
    document.id = id.to_string();
    document.revision = u64::try_from(expected + 1)
        .map_err(|_| ApiError::bad_request("profile revision is invalid"))?;
    document.name.clone_from(&name);
    let value = serde_json::to_value(document).map_err(ApiError::internal)?;
    let row = sqlx::query("UPDATE profiles SET revision = revision + 1, name = $1, document = $2, updated_at = NOW() WHERE id = $3 AND revision = $4 RETURNING id, owner_id, revision, name, document, updated_at, CASE WHEN owner_id = $5 THEN 'owner' ELSE 'editor' END AS role").bind(&name).bind(value).bind(id).bind(expected).bind(user.id).fetch_optional(&state.pool).await?;
    let Some(row) = row else {
        return Err(ApiError::conflict("profile changed; reload before saving"));
    };
    profile_output(&row).map(Json)
}

async fn remove(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM profiles WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("profile"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn compile_profile(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<CompileInput>,
) -> Result<Json<CompileResult>, ApiError> {
    let row = profile_row(&state, id, &user)
        .await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    let revision: i64 = row.try_get("revision").map_err(ApiError::internal)?;
    let document: Value = row.try_get("document").map_err(ApiError::internal)?;
    let profile: Profile = serde_json::from_value(document).map_err(ApiError::internal)?;
    let snapshots = fetch::load_snapshots(&state, id, &profile).await?;
    let owner_id: Uuid = row.try_get("owner_id").map_err(ApiError::internal)?;
    let custom_nodes =
        custom_nodes::load_selected(&state, owner_id, &profile.custom_node_ids).await?;
    let request = CompileRequest {
        protocol: 1,
        profile,
        snapshots,
        custom_nodes,
        target: input.target,
    };
    let result = compile(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let diagnostics = serde_json::to_value(&result.diagnostics).map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO artifacts (profile_id, revision, target, content, content_hash, node_count, diagnostics) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (profile_id, revision, target) DO UPDATE SET content = EXCLUDED.content, content_hash = EXCLUDED.content_hash, node_count = EXCLUDED.node_count, diagnostics = EXCLUDED.diagnostics, created_at = NOW()")
        .bind(id).bind(revision).bind(&result.format).bind(&result.content).bind(&result.artifact_hash).bind(i32::try_from(result.node_count).map_err(ApiError::internal)?).bind(diagnostics).execute(&state.pool).await?;
    Ok(Json(result))
}

async fn create_share(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<ShareOutput>), ApiError> {
    require_owner(&state, id, &user).await?;
    let token = random_token();
    let share_id = Uuid::new_v4();
    let prefix: String = token.chars().take(8).collect();
    let row = sqlx::query("INSERT INTO shares (id, profile_id, owner_id, token_hash, token_prefix) VALUES ($1, $2, $3, $4, $5) RETURNING id, token_prefix, enabled, created_at").bind(share_id).bind(id).bind(user.id).bind(token_hash(&token)).bind(&prefix).fetch_one(&state.pool).await?;
    let url = state
        .config
        .public_url
        .join(&format!("api/v1/public/subscriptions/{token}"))
        .map_err(ApiError::internal)?
        .to_string();
    Ok((StatusCode::CREATED, Json(share_output(&row, Some(url))?)))
}

async fn list_shares(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ShareOutput>>, ApiError> {
    require_owner(&state, id, &user).await?;
    let rows = sqlx::query("SELECT id, token_prefix, enabled, created_at FROM shares WHERE profile_id = $1 ORDER BY created_at DESC").bind(id).fetch_all(&state.pool).await?;
    rows.iter()
        .map(|row| share_output(row, None))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn revoke_share(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("UPDATE shares SET enabled = FALSE, revoked_at = NOW() WHERE id = $1 AND owner_id = $2 AND enabled = TRUE").bind(id).bind(user.id).execute(&state.pool).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("share"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_members(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<MemberOutput>>, ApiError> {
    require_owner(&state, id, &user).await?;
    let rows = sqlx::query("SELECT u.id AS user_id, u.email, pm.role FROM profile_members pm JOIN users u ON u.id = pm.user_id WHERE pm.profile_id = $1 ORDER BY u.email").bind(id).fetch_all(&state.pool).await?;
    rows.iter()
        .map(member_output)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn upsert_member(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<MemberInput>,
) -> Result<Json<MemberOutput>, ApiError> {
    require_owner(&state, id, &user).await?;
    if !matches!(input.role.as_str(), "viewer" | "editor") {
        return Err(ApiError::bad_request(
            "member role must be viewer or editor",
        ));
    }
    let email = input.email.trim().to_lowercase();
    let row = sqlx::query("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("user"))?;
    let member_id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    if member_id == user.id {
        return Err(ApiError::bad_request(
            "profile owner cannot be added as a member",
        ));
    }
    sqlx::query("INSERT INTO profile_members (profile_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT (profile_id, user_id) DO UPDATE SET role = EXCLUDED.role").bind(id).bind(member_id).bind(&input.role).execute(&state.pool).await?;
    Ok(Json(MemberOutput {
        user_id: member_id,
        email,
        role: input.role,
    }))
}

async fn remove_member(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_owner(&state, id, &user).await?;
    let result = sqlx::query("DELETE FROM profile_members WHERE profile_id = $1 AND user_id = $2")
        .bind(id)
        .bind(member_id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("member"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn profile_row(
    state: &AppState,
    id: Uuid,
    user: &AuthUser,
) -> Result<Option<sqlx::postgres::PgRow>, ApiError> {
    sqlx::query("SELECT p.id, p.owner_id, p.revision, p.name, p.document, p.updated_at, CASE WHEN p.owner_id = $2 THEN 'owner' ELSE pm.role END AS role FROM profiles p LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $2 WHERE p.id = $1 AND (p.owner_id = $2 OR pm.user_id = $2)").bind(id).bind(user.id).fetch_optional(&state.pool).await.map_err(Into::into)
}

async fn require_write(state: &AppState, id: Uuid, user: &AuthUser) -> Result<(), ApiError> {
    access_exists(state, id, user, "editor").await
}
async fn require_owner(state: &AppState, id: Uuid, user: &AuthUser) -> Result<(), ApiError> {
    access_exists(state, id, user, "owner").await
}
async fn access_exists(
    state: &AppState,
    id: Uuid,
    user: &AuthUser,
    role: &str,
) -> Result<(), ApiError> {
    let allowed: bool = if role == "owner" {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profiles WHERE id = $1 AND owner_id = $2)")
            .bind(id)
            .bind(user.id)
            .fetch_one(&state.pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profiles p LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $2 WHERE p.id = $1 AND (p.owner_id = $2 OR pm.role = 'editor'))").bind(id).bind(user.id).fetch_one(&state.pool).await?
    };
    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden("profile is not writable"))
    }
}

fn profile_output(row: &sqlx::postgres::PgRow) -> Result<ProfileOutput, ApiError> {
    Ok(ProfileOutput {
        id: row.try_get("id").map_err(ApiError::internal)?,
        owner_id: row.try_get("owner_id").map_err(ApiError::internal)?,
        revision: row.try_get("revision").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        document: row.try_get("document").map_err(ApiError::internal)?,
        role: row.try_get("role").map_err(ApiError::internal)?,
        updated_at: row.try_get("updated_at").map_err(ApiError::internal)?,
    })
}
fn share_output(row: &sqlx::postgres::PgRow, url: Option<String>) -> Result<ShareOutput, ApiError> {
    Ok(ShareOutput {
        id: row.try_get("id").map_err(ApiError::internal)?,
        token_prefix: row.try_get("token_prefix").map_err(ApiError::internal)?,
        url,
        enabled: row.try_get("enabled").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
    })
}
fn member_output(row: &sqlx::postgres::PgRow) -> Result<MemberOutput, ApiError> {
    Ok(MemberOutput {
        user_id: row.try_get("user_id").map_err(ApiError::internal)?,
        email: row.try_get("email").map_err(ApiError::internal)?,
        role: row.try_get("role").map_err(ApiError::internal)?,
    })
}
fn profile_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 100 {
        Err(ApiError::bad_request(
            "profile name must be between 1 and 100 characters",
        ))
    } else {
        Ok(value.into())
    }
}
fn revision_header(headers: &HeaderMap) -> Result<i64, ApiError> {
    headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| ApiError::bad_request("If-Match profile revision is required"))
}
