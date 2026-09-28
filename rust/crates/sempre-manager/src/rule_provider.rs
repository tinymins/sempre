use std::{collections::HashMap, sync::Arc};

use sempre_converter::{
    Profile, RuleProvider, Source, SourceSnapshot, Target, prepare_profile,
    rule_provider_has_rules, rule_provider_snapshot_id,
};
use sempre_subscription::{FetchResult, Fetcher, SubscriptionError};
use serde_json::{Map, json};

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Copy)]
pub(crate) enum RuleSnapshotPolicy {
    RuntimeBootstrap,
    RequireSnapshot,
}

impl RuleSnapshotPolicy {
    pub(crate) fn runtime(refresh: bool) -> Self {
        if refresh {
            Self::RequireSnapshot
        } else {
            Self::RuntimeBootstrap
        }
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn load_runtime_rule_provider_snapshots(
        &self,
        profile: &Profile,
        target: &Target,
        refresh: bool,
    ) -> Result<(Vec<SourceSnapshot>, Vec<String>), ManagerError> {
        self.load_rule_provider_snapshots(
            profile,
            target,
            refresh,
            true,
            RuleSnapshotPolicy::runtime(refresh),
        )
        .await
    }

    pub(crate) async fn load_rule_provider_snapshots(
        &self,
        profile: &Profile,
        target: &Target,
        force: bool,
        persist: bool,
        policy: RuleSnapshotPolicy,
    ) -> Result<(Vec<SourceSnapshot>, Vec<String>), ManagerError> {
        if target.core != "sing-box" || matches!(policy, RuleSnapshotPolicy::RuntimeBootstrap) {
            return Ok((Vec::new(), Vec::new()));
        }
        let effective = prepare_profile(profile, target)?;
        let document = self.store.read()?;
        let fetcher = if force && document.runtime.state == sempre_state::RuntimeState::Running {
            if let Some(config) = document.runtime.runtime_config.as_ref() {
                crate::rule_bootstrap::proxy_fetcher(
                    &self.fetcher,
                    &crate::rule_bootstrap::read_config(std::path::Path::new(config))?,
                )?
            } else {
                self.fetcher.clone()
            }
        } else {
            self.fetcher.clone()
        };
        let mut jobs = tokio::task::JoinSet::new();
        let concurrency = Arc::new(tokio::sync::Semaphore::new(6));
        for (index, provider) in effective.rule_providers.into_iter().enumerate() {
            let fetcher = fetcher.clone();
            let concurrency = Arc::clone(&concurrency);
            jobs.spawn(async move {
                let _permit = concurrency.acquire_owned().await.expect("semaphore open");
                let (tag, result) = inspect_rule_provider(fetcher, provider, force, persist).await;
                (index, tag, result)
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
            let Some(result) = result else { continue };
            match result {
                Ok(result) => {
                    if result.from_cache
                        && result.source.extra.get("last_status")
                            == Some(&serde_json::Value::String("last-known-good cache".into()))
                    {
                        warnings.push(format!(
                            "rule provider {tag:?} used its last-known-good snapshot"
                        ));
                    }
                    if !is_native_rule_set(&result.snapshot.content)
                        && rule_provider_has_rules(&result.snapshot.content)
                    {
                        snapshots.push(result.snapshot);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok((snapshots, warnings))
    }
}

async fn inspect_rule_provider(
    fetcher: Fetcher,
    provider: RuleProvider,
    force: bool,
    persist: bool,
) -> (String, Option<Result<FetchResult, SubscriptionError>>) {
    let tag = provider.tag.clone();
    if !needs_rule_inspection(&provider) {
        return (tag, None);
    }
    let mut extra = Map::new();
    extra.insert("cache_ttl_minutes".into(), json!(24 * 60));
    let source = Source {
        id: rule_provider_snapshot_id(&provider.tag),
        kind: "url".into(),
        enabled: true,
        url: provider.url,
        remark: provider.tag,
        prefix: String::new(),
        content: String::new(),
        user_agent: String::new(),
        extra,
    };
    let result = if force {
        if persist {
            fetcher
                .load(source, true, validate_rule_provider_content)
                .await
        } else {
            fetcher
                .inspect(source, true, validate_rule_provider_content)
                .await
        }
    } else {
        match fetcher.cached_rule_provider(source.clone()) {
            Ok(result) => Ok(result),
            Err(_) if persist => {
                fetcher
                    .load(source, false, validate_rule_provider_content)
                    .await
            }
            Err(_) => {
                fetcher
                    .inspect(source, false, validate_rule_provider_content)
                    .await
            }
        }
    };
    (tag, Some(result))
}

fn needs_rule_inspection(provider: &RuleProvider) -> bool {
    if provider.format == "binary" {
        return false;
    }
    let path = url::Url::parse(&provider.url)
        .ok()
        .map(|url| url.path().to_owned())
        .unwrap_or_default();
    let native_extension = std::path::Path::new(&path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json") || ext.eq_ignore_ascii_case("srs"));
    if native_extension {
        return false;
    }
    provider.format.is_empty() || provider.format == "source"
}

fn validate_rule_provider_content(content: &str) -> Result<(), SubscriptionError> {
    if is_native_rule_set(content) || rule_provider_has_rules(content) {
        Ok(())
    } else {
        Err(SubscriptionError::Invalid(
            "provider has no usable rules".into(),
        ))
    }
}

fn is_native_rule_set(content: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .is_some_and(|value| value["rules"].is_array())
}
