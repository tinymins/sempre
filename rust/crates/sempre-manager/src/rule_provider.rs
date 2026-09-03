use std::{collections::HashMap, sync::Arc};

use sempre_converter::{Profile, SourceSnapshot, Target, prepare_profile};

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
        let mut jobs = tokio::task::JoinSet::new();
        let concurrency = Arc::new(tokio::sync::Semaphore::new(6));
        for (index, provider) in effective.rule_providers.into_iter().enumerate() {
            let fetcher = self.fetcher.clone();
            let concurrency = Arc::clone(&concurrency);
            jobs.spawn(async move {
                let _permit = concurrency.acquire_owned().await.expect("semaphore open");
                let result = fetcher.load_rule_provider(&provider, force).await;
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
        let mut indexes = loaded.keys().copied().collect::<Vec<_>>();
        indexes.sort_unstable();
        for index in indexes {
            let (tag, result) = loaded.remove(&index).expect("provider result");
            match result {
                Ok(result) => snapshots.push(result.snapshot),
                Err(error) => {
                    return Err(sempre_subscription::SubscriptionError::Fetch(format!(
                        "rule provider {tag:?}: {error}"
                    ))
                    .into());
                }
            }
        }
        Ok((snapshots, Vec::new()))
    }
}

#[cfg(test)]
pub(crate) fn write_bundled_rule_fixture(layout: &sempre_state::Layout) {
    let bundled = sempre_converter::system_defaults()
        .rule_providers
        .into_iter()
        .map(|provider| {
            (
                provider.url,
                "payload:\n  - DOMAIN-SUFFIX,offline.example\n",
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    std::fs::create_dir_all(&layout.resources).expect("resources");
    std::fs::write(
        layout.resources.join("sempre-system-rules.json"),
        serde_json::to_vec(&bundled).expect("bundled rules"),
    )
    .expect("write bundled rules");
}
