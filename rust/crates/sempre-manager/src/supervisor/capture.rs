//! Keeps `OpenWrt` gateway capture in step with the core without tying it to the
//! core process: restarts and reloads leave the rules in place.
use std::{path::Path, time::Duration};

use sempre_transparent::{CaptureIntent, CaptureSettings, TransparentError};
use tokio::time::sleep;

use super::RuntimePlan;
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

// The coordinator waits up to 20 seconds for the core's listener before applying.
const APPLY_TIMEOUT: Duration = Duration::from_secs(40);
const LIST_MAX_AGE: Duration = Duration::from_hours(168);
const LIST_RETRY: Duration = Duration::from_hours(1);
const LIST_FETCH_TIMEOUT: Duration = Duration::from_mins(2);
const LIST_MAX_BYTES: usize = 4 * 1024 * 1024;

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(super) fn capture_intent(
        &self,
        config: &Path,
    ) -> Result<Option<CaptureIntent>, ManagerError> {
        let layout = self.store.layout();
        if !layout.openwrt {
            return Ok(None);
        }
        let settings = CaptureSettings::load(&layout.capture_settings)?;
        if !settings.enabled {
            return Ok(None);
        }
        let data = std::fs::read(config)
            .map_err(|error| ManagerError::io("read runtime configuration", error))?;
        let document = serde_json::from_slice(&data).map_err(|error| {
            TransparentError::Invalid(format!("decode runtime configuration: {error}"))
        })?;
        Ok(Some(CaptureIntent::from_config(settings, &document)?))
    }

    /// A rule failure is a failed start: reporting healthy here would hide lost capture.
    pub(super) async fn sync_capture(&self, plan: &RuntimePlan) -> Result<(), ManagerError> {
        let Some(capture) = self.transparent.capture() else {
            return Ok(());
        };
        let Some(intent) = &plan.capture else {
            capture.withdraw().await?;
            return Ok(());
        };
        self.lifecycle_phase(
            "capture",
            "Activating gateway capture",
            APPLY_TIMEOUT,
            capture.apply(intent),
        )
        .await
    }

    pub(super) async fn withdraw_capture(&self) -> Result<(), ManagerError> {
        if let Some(capture) = self.transparent.capture() {
            capture.withdraw().await?;
        }
        Ok(())
    }

    /// Honors a service stop; a plain daemon restart leaves the rules for its successor.
    pub(super) async fn release_capture_on_exit(&self) {
        if self.store.layout().take_capture_release()
            && let Err(error) = self.withdraw_capture().await
        {
            self.log_supervisor(&format!("withdraw gateway capture failed: {error}"));
        }
    }

    /// Refreshes the domestic prefix list while the core runs. Never returns.
    pub(super) async fn maintain_cn_list(&self, plan: &RuntimePlan) {
        let (Some(capture), Some(intent)) = (self.transparent.capture(), plan.capture.as_ref())
        else {
            return std::future::pending().await;
        };
        loop {
            let due = capture
                .cn_list_age()
                .map_or(Duration::ZERO, |age| LIST_MAX_AGE.saturating_sub(age));
            sleep(due).await;
            match self.refresh_cn_list(intent).await {
                Ok(()) => self.log_supervisor("GeoIP list updated"),
                Err(error) => {
                    self.log_supervisor(&format!("GeoIP update failed; list kept: {error}"));
                    sleep(LIST_RETRY).await;
                }
            }
        }
    }

    async fn refresh_cn_list(&self, intent: &CaptureIntent) -> Result<(), ManagerError> {
        let Some(capture) = self.transparent.capture() else {
            return Ok(());
        };
        let text = fetch_list(&intent.settings.geoip_update_url)
            .await
            .map_err(TransparentError::Invalid)?;
        let previous = capture.read_cn_list();
        capture.install_cn_list(&text)?;
        if let Err(error) = capture.apply(intent).await {
            if let Some(previous) = previous {
                capture.install_cn_list(&previous)?;
            }
            return Err(error.into());
        }
        Ok(())
    }
}

async fn fetch_list(url: &str) -> Result<String, String> {
    let client = sempre_http::client_builder()
        .timeout(LIST_FETCH_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?;
    let data = response.bytes().await.map_err(|error| error.to_string())?;
    if data.len() > LIST_MAX_BYTES {
        return Err("GeoIP list exceeds the size limit".into());
    }
    String::from_utf8(data.to_vec()).map_err(|_| "GeoIP list is not UTF-8".into())
}
