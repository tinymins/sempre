use crate::{AppState, auth::CurrentUser, error::ApiError, proxy_runtime};
use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row as _;
use std::{collections::HashSet, sync::Arc};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Outbound {
    System,
    Direct,
    Proxy {
        #[serde(rename = "proxyId")]
        proxy_id: Uuid,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HostRule {
    host: String,
    outbound: Outbound,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policy {
    pub revision: i64,
    default_outbound: Outbound,
    host_rules: Vec<HostRule>,
}

pub(crate) struct ResolvedRoute {
    pub host: String,
    pub matched_host: Option<String>,
    pub outbound: Outbound,
    pub proxy_url: Option<String>,
    pub name: String,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/admin/relay-egress",
            get(get_policy).put(save_policy),
        )
        .route("/api/v1/admin/relay-egress/test", axum::routing::post(test))
}

async fn require_admin(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users WHERE id=$1 AND role IN ('admin','superadmin'))",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    if !allowed {
        return Err(ApiError::forbidden("administrator privileges required"));
    }
    Ok(())
}

pub(crate) fn validate_url(input: &str) -> Result<Url, ApiError> {
    let url = Url::parse(input).map_err(|_| ApiError::bad_request("invalid upstream URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::bad_request(
            "upstream URL must use HTTP or HTTPS without credentials or a fragment",
        ));
    }
    Ok(url)
}

fn normalize_host(input: &str) -> Result<String, ApiError> {
    let host = input.trim();
    if host.is_empty() || host.contains(['/', ':', '?', '#', '@', '*']) {
        return Err(ApiError::bad_request(
            "host rules require an exact hostname without scheme, port or wildcard",
        ));
    }
    match url::Host::parse(host).map_err(|_| ApiError::bad_request("invalid host rule"))? {
        url::Host::Domain(host) => Ok(host.to_ascii_lowercase()),
        url::Host::Ipv4(host) => Ok(host.to_string()),
        url::Host::Ipv6(_) => Err(ApiError::bad_request(
            "host rules require a hostname or IPv4 address",
        )),
    }
}

impl Policy {
    pub(crate) async fn load(state: &AppState) -> Result<Self, ApiError> {
        let row = sqlx::query(
            "SELECT revision,default_outbound,host_rules FROM relay_egress_policy WHERE singleton",
        )
        .fetch_one(&state.pool)
        .await?;
        Ok(Self {
            revision: row.try_get("revision")?,
            default_outbound: serde_json::from_value(row.try_get("default_outbound")?)
                .map_err(ApiError::internal)?,
            host_rules: serde_json::from_value(row.try_get("host_rules")?)
                .map_err(ApiError::internal)?,
        })
    }

    pub(crate) async fn cache_identity(&self, state: &AppState) -> Result<String, ApiError> {
        let ids = std::iter::once(&self.default_outbound)
            .chain(self.host_rules.iter().map(|rule| &rule.outbound))
            .filter_map(|outbound| match outbound {
                Outbound::Proxy { proxy_id } => Some(*proxy_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let rows = sqlx::query("SELECT id,user_id,revision,active_revision,enabled FROM subscription_proxies WHERE id=ANY($1) ORDER BY id")
            .bind(ids).fetch_all(&state.pool).await?;
        let mut identity = format!("relay:{}", self.revision);
        for row in rows {
            use std::fmt::Write as _;
            write!(
                identity,
                ":{}:{}:{}:{:?}:{}",
                row.try_get::<Uuid, _>("id")?,
                row.try_get::<Uuid, _>("user_id")?,
                row.try_get::<i64, _>("revision")?,
                row.try_get::<Option<i64>, _>("active_revision")?,
                row.try_get::<bool, _>("enabled")?
            )
            .map_err(ApiError::internal)?;
        }
        Ok(identity)
    }

    fn select(&self, url: &Url) -> Result<ResolvedRoute, ApiError> {
        let host = url
            .host_str()
            .ok_or_else(|| ApiError::bad_request("upstream URL requires a host"))?
            .to_ascii_lowercase();
        let matched = self.host_rules.iter().find(|rule| rule.host == host);
        let outbound = matched
            .map_or(&self.default_outbound, |rule| &rule.outbound)
            .clone();
        let name = match &outbound {
            Outbound::System => "system".to_owned(),
            Outbound::Direct => "direct".to_owned(),
            Outbound::Proxy { proxy_id } => proxy_id.to_string(),
        };
        Ok(ResolvedRoute {
            host,
            matched_host: matched.map(|rule| rule.host.clone()),
            outbound,
            proxy_url: None,
            name,
        })
    }

    pub(crate) async fn resolve(
        &self,
        state: &AppState,
        url: &Url,
    ) -> Result<ResolvedRoute, ApiError> {
        let mut selected = self.select(url)?;
        if let Outbound::Proxy { proxy_id } = selected.outbound {
            let owner: Uuid = sqlx::query_scalar("SELECT p.user_id FROM subscription_proxies p JOIN users u ON u.id=p.user_id WHERE p.id=$1 AND u.role IN ('admin','superadmin')")
                .bind(proxy_id).fetch_optional(&state.pool).await?.ok_or_else(|| ApiError::unavailable("relay egress proxy is unavailable or its owner is not an administrator"))?;
            let route = proxy_runtime::resolve(state, owner, Some(proxy_id)).await?;
            selected.proxy_url = route.url;
            selected.name = route.name;
        }
        Ok(selected)
    }
}

async fn output(state: &AppState, policy: Policy) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query("SELECT p.id,p.name,p.enabled,p.status FROM subscription_proxies p JOIN users u ON u.id=p.user_id WHERE u.role IN ('admin','superadmin') ORDER BY p.created_at")
        .fetch_all(&state.pool).await?;
    let proxies = rows.iter().map(|row| Ok(json!({"id":row.try_get::<Uuid,_>("id")?,"name":row.try_get::<String,_>("name")?,"enabled":row.try_get::<bool,_>("enabled")?,"status":row.try_get::<String,_>("status")?}))).collect::<Result<Vec<_>,ApiError>>()?;
    let mut value = serde_json::to_value(policy).map_err(ApiError::internal)?;
    value["availableProxies"] = json!(proxies);
    Ok(Json(value))
}

async fn get_policy(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, user.id).await?;
    output(&state, Policy::load(&state).await?).await
}

async fn save_policy(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(mut policy): Json<Policy>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, user.id).await?;
    if policy.host_rules.len() > 256 {
        return Err(ApiError::bad_request("maximum 256 relay egress host rules"));
    }
    let mut hosts = HashSet::new();
    for rule in &mut policy.host_rules {
        rule.host = normalize_host(&rule.host)?;
        if !hosts.insert(rule.host.clone()) {
            return Err(ApiError::bad_request("duplicate relay egress host rule"));
        }
    }
    let mut tx = state.pool.begin().await?;
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM relay_egress_policy WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
    if revision != policy.revision {
        return Err(ApiError::conflict(
            "relay egress policy changed; reload before saving",
        ));
    }
    let references = std::iter::once(("*", &policy.default_outbound))
        .chain(
            policy
                .host_rules
                .iter()
                .map(|rule| (rule.host.as_str(), &rule.outbound)),
        )
        .filter_map(|(host, outbound)| match outbound {
            Outbound::Proxy { proxy_id } => Some((host, *proxy_id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Lock owners before proxy rows, matching account deletion's lock order.
    let ids = references.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    sqlx::query("SELECT u.id FROM users u WHERE u.id IN (SELECT user_id FROM subscription_proxies WHERE id=ANY($1)) ORDER BY u.id FOR SHARE")
        .bind(&ids).fetch_all(&mut *tx).await?;
    for (_, id) in &references {
        let valid = sqlx::query("SELECT p.id FROM subscription_proxies p JOIN users u ON u.id=p.user_id WHERE p.id=$1 AND p.enabled AND u.role IN ('admin','superadmin') FOR KEY SHARE OF p")
            .bind(id).fetch_optional(&mut *tx).await?.is_some();
        if !valid {
            return Err(ApiError::bad_request(
                "relay egress requires an enabled proxy owned by an administrator",
            ));
        }
    }
    sqlx::query("DELETE FROM relay_egress_proxy_references")
        .execute(&mut *tx)
        .await?;
    for (host, id) in references {
        sqlx::query(
            "INSERT INTO relay_egress_proxy_references (reference_key,proxy_id) VALUES ($1,$2)",
        )
        .bind(host)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    policy.revision += 1;
    sqlx::query("UPDATE relay_egress_policy SET revision=$1,default_outbound=$2,host_rules=$3 WHERE singleton")
        .bind(policy.revision).bind(json!(policy.default_outbound)).bind(json!(policy.host_rules)).execute(&mut *tx).await?;
    tx.commit().await?;
    output(&state, policy).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestInput {
    url: String,
}

async fn test(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<TestInput>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, user.id).await?;
    let url = validate_url(&input.url)?;
    let policy = Policy::load(&state).await?;
    let last_url = std::sync::Mutex::new(None);
    let started = std::time::Instant::now();
    let result =
        crate::resource_relay::fetch_with_policy_observed(&state, url.as_str(), &policy, &last_url)
            .await;
    let (route, status, bytes, error) = match result {
        Ok(result) => (
            result.route,
            Some(result.response.status),
            result.response.content.len(),
            None,
        ),
        Err(error) => {
            let attempted = last_url
                .into_inner()
                .map_err(ApiError::internal)?
                .unwrap_or(url);
            let mut selected = policy.select(&attempted)?;
            if let Outbound::Proxy { proxy_id } = selected.outbound {
                selected.name =
                    sqlx::query_scalar("SELECT name FROM subscription_proxies WHERE id=$1")
                        .bind(proxy_id)
                        .fetch_optional(&state.pool)
                        .await?
                        .unwrap_or(selected.name);
            }
            (selected, None, 0, Some(error.message().to_owned()))
        }
    };
    let ok = error.is_none() && status.is_some_and(|status| (200..300).contains(&status));
    let mut result = json!({"ok":ok,"host":route.host,"matchedHost":route.matched_host,"outbound":route.outbound,"outboundName":route.name,"httpStatus":status,"elapsedMs":started.elapsed().as_millis(),"bodyBytes":bytes});
    if let Some(error) = error {
        result["error"] = json!(error);
    }
    Ok(Json(result))
}

pub(crate) async fn ensure_schema(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let ready: bool =
        sqlx::query_scalar("SELECT to_regclass('public.relay_egress_policy') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !ready {
        return Err("relay egress schema required; run sempre-server migrate".into());
    }
    Ok(())
}
