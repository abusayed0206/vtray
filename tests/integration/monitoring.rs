use crate::mock_server::{MockResponse, TestEnv};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vt_tray::core::hash::sha256_file;
use vt_tray::core::settings::WatchedFolder;
use vt_tray::core::types::DetectionState;
use vt_tray::watcher::WatcherService;

fn folder(path: &Path) -> WatchedFolder {
    WatchedFolder {
        path: path.to_path_buf(),
        enabled: true,
        added_at: 0,
        last_checked_at: None,
    }
}

/// Drains the watcher until `target` shows up or `timeout` passes; returns everything seen.
fn drain_until(watcher: &WatcherService, target: &Path, timeout: Duration) -> Vec<PathBuf> {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        seen.extend(watcher.drain());
        if seen.iter().any(|path| path == target) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    seen
}

fn canonical_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    (dir, root)
}

#[test]
fn watches_new_executables_within_five_seconds_and_ignores_partial_files() {
    let (_dir, root) = canonical_tempdir();
    let watcher = WatcherService::start(&[folder(&root)], 0).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let candidate = root.join("dropper.exe");
    let partial = root.join("download.exe.part");
    let written = Instant::now();
    std::fs::write(&candidate, b"MZ-not-a-real-pe").unwrap();
    std::fs::write(&partial, b"MZ-not-a-real-pe").unwrap();
    let seen = drain_until(&watcher, &candidate, Duration::from_secs(8));
    assert!(seen.contains(&candidate));
    // SC-003: the watcher hands the file over within five seconds (2 s debounce included).
    assert!(written.elapsed() < Duration::from_secs(5));
    assert!(!seen.contains(&partial));
}

#[test]
fn repeated_writes_to_one_file_are_reported_once() {
    let (_dir, root) = canonical_tempdir();
    let watcher = WatcherService::start(&[folder(&root)], 0).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let candidate = root.join("setup.exe");
    for chunk in [&b"MZ"[..], b"part two", b"part three"] {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&candidate)
            .unwrap();
        file.write_all(chunk).unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut seen = drain_until(&watcher, &candidate, Duration::from_secs(8));
    std::thread::sleep(Duration::from_millis(2500));
    seen.extend(watcher.drain());
    assert_eq!(seen.iter().filter(|path| **path == candidate).count(), 1);
}

#[test]
fn a_removed_folder_is_no_longer_watched() {
    let (_dir, root) = canonical_tempdir();
    let (kept, removed) = (root.join("kept"), root.join("removed"));
    std::fs::create_dir(&kept).unwrap();
    std::fs::create_dir(&removed).unwrap();
    // The app restarts the watcher without the removed folder.
    drop(WatcherService::start(&[folder(&kept), folder(&removed)], 0).unwrap());
    let watcher = WatcherService::start(&[folder(&kept)], 0).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let ignored = removed.join("ignored.exe");
    let noticed = kept.join("noticed.exe");
    std::fs::write(&ignored, b"MZ").unwrap();
    std::fs::write(&noticed, b"MZ").unwrap();
    let seen = drain_until(&watcher, &noticed, Duration::from_secs(8));
    assert!(seen.contains(&noticed));
    assert!(!seen.contains(&ignored));
}

#[test]
fn a_deleted_folder_is_watched_again_when_it_returns() {
    let (_dir, root) = canonical_tempdir();
    let watched = root.join("usb");
    std::fs::create_dir(&watched).unwrap();
    let watcher =
        WatcherService::start_with(&[folder(&watched)], 0, Duration::from_millis(300)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    std::fs::remove_dir_all(&watched).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watcher.unavailable().contains(&watched) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(watcher.unavailable(), vec![watched.clone()]);
    std::fs::create_dir(&watched).unwrap();
    let returned = watched.join("after.exe");
    std::fs::write(&returned, b"MZ").unwrap();
    let seen = drain_until(&watcher, &returned, Duration::from_secs(8));
    assert!(seen.contains(&returned));
    assert!(watcher.unavailable().is_empty());
}

#[test]
fn a_folder_missing_at_startup_is_picked_up_later() {
    let (_dir, root) = canonical_tempdir();
    let later = root.join("not-yet");
    let watcher =
        WatcherService::start_with(&[folder(&later)], 0, Duration::from_millis(300)).unwrap();
    assert_eq!(watcher.unavailable(), vec![later.clone()]);
    std::fs::create_dir(&later).unwrap();
    let file = later.join("arrived.exe");
    std::fs::write(&file, b"MZ").unwrap();
    let seen = drain_until(&watcher, &file, Duration::from_secs(15));
    assert!(
        seen.contains(&file),
        "not detected; still unavailable: {:?}; seen: {seen:?}",
        watcher.unavailable()
    );
}

#[test]
fn resume_scan_finds_candidates_added_while_paused() {
    let (_dir, root) = canonical_tempdir();
    std::fs::write(root.join("old.exe"), b"MZ").unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let paused_at = std::time::SystemTime::now();
    std::thread::sleep(Duration::from_millis(50));
    std::fs::create_dir(root.join("sub")).unwrap();
    let new = root.join("sub").join("new.exe");
    std::fs::write(&new, b"MZ").unwrap();
    std::fs::write(root.join("notes.txt"), b"x").unwrap();
    let found = vt_tray::watcher::changed_since(&[root], paused_at, 100);
    assert_eq!(found, vec![new]);
}

#[test]
fn deleted_and_locked_files_are_skipped_without_a_request() {
    let env = TestEnv::new(vec![]);
    let core = env.core();
    let gone = env.file("gone.exe", b"MZ gone");
    std::fs::remove_file(&gone).unwrap();
    assert!(core.detect_file(&gone, true).is_err());

    let locked = env.file("locked.exe", b"MZ locked");
    let _exclusive = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap()
    };
    assert!(core.detect_file(&locked, true).is_err());
    assert!(env.server.requests().is_empty());
    assert!(env.cache.recent_detections(10).unwrap().is_empty());
}

#[test]
fn watched_folders_survive_a_restart() {
    let env = TestEnv::new(vec![]);
    let folder = env.dir.path().join("downloads");
    std::fs::create_dir(&folder).unwrap();
    let mut preferences = env.config.load().unwrap();
    assert!(preferences.watched_folders.is_empty());
    preferences.add_folder(folder.clone(), 1).unwrap();
    env.config.save(&preferences).unwrap();
    let reloaded = env.config.load().unwrap();
    assert_eq!(reloaded.watched_folders.len(), 1);
    assert!(reloaded.watched_folders[0].enabled);
    assert_eq!(
        reloaded.watched_folders[0].path.file_name(),
        folder.file_name()
    );
}

#[test]
fn records_a_local_detection_without_network_access() {
    let env = TestEnv::new(vec![]);
    let path = env.file("local.exe", b"MZ-local");
    let core = env.core();
    let result = core.detect_file(&path, false).unwrap();
    assert_eq!(result.state, DetectionState::New);
    assert!(result.summary.is_none());
    assert!(result.notify);
    assert_eq!(env.cache.recent_detections(10).unwrap().len(), 1);
    assert!(env.server.requests().is_empty());
    let duplicate = core.detect_file(&path, false).unwrap();
    assert!(!duplicate.notify);
    assert_eq!(env.cache.recent_detections(10).unwrap().len(), 1);
}

#[test]
fn auto_check_stores_a_known_result_only_when_opted_in() {
    let probe_dir = tempfile::tempdir().unwrap();
    let probe = probe_dir.path().join("auto.exe");
    std::fs::write(&probe, b"MZ-auto").unwrap();
    let fingerprint = sha256_file(&probe).unwrap();
    let body = format!(
        r#"{{
            "data": {{
                "id": "{}",
                "type": "file",
                "attributes": {{
                    "last_analysis_stats": {{
                        "malicious": 3,
                        "suspicious": 0,
                        "undetected": 50,
                        "harmless": 0,
                        "timeout": 0
                    }}
                }}
            }}
        }}"#,
        fingerprint.sha256
    );
    let env = TestEnv::new(vec![MockResponse::json(200, &body)]);
    let path = env.file("auto.exe", b"MZ-auto");
    let result = env.core().detect_file(&path, true).unwrap();
    assert_eq!(result.state, DetectionState::Checked);
    assert_eq!(result.summary.unwrap().flagged(), 3);
    assert_eq!(env.server.requests().len(), 1);
    assert_eq!(env.cache.recent_detections(10).unwrap().len(), 1);
}

#[test]
fn auto_check_of_an_unknown_file_does_not_upload() {
    let env = TestEnv::new(vec![MockResponse::json(404, "{}")]);
    let path = env.file("unknown.exe", b"MZ-unknown");
    let result = env.core().detect_file(&path, true).unwrap();
    assert_eq!(result.state, DetectionState::Unknown);
    assert!(result.summary.is_none());
    let requests = env.server.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].path.starts_with("/files/"));
    assert_eq!(requests[0].method, "GET");
}
