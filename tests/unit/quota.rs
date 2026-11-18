use chrono::{TimeZone, Utc};
use std::sync::Arc;
use vt_tray::core::cache::Cache;
use vt_tray::core::quota::{DAY_LIMIT, MANUAL_RESERVE, MINUTE_LIMIT, QuotaGate};
use vt_tray::core::types::{QuotaDenial, QuotaEndpoint};

fn noon() -> i64 {
    Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0)
        .unwrap()
        .timestamp()
}

#[test]
fn enforces_the_minute_window() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    let gate = QuotaGate::new(cache);
    for _ in 0..MINUTE_LIMIT {
        gate.reserve(noon(), QuotaEndpoint::FileLookup, true)
            .unwrap();
    }
    assert!(matches!(
        gate.reserve(noon(), QuotaEndpoint::FileLookup, true),
        Err(vt_tray::core::errors::AppError::QuotaDenied(
            QuotaDenial::Minute
        ))
    ));
}

#[test]
fn automatic_checks_honor_the_manual_reserve() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    for index in 0..(DAY_LIMIT - MANUAL_RESERVE) {
        cache
            .insert_quota_event(
                noon() - 3600 - index as i64,
                QuotaEndpoint::FileLookup,
                vt_tray::core::types::QuotaOutcome::Ok,
            )
            .unwrap();
    }
    let gate = QuotaGate::new(cache);
    assert!(matches!(
        gate.reserve(noon(), QuotaEndpoint::FileLookup, false),
        Err(vt_tray::core::errors::AppError::QuotaDenied(
            QuotaDenial::ManualReserve
        ))
    ));
    gate.reserve(noon(), QuotaEndpoint::FileLookup, true)
        .unwrap();
}

#[test]
fn pause_blocks_all_reservations() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    let gate = QuotaGate::new(cache);
    gate.pause(noon(), "rate limited", noon() + 60).unwrap();
    assert!(matches!(
        gate.reserve(noon() + 1, QuotaEndpoint::FileLookup, true),
        Err(vt_tray::core::errors::AppError::QuotaDenied(
            QuotaDenial::Paused
        ))
    ));
    gate.clear_pause(noon() + 1).unwrap();
    gate.reserve(noon() + 2, QuotaEndpoint::FileLookup, true)
        .unwrap();
}

#[test]
fn daily_limit_applies_to_manual_requests() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    for index in 0..DAY_LIMIT {
        cache
            .insert_quota_event(
                noon() - 3600 - index as i64,
                QuotaEndpoint::FileLookup,
                vt_tray::core::types::QuotaOutcome::Ok,
            )
            .unwrap();
    }
    let gate = QuotaGate::new(cache);
    assert!(matches!(
        gate.reserve(noon(), QuotaEndpoint::FileLookup, true),
        Err(vt_tray::core::errors::AppError::QuotaDenied(
            QuotaDenial::Day
        ))
    ));
}

#[test]
fn status_reports_remaining_capacity() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    cache
        .insert_quota_event(
            noon(),
            QuotaEndpoint::FileLookup,
            vt_tray::core::types::QuotaOutcome::Ok,
        )
        .unwrap();
    let gate = QuotaGate::new(cache);
    let status = gate.status(noon()).unwrap();
    assert_eq!(status.minute_used, 1);
    assert_eq!(status.day_used, 1);
    assert_eq!(status.manual_remaining, DAY_LIMIT - 1);
    assert_eq!(
        status.auto_remaining,
        DAY_LIMIT.saturating_sub(MANUAL_RESERVE).saturating_sub(1)
    );
}
