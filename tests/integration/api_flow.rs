use crate::mock_server::{MockResponse, MockServer};
use std::sync::Arc;
use vt_tray::api::client::{StaticKeyProvider, VtClient};
use vt_tray::core::errors::{ApiError, AppError};
use vt_tray::core::types::LookupOutcome;

fn client(server: &MockServer, key: &str) -> VtClient {
    VtClient::new(
        Arc::new(StaticKeyProvider::new(Some(key.to_string()))),
        server.base_url(),
    )
}

#[test]
fn parses_known_file_without_uploading() {
    let body = r#"{
        "data": {
            "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "type": "file",
            "attributes": {
                "last_analysis_stats": {
                    "malicious": 2,
                    "suspicious": 1,
                    "undetected": 60,
                    "harmless": 0,
                    "timeout": 0
                },
                "reputation": 12,
                "type_description": "PE32 executable",
                "meaningful_name": "sample.exe",
                "first_submission_date": 100,
                "last_analysis_date": 200
            }
        }
    }"#;
    let server = MockServer::start(vec![MockResponse::json(200, body)]);
    let client = client(&server, "test-key");
    let sha = "a".repeat(64);
    let outcome = client.lookup(&sha, 1234).unwrap();
    let LookupOutcome::Known(summary) = outcome else {
        panic!("expected a known result");
    };
    assert_eq!(summary.sha256, sha);
    assert_eq!(summary.size_bytes, 1234);
    assert_eq!(summary.flagged(), 3);
    assert_eq!(summary.undetected, 60);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, format!("/files/{sha}"));
    assert_eq!(requests[0].headers.get("x-apikey").unwrap(), "test-key");
}

#[test]
fn treats_404_as_unknown() {
    let server = MockServer::start(vec![MockResponse::json(
        404,
        r#"{"error":{"code":"NotFoundError"}}"#,
    )]);
    let client = client(&server, "test-key");
    let sha = "b".repeat(64);
    let outcome = client.lookup(&sha, 1).unwrap();
    assert_eq!(outcome, LookupOutcome::Unknown { sha256: sha });
}

#[test]
fn uploads_direct_multipart_with_exact_length() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"hello").unwrap();
    let response = r#"{"data":{"id":"analysis-direct","type":"analysis","attributes":{}}}"#;
    let server = MockServer::start(vec![MockResponse::json(200, response)]);
    let client = client(&server, "test-key");
    let result = client.upload_file(&path, 5).unwrap();
    assert_eq!(result.analysis_id, "analysis-direct");
    assert!(!result.used_upload_url);
    let sent = client.upload_progress();
    assert_eq!(sent.load(std::sync::atomic::Ordering::Relaxed), 5);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/files");
    let content_type = requests[0].headers.get("content-type").unwrap();
    let boundary = content_type.split("boundary=").nth(1).unwrap();
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.starts_with(&format!("--{boundary}\r\n")));
    assert!(body.contains("filename=\"sample.txt\""));
    assert!(body.ends_with(&format!("\r\n--{boundary}--\r\n")));
    assert_eq!(
        requests[0].headers.get("content-length").unwrap(),
        &requests[0].body.len().to_string()
    );
}

#[test]
fn uses_upload_url_for_large_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.bin");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(32 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let server = MockServer::start_with(|base| {
        vec![
            MockResponse::json(200, &format!(r#"{{"data":"{base}/upload-target"}}"#)),
            MockResponse::json(
                200,
                r#"{"data":{"id":"analysis-large","type":"analysis","attributes":{}}}"#,
            ),
        ]
    });
    let client = client(&server, "test-key");
    let result = client.upload_file(&path, 32 * 1024 * 1024 + 1).unwrap();
    assert_eq!(result.analysis_id, "analysis-large");
    assert!(result.used_upload_url);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, "/files/upload_url");
    assert!(requests[1].path.contains("/upload-target"));
}

#[test]
fn maps_rate_limits_and_retry_after() {
    let server = MockServer::start(vec![MockResponse {
        status: 429,
        body: br#"{"error":{"code":"QuotaExceededError"}}"#.to_vec(),
        headers: vec![("Retry-After".to_string(), "60".to_string())],
    }]);
    let client = client(&server, "test-key");
    let error = client.lookup(&"c".repeat(64), 1).unwrap_err();
    assert!(matches!(
        error,
        AppError::Api(ApiError::RateLimited {
            retry_after: Some(60)
        })
    ));
}

#[test]
fn maps_authentication_failures() {
    let server = MockServer::start(vec![MockResponse::json(401, "{}")]);
    let client = client(&server, "bad-key");
    let error = client.lookup(&"d".repeat(64), 1).unwrap_err();
    assert!(matches!(error, AppError::Api(ApiError::Unauthorized)));
}

#[test]
fn requests_rescan_without_uploading_content() {
    let server = MockServer::start(vec![MockResponse::json(
        200,
        r#"{"data":{"id":"analysis-rescan","type":"analysis","attributes":{}}}"#,
    )]);
    let client = client(&server, "test-key");
    let sha = "e".repeat(64);
    let result = client.rescan(&sha).unwrap();
    assert_eq!(result.analysis_id, "analysis-rescan");
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, format!("/files/{sha}/analyse"));
    assert!(requests[0].body.is_empty());
}

#[test]
fn zero_byte_upload_is_refused_before_any_request() {
    let server = MockServer::start(vec![]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.bin");
    std::fs::write(&path, b"").unwrap();
    let error = client(&server, "test-key")
        .upload_file(&path, 0)
        .unwrap_err();
    assert!(error.safe_message().contains("zero-byte"));
    assert!(server.requests().is_empty());
}

#[test]
fn upload_above_650_mib_is_refused_before_any_request() {
    let server = MockServer::start(vec![]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.iso");
    let size = 650 * 1024 * 1024 + 1;
    // Sparse file: no real disk space used.
    std::fs::File::create(&path).unwrap().set_len(size).unwrap();
    let error = client(&server, "test-key")
        .upload_file(&path, size)
        .unwrap_err();
    assert!(error.safe_message().contains("650"));
    assert!(server.requests().is_empty());
}

#[test]
fn analysis_poll_distinguishes_queued_from_completed() {
    let sha256 = "b".repeat(64);
    let completed = format!(
        r#"{{"data":{{"id":"an-1","type":"analysis","attributes":{{"status":"completed","sha256":"{sha256}","stats":{{"malicious":4,"suspicious":1,"undetected":50,"harmless":0,"timeout":0}}}}}}}}"#
    );
    let server = MockServer::start(vec![
        MockResponse::json(
            200,
            r#"{"data":{"id":"an-1","type":"analysis","attributes":{"status":"queued"}}}"#,
        ),
        MockResponse::json(200, &completed),
    ]);
    let client = client(&server, "test-key");
    let queued = client.poll_analysis("an-1", &sha256, 10).unwrap();
    assert!(!queued.complete);
    assert!(queued.summary.is_none());
    let done = client.poll_analysis("an-1", &sha256, 10).unwrap();
    assert!(done.complete);
    let summary = done.summary.unwrap();
    assert_eq!(summary.sha256, sha256);
    assert_eq!((summary.malicious, summary.suspicious), (4, 1));
    let requests = server.requests();
    assert!(
        requests
            .iter()
            .all(|r| r.method == "GET" && r.path == "/analyses/an-1")
    );
}

fn queued_upload_env() -> crate::mock_server::TestEnv {
    let queued = r#"{"data":{"id":"an-1","type":"analysis","attributes":{"status":"queued"}}}"#;
    let mut responses = vec![MockResponse::json(
        200,
        r#"{"data":{"id":"an-1","type":"analysis","attributes":{}}}"#,
    )];
    responses.extend((0..10).map(|_| MockResponse::json(200, queued)));
    crate::mock_server::TestEnv::new(responses)
}

#[test]
fn polling_ends_after_the_schedule_and_marks_the_job_failed() {
    use vt_tray::core::types::UploadStatus;
    use vt_tray::worker::upload::{POLL_DELAYS_SECONDS, upload_with_delays};

    // Production polls six times; the minute quota (4 requests) allows the upload plus
    // three immediate polls here, so this test uses a three-step schedule.
    assert_eq!(POLL_DELAYS_SECONDS.len(), 6);
    let env = queued_upload_env();
    let path = env.file("slow.exe", b"MZ slow analysis");
    let fingerprint = vt_tray::core::hash::sha256_file(&path).unwrap();
    let outcome = upload_with_delays(
        &env.cache,
        &env.client,
        &env.quota,
        &path,
        &fingerprint.sha256,
        fingerprint.size_bytes,
        1,
        &[0, 0, 0],
    )
    .unwrap();
    assert!(outcome.summary.is_none());
    assert_eq!(env.server.requests().len(), 4);
    let job = env.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Failed);
    assert!(env.cache.resumable_uploads().unwrap().is_empty());
}

#[test]
fn quota_denial_during_polling_keeps_the_upload_and_asks_to_check_later() {
    use vt_tray::core::types::UploadStatus;
    use vt_tray::worker::upload::{POLL_DELAYS_SECONDS, upload_with_delays};

    let env = queued_upload_env();
    let path = env.file("slow.exe", b"MZ slow analysis");
    let fingerprint = vt_tray::core::hash::sha256_file(&path).unwrap();
    // Six immediate polls: the 4/minute quota refuses the fourth poll.
    let outcome = upload_with_delays(
        &env.cache,
        &env.client,
        &env.quota,
        &path,
        &fingerprint.sha256,
        fingerprint.size_bytes,
        1,
        &[0; POLL_DELAYS_SECONDS.len()],
    )
    .expect("a quota stop after a successful upload is not an upload error");
    assert!(outcome.summary.is_none());
    assert_eq!(env.server.requests().len(), 4, "no request past the quota");
    let job = env.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Failed);
    assert!(job.last_error.unwrap().contains("again later"));
}
