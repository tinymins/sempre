use std::time::Duration;

use chrono::{DateTime, Utc};
use sempre_converter::Profile;
use sempre_subscription::SubscriptionError;
use serde_json::json;
use tokio::{sync::watch, time::sleep};

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

const MINIMUM_DELAY: Duration = Duration::from_secs(1);
const ERROR_RETRY: Duration = Duration::from_mins(1);

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub fn update_subscription_settings(
        &self,
        interval: Option<&str>,
        auto_restart: Option<bool>,
    ) -> Result<Vec<CoreChange>, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let interval = interval.map(normalize_interval).transpose()?;
        let before = self.store.read()?;
        self.store.update(|document| {
            if let Some(interval) = &interval {
                document.subscription.interval.clone_from(interval);
            }
            if let Some(auto_restart) = auto_restart {
                document.subscription_auto_restart = auto_restart;
            }
            Ok(())
        })?;
        let after = self.store.read()?;
        let mut changes = Vec::new();
        if before.subscription.interval != after.subscription.interval {
            changes.push(CoreChange {
                changed: true,
                message: format!(
                    "subscription schedule set to {}",
                    after.subscription.interval
                ),
                ..CoreChange::default()
            });
            self.notify_subscription_schedule_changed();
        } else if interval.is_some() {
            changes.push(CoreChange {
                message: format!(
                    "subscription schedule is already {}",
                    after.subscription.interval
                ),
                ..CoreChange::default()
            });
        }
        if before.subscription_auto_restart != after.subscription_auto_restart {
            changes.push(CoreChange {
                changed: true,
                message: format!(
                    "subscription automatic restart set to {}",
                    after.subscription_auto_restart
                ),
                ..CoreChange::default()
            });
        } else if auto_restart.is_some() {
            changes.push(CoreChange {
                message: format!(
                    "subscription automatic restart is already {}",
                    after.subscription_auto_restart
                ),
                ..CoreChange::default()
            });
        }
        Ok(changes)
    }

    pub async fn run_subscription_scheduler(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ManagerError> {
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            let delay = match self.next_subscription_update() {
                Ok(Some(delay)) => delay,
                Ok(None) => {
                    if matches!(
                        wait_for_update(self, &mut shutdown, ERROR_RETRY).await,
                        SchedulerEvent::Shutdown
                    ) {
                        return Ok(());
                    }
                    continue;
                }
                Err(error) => {
                    self.log_supervisor(&format!(
                        "resolve subscription update schedule failed: {error}"
                    ))?;
                    if matches!(
                        wait_for_update(self, &mut shutdown, ERROR_RETRY).await,
                        SchedulerEvent::Shutdown
                    ) {
                        return Ok(());
                    }
                    continue;
                }
            };
            match wait_for_update(self, &mut shutdown, delay).await {
                SchedulerEvent::Changed => continue,
                SchedulerEvent::Shutdown => return Ok(()),
                SchedulerEvent::Due => {}
            }
            match self.update_active_subscription().await {
                Ok(true) => {
                    self.log_supervisor("scheduled subscription update staged; restarting core")?;
                    self.request_runtime_reload();
                }
                Ok(false) => {
                    self.log_supervisor("scheduled subscription update completed")?;
                }
                Err(error) => {
                    self.log_supervisor(&format!("scheduled subscription update failed: {error}"))?;
                }
            }
        }
    }

    fn next_subscription_update(&self) -> Result<Option<Duration>, ManagerError> {
        let document = self.store.read()?;
        if document.subscription.interval == "off" {
            return Ok(None);
        }
        let Some(id) = document.active_profile_id.as_deref() else {
            return Ok(None);
        };
        let catalog = self.subscriptions.read()?;
        let Some(profile) = catalog.profiles.iter().find(|profile| profile.id == id) else {
            return Ok(None);
        };
        if !has_scheduled_sources(profile) {
            return Ok(None);
        }
        let interval = humantime::parse_duration(&document.subscription.interval)
            .map_err(|error| SubscriptionError::Invalid(error.to_string()))?;
        Ok(Some(next_delay(document.subscription.last_check, interval)))
    }

    async fn update_active_subscription(&self) -> Result<bool, ManagerError> {
        let document = self.store.read()?;
        let id = document
            .active_profile_id
            .ok_or_else(|| SubscriptionError::Invalid("no active subscription profile".into()))?;
        match self.refresh_subscription_profile(&id).await {
            Ok((change, _)) => {
                let document = self.store.read()?;
                Ok(change.changed && document.subscription_auto_restart)
            }
            Err(error) => {
                self.record_subscription_failure(&id, &error.to_string())?;
                Err(error)
            }
        }
    }

    fn record_subscription_failure(&self, id: &str, error: &str) -> Result<(), ManagerError> {
        let now = Utc::now();
        self.store.update(|document| {
            document.subscription.last_check = Some(now);
            document.subscription.last_result = Some("update failed".into());
            Ok(())
        })?;
        self.subscriptions.update(|catalog| {
            let profile = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == id)
                .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
            profile.extra.insert("last_check".into(), json!(now));
            profile.extra.insert("last_result".into(), json!(error));
            Ok(())
        })?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SchedulerEvent {
    Changed,
    Due,
    Shutdown,
}

async fn wait_for_update<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    delay: Duration,
) -> SchedulerEvent {
    tokio::select! {
        () = manager.wait_subscription_schedule_changed() => SchedulerEvent::Changed,
        () = shutdown_requested(shutdown) => SchedulerEvent::Shutdown,
        () = sleep(delay) => SchedulerEvent::Due,
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}

fn normalize_interval(value: &str) -> Result<String, ManagerError> {
    let value = value.trim().to_ascii_lowercase();
    if value == "off" {
        return Ok(value);
    }
    let duration = humantime::parse_duration(&value)
        .map_err(|error| SubscriptionError::Invalid(format!("invalid interval: {error}")))?;
    if duration < Duration::from_mins(5) {
        return Err(
            SubscriptionError::Invalid("subscription interval must be at least 5m".into()).into(),
        );
    }
    Ok(humantime::format_duration(duration).to_string())
}

fn has_scheduled_sources(profile: &Profile) -> bool {
    profile
        .sources
        .iter()
        .any(|source| source.enabled && source.kind == "url")
}

fn next_delay(last_check: Option<DateTime<Utc>>, interval: Duration) -> Duration {
    let Some(last_check) = last_check else {
        return MINIMUM_DELAY;
    };
    let Ok(interval) = chrono::Duration::from_std(interval) else {
        return MINIMUM_DELAY;
    };
    (last_check + interval - Utc::now())
        .to_std()
        .unwrap_or(MINIMUM_DELAY)
        .max(MINIMUM_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_intervals_and_rejects_busy_loops() {
        assert_eq!(normalize_interval(" 12H ").expect("interval"), "12h");
        assert_eq!(normalize_interval("OFF").expect("disabled"), "off");
        assert!(normalize_interval("4m").is_err());
        assert!(normalize_interval("daily").is_err());
    }

    #[test]
    fn due_updates_use_a_bounded_minimum_delay() {
        assert_eq!(next_delay(None, Duration::from_hours(1)), MINIMUM_DELAY);
        assert_eq!(
            next_delay(
                Some(Utc::now() - chrono::Duration::hours(2)),
                Duration::from_hours(1)
            ),
            MINIMUM_DELAY
        );
    }
}
