//! Quota behavior over time. The gate takes `now` explicitly, so days, sleep and clock
//! changes are simulated without waiting.
use crate::mock_server::TestEnv;
use chrono::{TimeZone, Utc};
use std::sync::Arc;
use vt_tray::core::cache::Cache;
use vt_tray::core::errors::AppError;
use vt_tray::core::quota::{DAY_LIMIT, MANUAL_RESERVE, MINUTE_LIMIT, QuotaGate};
use vt_tray::core::types::{QuotaDenial, QuotaEndpoint, QuotaOutcome};

fn at(day: u32, hour: u32, minute: u32, second: u32) -> i64 {
    Utc.with_ymd_and_hms(2026, 11, day, hour, minute, second)
        .unwrap()
        .timestamp()
}

fn denial(result: Result<i64, AppError>) -> Option<QuotaDenial> {
    match result {
        Err(AppError::QuotaDenied(denial)) => Some(denial),
        _ => None,
    }
}

fn fill_day(cache: &Cache, day: u32, count: u32) {
    for index in 0..count {
        cache
            .insert_quota_event(
                at(day, 1, 0, 0) + index as i64 * 60,
                QuotaEndpoint::FileLookup,
                QuotaOutcome::Ok,
            )
            .unwrap();
    }
}

#[test]
fn daily_limit_is_exhausted_then_resets_at_utc_midnight() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    fill_day(&cache, 18, DAY_LIMIT);
    let gate = QuotaGate::new(cache);
    let last_second = at(18, 23, 59, 59);
    assert_eq!(
        denial(gate.reserve(last_second, QuotaEndpoint::FileLookup, true)),
        Some(QuotaDenial::Day)
    );
    let status = gate.status(last_second).unwrap();
    assert_eq!(status.manual_remaining, 0);
    assert_eq!(status.day_reset_at, at(19, 0, 0, 0));
    gate.reserve(at(19, 0, 0, 0), QuotaEndpoint::FileLookup, true)
        .expect("a new UTC day restores capacity");
}

#[test]
fn manual_reserve_keeps_capacity_after_automatic_checks_stop() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    fill_day(&cache, 18, DAY_LIMIT - MANUAL_RESERVE);
    let gate = QuotaGate::new(cache);
    let now = at(18, 20, 0, 0);
    assert_eq!(
        denial(gate.reserve(now, QuotaEndpoint::FileLookup, false)),
        Some(QuotaDenial::ManualReserve)
    );
    assert_eq!(gate.status(now).unwrap().auto_remaining, 0);
    assert_eq!(gate.status(now).unwrap().manual_remaining, MANUAL_RESERVE);
    gate.reserve(now, QuotaEndpoint::FileLookup, true).unwrap();
}

#[test]
fn counters_and_pause_survive_a_restart() {
    let env = TestEnv::new(vec![]);
    let now = at(18, 12, 0, 0);
    for _ in 0..3 {
        env.quota
            .reserve(now, QuotaEndpoint::FileLookup, true)
            .unwrap();
    }
    env.quota
        .pause(now, "service rate limit", now + 300)
        .unwrap();

    let restarted = QuotaGate::new(Arc::new(env.reopen_cache()));
    let status = restarted.status(now + 1).unwrap();
    assert_eq!(status.day_used, 3);
    assert_eq!(status.minute_used, 3);
    assert_eq!(status.paused_reason.as_deref(), Some("service rate limit"));
    assert_eq!(
        denial(restarted.reserve(now + 1, QuotaEndpoint::FileLookup, true)),
        Some(QuotaDenial::Paused)
    );
    restarted
        .reserve(now + 301, QuotaEndpoint::FileLookup, true)
        .expect("the pause ends on time after a restart");
}

#[test]
fn sleep_and_resume_clear_the_minute_window_but_keep_the_day() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    let gate = QuotaGate::new(cache);
    let before_sleep = at(18, 10, 0, 0);
    for _ in 0..MINUTE_LIMIT {
        gate.reserve(before_sleep, QuotaEndpoint::FileLookup, true)
            .unwrap();
    }
    assert_eq!(
        denial(gate.reserve(before_sleep + 30, QuotaEndpoint::FileLookup, true)),
        Some(QuotaDenial::Minute)
    );
    let after_resume = at(18, 18, 0, 0);
    gate.reserve(after_resume, QuotaEndpoint::FileLookup, true)
        .unwrap();
    assert_eq!(
        gate.status(after_resume).unwrap().day_used,
        MINUTE_LIMIT + 1
    );
}

#[test]
fn clock_set_back_does_not_block_requests_for_long() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    let gate = QuotaGate::new(cache);
    let noon = at(18, 12, 0, 0);
    for _ in 0..MINUTE_LIMIT {
        gate.reserve(noon, QuotaEndpoint::FileLookup, true).unwrap();
    }
    // Clock moved back one hour: the noon requests are "in the future" now.
    let earlier = noon - 3600;
    let status = gate.status(earlier).unwrap();
    assert_eq!(status.minute_used, 0);
    assert_eq!(status.day_used, MINUTE_LIMIT, "still counted for the day");
    gate.reserve(earlier, QuotaEndpoint::FileLookup, true)
        .expect("not blocked until real time catches up");
    // Within one minute of the jump the requests still count, erring on the safe side.
    assert_eq!(
        denial(gate.reserve(noon - 30, QuotaEndpoint::FileLookup, true)),
        Some(QuotaDenial::Minute)
    );
}

#[test]
fn clock_set_back_across_midnight_ignores_tomorrows_requests() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    fill_day(&cache, 19, DAY_LIMIT);
    let gate = QuotaGate::new(cache);
    gate.reserve(at(18, 23, 0, 0), QuotaEndpoint::FileLookup, true)
        .expect("requests dated tomorrow do not use today's budget");
}

#[test]
fn seven_days_of_heavy_use_never_exceed_the_free_limits() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    let gate = QuotaGate::new(cache.clone());
    for day in 18..25 {
        let mut now = at(day, 8, 0, 0);
        let (mut auto_ok, mut manual_ok) = (0, 0);
        // Busy folder: far more automatic checks than the budget allows.
        for _ in 0..600 {
            if gate.reserve(now, QuotaEndpoint::FileLookup, false).is_ok() {
                auto_ok += 1;
            }
            now += 15;
        }
        // Then a burst of manual checks, several per second.
        for index in 0..300 {
            if gate.reserve(now, QuotaEndpoint::FileLookup, true).is_ok() {
                manual_ok += 1;
            }
            if index % 4 == 3 {
                now += 1;
            }
        }
        // And steady manual use afterwards.
        for _ in 0..200 {
            if gate.reserve(now, QuotaEndpoint::FileLookup, true).is_ok() {
                manual_ok += 1;
            }
            now += 15;
        }
        assert_eq!(
            auto_ok,
            DAY_LIMIT - MANUAL_RESERVE,
            "day {day}: auto budget"
        );
        assert_eq!(auto_ok + manual_ok, DAY_LIMIT, "day {day}: daily limit");
    }
    let timestamps: Vec<i64> = cache
        .quota_events(0)
        .unwrap()
        .into_iter()
        .map(|event| event.ts)
        .collect();
    assert_eq!(timestamps.len() as u32, 7 * DAY_LIMIT);
    for &ts in &timestamps {
        let in_minute = timestamps
            .iter()
            .filter(|&&other| other > ts - 60 && other <= ts)
            .count() as u32;
        assert!(
            in_minute <= MINUTE_LIMIT,
            "more than {MINUTE_LIMIT} requests in a minute at {ts}"
        );
    }
}

#[test]
fn no_warning_while_quota_is_comfortable() {
    let cache = Arc::new(Cache::in_memory().unwrap());
    fill_day(&cache, 18, 100);
    let gate = QuotaGate::new(cache);
    assert!(gate.take_new_warnings(at(18, 12, 0, 0)).unwrap().is_empty());
}

#[test]
fn warnings_appear_once_per_level_and_return_the_next_day() {
    use vt_tray::core::quota::{DAY_LOW_REMAINING, QuotaWarning};

    let env = TestEnv::new(vec![]);
    let noon = at(18, 12, 0, 0);
    fill_day(&env.cache, 18, DAY_LIMIT - MANUAL_RESERVE);
    assert_eq!(
        env.quota.take_new_warnings(noon).unwrap(),
        vec![QuotaWarning::AutoPaused]
    );
    assert!(
        env.quota.take_new_warnings(noon + 1).unwrap().is_empty(),
        "shown once"
    );

    // Survives a restart: still not repeated.
    let restarted = QuotaGate::new(Arc::new(env.reopen_cache()));
    assert!(restarted.take_new_warnings(noon + 2).unwrap().is_empty());

    // Getting close to the daily limit is a new, more urgent warning.
    for index in 0..(MANUAL_RESERVE - DAY_LOW_REMAINING) {
        env.cache
            .insert_quota_event(
                noon + 10 + index as i64,
                QuotaEndpoint::FileLookup,
                QuotaOutcome::Ok,
            )
            .unwrap();
    }
    assert_eq!(
        restarted.take_new_warnings(noon + 200).unwrap(),
        vec![QuotaWarning::DayLow {
            remaining: DAY_LOW_REMAINING
        }]
    );

    // A new UTC day starts clean; the first automatic pause warns again.
    let next_day = at(19, 12, 0, 0);
    fill_day(&env.cache, 19, DAY_LIMIT - MANUAL_RESERVE);
    assert_eq!(
        restarted.take_new_warnings(next_day).unwrap(),
        vec![QuotaWarning::AutoPaused]
    );
}

#[test]
fn monthly_warning_at_ninety_percent() {
    use vt_tray::core::quota::{MONTH_LIMIT, QuotaWarning};

    let cache = Arc::new(Cache::in_memory().unwrap());
    // Spread 90% of the month over earlier days, staying under each daily limit.
    let target = MONTH_LIMIT * 9 / 10;
    let mut inserted = 0;
    for day in 1..18 {
        for index in 0..DAY_LIMIT {
            if inserted == target {
                break;
            }
            cache
                .insert_quota_event(
                    at(day, 1, 0, 0) + index as i64 * 60,
                    QuotaEndpoint::FileLookup,
                    QuotaOutcome::Ok,
                )
                .unwrap();
            inserted += 1;
        }
    }
    // 17 days x 500 is 8,500, below 90%; top up on the 1st directly.
    for index in inserted..target {
        cache
            .insert_quota_event(
                at(1, 0, 0, 0) + index as i64 % 60,
                QuotaEndpoint::FileLookup,
                QuotaOutcome::Ok,
            )
            .unwrap();
    }
    let gate = QuotaGate::new(cache);
    assert_eq!(
        gate.take_new_warnings(at(18, 12, 0, 0)).unwrap(),
        vec![QuotaWarning::MonthLow {
            remaining: MONTH_LIMIT - target
        }]
    );
}

#[test]
fn clear_local_data_keeps_quota_counters_but_removal_wipes_everything() {
    let env = TestEnv::new(vec![]);
    let now = at(18, 12, 0, 0);
    env.quota
        .reserve(now, QuotaEndpoint::FileLookup, true)
        .unwrap();
    env.quota
        .pause(now, "service rate limit", now + 60)
        .unwrap();

    // "Clear local data": history goes, limits are still respected.
    env.cache.clear_history().unwrap();
    env.cache.clear_cache().unwrap();
    let status = env.quota.status(now).unwrap();
    assert_eq!(status.day_used, 1);
    assert!(status.paused_until.is_some());

    // "Remove VTTray from this PC": nothing is left, even after reopening the database.
    env.cache.wipe_all().unwrap();
    let reopened = QuotaGate::new(Arc::new(env.reopen_cache()));
    let status = reopened.status(now).unwrap();
    assert_eq!(status.day_used, 0);
    assert!(status.paused_until.is_none());
    assert!(status.paused_reason.is_none());
}
