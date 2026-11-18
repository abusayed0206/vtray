use crate::core::cache::Cache;
use crate::core::errors::AppError;
use crate::core::types::{QuotaDenial, QuotaEndpoint, QuotaOutcome, QuotaStatus};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use std::sync::{Arc, Mutex};

pub const MINUTE_LIMIT: u32 = 4;
pub const DAY_LIMIT: u32 = 500;
pub const MONTH_LIMIT: u32 = 15_500;
pub const MANUAL_RESERVE: u32 = 100;

/// Warn when this many manual requests or fewer are left today.
pub const DAY_LOW_REMAINING: u32 = 25;
/// Warn when this share of the monthly limit is used.
pub const MONTH_LOW_PERCENT: u32 = 90;

/// Advance notice before VirusTotal's limits stop checks (SC-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaWarning {
    /// Automatic checks stopped for today to keep the manual reserve.
    AutoPaused,
    DayLow {
        remaining: u32,
    },
    MonthLow {
        remaining: u32,
    },
}

/// Every warning that applies to `status`, most urgent first, each with the state key that
/// records it and the period (reset time) it belongs to.
pub fn quota_warnings(status: &QuotaStatus) -> Vec<(QuotaWarning, &'static str, i64)> {
    let mut warnings = Vec::new();
    if status.manual_remaining <= DAY_LOW_REMAINING {
        warnings.push((
            QuotaWarning::DayLow {
                remaining: status.manual_remaining,
            },
            "warned_day_low",
            status.day_reset_at,
        ));
    }
    if status.month_used * 100 >= status.month_limit * MONTH_LOW_PERCENT {
        warnings.push((
            QuotaWarning::MonthLow {
                remaining: status.month_limit.saturating_sub(status.month_used),
            },
            "warned_month_low",
            status.month_reset_at,
        ));
    }
    if status.auto_remaining == 0 {
        warnings.push((
            QuotaWarning::AutoPaused,
            "warned_auto_paused",
            status.day_reset_at,
        ));
    }
    warnings
}

pub struct QuotaGate {
    cache: Arc<Cache>,
    serial: Mutex<()>,
}

impl QuotaGate {
    pub fn new(cache: Arc<Cache>) -> Self {
        Self {
            cache,
            serial: Mutex::new(()),
        }
    }

    pub fn reserve(
        &self,
        now: i64,
        endpoint: QuotaEndpoint,
        manual: bool,
    ) -> Result<i64, AppError> {
        let _guard = self
            .serial
            .lock()
            .map_err(|_| AppError::Internal("quota lock was poisoned".to_string()))?;
        let status = self.status(now)?;
        if status.paused_until.is_some_and(|until| until > now) {
            return Err(AppError::QuotaDenied(QuotaDenial::Paused));
        }
        if status.minute_used >= MINUTE_LIMIT {
            return Err(AppError::QuotaDenied(QuotaDenial::Minute));
        }
        if status.day_used >= DAY_LIMIT {
            return Err(AppError::QuotaDenied(QuotaDenial::Day));
        }
        if status.month_used >= MONTH_LIMIT {
            return Err(AppError::QuotaDenied(QuotaDenial::Month));
        }
        if !manual && status.day_used >= DAY_LIMIT.saturating_sub(MANUAL_RESERVE) {
            return Err(AppError::QuotaDenied(QuotaDenial::ManualReserve));
        }
        let id = self
            .cache
            .insert_quota_event(now, endpoint, QuotaOutcome::Reserved)?;
        Ok(id)
    }

    pub fn complete(&self, id: i64, outcome: QuotaOutcome) -> Result<(), AppError> {
        self.cache.update_quota_outcome(id, outcome)
    }

    pub fn pause(&self, now: i64, reason: &str, until: i64) -> Result<(), AppError> {
        self.cache.set_quota_state("paused_reason", reason, now)?;
        self.cache
            .set_quota_state("paused_until", &until.to_string(), now)
    }

    pub fn clear_pause(&self, now: i64) -> Result<(), AppError> {
        self.cache.set_quota_state("paused_reason", "", now)?;
        self.cache.set_quota_state("paused_until", "0", now)
    }

    /// Warnings not yet shown in their current day or month, most urgent first. Each is
    /// recorded as shown, so it appears once per period, even across restarts.
    pub fn take_new_warnings(&self, now: i64) -> Result<Vec<QuotaWarning>, AppError> {
        let status = self.status(now)?;
        let mut fresh = Vec::new();
        for (warning, key, period) in quota_warnings(&status) {
            let period = period.to_string();
            if self.cache.get_quota_state(key)?.as_deref() != Some(period.as_str()) {
                self.cache.set_quota_state(key, &period, now)?;
                fresh.push(warning);
            }
        }
        Ok(fresh)
    }

    pub fn status(&self, now: i64) -> Result<QuotaStatus, AppError> {
        let (day_start, day_reset_at) = day_bounds(now);
        let (month_start, month_reset_at) = month_bounds(now);
        // Upper bounds keep a backward clock change from counting "future" events forever:
        // at most one extra minute of blocking, and day/month windows end at their reset.
        let minute_used = self
            .cache
            .count_quota_events_between(now.saturating_sub(59), now.saturating_add(60))?;
        let day_used = self
            .cache
            .count_quota_events_between(day_start, day_reset_at)?;
        let month_used = self
            .cache
            .count_quota_events_between(month_start, month_reset_at)?;
        let paused_reason = self.cache.get_quota_state("paused_reason")?;
        let paused_until = self
            .cache
            .get_quota_state("paused_until")?
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|until| *until > now);
        Ok(QuotaStatus {
            minute_used,
            minute_limit: MINUTE_LIMIT,
            day_used,
            day_limit: DAY_LIMIT,
            month_used,
            month_limit: MONTH_LIMIT,
            manual_remaining: DAY_LIMIT.saturating_sub(day_used),
            auto_remaining: DAY_LIMIT
                .saturating_sub(MANUAL_RESERVE)
                .saturating_sub(day_used),
            paused_reason: paused_reason.filter(|reason| !reason.is_empty()),
            paused_until,
            day_reset_at,
            month_reset_at,
        })
    }
}

pub fn day_bounds(now: i64) -> (i64, i64) {
    let Some(datetime) = DateTime::<Utc>::from_timestamp(now, 0) else {
        return (now, now.saturating_add(86_400));
    };
    let start = datetime
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc()
        .timestamp();
    let next = (datetime.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc()
        .timestamp();
    (start, next)
}

pub fn month_bounds(now: i64) -> (i64, i64) {
    let Some(datetime) = DateTime::<Utc>::from_timestamp(now, 0) else {
        return (now, now.saturating_add(2_592_000));
    };
    let date = datetime.date_naive();
    let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
        .unwrap_or(date)
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc()
        .timestamp();
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let next = NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap_or(date)
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc()
        .timestamp();
    (start, next)
}
