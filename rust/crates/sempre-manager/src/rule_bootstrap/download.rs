use std::{collections::HashMap, sync::Arc, time::Duration};

use sempre_state::DesiredState;
use sempre_subscription::Fetcher;

use super::{RemoteRule, config_error, normalize_rule, proxy_fetcher};
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner, supervisor::RuntimePlan};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(crate) async fn complete_rule_bootstrap(&self, plan: &RuntimePlan) {
        if plan.rules.resources.is_empty() {
            std::future::pending::<()>().await;
        }
        loop {
            let result = self.refresh_runtime_rules(plan).await;
            let failed = result.is_err();
            match result {
                Ok(true) if plan.rules.pending_count() > 0 => {
                    self.log_supervisor(
                        "online rules downloaded; switching from basic to complete configuration",
                    );
                    return;
                }
                Ok(true) => {
                    self.log_supervisor("online rule snapshots refreshed");
                }
                Ok(false) => {}
                Err(error) => {
                    self.log_supervisor(&format!(
                        "online rule update failed; keeping running configuration: {error}"
                    ));
                }
            }
            let delay = if failed {
                Duration::from_mins(1)
            } else {
                plan.rules
                    .resources
                    .iter()
                    .map(|rule| refresh_after(&self.fetcher, rule))
                    .min()
                    .unwrap_or(Duration::from_hours(24))
                    .max(Duration::from_secs(1))
            };
            tokio::time::sleep(delay).await;
        }
    }

    async fn refresh_runtime_rules(&self, plan: &RuntimePlan) -> Result<bool, ManagerError> {
        let fetcher = proxy_fetcher(&self.fetcher, &plan.rules.original)?;
        let mut jobs = tokio::task::JoinSet::new();
        let limit = Arc::new(tokio::sync::Semaphore::new(6));
        for resource in &plan.rules.resources {
            if !refresh_due(&fetcher, resource) {
                continue;
            }
            let resource = resource.clone();
            let fetcher = fetcher.clone();
            let limit = Arc::clone(&limit);
            jobs.spawn(async move {
                let _permit = limit
                    .acquire_owned()
                    .await
                    .expect("rule download semaphore");
                let snapshot = fetcher.fetch_rule_set(&resource.url).await?;
                let snapshot = normalize_rule(&fetcher, &resource, snapshot)?;
                Ok::<_, ManagerError>((resource.tag, snapshot))
            });
        }
        let mut candidates = HashMap::new();
        while let Some(result) = jobs.join_next().await {
            let (tag, snapshot) = result.map_err(config_error)??;
            candidates.insert(tag, snapshot);
        }
        if candidates.is_empty() {
            return Ok(false);
        }
        let _gate = self.runtime_rules_gate.lock().await;
        if let Some(candidate) = candidates.get("geoip-cn")
            && let Some(intent) = &plan.transparent.cn_routes
        {
            let binary = self.store.layout().core_binary(
                &plan.deployment.core,
                plan.deployment.repository.as_deref(),
                &plan.deployment.version,
            );
            let source = crate::cn_routes::Source {
                path: candidate.path.clone(),
                binary: plan.cn_source.as_ref().is_some_and(|s| s.binary),
            };
            let mut union = intent.clone();
            // Keep both snapshots bypassed until all cache publication completes.
            union
                .prefixes
                .extend(crate::cn_routes::read_prefixes(&binary, &source).await?);
            if let Some(previous) = &plan.cn_source {
                union
                    .prefixes
                    .extend(crate::cn_routes::read_prefixes(&binary, previous).await?);
            }
            self.transparent.apply_cn_routes(&union).await?;
        }
        // No await between the deployment check and publication: cancellation cannot
        // publish a partially downloaded candidate.
        let document = self.store.read()?;
        if document.desired_state != DesiredState::Running
            || document.active.as_ref() != Some(&plan.deployment)
        {
            return Err(ManagerError::RuntimeNotReady(
                "deployment changed during rule download".into(),
            ));
        }
        for resource in &plan.rules.resources {
            if let Some(snapshot) = candidates.get(&resource.tag) {
                self.fetcher
                    .accept_rule_set(&resource.url, &resource.format, snapshot)?;
            }
        }
        Ok(true)
    }
}

pub(super) fn refresh_due(fetcher: &Fetcher, resource: &RemoteRule) -> bool {
    refresh_after(fetcher, resource).is_zero()
}

fn refresh_after(fetcher: &Fetcher, resource: &RemoteRule) -> Duration {
    fetcher
        .cached_rule_set(&resource.url, &resource.format)
        .ok()
        .flatten()
        .map_or(Duration::ZERO, |snapshot| {
            let elapsed = (chrono::Utc::now() - snapshot.fetched_at)
                .to_std()
                .unwrap_or_default();
            resource.interval.saturating_sub(elapsed)
        })
}
