use std::{net::SocketAddr, sync::Arc};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{Request, header, request::Parts},
    middleware::Next,
    response::Response,
    routing::{delete, get},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, error::ApiError};

#[derive(Debug, Clone)]
pub(crate) struct AuthUser {
    pub id: Uuid,
}

pub(crate) fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/logout", delete(logout))
}

async fn me(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<UserOutput>, ApiError> {
    let row = sqlx::query("SELECT email FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(UserOutput {
        id: user.id,
        email: row.try_get("email").map_err(ApiError::internal)?,
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::StatusCode, ApiError> {
    let token = bearer_token(&headers)?;
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash(token))
        .execute(&state.pool)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(crate) struct Credentials {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub(crate) struct SessionOutput {
    token: String,
    expires_at: chrono::DateTime<Utc>,
    user: UserOutput,
}

#[derive(Serialize)]
struct UserOutput {
    id: Uuid,
    email: String,
}

pub(crate) async fn register(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Credentials>,
) -> Result<Json<SessionOutput>, ApiError> {
    if !state.config.allow_registration {
        return Err(ApiError::forbidden("registration is disabled"));
    }
    let (email, password_hash) = validate_and_hash(input).await?;
    let id = Uuid::new_v4();
    let result = sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&email)
        .bind(password_hash)
        .execute(&state.pool)
        .await;
    match result {
        Ok(_) => create_session(&state, id, email).await.map(Json),
        Err(error) if database_conflict(&error) => {
            Err(ApiError::conflict("email is already registered"))
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) async fn login(
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(input): Json<Credentials>,
) -> Result<Json<SessionOutput>, ApiError> {
    let email = normalize_email(&input.email)?;
    ensure_login_allowed(&state, &email, remote.ip()).await?;
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    let Some(row) = row else {
        record_login_failure(&state, &email, remote.ip()).await?;
        return Err(ApiError::unauthorized());
    };
    let id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    let hash: String = row.try_get("password_hash").map_err(ApiError::internal)?;
    let password = input.password;
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(ApiError::internal)?;
    if !valid {
        record_login_failure(&state, &email, remote.ip()).await?;
        return Err(ApiError::unauthorized());
    }
    clear_login_failures(&state, &email).await?;
    create_session(&state, id, email).await.map(Json)
}

async fn ensure_login_allowed(
    state: &AppState,
    email: &str,
    address: std::net::IpAddr,
) -> Result<(), ApiError> {
    for key in login_limit_keys(email, address) {
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM auth_limits WHERE key_hash = $1 AND blocked_until > NOW())",
        )
        .bind(key)
        .fetch_one(&state.pool)
        .await?;
        if blocked {
            return Err(ApiError::too_many_requests(
                "too many failed login attempts; try again later",
            ));
        }
    }
    Ok(())
}

async fn record_login_failure(
    state: &AppState,
    email: &str,
    address: std::net::IpAddr,
) -> Result<(), ApiError> {
    for (key, threshold) in login_limit_keys(email, address)
        .into_iter()
        .zip([5_i32, 30])
    {
        sqlx::query("INSERT INTO auth_limits (key_hash, failed_count) VALUES ($1, 1) ON CONFLICT (key_hash) DO UPDATE SET failed_count = CASE WHEN auth_limits.window_started_at < NOW() - INTERVAL '10 minutes' THEN 1 ELSE auth_limits.failed_count + 1 END, window_started_at = CASE WHEN auth_limits.window_started_at < NOW() - INTERVAL '10 minutes' THEN NOW() ELSE auth_limits.window_started_at END, blocked_until = CASE WHEN (CASE WHEN auth_limits.window_started_at < NOW() - INTERVAL '10 minutes' THEN 1 ELSE auth_limits.failed_count + 1 END) >= $2 THEN NOW() + INTERVAL '15 minutes' ELSE auth_limits.blocked_until END, updated_at = NOW()")
            .bind(key)
            .bind(threshold)
            .execute(&state.pool)
            .await?;
    }
    Ok(())
}

async fn clear_login_failures(state: &AppState, email: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM auth_limits WHERE key_hash = $1")
        .bind(token_hash(&format!("email:{email}")))
        .execute(&state.pool)
        .await?;
    Ok(())
}

fn login_limit_keys(email: &str, address: std::net::IpAddr) -> [Vec<u8>; 2] {
    [
        token_hash(&format!("email:{email}")),
        token_hash(&format!("ip:{address}")),
    ]
}

pub(crate) async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let token = bearer_token(request.headers())?;
    let row =
        sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1 AND expires_at > NOW()")
            .bind(token_hash(token))
            .fetch_optional(&state.pool)
            .await?;
    let Some(row) = row else {
        return Err(ApiError::unauthorized());
    };
    request.extensions_mut().insert(AuthUser {
        id: row.try_get("user_id").map_err(ApiError::internal)?,
    });
    Ok(next.run(request).await)
}

async fn validate_and_hash(input: Credentials) -> Result<(String, String), ApiError> {
    let email = normalize_email(&input.email)?;
    if input.password.len() < 12 || input.password.len() > 1024 {
        return Err(ApiError::bad_request(
            "password must be between 12 and 1024 characters",
        ));
    }
    let password = input.password;
    let hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|value| value.to_string())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)?;
    Ok((email, hash))
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(ApiError::bad_request("a valid email is required"));
    }
    Ok(email)
}

async fn create_session(
    state: &AppState,
    user_id: Uuid,
    email: String,
) -> Result<SessionOutput, ApiError> {
    let token = random_token();
    let expires_at = Utc::now() + Duration::days(state.config.session_days);
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token_hash(&token))
        .bind(user_id)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;
    Ok(SessionOutput {
        token,
        expires_at,
        user: UserOutput { id: user_id, email },
    })
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub(crate) fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
fn database_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(ApiError::unauthorized)
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
