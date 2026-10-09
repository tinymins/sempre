use std::fmt::Write as _;

use sempre_core::CoreRef;
use sempre_state::PendingConfigField;

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn update_dns_settings(
        &self,
        mut candidate: crate::DnsSettings,
    ) -> Result<(CoreChange, crate::DnsSettings), ManagerError> {
        candidate.normalize_and_validate()?;
        if !candidate.enabled && self.network_settings.read().mode == crate::NetworkMode::Gateway {
            return Err(ManagerError::InvalidOperation(
                "DNS frontend must remain enabled in gateway mode".into(),
            ));
        }
        let _operation = self.store.acquire_operation()?;
        let _runtime = self.runtime_rules_gate.lock().await;
        let document = self.store.read()?;
        let previous = self.dns_settings.read();
        candidate.revision = previous.revision;
        if candidate == previous {
            return Ok((CoreChange::default(), previous));
        }
        let requires_core_rebuild = previous.requires_core_rebuild(&candidate);
        if requires_core_rebuild
            && previous.enabled == candidate.enabled
            && previous.domestic_domains == candidate.domestic_domains
            && let Some(update) = crate::dns_rule_files::prepare_update(
                self.store.layout(),
                &document,
                &previous,
                &candidate,
            )?
        {
            return self
                .publish_dns_rule_update(&document, candidate, update)
                .await;
        }
        if !requires_core_rebuild {
            self.dns_frontend
                .update_upstreams(&candidate.direct_upstreams)
                .await?;
            let saved = match self.dns_settings.replace(candidate) {
                Ok(saved) => saved,
                Err(error) => {
                    let rollback = self
                        .dns_frontend
                        .update_upstreams(&previous.direct_upstreams)
                        .await
                        .err();
                    return Err(publication_error(error, None, rollback));
                }
            };
            return Ok((
                CoreChange {
                    message: "dns_settings_saved".into(),
                    ..CoreChange::default()
                },
                saved,
            ));
        }
        let legacy = crate::dns_rule_files::legacy_inline(&document, &previous);
        let saved = self.dns_settings.replace(candidate)?;
        if document.selected.is_none() {
            return Ok((CoreChange::default(), saved));
        }
        let Some(profile_id) = document.active_profile_id.as_deref() else {
            return Ok((CoreChange::default(), saved));
        };
        self.store.update(|document| {
            document.pending = true;
            crate::pending_changes::record_pending_fields(
                document,
                &[PendingConfigField::Dns],
                true,
            );
            Ok(())
        })?;
        let (mut change, _) = self
            .prepare_subscription_locked(profile_id, false, false)
            .await?;
        change.needs_restart = true;
        change.message = if legacy {
            "dns_rule_sets_legacy_restart_required"
        } else {
            "dns_settings_restart_required"
        }
        .into();
        Ok((change, saved))
    }

    async fn publish_dns_rule_update(
        &self,
        document: &sempre_state::Document,
        candidate: crate::DnsSettings,
        mut update: crate::dns_rule_files::RuleFileUpdate,
    ) -> Result<(CoreChange, crate::DnsSettings), ManagerError> {
        let reference = CoreRef {
            core: "sing-box".into(),
            repository: document.runtime.repository.clone(),
            reference: document.runtime.reference.clone().ok_or_else(|| {
                ManagerError::InvalidOperation("running core has no reference".into())
            })?,
        };
        let version =
            document.runtime.version.as_deref().ok_or_else(|| {
                ManagerError::InvalidOperation("running core has no version".into())
            })?;
        self.validate_config_path(&reference, version, update.validation_config())
            .await?;
        if let Err(error) = update.publish() {
            return Err(publication_error(error, update.rollback().err(), None));
        }
        let frontend = match self.dns_frontend.update_rules(&candidate).await {
            Ok(previous) => previous,
            Err(error) => {
                return Err(publication_error(error, update.rollback().err(), None));
            }
        };
        let result = update
            .commit()
            .and_then(|()| self.dns_settings.replace(candidate));
        match result {
            Ok(saved) => Ok((
                CoreChange {
                    changed: true,
                    message: "dns_rule_sets_published".into(),
                    ..CoreChange::default()
                },
                saved,
            )),
            Err(error) => {
                let files = update.rollback().err();
                let dns = self.dns_frontend.restore_rules(frontend).await.err();
                Err(publication_error(error, files, dns))
            }
        }
    }
}

fn publication_error(
    error: ManagerError,
    files: Option<ManagerError>,
    dns: Option<ManagerError>,
) -> ManagerError {
    if files.is_none() && dns.is_none() {
        return error;
    }
    let mut message = format!("DNS rule set update failed: {error}");
    if let Some(error) = files {
        write!(message, "; restoring rule files failed: {error}")
            .expect("write failure description");
    }
    if let Some(error) = dns {
        write!(message, "; restoring DNS frontend failed: {error}")
            .expect("write failure description");
    }
    ManagerError::InvalidOperation(message)
}
