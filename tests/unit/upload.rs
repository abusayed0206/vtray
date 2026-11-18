use crate::mock_server::{MockResponse, MockServer};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use vt_tray::api::client::{StaticKeyProvider, VtClient};
use vt_tray::core::cache::Cache;
use vt_tray::core::hash::sha256_file;
use vt_tray::core::quota::QuotaGate;
use vt_tray::core::types::UploadStatus;
use vt_tray::worker::upload::upload_with_cancel;

const UPLOADED: &str = r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{}}}"#;
const QUEUED: &str =
    r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{"status":"queued"}}}"#;

struct Rig {
    _dir: tempfile::TempDir,
    path: PathBuf,
    sha256: String,
    size: u64,
    server: MockServer,
    client: VtClient,
    cache: Arc<Cache>,
    quota: QuotaGate,
}

fn rig(responses: Vec<MockResponse>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"hello").unwrap();
    let fingerprint = sha256_file(&path).unwrap();
    let server = MockServer::start(responses);
    let keys = Arc::new(StaticKeyProvider::new(Some("test-key".to_string())));
    let client = VtClient::new(keys, server.base_url());
    let cache = Arc::new(Cache::in_memory().unwrap());
    let quota = QuotaGate::new(cache.clone());
    Rig {
        _dir: dir,
        path,
        sha256: fingerprint.sha256,
        size: fingerprint.size_bytes,
        server,
        client,
        cache,
        quota,
    }
}

fn run(rig: &Rig, delays: &[u64], cancel: Option<&AtomicBool>) -> Result<(), String> {
    upload_with_cancel(
        &rig.cache,
        &rig.client,
        &rig.quota,
        &rig.path,
        &rig.sha256,
        rig.size,
        123,
        delays,
        cancel,
    )
    .map(|_| ())
    .map_err(|error| error.safe_message())
}

#[test]
fn upload_without_recorded_consent_sends_nothing() {
    let rig = rig(vec![MockResponse::json(200, UPLOADED)]);
    let error = upload_with_cancel(
        &rig.cache,
        &rig.client,
        &rig.quota,
        &rig.path,
        &rig.sha256,
        rig.size,
        0, // no consent time
        &[0],
        None,
    )
    .unwrap_err();
    assert!(error.safe_message().contains("consent"));
    assert!(rig.server.requests().is_empty());
    assert!(rig.cache.get_upload(1).unwrap().is_none());
    assert_eq!(rig.cache.count_quota_events(0).unwrap(), 0);
}

#[test]
fn failed_transfer_marks_the_job_failed_and_is_not_resumed() {
    let rig = rig(vec![MockResponse::json(500, "{}")]);
    assert!(run(&rig, &[0], None).is_err());
    let job = rig.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Failed);
    assert!(job.last_error.is_some());
    assert_eq!(rig.server.requests().len(), 1);
    assert!(rig.cache.resumable_uploads().unwrap().is_empty());
}

#[test]
fn cancel_before_polling_marks_canceled_and_sends_no_poll() {
    let rig = rig(vec![MockResponse::json(200, UPLOADED)]);
    let cancel = AtomicBool::new(true);
    let error = run(&rig, &[0, 0], Some(&cancel)).unwrap_err();
    assert!(error.contains("canceled"));
    let job = rig.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Canceled);
    assert_eq!(rig.server.requests().len(), 1);
    assert!(rig.cache.resumable_uploads().unwrap().is_empty());
}

#[test]
fn poll_cap_marks_the_job_failed_with_a_retryable_reason() {
    let rig = rig(vec![
        MockResponse::json(200, UPLOADED),
        MockResponse::json(200, QUEUED),
        MockResponse::json(200, QUEUED),
    ]);
    run(&rig, &[0, 0], None).unwrap();
    let job = rig.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Failed);
    assert!(job.last_error.unwrap().contains("again later"));
    assert_eq!(rig.server.requests().len(), 3);
    assert!(rig.cache.resumable_uploads().unwrap().is_empty());
}

#[test]
fn completed_job_records_consent_and_analysis_id() {
    // SHA-256 of b"hello", the content every rig writes.
    let completed = r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{"status":"completed","sha256":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","stats":{"malicious":0,"suspicious":0,"undetected":1,"harmless":0,"timeout":0}}}}"#;
    let rig = rig(vec![
        MockResponse::json(200, UPLOADED),
        MockResponse::json(200, completed),
    ]);
    run(&rig, &[0], None).unwrap();
    let job = rig.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Completed);
    assert_eq!(job.consent_at, 123);
    assert_eq!(job.analysis_id.as_deref(), Some("analysis-1"));
}
