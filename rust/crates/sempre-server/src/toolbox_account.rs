use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{self, CurrentUser},
    error::ApiError,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/account/profile",
            get(profile).patch(update_profile),
        )
        .route(
            "/api/v1/account/password",
            axum::routing::post(change_password),
        )
        .route(
            "/api/v1/account/avatar",
            axum::routing::post(upload_avatar)
                .layer(DefaultBodyLimit::max(6 * 1024 * 1024))
                .delete(delete_avatar),
        )
}

pub(crate) fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/avatars/{key}", get(get_avatar))
}

async fn profile(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Value>, ApiError> {
    let current = auth::load_user(&state, user.id).await?;
    Ok(Json(json!({"user": current})))
}

#[derive(Deserialize)]
struct ProfilePatch {
    name: Option<String>,
    email: Option<String>,
    settings: Option<Value>,
}

async fn update_profile(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ProfilePatch>,
) -> Result<Json<Value>, ApiError> {
    let name = input.name.as_deref().map(str::trim);
    if name.is_some_and(|value| value.is_empty() || value.chars().count() > 100) {
        return Err(ApiError::bad_request(
            "name must be between 1 and 100 characters",
        ));
    }
    let email = input
        .email
        .as_deref()
        .map(auth::normalize_email)
        .transpose()?;
    let settings = input.settings.map(validate_settings).transpose()?;
    let mut transaction = state.pool.begin().await?;
    let result = sqlx::query("UPDATE users SET name=COALESCE($2,name), email=COALESCE($3,email), settings=COALESCE(settings,'{}'::jsonb) || COALESCE($4,'{}'::jsonb) WHERE id=$1")
        .bind(user.id).bind(name).bind(email).bind(settings)
        .execute(&mut *transaction).await;
    match result {
        Ok(_) => transaction.commit().await?,
        Err(error)
            if error
                .as_database_error()
                .is_some_and(|db| db.code().as_deref() == Some("23505")) =>
        {
            return Err(ApiError::conflict("email is already registered"));
        }
        Err(error) => return Err(error.into()),
    }
    let current = auth::load_user(&state, user.id).await?;
    Ok(Json(json!({"user": current})))
}

fn validate_settings(value: Value) -> Result<Value, ApiError> {
    let object = value
        .as_object()
        .ok_or_else(|| ApiError::bad_request("settings must be an object"))?;
    for (key, choices) in [
        (
            "langMode",
            &["auto", "zh-CN", "en-US", "de-DE", "ja-JP", "zh-TW"][..],
        ),
        ("themeMode", &["auto", "light", "dark"][..]),
        (
            "accentColor",
            &["emerald", "amber", "rose", "violet", "blue", "cyan"][..],
        ),
    ] {
        if let Some(setting) = object.get(key) {
            let text = setting
                .as_str()
                .ok_or_else(|| ApiError::bad_request(format!("{key} must be a string")))?;
            if !choices.contains(&text) {
                return Err(ApiError::bad_request(format!("unsupported {key}")));
            }
        }
    }
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "langMode" | "themeMode" | "accentColor"))
    {
        return Err(ApiError::bad_request(
            "avatarKey is managed by avatar upload; unsupported settings key",
        ));
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordInput {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<PasswordInput>,
) -> Result<StatusCode, ApiError> {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    if !auth::verify_password(input.current_password, hash.clone()).await? {
        return Err(ApiError::bad_request("current password is incorrect"));
    }
    let next_hash = auth::hash_password(input.new_password).await?;
    let mut transaction = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1 AND password_hash=$3")
        .bind(user.id)
        .bind(next_hash)
        .bind(hash)
        .execute(&mut *transaction)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::conflict(
            "password changed; reload before trying again",
        ));
    }
    sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND id<>$2")
        .bind(user.id)
        .bind(user.session_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let field = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?
        .ok_or_else(|| ApiError::bad_request("file is required"))?;
    if field.name() != Some("file") {
        return Err(ApiError::bad_request("multipart field must be named file"));
    }
    let content_type = field.content_type().unwrap_or("").to_owned();
    if !matches!(
        content_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err(ApiError::bad_request(
            "avatar must be PNG, JPEG, WebP, or GIF",
        ));
    }
    let bytes = field
        .bytes()
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 {
        return Err(ApiError::bad_request("avatar must be 1 byte to 5 MB"));
    }
    if !image_matches(&content_type, &bytes) {
        return Err(ApiError::bad_request(
            "avatar data does not match its image type",
        ));
    }
    let key = Uuid::new_v4();
    let mut transaction = state.pool.begin().await?;
    let prior: Option<String> =
        sqlx::query_scalar("SELECT settings->>'avatarKey' FROM users WHERE id=$1 FOR UPDATE")
            .bind(user.id)
            .fetch_one(&mut *transaction)
            .await?;
    sqlx::query("INSERT INTO user_avatars (id,owner_id,content_type,data) VALUES ($1,$2,$3,$4)")
        .bind(key)
        .bind(user.id)
        .bind(&content_type)
        .bind(bytes.as_ref())
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE users SET settings=jsonb_set(COALESCE(settings,'{}'::jsonb),'{avatarKey}',to_jsonb($2::text),true) WHERE id=$1")
        .bind(user.id).bind(key.to_string()).execute(&mut *transaction).await?;
    if let Some(prior) = prior.and_then(|value| Uuid::parse_str(&value).ok()) {
        sqlx::query("DELETE FROM user_avatars WHERE id=$1 AND owner_id=$2")
            .bind(prior)
            .bind(user.id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(Json(
        json!({"avatarKey": key.to_string(), "avatarUrl": format!("/api/v1/avatars/{key}")}),
    ))
}

async fn delete_avatar(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let prior: Option<String> =
        sqlx::query_scalar("SELECT settings->>'avatarKey' FROM users WHERE id=$1 FOR UPDATE")
            .bind(user.id)
            .fetch_one(&mut *transaction)
            .await?;
    sqlx::query(
        "UPDATE users SET settings=COALESCE(settings,'{}'::jsonb) - 'avatarKey' WHERE id=$1",
    )
    .bind(user.id)
    .execute(&mut *transaction)
    .await?;
    if let Some(prior) = prior.and_then(|value| Uuid::parse_str(&value).ok()) {
        sqlx::query("DELETE FROM user_avatars WHERE id=$1 AND owner_id=$2")
            .bind(prior)
            .bind(user.id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_avatar(
    State(state): State<Arc<AppState>>,
    Path(key): Path<Uuid>,
) -> Result<Response, ApiError> {
    let row = sqlx::query("SELECT content_type,data FROM user_avatars WHERE id=$1")
        .bind(key)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("avatar"))?;
    let content_type: String = row.try_get("content_type").map_err(ApiError::internal)?;
    let data: Vec<u8> = row.try_get("data").map_err(ApiError::internal)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type).map_err(ApiError::internal)?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok((headers, data).into_response())
}

fn image_matches(content_type: &str, bytes: &[u8]) -> bool {
    match content_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}
