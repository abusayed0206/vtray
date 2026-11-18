use crate::core::errors::AppError;
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIIF_WARNING,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};
use windows::core::PCWSTR;

pub const TRAY_CALLBACK_MESSAGE: u32 = 0x8000 + 1;
const TRAY_ID: u32 = 1;

pub struct TrayIcon {
    window: HWND,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    added: bool,
}

impl TrayIcon {
    pub fn new(window: HWND) -> Result<Self, AppError> {
        let icon = load_icon();
        let mut tray = Self {
            window,
            icon,
            added: false,
        };
        tray.add()?;
        Ok(tray)
    }

    pub fn add(&mut self) -> Result<(), AppError> {
        let mut data = self.base_data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = TRAY_CALLBACK_MESSAGE;
        data.hIcon = self.icon;
        copy_wide(&mut data.szTip, crate::ui::strings::TIP_DEFAULT);
        let result = unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
        if !result.as_bool() {
            return Err(AppError::Internal(
                "could not add the tray icon; Explorer may not be ready".to_string(),
            ));
        }
        self.added = true;
        Ok(())
    }

    pub fn set_tooltip(&mut self, text: &str) -> Result<(), AppError> {
        if !self.added {
            return self.add();
        }
        let mut data = self.base_data();
        data.uFlags = NIF_TIP | NIF_SHOWTIP;
        copy_wide(&mut data.szTip, text);
        let result = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        if result.as_bool() {
            Ok(())
        } else {
            Err(AppError::Internal(
                "could not update the tray tooltip".to_string(),
            ))
        }
    }

    pub fn notify(&mut self, title: &str, text: &str, warning: bool) -> Result<(), AppError> {
        if !self.added {
            self.add()?;
        }
        let mut data = self.base_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, text);
        let result = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        if result.as_bool() {
            Ok(())
        } else {
            Err(AppError::Internal(
                "could not show the tray notification".to_string(),
            ))
        }
    }

    pub fn error(&mut self, title: &str, text: &str) -> Result<(), AppError> {
        if !self.added {
            self.add()?;
        }
        let mut data = self.base_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_ERROR;
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, text);
        let result = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        if result.as_bool() {
            Ok(())
        } else {
            Err(AppError::Internal(
                "could not show the tray notification".to_string(),
            ))
        }
    }

    fn base_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: TRAY_ID,
            ..Default::default()
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        if self.added {
            let data = self.base_data();
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            self.added = false;
        }
    }
}

fn load_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    let module = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let instance = HINSTANCE(module.0);
    let resource = PCWSTR(std::ptr::without_provenance(1));
    unsafe { LoadIconW(Some(instance), resource) }
        .or_else(|_| unsafe { LoadIconW(None, IDI_APPLICATION) })
        .unwrap_or_default()
}

fn copy_wide<const N: usize>(target: &mut [u16; N], value: &str) {
    let encoded: Vec<u16> = value.encode_utf16().take(N.saturating_sub(1)).collect();
    target.fill(0);
    target[..encoded.len()].copy_from_slice(&encoded);
}
