use crate::core::errors::AppError;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::HSTRING;

pub const MUTEX_NAME: &str = r"Local\VTTray.SingleInstance";

pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    pub fn acquire(name: &str) -> Result<Option<Self>, AppError> {
        let name = HSTRING::from(name);
        let handle = unsafe { CreateMutexW(None, true, &name) }.map_err(|error| {
            AppError::Internal(format!(
                "could not create the single-instance mutex: {error}"
            ))
        })?;
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_exists {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Ok(None);
        }
        Ok(Some(Self { handle }))
    }

    pub fn acquire_default() -> Result<Option<Self>, AppError> {
        Self::acquire(MUTEX_NAME)
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
