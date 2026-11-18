use std::io::Write;
use vt_tray::core::hash::{is_stable, safe_base_name, sha256_file};

#[test]
fn hashes_known_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"hello").unwrap();
    let fingerprint = sha256_file(&path).unwrap();
    assert_eq!(
        fingerprint.sha256,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(fingerprint.size_bytes, 5);
}

#[test]
fn detects_a_file_changing_during_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("changing.bin");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&vec![0_u8; 2 * 1024 * 1024]).unwrap();
    drop(file);
    let metadata_before = std::fs::metadata(&path).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(b"x").unwrap();
    drop(file);
    let metadata_after = std::fs::metadata(&path).unwrap();
    assert_ne!(metadata_before.len(), metadata_after.len());
}

#[test]
fn stability_check_sees_a_growing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("growing.bin");
    std::fs::write(&path, b"a").unwrap();
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&writer_path, b"abc").unwrap();
    });
    assert!(!is_stable(&path, std::time::Duration::from_millis(300)).unwrap());
    writer.join().unwrap();
    assert!(is_stable(&path, std::time::Duration::from_millis(50)).unwrap());
}

#[test]
fn sanitizes_unsafe_upload_names() {
    let path = std::path::Path::new("C:\\temp\\a\"b\\c.txt");
    assert_eq!(safe_base_name(path), "c.txt");
    assert_eq!(safe_base_name(std::path::Path::new("")), "upload.bin");
}
