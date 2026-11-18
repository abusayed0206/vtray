//! Installation is done by the Inno Setup installer (`installer/vtray.iss`): program folder,
//! Start menu shortcut, and the Settings > Apps entry. This module covers what the app must
//! clean up itself, and finds the installer's uninstaller.
use crate::core::errors::AppError;
use std::path::{Path, PathBuf};

/// The only folder name VT Tray will ever delete recursively.
const OWNED_DIR_NAME: &str = "VT Tray";
/// Inno Setup's uninstaller, written next to the installed program.
const UNINSTALLER: &str = "unins000.exe";

/// The Inno Setup uninstaller when `exe` is an installed copy; None for a portable copy.
pub fn uninstaller_for(exe: &Path) -> Option<PathBuf> {
    let uninstaller = exe.parent()?.join(UNINSTALLER);
    uninstaller.is_file().then_some(uninstaller)
}

fn check_owned(dir: &Path) -> Result<(), AppError> {
    if dir.file_name().and_then(|name| name.to_str()) == Some(OWNED_DIR_NAME) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "refusing to delete a folder VT Tray does not own: {}",
            dir.display()
        )))
    }
}

/// Deletes VT Tray's own folders now; used when no VT Tray process holds them open.
pub fn delete_now(dirs: &[PathBuf]) -> Vec<String> {
    let mut problems = Vec::new();
    for dir in dirs {
        if let Err(error) = check_owned(dir) {
            problems.push(error.safe_message());
            continue;
        }
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => problems.push(format!("{}: {error}", dir.display())),
        }
    }
    problems
}

/// Deletes folders a few seconds after this process exits (a running program cannot delete
/// its own open log files). Only folders named exactly "VT Tray" are accepted.
pub fn schedule_delete(dirs: &[PathBuf]) -> Result<(), AppError> {
    let mut script = String::from("ping -n 4 127.0.0.1 >nul");
    for dir in dirs {
        check_owned(dir)?;
        if dir.exists() {
            script.push_str(&format!(" & rmdir /s /q \"{}\"", dir.display()));
        }
    }
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd.exe")
        .arg("/c")
        .raw_arg(&script)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(AppError::Io)
}
