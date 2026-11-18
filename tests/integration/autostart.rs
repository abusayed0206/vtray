use crate::mock_server::ScratchKey;
use std::path::Path;
use vt_tray::platform::windows::autostart::{VALUE_NAME, is_enabled_at, set_enabled_at};

const EXE: &str = r"C:\Users\Test\AppData\Local\Programs\VT Tray\vt-tray.exe";

#[test]
fn autostart_is_added_quoted_and_removed_cleanly() {
    let scratch = ScratchKey::new("autostart");
    let run = scratch.child("Run");
    let root = ScratchKey::hkcu();
    assert!(!is_enabled_at(&root, &run).unwrap());

    set_enabled_at(&root, &run, Path::new(EXE), true).unwrap();
    assert!(is_enabled_at(&root, &run).unwrap());
    let value: String = root
        .open_subkey(&run)
        .unwrap()
        .get_value(VALUE_NAME)
        .unwrap();
    // Quoted, so a path with spaces starts the right program.
    assert_eq!(value, format!("\"{EXE}\""));

    set_enabled_at(&root, &run, Path::new(EXE), false).unwrap();
    assert!(!is_enabled_at(&root, &run).unwrap());
    let key = root.open_subkey(&run).unwrap();
    assert!(key.get_value::<String, _>(VALUE_NAME).is_err());
}

#[test]
fn removing_autostart_leaves_other_programs_alone() {
    let scratch = ScratchKey::new("autostart-others");
    let run = scratch.child("Run");
    let root = ScratchKey::hkcu();
    let (key, _) = root.create_subkey(&run).unwrap();
    key.set_value("OtherApp", &r#""C:\Other\other.exe""#)
        .unwrap();

    set_enabled_at(&root, &run, Path::new(EXE), true).unwrap();
    set_enabled_at(&root, &run, Path::new(EXE), false).unwrap();
    let other: String = key.get_value("OtherApp").unwrap();
    assert_eq!(other, r#""C:\Other\other.exe""#);
}

#[test]
fn disabling_when_never_enabled_is_harmless() {
    let scratch = ScratchKey::new("autostart-absent");
    let run = scratch.child("Run");
    let root = ScratchKey::hkcu();
    set_enabled_at(&root, &run, Path::new(EXE), false).unwrap();
    assert!(!scratch.exists("Run"), "disabling must not create keys");
}
