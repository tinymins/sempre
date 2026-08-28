use chrono::{DateTime, Datelike, Local, Months, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_RETENTION_HOURS: u32 = 24;
const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const MIN_RETENTION_HOURS: u32 = 1;
pub(crate) const MAX_RETENTION_HOURS: u32 = 24 * 30;
pub(crate) const MIN_MAX_BYTES: u64 = 1024 * 1024;
const MAX_MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TrafficSettings {
    pub retention_hours: u32,
    #[serde(default)]
    pub reset_day: Option<u8>,
    pub max_bytes: u64,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            retention_hours: DEFAULT_RETENTION_HOURS,
            reset_day: None,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum RotationError {
    #[error("retention_hours must be between {MIN_RETENTION_HOURS} and {MAX_RETENTION_HOURS}")]
    Retention,
    #[error("reset_day must be between 1 and 31")]
    ResetDay,
    #[error("max_bytes must be between {MIN_MAX_BYTES} and {MAX_MAX_BYTES}")]
    MaximumSize,
}

pub(crate) fn validate(settings: &TrafficSettings) -> Result<(), RotationError> {
    if !(MIN_RETENTION_HOURS..=MAX_RETENTION_HOURS).contains(&settings.retention_hours) {
        return Err(RotationError::Retention);
    }
    if settings
        .reset_day
        .is_some_and(|day| !(1..=31).contains(&day))
    {
        return Err(RotationError::ResetDay);
    }
    if !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&settings.max_bytes) {
        return Err(RotationError::MaximumSize);
    }
    Ok(())
}

pub(crate) fn cutoff(settings: &TrafficSettings, now: i64) -> i64 {
    settings.reset_day.map_or_else(
        || now - i64::from(settings.retention_hours) * 3_600_000,
        |day| monthly_cutoff(now, day).unwrap_or(now),
    )
}

fn monthly_cutoff(now: i64, day: u8) -> Option<i64> {
    let local_now = DateTime::from_timestamp_millis(now)?.with_timezone(&Local);
    let reset_date = monthly_reset_date(local_now.date_naive(), day)?;
    (0..24).find_map(|hour| {
        let local_time = reset_date.and_hms_opt(hour, 0, 0)?;
        Local
            .from_local_datetime(&local_time)
            .earliest()
            .map(|value| value.timestamp_millis())
    })
}

fn monthly_reset_date(now: NaiveDate, day: u8) -> Option<NaiveDate> {
    let current = reset_date(now.year(), now.month(), day)?;
    if now >= current {
        return Some(current);
    }
    let previous =
        NaiveDate::from_ymd_opt(now.year(), now.month(), 1)?.checked_sub_months(Months::new(1))?;
    reset_date(previous.year(), previous.month(), day)
}

fn reset_date(year: i32, month: u32, day: u8) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let last = first.checked_add_months(Months::new(1))?.pred_opt()?;
    NaiveDate::from_ymd_opt(year, month, u32::from(day).min(last.day()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monthly_cycle_starts_on_configured_day() {
        assert_eq!(
            monthly_reset_date(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), 21),
            NaiveDate::from_ymd_opt(2026, 7, 21)
        );
        assert_eq!(
            monthly_reset_date(NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(), 21),
            NaiveDate::from_ymd_opt(2026, 8, 21)
        );
    }

    #[test]
    fn monthly_cycle_clamps_to_last_day_of_short_months() {
        assert_eq!(
            monthly_reset_date(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(), 31),
            NaiveDate::from_ymd_opt(2026, 2, 28)
        );
        assert_eq!(
            monthly_reset_date(NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(), 31),
            NaiveDate::from_ymd_opt(2024, 2, 29)
        );
        assert_eq!(
            monthly_reset_date(NaiveDate::from_ymd_opt(2026, 4, 30).unwrap(), 31),
            NaiveDate::from_ymd_opt(2026, 4, 30)
        );
    }

    #[test]
    fn rejects_invalid_reset_days() {
        let settings = TrafficSettings {
            retention_hours: 24,
            reset_day: Some(0),
            max_bytes: DEFAULT_MAX_BYTES,
        };
        assert!(matches!(validate(&settings), Err(RotationError::ResetDay)));
    }

    #[test]
    fn legacy_settings_default_to_rolling_retention() {
        let settings: TrafficSettings =
            serde_json::from_str(r#"{"retention_hours":72,"max_bytes":33554432}"#)
                .expect("legacy settings");
        assert_eq!(settings.reset_day, None);
    }
}
