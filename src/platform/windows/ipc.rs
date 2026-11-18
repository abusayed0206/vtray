use crate::core::errors::AppError;
use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA};
use windows::core::{HSTRING, PCWSTR};

pub const HIDDEN_WINDOW_CLASS: &str = "VTTrayHiddenWindow";
const COPYDATA_CHECK_PATH: usize = 1;
const MAX_IPC_BYTES: u32 = 64 * 1024;

pub fn send_check_with_timeout(
    path: &Path,
    timeout: std::time::Duration,
) -> Result<bool, AppError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if send_check(path)? {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn send_check(path: &Path) -> Result<bool, AppError> {
    let class = HSTRING::from(HIDDEN_WINDOW_CLASS);
    let window = match unsafe { FindWindowW(&class, PCWSTR::null()) } {
        Ok(window) if !window.0.is_null() => window,
        _ => return Ok(false),
    };
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let cb_data = (wide.len() * std::mem::size_of::<u16>()) as u32;
    if cb_data > MAX_IPC_BYTES {
        return Err(AppError::Validation(
            "the selected path is too long".to_string(),
        ));
    }
    let data = COPYDATASTRUCT {
        dwData: COPYDATA_CHECK_PATH,
        cbData: cb_data,
        lpData: wide.as_mut_ptr().cast(),
    };
    let result = unsafe {
        SendMessageW(
            window,
            WM_COPYDATA,
            Some(WPARAM(0)),
            Some(LPARAM(&data as *const COPYDATASTRUCT as isize)),
        )
    };
    Ok(result.0 != 0)
}

/// # Safety
/// `lparam` must reference a valid `COPYDATASTRUCT` for the duration of the call.
pub unsafe fn read_check_path(lparam: LPARAM) -> Option<PathBuf> {
    let pointer = lparam.0 as *const COPYDATASTRUCT;
    if pointer.is_null() {
        return None;
    }
    let data = unsafe { &*pointer };
    if data.dwData != COPYDATA_CHECK_PATH
        || data.cbData == 0
        || data.cbData > MAX_IPC_BYTES
        || data.cbData % 2 != 0
        || data.lpData.is_null()
    {
        return None;
    }
    let len = (data.cbData / 2) as usize;
    let wide = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, len) };
    if wide.last() != Some(&0) || wide[..len - 1].contains(&0) {
        return None;
    }
    let path = PathBuf::from(OsString::from_wide(&wide[..len - 1]));
    if path.as_os_str().is_empty() || !path.is_absolute() {
        None
    } else {
        Some(path)
    }
}

pub fn write_pending(path: &Path, file: &Path) -> Result<(), AppError> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = path.to_string_lossy();
    let temp = file.with_extension("tmp");
    fs::write(&temp, encoded.as_bytes())?;
    fs::rename(temp, file)?;
    Ok(())
}

pub fn take_pending(file: &Path) -> Option<PathBuf> {
    let bytes = fs::read(file).ok()?;
    let _ = fs::remove_file(file);
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        let path = PathBuf::from(text);
        path.is_absolute().then_some(path)
    }
}
