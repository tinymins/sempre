use std::{collections::HashMap, sync::Arc};

use sempre_converter::{
    Profile, Source, SourceSnapshot, Target, prepare_profile, rule_provider_has_rules,
    rule_provider_snapshot_id,
};
use sempre_subscription::SubscriptionError;
use serde_json::{Map, Value, json};

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn load_rule_provider_snapshots(
        &self,
        profile: &Profile,
        target: &Target,
        force: bool,
    ) -> Result<(Vec<SourceSnapshot>, Vec<String>), ManagerError> {
        if target.core != "sing-box" {
            return Ok((Vec::new(), Vec::new()));
        }
        let effective = prepare_profile(profile, target)?;
        let allow_failures = effective
            .extra
            .get("use_system_rules")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut jobs = tokio::task::JoinSet::new();
        let concurrency = Arc::new(tokio::sync::Semaphore::new(6));
        for (index, provider) in effective.rule_providers.into_iter().enumerate() {
            let fetcher = self.fetcher.clone();
            let concurrency = Arc::clone(&concurrency);
            jobs.spawn(async move {
                let _permit = concurrency.acquire_owned().await.expect("semaphore open");
                let mut extra = Map::new();
                extra.insert("cache_ttl_minutes".into(), json!(24 * 60));
                let source = Source {
                    id: rule_provider_snapshot_id(&provider.tag),
                    kind: "url".into(),
                    enabled: true,
                    url: provider.url,
                    remark: provider.tag.clone(),
                    prefix: String::new(),
                    content: String::new(),
                    user_agent: String::new(),
                    extra,
                };
                let result = fetcher
                    .load(source, force, validate_rule_provider_content)
                    .await;
                (index, provider.tag, result)
            });
        }
        let mut loaded = HashMap::new();
        while let Some(result) = jobs.join_next().await {
            let (index, tag, result) = result.map_err(|error| {
                ManagerError::InvalidOperation(format!("load rule provider task: {error}"))
            })?;
            loaded.insert(index, (tag, result));
        }
        let mut snapshots = Vec::new();
        let mut warnings = Vec::new();
        let mut indexes = loaded.keys().copied().collect::<Vec<_>>();
        indexes.sort_unstable();
        for index in indexes {
            let (tag, result) = loaded.remove(&index).expect("provider result");
            match result {
                Ok(result) => snapshots.push(result.snapshot),
                Err(error) if allow_failures => {
                    warnings.push(format!("rule provider {tag:?}: {error}"));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok((snapshots, warnings))
    }
}

fn validate_rule_provider_content(content: &str) -> Result<(), SubscriptionError> {
    if rule_provider_has_rules(content) {
        Ok(())
    } else {
        Err(SubscriptionError::Invalid(
            "provider has no convertible rules".into(),
        ))
    }
}
