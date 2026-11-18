use crate::mock_server::{MockResponse, TestEnv};
use vt_tray::core::types::LookupOutcome;

fn known_body(sha256: &str, name: &str) -> String {
    format!(
        r#"{{
            "data": {{
                "id": "{sha256}",
                "type": "file",
                "attributes": {{
                    "last_analysis_stats": {{
                        "malicious": 1,
                        "suspicious": 0,
                        "undetected": 60,
                        "harmless": 0,
                        "timeout": 0
                    }},
                    "meaningful_name": "{name}"
                }}
            }}
        }}"#
    )
}

#[test]
fn checks_an_unknown_file_without_uploading_it() {
    let env = TestEnv::new(vec![MockResponse::json(404, "{}")]);
    let path = env.file("unknown.bin", b"not known anywhere");
    let result = env.core().check_file(&path, true).unwrap();
    assert!(matches!(result.outcome, LookupOutcome::Unknown { .. }));
    assert!(!result.cached);
    let requests = env.server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert!(requests[0].path.starts_with("/files/"));
}

fn sha_of(contents: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe");
    std::fs::write(&probe, contents).unwrap();
    vt_tray::core::hash::sha256_file(&probe).unwrap().sha256
}

#[test]
fn returns_a_fresh_cache_hit_without_a_second_request() {
    let sha256 = sha_of(b"already known");
    let env = TestEnv::new(vec![MockResponse::json(
        200,
        &known_body(&sha256, "known.bin"),
    )]);
    let path = env.file("known.bin", b"already known");
    let core = env.core();
    let first = core.check_file(&path, true).unwrap();
    assert!(matches!(first.outcome, LookupOutcome::Known(_)));
    assert!(!first.cached);
    let second = core.check_file(&path, true).unwrap();
    assert!(second.cached);
    assert!(matches!(second.outcome, LookupOutcome::Known(_)));
    assert_eq!(env.server.requests().len(), 1);
}

#[test]
fn cached_result_survives_a_restart() {
    let sha256 = sha_of(b"restart me");
    let env = TestEnv::new(vec![MockResponse::json(200, &known_body(&sha256, "a.bin"))]);
    let path = env.file("a.bin", b"restart me");
    env.core().check_file(&path, true).unwrap();
    let reopened = env.reopen_cache();
    assert!(reopened.get_file(&sha256).unwrap().is_some());
}

#[test]
fn rejects_a_directory_before_a_request() {
    let env = TestEnv::new(vec![]);
    let error = env.core().check_file(env.dir.path(), true).unwrap_err();
    assert!(error.safe_message().contains("regular file"));
    assert!(env.server.requests().is_empty());
}
