use sempre_state::PendingConfigField;

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn update_dns_settings(
        &self,
        candidate: crate::DnsSettings,
    ) -> Result<(CoreChange, crate::DnsSettings), ManagerError> {
        if !candidate.enabled && self.network_settings.read().mode == crate::NetworkMode::Gateway {
            return Err(ManagerError::InvalidOperation(
                "DNS frontend must remain enabled in gateway mode".into(),
            ));
        }
        let _operation = self.store.acquire_operation()?;
        let document = self.store.read()?;
        let previous = self.dns_settings.read();
        let requires_core_rebuild = previous.requires_core_rebuild(&candidate);
        let saved = self.dns_settings.replace(candidate)?;
        if !requires_core_rebuild {
            self.dns_frontend
                .update_upstreams(&saved.direct_upstreams)
                .await?;
            return Ok((CoreChange::default(), saved));
        }
        if document.selected.is_none() {
            return Ok((CoreChange::default(), saved));
        }
        let Some(profile_id) = document.active_profile_id.as_deref() else {
            return Ok((CoreChange::default(), saved));
        };
        match self
            .prepare_subscription_locked(profile_id, false, false)
            .await
        {
            Ok((change, _)) => {
                if change.changed {
                    self.store.update(|document| {
                        crate::pending_changes::record_pending_fields(
                            document,
                            &[PendingConfigField::Dns],
                            true,
                        );
                        Ok(())
                    })?;
                }
                Ok((change, saved))
            }
            Err(error) => {
                self.dns_settings.restore(previous)?;
                Err(error)
            }
        }
    }
}
