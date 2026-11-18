use std::time::{Duration, Instant};
use vt_tray::platform::windows::setup::{delete_now, schedule_delete, uninstaller_for};

#[test]
fn installed_copies_are_recognised_by_their_uninstaller() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("VTray.exe");
    std::fs::write(&exe, b"program").unwrap();
    assert!(uninstaller_for(&exe).is_none(), "portable copy");
    std::fs::write(dir.path().join("unins000.exe"), b"uninstaller").unwrap();
    assert_eq!(uninstaller_for(&exe), Some(dir.path().join("unins000.exe")));
}

#[test]
fn only_folders_named_vt_tray_are_ever_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = dir.path().join("Documents");
    std::fs::create_dir(&foreign).unwrap();
    assert!(schedule_delete(std::slice::from_ref(&foreign)).is_err());
    assert_eq!(delete_now(std::slice::from_ref(&foreign)).len(), 1);
    assert!(foreign.exists());
}

#[test]
fn owned_folders_are_deleted_now_or_shortly_after_exit() {
    let dir = tempfile::tempdir().unwrap();
    let now = dir.path().join("a").join("VT Tray");
    std::fs::create_dir_all(now.join("logs")).unwrap();
    assert!(delete_now(std::slice::from_ref(&now)).is_empty());
    assert!(!now.exists());
    assert!(
        delete_now(std::slice::from_ref(&now)).is_empty(),
        "already gone is fine"
    );

    let later = dir.path().join("b").join("VT Tray");
    std::fs::create_dir_all(later.join("logs")).unwrap();
    std::fs::write(later.join("logs").join("a.log"), b"x").unwrap();
    schedule_delete(std::slice::from_ref(&later)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while later.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!later.exists());
    assert!(
        dir.path().join("b").exists(),
        "the parent folder is left alone"
    );
}
