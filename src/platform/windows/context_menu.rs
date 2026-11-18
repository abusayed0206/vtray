use crate::core::errors::AppError;
use std::path::Path;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// Per-user shell key for all files; both VT Tray entries live under it.
pub const SHELL_KEY: &str = r"Software\Classes\*\shell";
/// Hash-only quick check (FR-020): never offers an upload.
pub const CHECK_VERB: &str = "VTTrayCheck";
/// Checks first, then shows the per-file consent dialog if VirusTotal does not know the file.
pub const UPLOAD_VERB: &str = "VTTrayUpload";

pub fn register(executable: &Path) -> Result<(), AppError> {
    register_at(&RegKey::predef(HKEY_CURRENT_USER), SHELL_KEY, executable)
}

pub fn unregister() -> Result<(), AppError> {
    unregister_at(&RegKey::predef(HKEY_CURRENT_USER), SHELL_KEY)
}

pub fn is_registered() -> Result<bool, AppError> {
    is_registered_at(&RegKey::predef(HKEY_CURRENT_USER), SHELL_KEY)
}

/// Writes both Explorer entries under `root\shell_key`. Tests pass a scratch key.
pub fn register_at(root: &RegKey, shell_key: &str, executable: &Path) -> Result<(), AppError> {
    write_verb(
        root,
        &format!(r"{shell_key}\{CHECK_VERB}"),
        "Check with VT Tray",
        executable,
        "--check",
    )?;
    write_verb(
        root,
        &format!(r"{shell_key}\{UPLOAD_VERB}"),
        "Upload with VT Tray...",
        executable,
        "--upload",
    )
}

fn write_verb(
    root: &RegKey,
    verb_key: &str,
    label: &str,
    executable: &Path,
    argument: &str,
) -> Result<(), AppError> {
    let config = |error: std::io::Error| AppError::Config(error.to_string());
    let (key, _) = root.create_subkey(verb_key).map_err(config)?;
    key.set_value("", &label).map_err(config)?;
    key.set_value("Icon", &executable.display().to_string())
        .map_err(config)?;
    let (command, _) = key.create_subkey("command").map_err(config)?;
    let value = format!("\"{}\" {argument} \"%1\"", executable.display());
    command.set_value("", &value).map_err(config)
}

/// Removes both entries and everything under them, so no orphaned `command` key remains.
pub fn unregister_at(root: &RegKey, shell_key: &str) -> Result<(), AppError> {
    for verb in [CHECK_VERB, UPLOAD_VERB] {
        match root.delete_subkey_all(format!(r"{shell_key}\{verb}")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::Config(error.to_string())),
        }
    }
    Ok(())
}

pub fn is_registered_at(root: &RegKey, shell_key: &str) -> Result<bool, AppError> {
    match root.open_subkey(format!(r"{shell_key}\{CHECK_VERB}")) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::Config(error.to_string())),
    }
}
