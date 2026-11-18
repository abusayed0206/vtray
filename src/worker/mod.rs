pub mod upload;

use crate::api::client::VtClient;
use crate::core::cache::Cache;
use crate::core::candidates::is_candidate;
use crate::core::errors::{ApiError, AppError};
use crate::core::hash::{modified_ns, safe_base_name, sha256_file};
use crate::core::policy::file_is_reasonable_for_upload;
use crate::core::quota::QuotaGate;
use crate::core::types::{
    DetectionState, FileRecord, LookupOutcome, LookupSummary, PathRecord, QuotaEndpoint,
    QuotaOutcome, VerdictState,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const CACHE_FRESH_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub outcome: LookupOutcome,
    pub cached: bool,
    /// False for Explorer quick checks: an unknown file never gets an upload offer (FR-020).
    pub offer_upload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionResult {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub summary: Option<LookupSummary>,
    pub state: DetectionState,
    pub error: Option<String>,
    pub notify: bool,
}

pub struct WorkerCore {
    cache: Arc<Cache>,
    client: Arc<VtClient>,
    quota: Arc<QuotaGate>,
}

impl WorkerCore {
    pub fn new(cache: Arc<Cache>, client: Arc<VtClient>, quota: Arc<QuotaGate>) -> Self {
        Self {
            cache,
            client,
            quota,
        }
    }

    pub fn fingerprint_file(
        &self,
        path: &Path,
    ) -> Result<crate::core::hash::FileFingerprint, AppError> {
        if !file_is_reasonable_for_upload(path) {
            return Err(AppError::Validation(
                "the selected path is not a regular file".to_string(),
            ));
        }
        sha256_file(path)
    }

    pub fn check_file(&self, path: &Path, manual: bool) -> Result<CheckResult, AppError> {
        if !file_is_reasonable_for_upload(path) {
            return Err(AppError::Validation(
                "the selected path is not a regular file".to_string(),
            ));
        }
        let metadata = std::fs::metadata(path)?;
        let size_bytes = metadata.len();
        let mtime_ns = modified_ns(&metadata);
        let sha256 = match self.cache.get_path(path)? {
            Some(record) if record.size_bytes == size_bytes && record.mtime_ns == mtime_ns => {
                match record.sha256 {
                    Some(sha256) => sha256,
                    None => self.hash_and_cache(path, size_bytes, mtime_ns)?,
                }
            }
            _ => self.hash_and_cache(path, size_bytes, mtime_ns)?,
        };
        let now = crate::core::config::now_stamp() as i64;
        if let Some(record) = self.cache.get_file(&sha256)?
            && matches!(
                record.verdict_state,
                VerdictState::Known | VerdictState::Stale
            )
            && !record.is_stale(now, CACHE_FRESH_SECONDS)
        {
            return Ok(CheckResult {
                path: path.to_path_buf(),
                size_bytes,
                sha256,
                outcome: LookupOutcome::Known(summary_from_record(&record)),
                cached: true,
                offer_upload: false,
            });
        }
        let reservation = self.quota.reserve(now, QuotaEndpoint::FileLookup, manual)?;
        let outcome = match self.client.lookup(&sha256, size_bytes) {
            Ok(outcome) => {
                self.quota.complete(reservation, QuotaOutcome::Ok)?;
                outcome
            }
            Err(error) => {
                let _ = self.quota.complete(reservation, quota_outcome(&error));
                return Err(error);
            }
        };
        if let LookupOutcome::Known(summary) = &outcome {
            let record = record_from_summary(summary, now);
            self.cache.upsert_file(&record)?;
            mark_detection(
                &self.cache,
                path,
                &sha256,
                size_bytes,
                DetectionState::Checked,
                now,
            )?;
        } else {
            mark_detection(
                &self.cache,
                path,
                &sha256,
                size_bytes,
                DetectionState::Unknown,
                now,
            )?;
        }
        Ok(CheckResult {
            path: path.to_path_buf(),
            size_bytes,
            sha256,
            outcome,
            cached: false,
            offer_upload: false,
        })
    }

    pub fn detect_file(&self, path: &Path, auto_check: bool) -> Result<DetectionResult, AppError> {
        if !is_candidate(path) {
            return Err(AppError::Validation(
                "the event is not for a checkable file".to_string(),
            ));
        }
        let metadata = std::fs::metadata(path)?;
        let size_bytes = metadata.len();
        let mtime_ns = modified_ns(&metadata);
        let cached = match self.cache.get_path(path)? {
            Some(record) if record.size_bytes == size_bytes && record.mtime_ns == mtime_ns => {
                record.sha256
            }
            _ => None,
        };
        let sha256 = match cached {
            Some(sha256) => sha256,
            None => {
                // Second stability read after the debouncer; a later write raises a new event.
                if !crate::core::hash::is_stable(path, std::time::Duration::from_secs(1))? {
                    return Err(AppError::Validation(
                        "the file is still changing".to_string(),
                    ));
                }
                self.hash_and_cache(path, size_bytes, mtime_ns)?
            }
        };
        let now = crate::core::config::now_stamp() as i64;
        let existing = self.cache.find_open_detection(path, &sha256)?;
        let notify = existing
            .as_ref()
            .is_none_or(|detection| now.saturating_sub(detection.detected_at) >= 30);
        let detection_id = match existing {
            Some(detection) => detection.id,
            None => self.cache.insert_detection(
                path,
                Some(&sha256),
                &safe_base_name(path),
                Some(size_bytes),
                now,
                DetectionState::New,
            )?,
        };
        let mut summary = None;
        let mut state = DetectionState::New;
        let mut error_text = None;
        if auto_check {
            match self.cached_summary(&sha256, now) {
                Some(cached) => {
                    summary = Some(cached);
                    state = DetectionState::Checked;
                }
                None => match self.check_without_cached(&sha256, size_bytes, now, false) {
                    Ok(Some(known)) => {
                        summary = Some(known);
                        state = DetectionState::Checked;
                    }
                    Ok(None) => {
                        state = DetectionState::Unknown;
                    }
                    Err(AppError::Credential(_)) | Err(AppError::QuotaDenied(_)) => {}
                    Err(error) => {
                        state = DetectionState::Error;
                        error_text = Some(error.safe_message());
                    }
                },
            }
        }
        self.cache
            .update_detection_state(detection_id, state, Some(now), error_text.as_deref())?;
        Ok(DetectionResult {
            path: path.to_path_buf(),
            size_bytes,
            sha256,
            summary,
            state,
            error: error_text,
            notify,
        })
    }

    fn cached_summary(&self, sha256: &str, now: i64) -> Option<LookupSummary> {
        let record = self.cache.get_file(sha256).ok().flatten()?;
        if matches!(
            record.verdict_state,
            VerdictState::Known | VerdictState::Stale
        ) && !record.is_stale(now, CACHE_FRESH_SECONDS)
        {
            Some(summary_from_record(&record))
        } else {
            None
        }
    }

    fn check_without_cached(
        &self,
        sha256: &str,
        size_bytes: u64,
        now: i64,
        manual: bool,
    ) -> Result<Option<LookupSummary>, AppError> {
        let reservation = self.quota.reserve(now, QuotaEndpoint::FileLookup, manual)?;
        match self.client.lookup(sha256, size_bytes) {
            Ok(LookupOutcome::Known(summary)) => {
                self.quota.complete(reservation, QuotaOutcome::Ok)?;
                self.cache
                    .upsert_file(&record_from_summary(&summary, now))?;
                Ok(Some(summary))
            }
            Ok(LookupOutcome::Unknown { .. }) => {
                self.quota.complete(reservation, QuotaOutcome::Ok)?;
                Ok(None)
            }
            Err(error) => {
                let _ = self.quota.complete(reservation, quota_outcome(&error));
                Err(error)
            }
        }
    }

    fn hash_and_cache(
        &self,
        path: &Path,
        size_before: u64,
        mtime_before: i128,
    ) -> Result<String, AppError> {
        let fingerprint = sha256_file(path)?;
        if fingerprint.size_bytes != size_before || fingerprint.mtime_ns != mtime_before {
            return Err(AppError::Validation(
                "the file changed while it was being hashed; try again".to_string(),
            ));
        }
        self.cache.upsert_path(&PathRecord {
            path: path.to_path_buf(),
            size_bytes: fingerprint.size_bytes,
            mtime_ns: fingerprint.mtime_ns,
            sha256: Some(fingerprint.sha256.clone()),
            last_seen_at: crate::core::config::now_stamp() as i64,
        })?;
        Ok(fingerprint.sha256)
    }
}

pub enum WorkerCommand {
    Check {
        path: PathBuf,
        manual: bool,
        /// Menu checks may offer an upload; quick checks from Explorer never do.
        offer_upload: bool,
    },
    Fingerprint {
        path: PathBuf,
    },
    Detect {
        path: PathBuf,
        auto_check: bool,
    },
    Upload {
        path: PathBuf,
        sha256: String,
        size_bytes: u64,
        consent_at: i64,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    CheckReady(CheckResult),
    FingerprintReady {
        path: PathBuf,
        sha256: String,
        size_bytes: u64,
    },
    FingerprintFailed {
        path: PathBuf,
        message: String,
    },
    Detected(DetectionResult),
    CheckFailed {
        path: PathBuf,
        message: String,
        auth: bool,
        retry_after: Option<u64>,
    },
    UploadFinished(upload::UploadOutcome),
    UploadFailed {
        path: PathBuf,
        message: String,
        auth: bool,
        retry_after: Option<u64>,
    },
}

/// Queued commands beyond this are refused instead of blocking the UI thread.
pub const WORKER_QUEUE_CAPACITY: usize = 128;
/// Quit waits this long for the current task, then lets process exit end it.
const SHUTDOWN_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

pub struct Worker {
    commands: SyncSender<WorkerCommand>,
    events: Arc<Mutex<VecDeque<WorkerEvent>>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(
        cache: Arc<Cache>,
        client: Arc<VtClient>,
        quota: Arc<QuotaGate>,
        window: usize,
    ) -> Self {
        let (commands, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let worker_events = Arc::clone(&events);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let thread = thread::spawn(move || {
            run_worker(
                WorkerCore::new(cache, client, quota),
                receiver,
                worker_events,
                window,
                worker_cancel,
            );
        });
        Self {
            commands,
            events,
            cancel,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: WorkerCommand) -> Result<(), AppError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => AppError::Validation(
                    "VT Tray is busy with other files; try again shortly".into(),
                ),
                TrySendError::Disconnected(_) => {
                    AppError::Internal("the check worker has stopped".to_string())
                }
            })
    }

    pub fn drain_events(&self) -> Vec<WorkerEvent> {
        let mut events = self.events.lock().unwrap();
        events.drain(..).collect()
    }

    /// Stops the worker. Returns false if a task (such as an upload body in flight) did not
    /// stop within the wait; that thread is left to end with the process.
    pub fn shutdown(&mut self) -> bool {
        self.cancel.store(true, Ordering::Relaxed);
        // A full queue is fine: the worker checks `cancel` before running the next command.
        let _ = self.commands.try_send(WorkerCommand::Shutdown);
        let Some(thread) = self.thread.take() else {
            return true;
        };
        let deadline = std::time::Instant::now() + SHUTDOWN_WAIT;
        while !thread.is_finished() && std::time::Instant::now() < deadline {
            thread::sleep(std::time::Duration::from_millis(20));
        }
        if thread.is_finished() {
            let _ = thread.join();
            true
        } else {
            log::warn!("the worker was still busy at shutdown; exiting without waiting");
            false
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_worker(
    core: WorkerCore,
    receiver: Receiver<WorkerCommand>,
    events: Arc<Mutex<VecDeque<WorkerEvent>>>,
    window: usize,
    cancel: Arc<AtomicBool>,
) {
    while let Ok(command) = receiver.recv() {
        if cancel.load(Ordering::Relaxed) || matches!(command, WorkerCommand::Shutdown) {
            break;
        }
        log::info!("worker started: {}", describe_command(&command));
        let failure = failure_event(&command);
        // Supervision: a panic in one task becomes a failure event; the worker keeps serving.
        let event = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_command(&core, command, &cancel)
        })) {
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(_) => {
                log::error!("a worker task panicked; the worker keeps running");
                match failure {
                    Some(event) => event,
                    None => continue,
                }
            }
        };
        log::info!("worker finished: {}", describe_event(&event));
        if let Ok(mut queue) = events.lock() {
            if queue.len() >= 1000 {
                queue.pop_front();
            }
            queue.push_back(event);
        }
        if window != 0 {
            let hwnd = HWND(window as *mut core::ffi::c_void);
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    crate::app::window::WM_APP_EVENT,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
}

/// Log line for a command: kind and base name only (FR-013: no full paths, no secrets).
fn describe_command(command: &WorkerCommand) -> String {
    match command {
        WorkerCommand::Check { path, manual, .. } => {
            format!("check {} (manual: {manual})", safe_base_name(path))
        }
        WorkerCommand::Fingerprint { path } => format!("fingerprint {}", safe_base_name(path)),
        WorkerCommand::Detect { path, auto_check } => {
            format!("detect {} (auto check: {auto_check})", safe_base_name(path))
        }
        WorkerCommand::Upload {
            path, size_bytes, ..
        } => format!("upload {} ({size_bytes} bytes)", safe_base_name(path)),
        WorkerCommand::Shutdown => "shutdown".to_string(),
    }
}

fn describe_event(event: &WorkerEvent) -> String {
    match event {
        WorkerEvent::CheckReady(result) => format!(
            "check {}: {} ({})",
            safe_base_name(&result.path),
            match result.outcome {
                LookupOutcome::Known(_) => "known",
                LookupOutcome::Unknown { .. } => "unknown",
            },
            if result.cached { "cache" } else { "VirusTotal" }
        ),
        WorkerEvent::FingerprintReady { path, .. } => {
            format!("fingerprint {}: done", safe_base_name(path))
        }
        WorkerEvent::FingerprintFailed { path, message } => {
            format!("fingerprint {} failed: {message}", safe_base_name(path))
        }
        WorkerEvent::Detected(result) => {
            format!(
                "detect {}: {:?}",
                safe_base_name(&result.path),
                result.state
            )
        }
        WorkerEvent::CheckFailed { path, message, .. } => {
            format!("check {} failed: {message}", safe_base_name(path))
        }
        WorkerEvent::UploadFinished(outcome) => format!(
            "upload {}: {}",
            safe_base_name(&outcome.path),
            if outcome.summary.is_some() {
                "analysis completed"
            } else {
                "analysis still running"
            }
        ),
        WorkerEvent::UploadFailed { path, message, .. } => {
            format!("upload {} failed: {message}", safe_base_name(path))
        }
    }
}

/// Event reported if the task for `command` panics, so the UI never waits forever.
fn failure_event(command: &WorkerCommand) -> Option<WorkerEvent> {
    let message = "internal error; see the log folder".to_string();
    match command {
        WorkerCommand::Check { path, .. } => Some(WorkerEvent::CheckFailed {
            path: path.clone(),
            message,
            auth: false,
            retry_after: None,
        }),
        WorkerCommand::Fingerprint { path } => Some(WorkerEvent::FingerprintFailed {
            path: path.clone(),
            message,
        }),
        WorkerCommand::Upload { path, .. } => Some(WorkerEvent::UploadFailed {
            path: path.clone(),
            message,
            auth: false,
            retry_after: None,
        }),
        WorkerCommand::Detect { .. } | WorkerCommand::Shutdown => None,
    }
}

fn run_command(
    core: &WorkerCore,
    command: WorkerCommand,
    cancel: &AtomicBool,
) -> Option<WorkerEvent> {
    let event = match command {
        WorkerCommand::Check {
            path,
            manual,
            offer_upload,
        } => match core.check_file(&path, manual) {
            Ok(result) => WorkerEvent::CheckReady(CheckResult {
                offer_upload,
                ..result
            }),
            Err(error) => WorkerEvent::CheckFailed {
                path,
                message: failure_text(&error),
                auth: error.is_auth_error(),
                retry_after: retry_after(&error),
            },
        },
        WorkerCommand::Fingerprint { path } => match core.fingerprint_file(&path) {
            Ok(fingerprint) => WorkerEvent::FingerprintReady {
                path,
                sha256: fingerprint.sha256,
                size_bytes: fingerprint.size_bytes,
            },
            Err(error) => WorkerEvent::FingerprintFailed {
                path,
                message: failure_text(&error),
            },
        },
        WorkerCommand::Detect { path, auto_check } => match core.detect_file(&path, auto_check) {
            Ok(result) => WorkerEvent::Detected(result),
            Err(error) => {
                log::debug!(
                    "watcher candidate {} was ignored: {}",
                    safe_base_name(&path),
                    error.safe_message()
                );
                return None;
            }
        },
        WorkerCommand::Upload {
            path,
            sha256,
            size_bytes,
            consent_at,
        } => {
            match upload::upload_with_cancel(
                &core.cache,
                &core.client,
                &core.quota,
                &path,
                &sha256,
                size_bytes,
                consent_at,
                &upload::POLL_DELAYS_SECONDS,
                Some(cancel),
            ) {
                Ok(outcome) => WorkerEvent::UploadFinished(outcome),
                Err(error) => WorkerEvent::UploadFailed {
                    path,
                    message: failure_text(&error),
                    auth: error.is_auth_error(),
                    retry_after: retry_after(&error),
                },
            }
        }
        WorkerCommand::Shutdown => return None,
    };
    Some(event)
}

fn retry_after(error: &AppError) -> Option<u64> {
    error
        .retry_after()
        .or_else(|| error.is_rate_limited().then_some(60))
}

fn summary_from_record(record: &FileRecord) -> LookupSummary {
    LookupSummary {
        sha256: record.sha256.clone(),
        size_bytes: record.size_bytes,
        malicious: record.malicious_count,
        suspicious: record.suspicious_count,
        undetected: record.undetected_count,
        harmless: record.harmless_count,
        timeout: record.timeout_count,
        reputation: record.reputation,
        type_description: record.type_description.clone(),
        meaningful_name: record.meaningful_name.clone(),
        first_submission_at: record.first_submission_at,
        last_analysis_at: record.last_analysis_at,
    }
}

pub(crate) fn record_from_summary(summary: &LookupSummary, checked_at: i64) -> FileRecord {
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

pub(crate) fn mark_detection(
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
        cache.insert_detection(
            path,
            Some(sha256),
            &safe_base_name(path),
            Some(size_bytes),
            now,
            state,
        )?;
    }
    Ok(())
}

pub(crate) fn quota_outcome(error: &AppError) -> QuotaOutcome {
    match error {
        AppError::Api(ApiError::Network(_)) => QuotaOutcome::NetworkError,
        AppError::Api(ApiError::Canceled) | AppError::Canceled => QuotaOutcome::Canceled,
        _ => QuotaOutcome::HttpError,
    }
}

/// Plain-language text for the user; the technical detail goes to the log only.
fn failure_text(error: &AppError) -> String {
    log::warn!("task failed: {}", error.safe_message());
    crate::ui::strings::error_message(error)
}
