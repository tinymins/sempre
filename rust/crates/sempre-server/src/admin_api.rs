use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row as _, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{self, CurrentUser},
    error::ApiError,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUser {
    id: Uuid,
    name: String,
    email: String,
    role: String,
    last_login_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

fn user_output(row: &PgRow) -> Result<AdminUser, ApiError> {
    Ok(AdminUser {
        id: row.try_get("id").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        email: row.try_get("email").map_err(ApiError::internal)?,
        role: row.try_get("role").map_err(ApiError::internal)?,
        last_login_at: row.try_get("last_login_at").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Invitation {
    id: Uuid,
    code: String,
    created_by: Uuid,
    used_by: Option<Uuid>,
    used_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

fn invitation_output(row: &PgRow) -> Result<Invitation, ApiError> {
    Ok(Invitation {
        id: row.try_get("id").map_err(ApiError::internal)?,
        code: row.try_get("code").map_err(ApiError::internal)?,
        created_by: row.try_get("created_by").map_err(ApiError::internal)?,
        used_by: row.try_get("used_by").map_err(ApiError::internal)?,
        used_at: row.try_get("used_at").map_err(ApiError::internal)?,
        expires_at: row.try_get("expires_at").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
    })
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/admin/settings",
            get(settings).patch(update_settings),
        )
        .route("/api/v1/admin/users", get(list_users).post(create_user))
        .route(
            "/api/v1/admin/users/{id}",
            axum::routing::patch(change_role).delete(delete_user),
        )
        .route(
            "/api/v1/admin/users/{id}/password",
            axum::routing::post(reset_password),
        )
        .route(
            "/api/v1/admin/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route(
            "/api/v1/admin/invitations/{id}",
            axum::routing::delete(delete_invitation),
        )
}

async fn require_role(
    state: &AppState,
    user_id: Uuid,
    superadmin: bool,
) -> Result<String, ApiError> {
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if superadmin && role != "superadmin" {
        return Err(ApiError::forbidden("superadmin privileges required"));
    }
    if !superadmin && role != "superadmin" && role != "admin" {
        return Err(ApiError::forbidden("administrator privileges required"));
    }
    Ok(role)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    allow_registration: bool,
}

async fn current_settings(state: &AppState) -> Result<Settings, ApiError> {
    let row = sqlx::query("SELECT COALESCE((SELECT allow_registration FROM system_settings ORDER BY created_at LIMIT 1),TRUE) AS allow_registration")
        .fetch_one(&state.pool).await?;
    Ok(Settings {
        allow_registration: row
            .try_get("allow_registration")
            .map_err(ApiError::internal)?,
    })
}

async fn settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Settings>, ApiError> {
    require_role(&state, user.id, false).await?;
    Ok(Json(current_settings(&state).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPatch {
    allow_registration: Option<bool>,
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<SettingsPatch>,
) -> Result<Json<Settings>, ApiError> {
    require_role(&state, user.id, false).await?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("INSERT INTO system_settings DEFAULT VALUES ON CONFLICT DO NOTHING")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE system_settings SET allow_registration=COALESCE($1,allow_registration), updated_at=NOW()")
        .bind(input.allow_registration)
        .execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(current_settings(&state).await?))
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<AdminUser>>, ApiError> {
    require_role(&state, user.id, false).await?;
    let rows = sqlx::query(
        "SELECT id,name,email,role,last_login_at,created_at FROM users ORDER BY created_at,id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.iter()
            .map(user_output)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn role(value: &str) -> Result<&str, ApiError> {
    if matches!(value, "superadmin" | "admin" | "user") {
        Ok(value)
    } else {
        Err(ApiError::bad_request(
            "role must be superadmin, admin, or user",
        ))
    }
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
    password: String,
    role: Option<String>,
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateUser>,
) -> Result<(StatusCode, Json<AdminUser>), ApiError> {
    require_role(&state, user.id, true).await?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::bad_request("name must be 1–100 characters"));
    }
    let email = auth::normalize_email(&input.email)?;
    let role = role(input.role.as_deref().unwrap_or("user"))?;
    let hash = auth::hash_password(input.password).await?;
    let id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await?;
    let row = sqlx::query("INSERT INTO users (id,name,email,password_hash,role) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (email) DO NOTHING RETURNING id,name,email,role,last_login_at,created_at")
        .bind(id).bind(name).bind(email).bind(hash).bind(role)
        .fetch_optional(&mut *transaction).await?
        .ok_or_else(|| ApiError::conflict("email is already registered"))?;
    let output = user_output(&row)?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(output)))
}

#[derive(Deserialize)]
struct RolePatch {
    role: String,
}

async fn change_role(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RolePatch>,
) -> Result<Json<AdminUser>, ApiError> {
    require_role(&state, user.id, true).await?;
    if id == user.id {
        return Err(ApiError::bad_request("cannot change your own role"));
    }
    let role = role(&input.role)?;
    let row = sqlx::query("UPDATE users SET role=$2 WHERE id=$1 AND role<>'superadmin' RETURNING id,name,email,role,last_login_at,created_at")
        .bind(id).bind(role).fetch_optional(&state.pool).await?
        .ok_or_else(|| ApiError::forbidden("user not found or superadmin role is protected"))?;
    Ok(Json(user_output(&row)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetPassword {
    new_password: String,
}

async fn reset_password(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ResetPassword>,
) -> Result<StatusCode, ApiError> {
    require_role(&state, user.id, true).await?;
    if id == user.id {
        return Err(ApiError::bad_request(
            "change your own password in account settings",
        ));
    }
    let hash = auth::hash_password(input.new_password).await?;
    let mut transaction = state.pool.begin().await?;
    let result =
        sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1 AND role<>'superadmin'")
            .bind(id)
            .bind(hash)
            .execute(&mut *transaction)
            .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("user"));
    }
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_role(&state, user.id, true).await?;
    if id == user.id {
        return Err(ApiError::bad_request("cannot delete your own account"));
    }
    let mut transaction = state.pool.begin().await?;
    let target: Option<String> =
        sqlx::query_scalar("SELECT role FROM users WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    match target.as_deref() {
        None => return Err(ApiError::not_found("user")),
        Some("superadmin") => {
            return Err(ApiError::forbidden("superadmin accounts cannot be deleted"));
        }
        _ => {}
    }
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM proxy_access_logs WHERE subscribe_id IN (SELECT id FROM proxy_subscribes WHERE user_id=$1)")
        .bind(id).execute(&mut *transaction).await?;
    sqlx::query("DELETE FROM proxy_subscribes WHERE user_id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM proxy_custom_nodes WHERE user_id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM invitation_codes WHERE created_by=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_invitations(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Invitation>>, ApiError> {
    require_role(&state, user.id, true).await?;
    let rows = sqlx::query("SELECT * FROM invitation_codes ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(
        rows.iter()
            .map(invitation_output)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvitationInput {
    expires_in_hours: Option<f64>,
}

async fn create_invitation(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<InvitationInput>,
) -> Result<(StatusCode, Json<Invitation>), ApiError> {
    require_role(&state, user.id, true).await?;
    let expires_at = if let Some(hours) = input.expires_in_hours {
        if !hours.is_finite() || !(0.0..=8760.0).contains(&hours) || hours == 0.0 {
            return Err(ApiError::bad_request(
                "expiresInHours must be positive and at most 8760",
            ));
        }
        let duration = std::time::Duration::try_from_secs_f64(hours * 3600.0)
            .map_err(|_| ApiError::bad_request("expiresInHours is invalid"))?;
        Some(Utc::now() + Duration::from_std(duration).map_err(ApiError::internal)?)
    } else {
        None
    };
    let code = Uuid::new_v4().to_string();
    let row = sqlx::query(
        "INSERT INTO invitation_codes (code,created_by,expires_at) VALUES ($1,$2,$3) RETURNING *",
    )
    .bind(code)
    .bind(user.id)
    .bind(expires_at)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(invitation_output(&row)?)))
}

async fn delete_invitation(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_role(&state, user.id, true).await?;
    let result = sqlx::query("DELETE FROM invitation_codes WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("invitation"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn consume_invitation(
    transaction: &mut Transaction<'_, Postgres>,
    code: &str,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let result = sqlx::query("UPDATE invitation_codes SET used_by=$2,used_at=NOW() WHERE code=$1 AND used_by IS NULL AND used_at IS NULL AND (expires_at IS NULL OR expires_at>NOW())")
        .bind(code).bind(user_id).execute(&mut **transaction).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::bad_request(
            "invitation is invalid, expired, or already used",
        ));
    }
    Ok(())
}
