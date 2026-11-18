use std::path::PathBuf;
use vt_tray::core::cache::Cache;
use vt_tray::core::types::{
    DetectionState, FileRecord, PathRecord, QuotaEndpoint, QuotaOutcome, UploadJob, UploadStatus,
    VerdictState,
};

fn sample_file(sha: &str, now: i64) -> FileRecord {
    FileRecord {
        sha256: sha.to_string(),
        size_bytes: 123,
        verdict_state: VerdictState::Known,
        reputation: Some(5),
        malicious_count: 1,
        suspicious_count: 0,
        undetected_count: 60,
        harmless_count: 0,
        timeout_count: 0,
        type_description: Some("PE32".to_string()),
        meaningful_name: Some("sample.exe".to_string()),
        first_submission_at: Some(now - 100),
        last_analysis_at: Some(now - 10),
        last_checked_at: now,
        summary_json: None,
    }
}

#[test]
fn upserts_and_reads_files() {
    let cache = Cache::in_memory().unwrap();
    let record = sample_file("a".repeat(64).as_str(), 1000);
    cache.upsert_file(&record).unwrap();
    assert_eq!(cache.get_file(&record.sha256).unwrap(), Some(record));
    assert_eq!(cache.file_count().unwrap(), 1);
}

#[test]
fn caches_paths_and_invalidates_by_metadata() {
    let cache = Cache::in_memory().unwrap();
    let path = PathBuf::from("C:\\temp\\sample.exe");
    let record = PathRecord {
        path: path.clone(),
        size_bytes: 10,
        mtime_ns: 20,
        sha256: Some("b".repeat(64)),
        last_seen_at: 100,
    };
    cache.upsert_path(&record).unwrap();
    let loaded = cache.get_path(&path).unwrap().unwrap();
    assert_eq!(loaded.size_bytes, 10);
    assert_eq!(loaded.mtime_ns, 20);
    cache.delete_path(&path).unwrap();
    assert!(cache.get_path(&path).unwrap().is_none());
}

#[test]
fn stores_and_updates_detections() {
    let cache = Cache::in_memory().unwrap();
    let path = PathBuf::from("C:\\temp\\sample.exe");
    let sha = "c".repeat(64);
    let id = cache
        .insert_detection(
            &path,
            Some(&sha),
            "sample.exe",
            Some(10),
            100,
            DetectionState::New,
        )
        .unwrap();
    let open = cache.find_open_detection(&path, &sha).unwrap().unwrap();
    assert_eq!(open.id, id);
    cache
        .update_detection_state(id, DetectionState::Dismissed, Some(101), None)
        .unwrap();
    assert!(cache.find_open_detection(&path, &sha).unwrap().is_none());
}

#[test]
fn stores_upload_jobs_and_resumes_open_ones() {
    let cache = Cache::in_memory().unwrap();
    let job = UploadJob {
        id: 0,
        sha256: "d".repeat(64),
        base_name: "sample.txt".to_string(),
        size_bytes: 10,
        consent_at: 100,
        status: UploadStatus::Pending,
        analysis_id: None,
        upload_url_used: false,
        attempts: 0,
        created_at: 100,
        updated_at: 100,
        last_error: None,
    };
    let id = cache.insert_upload(&job).unwrap();
    assert_eq!(cache.resumable_uploads().unwrap().len(), 1);
    cache
        .update_upload(
            id,
            UploadStatus::Completed,
            Some("analysis"),
            false,
            1,
            200,
            None,
        )
        .unwrap();
    assert!(cache.resumable_uploads().unwrap().is_empty());
    assert_eq!(
        cache.get_upload(id).unwrap().unwrap().status,
        UploadStatus::Completed
    );
}

#[test]
fn counts_quota_events() {
    let cache = Cache::in_memory().unwrap();
    cache
        .insert_quota_event(100, QuotaEndpoint::FileLookup, QuotaOutcome::Ok)
        .unwrap();
    cache
        .insert_quota_event(101, QuotaEndpoint::AnalysisPoll, QuotaOutcome::Canceled)
        .unwrap();
    assert_eq!(cache.count_quota_events(99).unwrap(), 1);
    assert_eq!(cache.count_quota_events(102).unwrap(), 0);
}

#[test]
fn prunes_old_rows_and_keeps_recent_ones() {
    let cache = Cache::in_memory().unwrap();
    let old = sample_file(&"e".repeat(64), 1000);
    cache.upsert_file(&old).unwrap();
    cache
        .upsert_path(&PathRecord {
            path: PathBuf::from("C:\\temp\\old.exe"),
            size_bytes: 1,
            mtime_ns: 1,
            sha256: Some(old.sha256.clone()),
            last_seen_at: 1000,
        })
        .unwrap();
    cache.prune(1000 + 40 * 86_400, 30).unwrap();
    assert_eq!(cache.file_count().unwrap(), 0);
}
