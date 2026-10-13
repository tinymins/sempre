use crate::{AppState, error::ApiError, subscription_proxies};
use sempre_proxy::Engine;
use serde_json::{Value, json};
use sqlx::{Postgres, Row as _, Transaction};
use uuid::Uuid;

pub(crate) async fn content(
    tx: &mut Transaction<'_, Postgres>,
    node: Uuid,
    owner: Uuid,
) -> Result<String, ApiError> {
    let content: String = sqlx::query_scalar("SELECT content FROM proxy_custom_nodes WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text)) FOR SHARE")
        .bind(node).bind(owner).fetch_optional(&mut **tx).await?
        .ok_or_else(|| ApiError::not_found("custom node"))?;
    document(&content)
}

fn document(content: &str) -> Result<String, ApiError> {
    let node = sempre_converter::parse_jsonc_value(content)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(json!({"proxies": [node]}).to_string())
}

// The caller holds the node lock before locking its proxy references.
pub(crate) async fn sync(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    node: Uuid,
    owner: Uuid,
    content: Option<&str>,
    authorized: Option<&[Uuid]>,
) -> Result<Vec<Uuid>, ApiError> {
    let rows = sqlx::query("SELECT id,user_id,engine FROM subscription_proxies WHERE custom_node_id=$1 ORDER BY id FOR UPDATE")
        .bind(node).fetch_all(&mut **tx).await?;
    let mut changed = Vec::new();
    for row in rows {
        let user: Uuid = row.try_get("user_id")?;
        if authorized.is_some_and(|ids| user != owner && !ids.contains(&user)) {
            return Err(ApiError::conflict(
                "custom node is bound to a proxy owned by a removed authorized user; unbind the proxy first",
            ));
        }
        if let Some(content) = content {
            let id: Uuid = row.try_get("id")?;
            let engine: Engine = serde_json::from_value(Value::String(row.try_get("engine")?))
                .map_err(ApiError::internal)?;
            let (cipher, engine, protocol) =
                subscription_proxies::encode(state, id, &document(content)?, Some(engine))?;
            sqlx::query("UPDATE subscription_proxies SET configuration=$2,engine=$3,protocol=$4,revision=revision+1,status=CASE WHEN enabled THEN 'starting' ELSE 'stopped' END,last_error=NULL,updated_at=NOW() WHERE id=$1")
                .bind(id).bind(cipher).bind(engine).bind(protocol).execute(&mut **tx).await?;
            changed.push(id);
        }
    }
    Ok(changed)
}
