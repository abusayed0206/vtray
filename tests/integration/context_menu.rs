use crate::mock_server::ScratchKey;
use std::path::Path;
use vt_tray::platform::windows::context_menu::{is_registered_at, register_at, unregister_at};

const EXE: &str = r"C:\Program Files\VT Tray\vt-tray.exe";

#[test]
fn context_menu_runs_a_hash_only_quick_check() {
    let scratch = ScratchKey::new("context-menu");
    let verb = scratch.child(r"shell\VTTrayCheck");
    let root = ScratchKey::hkcu();
    register_at(&root, &verb, Path::new(EXE)).unwrap();
    assert!(is_registered_at(&root, &verb).unwrap());
    let key = root.open_subkey(&verb).unwrap();
    let label: String = key.get_value("").unwrap();
    assert_eq!(label, "Check with VTTray");
    let command: String = key.open_subkey("command").unwrap().get_value("").unwrap();
    assert_eq!(command, format!("\"{EXE}\" --check \"%1\""));
    // FR-020: the Explorer action can only check, never upload.
    assert!(!command.to_ascii_lowercase().contains("upload"));
}

#[test]
fn unregister_removes_every_key_it_created() {
    let scratch = ScratchKey::new("context-menu-orphans");
    let verb = scratch.child(r"shell\VTTrayCheck");
    let root = ScratchKey::hkcu();
    register_at(&root, &verb, Path::new(EXE)).unwrap();
    assert!(scratch.exists(r"shell\VTTrayCheck\command"));

    unregister_at(&root, &verb).unwrap();
    assert!(!is_registered_at(&root, &verb).unwrap());
    assert!(!scratch.exists(r"shell\VTTrayCheck"));
    assert!(
        !scratch.exists(r"shell\VTTrayCheck\command"),
        "no orphaned command key"
    );
    assert!(scratch.exists("shell"), "parent keys owned by Windows stay");

    unregister_at(&root, &verb).expect("removing twice is harmless");
}

#[test]
fn registering_again_replaces_a_stale_executable_path() {
    let scratch = ScratchKey::new("context-menu-moved");
    let verb = scratch.child(r"shell\VTTrayCheck");
    let root = ScratchKey::hkcu();
    register_at(&root, &verb, Path::new(r"D:\old\vt-tray.exe")).unwrap();
    register_at(&root, &verb, Path::new(EXE)).unwrap();
    let command: String = root
        .open_subkey(&verb)
        .unwrap()
        .open_subkey("command")
        .unwrap()
        .get_value("")
        .unwrap();
    assert!(command.starts_with(&format!("\"{EXE}\"")));
}
