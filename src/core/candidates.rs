use std::fs::File;
use std::io::Read;
use std::path::Path;

const SCRIPT_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "scr", "com", "bat", "cmd", "ps1", "vbs", "js", "jse", "wsf", "wsh",
    "msi", "msix", "appx", "lnk", "iso", "img", "hta", "cpl", "ocx", "jar",
];

const PARTIAL_SUFFIXES: &[&str] = &["tmp", "part", "crdownload", "partial", "download"];

pub fn is_candidate(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if is_partial_name(name) {
        return false;
    }
    if metadata.len() == 0 {
        return false;
    }
    if has_candidate_extension(name) {
        return true;
    }
    looks_like_pe(path)
}

pub fn is_partial_name(name: &str) -> bool {
    if name.starts_with("~$") || name.starts_with(".~") {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    PARTIAL_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(&format!(".{suffix}")))
}

pub fn has_candidate_extension(name: &str) -> bool {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    SCRIPT_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == extension)
}

pub fn looks_like_pe(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 64];
    let Ok(read) = file.read(&mut header) else {
        return false;
    };
    if read < 64 || &header[0..2] != b"MZ" {
        return false;
    }
    let offset =
        u32::from_le_bytes([header[0x3c], header[0x3d], header[0x3e], header[0x3f]]) as u64;
    if offset > 1024 * 1024 {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut signature = [0_u8; 4];
    use std::io::{Seek, SeekFrom};
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    file.read_exact(&mut signature).is_ok() && &signature == b"PE\0\0"
}

pub fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
