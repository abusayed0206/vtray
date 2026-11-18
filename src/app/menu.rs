use crate::core::errors::AppError;
use windows::Win32::Foundation::{HWND, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
};
use windows::core::{HSTRING, PCWSTR};

pub struct Menu {
    handle: HMENU,
}

impl Menu {
    pub fn popup() -> Result<Self, AppError> {
        let handle = unsafe { CreatePopupMenu() }
            .map_err(|error| AppError::Internal(format!("could not create menu: {error}")))?;
        Ok(Self { handle })
    }

    pub fn item(&self, id: u32, text: &str) -> Result<(), AppError> {
        self.append(MF_STRING, id, text)
    }

    pub fn checked(&self, id: u32, text: &str, checked: bool) -> Result<(), AppError> {
        let flags = if checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        self.append(flags, id, text)
    }

    pub fn disabled(&self, text: &str) -> Result<(), AppError> {
        self.append(MF_STRING | MF_GRAYED, 0, text)
    }

    pub fn separator(&self) -> Result<(), AppError> {
        unsafe { AppendMenuW(self.handle, MF_SEPARATOR, 0, PCWSTR::null()) }
            .map_err(|error| AppError::Internal(format!("could not add menu separator: {error}")))
    }

    pub fn submenu(&self, text: &str, submenu: Menu) -> Result<(), AppError> {
        let title = HSTRING::from(text);
        unsafe { AppendMenuW(self.handle, MF_POPUP, submenu.handle.0 as usize, &title) }
            .map_err(|error| AppError::Internal(format!("could not add submenu: {error}")))?;
        std::mem::forget(submenu);
        Ok(())
    }

    fn append(
        &self,
        flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
        id: u32,
        text: &str,
    ) -> Result<(), AppError> {
        let text = HSTRING::from(text);
        unsafe { AppendMenuW(self.handle, flags, id as usize, &text) }
            .map_err(|error| AppError::Internal(format!("could not add menu item: {error}")))
    }

    pub fn track(self, window: HWND) -> u32 {
        unsafe {
            let _ = SetForegroundWindow(window);
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let command = TrackPopupMenu(
                self.handle,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
                point.x,
                point.y,
                Some(0),
                window,
                None,
            );
            let _ = PostMessageW(
                Some(window),
                WM_NULL,
                WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
            command.0 as u32
        }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.handle);
        }
    }
}
