use std::path::PathBuf;
use vt_tray::core::config::ConfigStore;
use vt_tray::core::settings::{Preferences, validate_watched_folder};

#[test]
fn defaults_are_safe() {
    let preferences = Preferences::default();
    assert!(!preferences.auto_check_enabled);
    assert!(preferences.notifications_enabled);
    assert!(!preferences.autostart_enabled);
    assert!(!preferences.monitoring_paused);
    assert_eq!(preferences.cache_retention_days, 30);
}

#[test]
fn adds_and_deduplicates_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut preferences = Preferences::default();
    preferences
        .add_folder(dir.path().to_path_buf(), 100)
        .unwrap();
    assert!(
        preferences
            .add_folder(dir.path().to_path_buf(), 101)
            .is_err()
    );
    assert_eq!(preferences.watched_folders.len(), 1);
    assert!(preferences.remove_folder(dir.path()));
    assert!(preferences.watched_folders.is_empty());
}

#[test]
fn rejects_relative_and_root_folders() {
    assert!(validate_watched_folder(&PathBuf::from("relative")).is_err());
    if let Ok(root) = std::path::Path::new("C:\\").canonicalize() {
        assert!(validate_watched_folder(&root).is_err());
    }
}

#[test]
fn saves_and_loads_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path().join("config.json"));
    let preferences = Preferences {
        auto_check_enabled: true,
        cache_retention_days: 7,
        ..Default::default()
    };
    store.save(&preferences).unwrap();
    let loaded = store.load().unwrap();
    assert!(loaded.auto_check_enabled);
    assert_eq!(loaded.cache_retention_days, 7);
}

#[test]
fn recovers_from_corrupt_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, b"{not json").unwrap();
    let store = ConfigStore::new(path.clone());
    let loaded = store.load().unwrap();
    assert_eq!(loaded, Preferences::default());
    assert!(!path.exists());
}

#[test]
fn sanitizes_unknown_log_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, br#"{"log_level":"verbose"}"#).unwrap();
    let loaded = ConfigStore::new(path).load().unwrap();
    assert_eq!(loaded.log_level, "info");
}
