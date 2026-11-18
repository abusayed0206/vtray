use crate::core::errors::AppError;

pub const DIRECT_UPLOAD_LIMIT: u64 = 32 * 1024 * 1024;
pub const LARGE_WARNING_LIMIT: u64 = 200 * 1024 * 1024;
pub const MAX_UPLOAD_LIMIT: u64 = 650 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRoute {
    Direct,
    UploadUrl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    Present,
    Absent,
    Invalid,
}

pub fn upload_route(size_bytes: u64) -> Result<UploadRoute, AppError> {
    if size_bytes == 0 {
        return Err(AppError::Validation(
            "zero-byte files cannot be uploaded".to_string(),
        ));
    }
    if size_bytes > MAX_UPLOAD_LIMIT {
        return Err(AppError::Validation(
            "file is larger than the 650 MB upload limit".to_string(),
        ));
    }
    if size_bytes > DIRECT_UPLOAD_LIMIT {
        Ok(UploadRoute::UploadUrl)
    } else {
        Ok(UploadRoute::Direct)
    }
}

pub fn requires_large_warning(size_bytes: u64) -> bool {
    size_bytes > LARGE_WARNING_LIMIT
}

pub fn consent_satisfied(consent_at: Option<i64>) -> bool {
    consent_at.is_some_and(|value| value > 0)
}

pub fn auto_check_allowed(
    enabled: bool,
    monitoring_paused: bool,
    credential: CredentialStatus,
    auto_remaining: u32,
) -> bool {
    enabled
        && !monitoring_paused
        && matches!(credential, CredentialStatus::Present)
        && auto_remaining > 0
}

pub fn file_is_reasonable_for_upload(path: &std::path::Path) -> bool {
    path.is_file()
        && std::fs::symlink_metadata(path)
            .map(|metadata| !metadata.file_type().is_symlink())
            .unwrap_or(false)
}
