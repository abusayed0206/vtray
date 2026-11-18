use std::io::Write;
use vt_tray::core::candidates::{
    has_candidate_extension, is_candidate, is_partial_name, looks_like_pe,
};

fn temporary_path(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(bytes).unwrap();
    path
}

#[test]
fn recognizes_script_and_pe_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let exe = temporary_path(&dir, "installer.exe", b"not really a pe");
    let script = temporary_path(&dir, "script.ps1", b"Write-Host hello");
    assert!(is_candidate(&exe));
    assert!(is_candidate(&script));
    assert!(has_candidate_extension("SETUP.EXE"));
}

#[test]
fn recognizes_pe_magic_without_a_known_extension() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = vec![0_u8; 256];
    bytes[0] = b'M';
    bytes[1] = b'Z';
    bytes[0x3c..0x40].copy_from_slice(&(0x40_u32).to_le_bytes());
    bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
    let path = temporary_path(&dir, "payload.bin", &bytes);
    assert!(looks_like_pe(&path));
    assert!(is_candidate(&path));
}

#[test]
fn ignores_partial_names_and_empty_files() {
    assert!(is_partial_name("download.exe.tmp"));
    assert!(is_partial_name("download.exe.crdownload"));
    assert!(is_partial_name("~$document.docx"));
    assert!(!is_partial_name("setup.exe"));
    let dir = tempfile::tempdir().unwrap();
    let empty = temporary_path(&dir, "empty.exe", b"");
    assert!(!is_candidate(&empty));
}

#[test]
fn rejects_regular_non_pe_files_without_script_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let text = temporary_path(&dir, "notes.txt", b"hello");
    assert!(!is_candidate(&text));
}
