use crate::core::errors::AppError;
use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, FindWindowW, SendMessageW, WM_COPYDATA,
};
use windows::core::{HSTRING, PCWSTR};

pub const HIDDEN_WINDOW_CLASS: &str = "VTTrayHiddenWindow";
const COPYDATA_CHECK_PATH: usize = 1;
const COPYDATA_UPLOAD_PATH: usize = 2;
const MAX_IPC_BYTES: u32 = 64 * 1024;
/// First line of the pending file for an upload request; a bare path means a check.
const PENDING_UPLOAD_MARKER: &str = "upload";

/// A request from Explorer or a second launch to the running tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub path: PathBuf,
    /// `--upload`: check first, then the consent dialog if VirusTotal does not know the file.
    /// `--check`: hash-only, never offers an upload (FR-020).
    pub upload: bool,
}

pub fn send_with_timeout(
    handoff: &Handoff,
    timeout: std::time::Duration,
) -> Result<bool, AppError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if send(handoff)? {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn send(handoff: &Handoff) -> Result<bool, AppError> {
    let class = HSTRING::from(HIDDEN_WINDOW_CLASS);
    let window = match unsafe { FindWindowW(&class, PCWSTR::null()) } {
        Ok(window) if !window.0.is_null() => window,
        _ => return Ok(false),
    };
    let mut wide: Vec<u16> = handoff.path.as_os_str().encode_wide().collect();
    wide.push(0);
    let cb_data = (wide.len() * std::mem::size_of::<u16>()) as u32;
    if cb_data > MAX_IPC_BYTES {
        return Err(AppError::Validation(
            "the selected path is too long".to_string(),
        ));
    }
    let data = COPYDATASTRUCT {
        dwData: if handoff.upload {
            COPYDATA_UPLOAD_PATH
        } else {
            COPYDATA_CHECK_PATH
        },
        cbData: cb_data,
        lpData: wide.as_mut_ptr().cast(),
    };
    // We were started by the user's click, so we may let the tray bring its dialog to the front.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
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

/// Asks a running tray to quit (as its Exit command does) and waits for it to go away.
/// Returns true when no tray is running afterwards.
pub fn close_running(timeout: std::time::Duration) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let class = HSTRING::from(HIDDEN_WINDOW_CLASS);
    let find = || match unsafe { FindWindowW(&class, PCWSTR::null()) } {
        Ok(window) if !window.0.is_null() => Some(window),
        _ => None,
    };
    let Some(window) = find() else {
        return true;
    };
    unsafe {
        let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if find().is_none() {
            // The window is gone; give the process a moment to release its files.
            std::thread::sleep(std::time::Duration::from_millis(500));
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// # Safety
/// `lparam` must reference a valid `COPYDATASTRUCT` for the duration of the call.
pub unsafe fn read_handoff(lparam: LPARAM) -> Option<Handoff> {
    let pointer = lparam.0 as *const COPYDATASTRUCT;
    if pointer.is_null() {
        return None;
    }
    let data = unsafe { &*pointer };
    let upload = match data.dwData {
        COPYDATA_CHECK_PATH => false,
        COPYDATA_UPLOAD_PATH => true,
        _ => return None,
    };
    if data.cbData == 0
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
        Some(Handoff { path, upload })
    }
}

pub fn write_pending(handoff: &Handoff, file: &Path) -> Result<(), AppError> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let path = handoff.path.to_string_lossy();
    let encoded = if handoff.upload {
        format!("{PENDING_UPLOAD_MARKER}\n{path}")
    } else {
        path.into_owned()
    };
    let temp = file.with_extension("tmp");
    fs::write(&temp, encoded.as_bytes())?;
    fs::rename(temp, file)?;
    Ok(())
}

pub fn take_pending(file: &Path) -> Option<Handoff> {
    let bytes = fs::read(file).ok()?;
    let _ = fs::remove_file(file);
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim();
    let (upload, text) = match text.split_once('\n') {
        Some((PENDING_UPLOAD_MARKER, rest)) => (true, rest.trim()),
        _ => (false, text),
    };
    if text.is_empty() {
        None
    } else {
        let path = PathBuf::from(text);
        path.is_absolute().then_some(Handoff { path, upload })
    }
}
