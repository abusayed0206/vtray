use crate::mock_server::ScratchKey;
use std::path::Path;
use vt_tray::platform::windows::context_menu::{
    CHECK_VERB, UPLOAD_VERB, is_registered_at, register_at, unregister_at,
};

const EXE: &str = r"C:\Users\Test\AppData\Local\Programs\VT Tray\VTray.exe";

fn verb(scratch: &ScratchKey, name: &str) -> (String, String) {
    let root = ScratchKey::hkcu();
    let key = root
        .open_subkey(scratch.child(&format!(r"shell\{name}")))
        .unwrap();
    let label: String = key.get_value("").unwrap();
    let command: String = key.open_subkey("command").unwrap().get_value("").unwrap();
    (label, command)
}

#[test]
fn check_entry_is_hash_only_and_upload_entry_is_separate() {
    let scratch = ScratchKey::new("context-menu");
    let shell = scratch.child("shell");
    let root = ScratchKey::hkcu();
    register_at(&root, &shell, Path::new(EXE)).unwrap();
    assert!(is_registered_at(&root, &shell).unwrap());

    let (label, command) = verb(&scratch, CHECK_VERB);
    assert_eq!(label, "Check with VT Tray");
    assert_eq!(command, format!("\"{EXE}\" --check \"%1\""));
    // FR-020: the check entry can only check, never upload.
    assert!(!command.to_ascii_lowercase().contains("upload"));

    let (label, command) = verb(&scratch, UPLOAD_VERB);
    assert_eq!(label, "Upload with VT Tray...");
    assert_eq!(command, format!("\"{EXE}\" --upload \"%1\""));
}

#[test]
fn unregister_removes_every_key_it_created() {
    let scratch = ScratchKey::new("context-menu-orphans");
    let shell = scratch.child("shell");
    let root = ScratchKey::hkcu();
    register_at(&root, &shell, Path::new(EXE)).unwrap();
    assert!(scratch.exists(&format!(r"shell\{CHECK_VERB}\command")));
    assert!(scratch.exists(&format!(r"shell\{UPLOAD_VERB}\command")));

    unregister_at(&root, &shell).unwrap();
    assert!(!is_registered_at(&root, &shell).unwrap());
    for name in [CHECK_VERB, UPLOAD_VERB] {
        assert!(
            !scratch.exists(&format!(r"shell\{name}")),
            "{name} left behind"
        );
    }
    assert!(scratch.exists("shell"), "parent keys owned by Windows stay");

    unregister_at(&root, &shell).expect("removing twice is harmless");
}

#[test]
fn registering_again_replaces_a_stale_executable_path() {
    let scratch = ScratchKey::new("context-menu-moved");
    let shell = scratch.child("shell");
    let root = ScratchKey::hkcu();
    register_at(&root, &shell, Path::new(r"D:\old\vt-tray.exe")).unwrap();
    register_at(&root, &shell, Path::new(EXE)).unwrap();
    for name in [CHECK_VERB, UPLOAD_VERB] {
        let (_, command) = verb(&scratch, name);
        assert!(
            command.starts_with(&format!("\"{EXE}\"")),
            "{name}: {command}"
        );
    }
}
