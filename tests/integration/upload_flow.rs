//! Full manual flow: check (unknown) -> consented upload -> analysis poll -> cached re-check.
use crate::mock_server::{MockResponse, TestEnv};
use std::path::Path;
use std::sync::atomic::Ordering;
use vt_tray::core::hash::sha256_file;
use vt_tray::core::types::{LookupOutcome, UploadStatus};
use vt_tray::worker::upload::upload_with_delays;

const NOT_FOUND: &str = r#"{"error":{"code":"NotFoundError","message":"not found"}}"#;

fn completed(sha256: &str) -> String {
    format!(
        r#"{{"data":{{"id":"analysis-1","type":"analysis","attributes":{{"status":"completed","sha256":"{sha256}","stats":{{"malicious":2,"suspicious":0,"undetected":60,"harmless":0,"timeout":0}}}}}}}}"#
    )
}

fn run_flow(flow: &TestEnv, path: &Path) -> String {
    // 1. Manual check: unknown, nothing uploaded.
    let check = flow.core().check_file(path, true).unwrap();
    assert!(matches!(check.outcome, LookupOutcome::Unknown { .. }));
    assert!(!check.cached);
    let requests = flow.server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, format!("/files/{}", check.sha256));

    // 2. User consents (the app shows the dialog; consent time is recorded) and uploads.
    let outcome = upload_with_delays(
        &flow.cache,
        &flow.client,
        &flow.quota,
        path,
        &check.sha256,
        check.size_bytes,
        1_700_000_000,
        &[0],
    )
    .unwrap();
    let summary = outcome.summary.expect("analysis completed");
    assert_eq!(summary.malicious, 2);
    let job = flow.cache.get_upload(1).unwrap().unwrap();
    assert_eq!(job.status, UploadStatus::Completed);
    assert_eq!(job.consent_at, 1_700_000_000);
    assert_eq!(
        flow.client.upload_progress().load(Ordering::Relaxed),
        check.size_bytes
    );

    // 3. Re-check is answered from the cache with the uploaded result and no new request.
    let before = flow.server.requests().len();
    let again = flow.core().check_file(path, true).unwrap();
    assert!(again.cached);
    match again.outcome {
        LookupOutcome::Known(known) => assert_eq!(known.malicious, 2),
        other => panic!("expected a known result, got {other:?}"),
    }
    assert_eq!(flow.server.requests().len(), before);

    // The original file is untouched.
    assert_eq!(sha256_file(path).unwrap().sha256, check.sha256);
    check.sha256
}

#[test]
fn small_file_check_upload_poll_and_cached_recheck() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool.exe");
    std::fs::write(&path, b"MZ benign test payload").unwrap();
    let sha256 = sha256_file(&path).unwrap().sha256;
    let flow = TestEnv::with(|_| {
        vec![
            MockResponse::json(404, NOT_FOUND),
            MockResponse::json(
                200,
                r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{}}}"#,
            ),
            MockResponse::json(200, &completed(&sha256)),
        ]
    });
    run_flow(&flow, &path);
    let requests = flow.server.requests();
    let paths: Vec<_> = requests.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths[1..], ["/files", "/analyses/analysis-1"]);
    let body = String::from_utf8_lossy(&requests[1].body);
    assert!(body.contains("filename=\"tool.exe\""));
    assert!(!body.contains(&dir.path().to_string_lossy().into_owned()));
    // lookup + upload + one poll
    assert_eq!(flow.cache.count_quota_events(0).unwrap(), 3);
}

#[test]
fn large_file_uses_upload_url_route_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("installer.msi");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(32 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let sha256 = sha256_file(&path).unwrap().sha256;
    let flow = TestEnv::with(|base| {
        vec![
            MockResponse::json(404, NOT_FOUND),
            MockResponse::json(200, &format!(r#"{{"data":"{base}/upload-target"}}"#)),
            MockResponse::json(
                200,
                r#"{"data":{"id":"analysis-1","type":"analysis","attributes":{}}}"#,
            ),
            MockResponse::json(200, &completed(&sha256)),
        ]
    });
    run_flow(&flow, &path);
    let requests = flow.server.requests();
    assert_eq!(requests[1].path, "/files/upload_url");
    assert!(requests[2].path.contains("/upload-target"));
    assert_eq!(requests[3].path, "/analyses/analysis-1");
    assert!(flow.cache.get_upload(1).unwrap().unwrap().upload_url_used);
    // lookup + upload_url + upload + one poll
    assert_eq!(flow.cache.count_quota_events(0).unwrap(), 4);
}
