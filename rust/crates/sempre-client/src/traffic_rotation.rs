use chrono::{DateTime, Datelike, Local, Months, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_RETENTION_HOURS: u32 = 24;
const DEFAULT_RETENTION_MONTHS: u16 = 12;
const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const MIN_RETENTION_HOURS: u32 = 1;
pub(crate) const MAX_RETENTION_HOURS: u32 = 24 * 30;
const MIN_RETENTION_MONTHS: u16 = 1;
const MAX_RETENTION_MONTHS: u16 = 120;
pub(crate) const MIN_MAX_BYTES: u64 = 1024 * 1024;
const MAX_MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct TrafficSettings {
    pub retention_hours: Option<u32>,
    pub reset_day: Option<u8>,
    pub retention_months: Option<u16>,
    pub max_bytes: Option<u64>,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            retention_hours: Some(DEFAULT_RETENTION_HOURS),
            reset_day: None,
            retention_months: Some(DEFAULT_RETENTION_MONTHS),
            max_bytes: Some(DEFAULT_MAX_BYTES),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum RotationError {
    #[error("retention_hours must be between {MIN_RETENTION_HOURS} and {MAX_RETENTION_HOURS}")]
    Retention,
    #[error("reset_day must be between 1 and 31")]
    ResetDay,
    #[error("retention_months must be between {MIN_RETENTION_MONTHS} and {MAX_RETENTION_MONTHS}")]
    RetentionMonths,
    #[error("max_bytes must be between {MIN_MAX_BYTES} and {MAX_MAX_BYTES}")]
    MaximumSize,
    #[error("traffic history must keep either a time limit or a storage limit")]
    Unbounded,
}

pub(crate) fn validate(settings: &TrafficSettings) -> Result<(), RotationError> {
    if settings
        .retention_hours
        .is_some_and(|hours| !(MIN_RETENTION_HOURS..=MAX_RETENTION_HOURS).contains(&hours))
    {
        return Err(RotationError::Retention);
    }
    if settings
        .reset_day
        .is_some_and(|day| !(1..=31).contains(&day))
    {
        return Err(RotationError::ResetDay);
    }
    if settings
        .retention_months
        .is_some_and(|months| !(MIN_RETENTION_MONTHS..=MAX_RETENTION_MONTHS).contains(&months))
    {
        return Err(RotationError::RetentionMonths);
    }
    if settings
        .max_bytes
        .is_some_and(|bytes| !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&bytes))
    {
        return Err(RotationError::MaximumSize);
    }
    let time_limited = settings.reset_day.map_or_else(
        || settings.retention_hours.is_some(),
        |_| settings.retention_months.is_some(),
    );
    if !time_limited && settings.max_bytes.is_none() {
        return Err(RotationError::Unbounded);
    }
    Ok(())
}

pub(crate) fn storage_cutoff(settings: &TrafficSettings, now: i64) -> Option<i64> {
    settings.reset_day.map_or_else(
        || {
            settings
                .retention_hours
                .map(|hours| now - i64::from(hours) * 3_600_000)
        },
        |day| {
            settings
                .retention_months
                .and_then(|months| monthly_cutoff(now, day, months))
        },
    )
}

pub(crate) fn summary_cutoff(settings: &TrafficSettings, now: i64) -> Option<i64> {
    settings.reset_day.map_or_else(
        || storage_cutoff(settings, now),
        |day| monthly_cutoff(now, day, 1),
    )
}

fn monthly_cutoff(now: i64, day: u8, months: u16) -> Option<i64> {
    let local_now = DateTime::from_timestamp_millis(now)?.with_timezone(&Local);
    let reset_date = monthly_retention_date(local_now.date_naive(), day, months)?;
    (0..24).find_map(|hour| {
        let local_time = reset_date.and_hms_opt(hour, 0, 0)?;
        Local
            .from_local_datetime(&local_time)
            .earliest()
            .map(|value| value.timestamp_millis())
    })
}

fn monthly_retention_date(now: NaiveDate, day: u8, months: u16) -> Option<NaiveDate> {
    monthly_reset_date(now, day)?
        .checked_sub_months(Months::new(u32::from(months.saturating_sub(1))))
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
    fn monthly_retention_includes_configured_number_of_cycles() {
        assert_eq!(
            monthly_retention_date(NaiveDate::from_ymd_opt(2026, 8, 28).unwrap(), 21, 12),
            NaiveDate::from_ymd_opt(2025, 9, 21)
        );
    }

    #[test]
    fn rejects_invalid_reset_days() {
        let settings = TrafficSettings {
            retention_hours: Some(24),
            reset_day: Some(0),
            retention_months: Some(12),
            max_bytes: Some(DEFAULT_MAX_BYTES),
        };
        assert!(matches!(validate(&settings), Err(RotationError::ResetDay)));
    }

    #[test]
    fn legacy_settings_default_to_rolling_retention() {
        let settings: TrafficSettings =
            serde_json::from_str(r#"{"retention_hours":72,"max_bytes":33554432}"#)
                .expect("legacy settings");
        assert_eq!(settings.reset_day, None);
        assert_eq!(settings.retention_hours, Some(72));
        assert_eq!(settings.retention_months, Some(12));
        assert_eq!(settings.max_bytes, Some(DEFAULT_MAX_BYTES));
    }

    #[test]
    fn rejects_completely_unbounded_storage() {
        let settings = TrafficSettings {
            retention_hours: None,
            reset_day: Some(21),
            retention_months: None,
            max_bytes: None,
        };
        assert!(matches!(validate(&settings), Err(RotationError::Unbounded)));
    }
}
