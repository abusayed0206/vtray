use crate::core::errors::AppError;
use std::path::Path;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR};

pub fn open_path(path: &Path) -> Result<(), AppError> {
    let target = HSTRING::from(path.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(AppError::Internal(format!(
            "could not open {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn open_url(url: &str) -> Result<(), AppError> {
    let target = HSTRING::from(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(AppError::Internal("could not open the browser".to_string()));
    }
    Ok(())
}
