use crate::core::errors::AppError;
use std::path::Path;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub const VERB_KEY: &str = r"Software\Classes\*\shell\VTTrayCheck";

pub fn register(executable: &Path) -> Result<(), AppError> {
    register_at(&RegKey::predef(HKEY_CURRENT_USER), VERB_KEY, executable)
}

pub fn unregister() -> Result<(), AppError> {
    unregister_at(&RegKey::predef(HKEY_CURRENT_USER), VERB_KEY)
}

pub fn is_registered() -> Result<bool, AppError> {
    is_registered_at(&RegKey::predef(HKEY_CURRENT_USER), VERB_KEY)
}

/// Writes the hash-only quick-check verb under `root\verb_key`. Tests pass a scratch key.
pub fn register_at(root: &RegKey, verb_key: &str, executable: &Path) -> Result<(), AppError> {
    let (key, _) = root
        .create_subkey(verb_key)
        .map_err(|error| AppError::Config(error.to_string()))?;
    key.set_value("", &"Check with VTTray")
        .map_err(|error| AppError::Config(error.to_string()))?;
    key.set_value("Icon", &executable.display().to_string())
        .map_err(|error| AppError::Config(error.to_string()))?;
    let (command, _) = key
        .create_subkey("command")
        .map_err(|error| AppError::Config(error.to_string()))?;
    let value = format!("\"{}\" --check \"%1\"", executable.display());
    command
        .set_value("", &value)
        .map_err(|error| AppError::Config(error.to_string()))?;
    Ok(())
}

/// Removes the verb and everything under it, so no orphaned `command` key remains.
pub fn unregister_at(root: &RegKey, verb_key: &str) -> Result<(), AppError> {
    match root.delete_subkey_all(verb_key) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Config(error.to_string())),
    }
}

pub fn is_registered_at(root: &RegKey, verb_key: &str) -> Result<bool, AppError> {
    match root.open_subkey(verb_key) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::Config(error.to_string())),
    }
}
