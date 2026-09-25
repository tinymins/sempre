use std::sync::Arc;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, error::ApiError};

#[derive(Clone)]
pub(crate) struct AuthUser {
    pub id: Uuid,
    pub session_id: Uuid,
}

pub(crate) fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/logout", post(logout))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserOutput {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub settings: Option<Value>,
}

#[derive(Serialize)]
struct AuthOutput {
    user: UserOutput,
}

#[derive(Deserialize)]
pub(crate) struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub(crate) struct Registration {
    name: String,
    email: String,
    password: String,
    #[serde(rename = "invitationCode")]
    invitation_code: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthConfig {
    allow_registration: bool,
    first_user: bool,
}

pub(crate) async fn public_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    let first_user: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM users)")
        .fetch_one(&state.pool)
        .await?;
    let allow_registration: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT allow_registration FROM system_settings ORDER BY created_at LIMIT 1), TRUE)",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(
        serde_json::to_value(AuthConfig {
            allow_registration: first_user || allow_registration,
            first_user,
        })
        .map_err(ApiError::internal)?,
    ))
}

async fn me(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<AuthOutput>, ApiError> {
    Ok(Json(AuthOutput {
        user: load_user(&state, user.id).await?,
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session_id = session_cookie(&headers)?;
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(session_id)
        .execute(&state.pool)
        .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("SESSION_ID=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"),
    );
    Ok(response)
}

pub(crate) async fn register(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Registration>,
) -> Result<Response, ApiError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::bad_request(
            "name must be between 1 and 100 characters",
        ));
    }
    let email = normalize_email(&input.email)?;
    let password_hash = hash_password(input.password).await?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(1397055058)")
        .execute(&mut *transaction)
        .await?;
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *transaction)
        .await?;
    if user_count > 0 {
        let allow_registration: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT allow_registration FROM system_settings ORDER BY created_at LIMIT 1), TRUE)",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if !allow_registration && input.invitation_code.as_deref().is_none_or(str::is_empty) {
            return Err(ApiError::forbidden("registration is disabled"));
        }
    }
    let id = Uuid::new_v4();
    let role = if user_count == 0 {
        "superadmin"
    } else {
        "user"
    };
    let result = sqlx::query(
        "INSERT INTO users (id, name, email, password_hash, role) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(name)
    .bind(&email)
    .bind(password_hash)
    .bind(role)
    .execute(&mut *transaction)
    .await;
    match result {
        Ok(_) => {
            if user_count > 0
                && let Some(code) = input.invitation_code.as_deref()
            {
                crate::toolbox_admin::consume_invitation(&mut transaction, code, id).await?;
            }
            transaction.commit().await?;
        }
        Err(error) if database_conflict(&error) => {
            return Err(ApiError::conflict("email is already registered"));
        }
        Err(error) => return Err(error.into()),
    }
    create_session(&state, id).await
}

pub(crate) async fn login(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Credentials>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&input.email)?;
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::invalid_credentials)?;
    let hash: String = row.try_get("password_hash").map_err(ApiError::internal)?;
    let valid = verify_password(input.password, hash).await?;
    if !valid {
        return Err(ApiError::invalid_credentials());
    }
    let id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    create_session(&state, id).await
}

pub(crate) async fn verify_password(password: String, hash: String) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).ok().is_some_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
    })
    .await
    .map_err(ApiError::internal)
}

async fn create_session(state: &AppState, user_id: Uuid) -> Result<Response, ApiError> {
    let session_id = Uuid::new_v4();
    let expiry = Utc::now() + Duration::days(7);
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(user_id)
        .bind(expiry)
        .execute(&state.pool)
        .await?;
    let mut response = Json(AuthOutput {
        user: load_user(state, user_id).await?,
    })
    .into_response();
    let secure = if state.config.public_url.scheme() == "https" {
        "; Secure"
    } else {
        ""
    };
    let value =
        format!("SESSION_ID={session_id}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800{secure}");
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&value).map_err(ApiError::internal)?,
    );
    Ok(response)
}

pub(crate) async fn load_user(state: &AppState, id: Uuid) -> Result<UserOutput, ApiError> {
    let row = sqlx::query("SELECT id, name, email, role, settings FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(UserOutput {
        id: row.try_get("id").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        email: row.try_get("email").map_err(ApiError::internal)?,
        role: row.try_get("role").map_err(ApiError::internal)?,
        settings: row.try_get("settings").map_err(ApiError::internal)?,
    })
}

pub(crate) async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let session_id = session_cookie(request.headers())?;
    let row = sqlx::query("SELECT user_id FROM sessions WHERE id = $1 AND expires_at > NOW()")
        .bind(session_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    request.extensions_mut().insert(AuthUser {
        id: row.try_get("user_id").map_err(ApiError::internal)?,
        session_id,
    });
    Ok(next.run(request).await)
}

fn session_cookie(headers: &HeaderMap) -> Result<Uuid, ApiError> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix("SESSION_ID="))
        })
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(ApiError::unauthorized)
}

pub(crate) async fn hash_password(password: String) -> Result<String, ApiError> {
    if !(12..=1024).contains(&password.len()) {
        return Err(ApiError::bad_request(
            "password must be between 12 and 1024 characters",
        ));
    }
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|value| value.to_string())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)
}

pub(crate) fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(ApiError::bad_request("a valid email is required"));
    }
    Ok(email)
}

fn database_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
}

pub(crate) struct CurrentUser(pub AuthUser);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .map(Self)
            .ok_or_else(ApiError::unauthorized)
    }
}
