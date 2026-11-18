use crate::mock_server::{MockResponse, MockServer};
use std::sync::Arc;
use vt_tray::api::client::{StaticKeyProvider, VtClient};
use vt_tray::core::cache::Cache;
use vt_tray::core::hash::sha256_file;
use vt_tray::core::quota::QuotaGate;
use vt_tray::core::types::UploadStatus;
use vt_tray::worker::upload::upload_with_delays;

#[test]
fn uploads_after_consent_and_stores_the_completed_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"hello").unwrap();
    let fingerprint = sha256_file(&path).unwrap();
    let upload_body = r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{}}}"#;
    let analysis_body = format!(
        r#"{{
            "data": {{
                "id": "analysis-1",
                "type": "analysis",
                "attributes": {{
                    "status": "completed",
                    "sha256": "{}",
                    "stats": {{
                        "malicious": 0,
                        "suspicious": 1,
                        "undetected": 60,
                        "harmless": 0,
                        "timeout": 0
                    }}
                }}
            }}
        }}"#,
        fingerprint.sha256
    );
    let server = MockServer::start(vec![
        MockResponse::json(200, upload_body),
        MockResponse::json(200, &analysis_body),
    ]);
    let keys = Arc::new(StaticKeyProvider::new(Some("test-key".to_string())));
    let client = VtClient::new(keys, server.base_url());
    let cache = Arc::new(Cache::in_memory().unwrap());
    let quota = QuotaGate::new(cache.clone());
    let outcome = upload_with_delays(
        &cache,
        &client,
        &quota,
        &path,
        &fingerprint.sha256,
        fingerprint.size_bytes,
        123,
        &[0],
    )
    .unwrap();
    let summary = outcome.summary.expect("completed analysis");
    assert_eq!(summary.suspicious, 1);
    let job = cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Completed);
    assert_eq!(job.consent_at, 123);
    let record = cache.get_file(&fingerprint.sha256).unwrap().unwrap();
    assert_eq!(record.malicious_count, 0);
    assert_eq!(record.suspicious_count, 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, "/files");
    assert_eq!(requests[1].path, "/analyses/analysis-1");
    assert_eq!(cache.count_quota_events(0).unwrap(), 2);
}

#[test]
fn refuses_to_upload_when_the_file_changed_after_consent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"hello").unwrap();
    let fingerprint = sha256_file(&path).unwrap();
    std::fs::write(&path, b"not hello anymore").unwrap();
    let server = MockServer::start(vec![]);
    let keys = Arc::new(StaticKeyProvider::new(Some("test-key".to_string())));
    let client = VtClient::new(keys, server.base_url());
    let cache = Arc::new(Cache::in_memory().unwrap());
    let quota = QuotaGate::new(cache.clone());
    let error = upload_with_delays(
        &cache,
        &client,
        &quota,
        &path,
        &fingerprint.sha256,
        fingerprint.size_bytes,
        123,
        &[0],
    )
    .unwrap_err();
    assert!(error.safe_message().contains("changed"));
    assert!(server.requests().is_empty());
}

#[test]
fn zero_byte_files_are_rejected_before_a_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.bin");
    std::fs::write(&path, b"").unwrap();
    let fingerprint = sha256_file(&path).unwrap();
    let server = MockServer::start(vec![]);
    let keys = Arc::new(StaticKeyProvider::new(Some("test-key".to_string())));
    let client = VtClient::new(keys, server.base_url());
    let cache = Arc::new(Cache::in_memory().unwrap());
    let quota = QuotaGate::new(cache.clone());
    let error = upload_with_delays(
        &cache,
        &client,
        &quota,
        &path,
        &fingerprint.sha256,
        0,
        123,
        &[0],
    )
    .unwrap_err();
    assert!(error.safe_message().contains("zero-byte"));
    assert!(server.requests().is_empty());
}
