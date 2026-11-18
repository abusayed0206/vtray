use crate::app::App;
use crate::core::errors::AppError;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, RegisterClassW, RegisterWindowMessageW, SetWindowLongPtrW,
    TranslateMessage, WM_COMMAND, WM_COPYDATA, WM_DESTROY, WM_NCCREATE, WM_QUIT, WM_TIMER,
    WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR};

pub const WM_APP_EVENT: u32 = 0x8000 + 2;
pub const WM_APP_WATCH: u32 = 0x8000 + 3;
pub const WM_APP_PENDING: u32 = 0x8000 + 4;
pub const HIDDEN_CLASS: &str = crate::platform::windows::ipc::HIDDEN_WINDOW_CLASS;

pub fn create_hidden_window() -> Result<HWND, AppError> {
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| AppError::Internal(format!("could not get the module handle: {error}")))?;
    let instance = HINSTANCE(module.0);
    let class_name = HSTRING::from(HIDDEN_CLASS);
    let wnd_class = WNDCLASSW {
        hInstance: instance,
        lpfnWndProc: Some(window_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&wnd_class);
    }
    let title = HSTRING::from("VT Tray");
    let window = unsafe {
        CreateWindowExW(
            Default::default(),
            &class_name,
            &title,
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .map_err(|error| AppError::Internal(format!("could not create the tray window: {error}")))?;
    Ok(window)
}

pub fn install_app(window: HWND, app: Box<App>) {
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(app) as isize);
    }
}

pub fn initialize_app(window: HWND) {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if pointer != 0 {
        let app = unsafe { &mut *(pointer as *mut App) };
        app.initialize();
    }
}

pub fn remove_app(window: HWND) -> Option<Box<App>> {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if pointer == 0 {
        return None;
    }
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, 0);
        Some(Box::from_raw(pointer as *mut App))
    }
}

pub fn message_loop() -> Result<(), AppError> {
    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 {
            return Err(AppError::Internal("the message loop failed".to_string()));
        }
        if result.0 == 0 || message.message == WM_QUIT {
            return Ok(());
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    let app = pointer as *mut App;
    if app.is_null() {
        if message == WM_NCCREATE {
            return LRESULT(1);
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let app = unsafe { &mut *app };
    if message != WM_TIMER {
        app.note_activity();
    }
    match message {
        WM_APP_EVENT => {
            app.handle_worker_events();
            LRESULT(0)
        }
        WM_APP_WATCH => {
            app.handle_watch_events();
            LRESULT(0)
        }
        WM_TIMER => {
            app.on_timer(wparam.0);
            LRESULT(0)
        }
        WM_APP_PENDING => {
            app.process_pending_checks();
            LRESULT(0)
        }
        crate::app::tray::TRAY_CALLBACK_MESSAGE => {
            app.handle_tray_event(lparam.0 as u32);
            LRESULT(0)
        }
        WM_COMMAND => {
            app.handle_command(wparam.0 as u32 & 0xffff);
            LRESULT(0)
        }
        WM_COPYDATA => {
            let accepted = app.handle_copy_data(lparam);
            LRESULT(if accepted { 1 } else { 0 })
        }
        WM_DESTROY => {
            app.shutdown();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ if message == app.taskbar_message() => {
            app.handle_taskbar_created();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

pub fn taskbar_created_message() -> u32 {
    let name = HSTRING::from("TaskbarCreated");
    unsafe { RegisterWindowMessageW(&name) }
}
