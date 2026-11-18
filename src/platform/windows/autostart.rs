use crate::core::errors::AppError;
use std::path::Path;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "VTTray";

pub fn set_enabled(executable: &Path, enabled: bool) -> Result<(), AppError> {
    set_enabled_at(
        &RegKey::predef(HKEY_CURRENT_USER),
        RUN_KEY,
        executable,
        enabled,
    )
}

pub fn is_enabled() -> Result<bool, AppError> {
    is_enabled_at(&RegKey::predef(HKEY_CURRENT_USER), RUN_KEY)
}

/// Adds or removes only our value under `root\run_key`; other programs' entries are untouched.
/// Tests pass a scratch key instead of the real Run key.
pub fn set_enabled_at(
    root: &RegKey,
    run_key: &str,
    executable: &Path,
    enabled: bool,
) -> Result<(), AppError> {
    if enabled {
        let (key, _) = root
            .create_subkey(run_key)
            .map_err(|error| AppError::Config(error.to_string()))?;
        let command = format!("\"{}\"", executable.display());
        key.set_value(VALUE_NAME, &command)
            .map_err(|error| AppError::Config(error.to_string()))?;
    } else {
        let key = match root.open_subkey_with_flags(run_key, KEY_SET_VALUE | KEY_QUERY_VALUE) {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(AppError::Config(error.to_string())),
        };
        match key.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::Config(error.to_string())),
        }
    }
    Ok(())
}

pub fn is_enabled_at(root: &RegKey, run_key: &str) -> Result<bool, AppError> {
    let key = match root.open_subkey_with_flags(run_key, KEY_QUERY_VALUE) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(AppError::Config(error.to_string())),
    };
    match key.get_value::<String, _>(VALUE_NAME) {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::Config(error.to_string())),
    }
}
