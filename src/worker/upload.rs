use crate::api::client::VtClient;
use crate::core::cache::Cache;
use crate::core::errors::{ApiError, AppError};
use crate::core::hash::safe_base_name;
use crate::core::policy::UploadRoute;
use crate::core::policy::upload_route;
use crate::core::quota::QuotaGate;
use crate::core::types::{
    DetectionState, FileRecord, LookupSummary, QuotaEndpoint, QuotaOutcome, UploadJob,
    UploadStatus, VerdictState,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

pub const POLL_DELAYS_SECONDS: [u64; 6] = [10, 30, 60, 120, 240, 300];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadOutcome {
    pub path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
    pub analysis_id: String,
    pub used_upload_url: bool,
    pub summary: Option<LookupSummary>,
}

pub fn upload(
    cache: &Cache,
    client: &VtClient,
    quota: &QuotaGate,
    path: &Path,
    sha256: &str,
    size_bytes: u64,
    consent_at: i64,
) -> Result<UploadOutcome, AppError> {
    upload_with_delays(
        cache,
        client,
        quota,
        path,
        sha256,
        size_bytes,
        consent_at,
        &POLL_DELAYS_SECONDS,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn upload_with_delays(
    cache: &Cache,
    client: &VtClient,
    quota: &QuotaGate,
    path: &Path,
    sha256: &str,
    size_bytes: u64,
    consent_at: i64,
    delays: &[u64],
) -> Result<UploadOutcome, AppError> {
    upload_with_cancel(
        cache, client, quota, path, sha256, size_bytes, consent_at, delays, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn upload_with_cancel(
    cache: &Cache,
    client: &VtClient,
    quota: &QuotaGate,
    path: &Path,
    sha256: &str,
    size_bytes: u64,
    consent_at: i64,
    delays: &[u64],
    cancel: Option<&AtomicBool>,
) -> Result<UploadOutcome, AppError> {
    // Constitution I: no file content leaves the machine without a recorded consent.
    if !crate::core::policy::consent_satisfied(Some(consent_at)) {
        return Err(AppError::Validation(
            "an upload needs the user's consent first".to_string(),
        ));
    }
    let fingerprint = crate::core::hash::sha256_file(path)?;
    if fingerprint.sha256 != sha256 || fingerprint.size_bytes != size_bytes {
        return Err(AppError::Validation(
            "the file changed after it was checked; run the check again".to_string(),
        ));
    }
    let now = crate::core::config::now_stamp() as i64;
    let route = upload_route(size_bytes)?;
    let upload_reservation = quota.reserve(now, QuotaEndpoint::FileUpload, true)?;
    let url_reservation = if route == UploadRoute::UploadUrl {
        Some(quota.reserve(now, QuotaEndpoint::UploadUrl, true)?)
    } else {
        None
    };
    let job = UploadJob {
        id: 0,
        sha256: sha256.to_string(),
        base_name: safe_base_name(path),
        size_bytes,
        consent_at,
        status: UploadStatus::Pending,
        analysis_id: None,
        upload_url_used: route == UploadRoute::UploadUrl,
        attempts: 1,
        created_at: now,
        updated_at: now,
        last_error: None,
    };
    let job_id = cache.insert_upload(&job)?;
    cache.update_upload(
        job_id,
        UploadStatus::Uploading,
        None,
        route == UploadRoute::UploadUrl,
        1,
        now,
        None,
    )?;
    let api_result = match client.upload_file(path, size_bytes) {
        Ok(result) => {
            quota.complete(upload_reservation, QuotaOutcome::Ok)?;
            if let Some(id) = url_reservation {
                quota.complete(id, QuotaOutcome::Ok)?;
            }
            result
        }
        Err(error) => {
            let outcome = quota_outcome(&error);
            quota.complete(upload_reservation, outcome)?;
            if let Some(id) = url_reservation {
                quota.complete(id, outcome)?;
            }
            let message = error.safe_message();
            cache.update_upload(
                job_id,
                UploadStatus::Failed,
                None,
                job.upload_url_used,
                1,
                crate::core::config::now_stamp() as i64,
                Some(&message),
            )?;
            return Err(error);
        }
    };
    cache.update_upload(
        job_id,
        UploadStatus::Analyzing,
        Some(&api_result.analysis_id),
        api_result.used_upload_url,
        1,
        crate::core::config::now_stamp() as i64,
        None,
    )?;
    if is_canceled(cancel) {
        mark_canceled(cache, job_id, &api_result)?;
        return Err(AppError::Canceled);
    }

    let mut summary = None;
    for delay in delays {
        if is_canceled(cancel) {
            mark_canceled(cache, job_id, &api_result)?;
            return Err(AppError::Canceled);
        }
        if *delay > 0 && !sleep_cancellable(Duration::from_secs(*delay), cancel) {
            mark_canceled(cache, job_id, &api_result)?;
            return Err(AppError::Canceled);
        }
        let reserve = match quota.reserve(
            crate::core::config::now_stamp() as i64,
            QuotaEndpoint::AnalysisPoll,
            true,
        ) {
            Ok(reserve) => reserve,
            // The upload already succeeded; stop polling and let the user check later.
            Err(AppError::QuotaDenied(_)) => break,
            Err(error) => return Err(error),
        };
        match client.poll_analysis(&api_result.analysis_id, sha256, size_bytes) {
            Ok(progress) => {
                quota.complete(reserve, QuotaOutcome::Ok)?;
                if progress.complete {
                    summary = progress.summary;
                    break;
                }
            }
            Err(error) => {
                quota.complete(reserve, quota_outcome(&error))?;
                let message = error.safe_message();
                cache.update_upload(
                    job_id,
                    UploadStatus::Failed,
                    Some(&api_result.analysis_id),
                    api_result.used_upload_url,
                    1,
                    crate::core::config::now_stamp() as i64,
                    Some(&message),
                )?;
                return Err(error);
            }
        }
    }

    let completed_at = crate::core::config::now_stamp() as i64;
    if let Some(summary) = &summary {
        let record = record_from_summary(summary, completed_at);
        cache.upsert_file(&record)?;
        mark_detection(
            cache,
            path,
            sha256,
            size_bytes,
            DetectionState::Reported,
            completed_at,
        )?;
        cache.update_upload(
            job_id,
            UploadStatus::Completed,
            Some(&api_result.analysis_id),
            api_result.used_upload_url,
            1,
            completed_at,
            None,
        )?;
    } else {
        // Poll cap or quota stopped polling: the job ends as failed; only the user retries.
        cache.update_upload(
            job_id,
            UploadStatus::Failed,
            Some(&api_result.analysis_id),
            api_result.used_upload_url,
            1,
            completed_at,
            Some("analysis did not finish in time; check the file again later"),
        )?;
    }
    Ok(UploadOutcome {
        path: path.to_path_buf(),
        sha256: sha256.to_string(),
        size_bytes,
        analysis_id: api_result.analysis_id,
        used_upload_url: api_result.used_upload_url,
        summary,
    })
}

fn mark_canceled(
    cache: &Cache,
    job_id: i64,
    result: &crate::api::client::UploadApiResult,
) -> Result<(), AppError> {
    cache.update_upload(
        job_id,
        UploadStatus::Canceled,
        Some(&result.analysis_id),
        result.used_upload_url,
        1,
        crate::core::config::now_stamp() as i64,
        Some("upload canceled"),
    )
}

fn is_canceled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed))
}

fn sleep_cancellable(duration: Duration, cancel: Option<&AtomicBool>) -> bool {
    let Some(cancel) = cancel else {
        sleep(duration);
        return true;
    };
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        sleep(
            Duration::from_millis(100)
                .min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
    }
    !cancel.load(Ordering::Relaxed)
}

fn record_from_summary(summary: &LookupSummary, checked_at: i64) -> FileRecord {
    FileRecord {
        sha256: summary.sha256.clone(),
        size_bytes: summary.size_bytes,
        verdict_state: VerdictState::Known,
        reputation: summary.reputation,
        malicious_count: summary.malicious,
        suspicious_count: summary.suspicious,
        undetected_count: summary.undetected,
        harmless_count: summary.harmless,
        timeout_count: summary.timeout,
        type_description: summary.type_description.clone(),
        meaningful_name: summary.meaningful_name.clone(),
        first_submission_at: summary.first_submission_at,
        last_analysis_at: summary.last_analysis_at,
        last_checked_at: checked_at,
        summary_json: None,
    }
}

fn mark_detection(
    cache: &Cache,
    path: &Path,
    sha256: &str,
    size_bytes: u64,
    state: DetectionState,
    now: i64,
) -> Result<(), AppError> {
    if let Some(detection) = cache.find_open_detection(path, sha256)? {
        cache.update_detection_state(detection.id, state, Some(now), None)?;
    } else {
        let id = cache.insert_detection(
            path,
            Some(sha256),
            &safe_base_name(path),
            Some(size_bytes),
            now,
            state,
        )?;
        let _ = id;
    }
    Ok(())
}

fn quota_outcome(error: &AppError) -> QuotaOutcome {
    match error {
        AppError::Api(ApiError::Network(_)) => QuotaOutcome::NetworkError,
        AppError::Api(ApiError::Canceled) | AppError::Canceled => QuotaOutcome::Canceled,
        _ => QuotaOutcome::HttpError,
    }
}
