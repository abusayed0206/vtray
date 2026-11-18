use crate::core::errors::AppError;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub sha256: String,
    pub size_bytes: u64,
    pub mtime_ns: i128,
}

pub fn sha256_file(path: &Path) -> Result<FileFingerprint, AppError> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(AppError::Validation(
            "path is not a regular file".to_string(),
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    if after.len() != before.len() || modified_ns(&after) != modified_ns(&before) {
        return Err(AppError::Validation(
            "file changed while it was being fingerprinted".into(),
        ));
    }
    Ok(FileFingerprint {
        sha256: hex::encode(hasher.finalize()),
        size_bytes: total,
        mtime_ns: modified_ns(&after),
    })
}

/// True when size and modification time match across two reads `wait` apart.
pub fn is_stable(path: &Path, wait: std::time::Duration) -> Result<bool, AppError> {
    let first = std::fs::metadata(path)?;
    std::thread::sleep(wait);
    let second = std::fs::metadata(path)?;
    Ok(first.len() == second.len() && modified_ns(&first) == modified_ns(&second))
}

pub fn modified_ns(metadata: &std::fs::Metadata) -> i128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i128)
        .unwrap_or(0)
}

pub fn safe_base_name(path: &Path) -> String {
    let raw = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cleaned: String = raw
        .chars()
        .filter(|ch| !ch.is_control())
        .map(|ch| match ch {
            '"' | '\\' | '/' => '_',
            _ => ch,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "upload.bin".to_string()
    } else {
        trimmed.chars().take(200).collect()
    }
}
