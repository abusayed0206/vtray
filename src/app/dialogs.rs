use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, CreateFontIndirectW, DT_CALCRECT, DT_NOPREFIX, DT_WORDBREAK, DeleteObject,
    DrawTextW, GetDC, GetMonitorInfoW, GetSysColorBrush, HDC, HFONT, HGDIOBJ, HMONITOR,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForMonitor, MDT_EFFECTIVE_DPI, SystemParametersInfoForDpi,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_DEFPUSHBUTTON, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    ES_AUTOHSCROLL, ES_PASSWORD, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IsDialogMessageW, LoadCursorW, MSG,
    NONCLIENTMETRICSW, PM_REMOVE, PeekMessageW, RegisterClassW, SPI_GETNONCLIENTMETRICS, SW_SHOW,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{HSTRING, PCWSTR};

const ID_OK: usize = 1;
const ID_CANCEL: usize = 2;
const ID_EDIT: usize = 3;

struct DialogState {
    result: Option<String>,
    done: AtomicBool,
}

pub fn prompt_secret(owner: HWND, title: &str, message: &str) -> Option<String> {
    unsafe {
        let _ = EnableWindow(owner, false);
        let result = create_dialog(owner, title, message);
        let _ = EnableWindow(owner, true);
        result
    }
}

/// Client-area layout in physical pixels, derived from the monitor DPI and the message font.
struct Layout {
    dpi: u32,
    margin: i32,
    gap: i32,
    width: i32,
    message_h: i32,
    edit_h: i32,
    hint_h: i32,
    button_w: i32,
    button_h: i32,
}

impl Layout {
    fn client_size(&self) -> (i32, i32) {
        let height = self.margin
            + self.message_h
            + self.gap
            + self.edit_h
            + self.gap / 2
            + self.hint_h
            + self.margin
            + self.button_h
            + self.margin;
        (self.width + 2 * self.margin, height)
    }
}

use crate::ui::strings::KEY_HINT as HINT;

fn measure(dc: HDC, text: &str, width: i32) -> i32 {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: 0,
    };
    unsafe {
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
        );
    }
    rect.bottom
}

/// DPI of the monitor under the cursor, where the dialog will open.
fn cursor_monitor() -> (HMONITOR, u32) {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let (mut dpi_x, mut dpi_y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        (monitor, dpi_x)
    }
}

fn message_font(dpi: u32) -> HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut core::ffi::c_void),
            0,
            dpi,
        );
        CreateFontIndirectW(&metrics.lfMessageFont)
    }
}

fn layout_for(dpi: u32, font: HFONT, message: &str) -> Layout {
    let scale = |value: i32| value * dpi as i32 / 96;
    let width = scale(380);
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let message_h = measure(dc, message, width);
        let hint_h = measure(dc, HINT, width);
        let line_h = measure(dc, "Ag", width);
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        Layout {
            dpi,
            margin: scale(16),
            gap: scale(12),
            width,
            message_h,
            edit_h: line_h + scale(10),
            hint_h,
            button_w: scale(88),
            button_h: line_h + scale(12),
        }
    }
}

fn create_dialog(owner: HWND, title: &str, message: &str) -> Option<String> {
    let module = unsafe { GetModuleHandleW(None) }.ok()?;
    let instance = windows::Win32::Foundation::HINSTANCE(module.0);
    let class_name = HSTRING::from("VTTraySecretDialog");
    let class = WNDCLASSW {
        hInstance: instance,
        lpfnWndProc: Some(dialog_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
    }
    let mut state = DialogState {
        result: None,
        done: AtomicBool::new(false),
    };
    // shortcut: sized once for the opening monitor; dragging to a different-DPI monitor does not relayout.
    let (monitor, dpi) = cursor_monitor();
    let font = message_font(dpi);
    let layout = layout_for(dpi, font, message);
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let ex_style = WS_EX_DLGMODALFRAME;
    let (client_w, client_h) = layout.client_size();
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, layout.dpi);
    }
    let (outer_w, outer_h) = (frame.right - frame.left, frame.bottom - frame.top);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let work = if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: 1024,
            bottom: 768,
        }
    };
    let x = work.left + (work.right - work.left - outer_w) / 2;
    let y = work.top + (work.bottom - work.top - outer_h) / 2;
    let title = HSTRING::from(title);
    let window = unsafe {
        CreateWindowExW(
            ex_style,
            &class_name,
            &title,
            style,
            x,
            y,
            outer_w,
            outer_h,
            Some(owner),
            None,
            Some(instance),
            None,
        )
    }
    .ok()?;
    unsafe {
        SetWindowLongPtrW(
            window,
            GWLP_USERDATA,
            &mut state as *mut DialogState as isize,
        );
    }
    let l = &layout;
    let mut top = l.margin;
    create_control(
        window,
        font,
        "STATIC",
        message,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x80), /* SS_NOPREFIX */
        Default::default(),
        (l.margin, top, l.width, l.message_h),
        0,
    )?;
    top += l.message_h + l.gap;
    let edit = create_control(
        window,
        font,
        "EDIT",
        "",
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WINDOW_STYLE(ES_PASSWORD as u32)
            | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        WS_EX_CLIENTEDGE,
        (l.margin, top, l.width, l.edit_h),
        ID_EDIT,
    )?;
    top += l.edit_h + l.gap / 2;
    create_control(
        window,
        font,
        "STATIC",
        HINT,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x80), /* SS_NOPREFIX */
        Default::default(),
        (l.margin, top, l.width, l.hint_h),
        0,
    )?;
    top += l.hint_h + l.margin;
    let right = l.margin + l.width;
    create_control(
        window,
        font,
        "BUTTON",
        "Save key",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        Default::default(),
        (
            right - 2 * l.button_w - l.gap / 2,
            top,
            l.button_w,
            l.button_h,
        ),
        ID_OK,
    )?;
    create_control(
        window,
        font,
        "BUTTON",
        "Cancel",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        Default::default(),
        (right - l.button_w, top, l.button_w, l.button_h),
        ID_CANCEL,
    )?;
    unsafe {
        let _ = ShowWindow(window, SW_SHOW);
        let _ = UpdateWindow(window);
        let _ = SetForegroundWindow(window);
        let _ = SetFocus(Some(edit));
    }
    while !state.done.load(Ordering::Relaxed) {
        let mut message = MSG::default();
        let has_message = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool();
        if has_message {
            unsafe {
                if !IsDialogMessageW(window, &message).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(window)).as_bool() {
            let _ = DestroyWindow(window);
        }
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
    state.result.take()
}

#[allow(clippy::too_many_arguments)]
fn create_control(
    owner: HWND,
    font: HFONT,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    (x, y, width, height): (i32, i32, i32, i32),
    id: usize,
) -> Option<HWND> {
    let class = HSTRING::from(class);
    let text = HSTRING::from(text);
    let menu = if id == 0 {
        None
    } else {
        Some(HMENU(id as *mut core::ffi::c_void))
    };
    let control = unsafe {
        CreateWindowExW(
            ex_style,
            &class,
            &text,
            style,
            x,
            y,
            width,
            height,
            Some(owner),
            menu,
            None,
            None,
        )
    }
    .ok()?;
    unsafe {
        SendMessageW(
            control,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
    Some(control)
}

unsafe extern "system" fn dialog_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if pointer == 0 {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &mut *(pointer as *mut DialogState) };
    match message {
        WM_COMMAND => {
            let id = wparam.0 & 0xffff;
            if id == ID_OK {
                if let Some(value) = read_edit(window)
                    && !value.trim().is_empty()
                {
                    state.result = Some(value);
                    state.done.store(true, Ordering::Relaxed);
                    unsafe {
                        let _ = DestroyWindow(window);
                    }
                }
            } else if id == ID_CANCEL {
                state.done.store(true, Ordering::Relaxed);
                unsafe {
                    let _ = DestroyWindow(window);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            state.done.store(true, Ordering::Relaxed);
            unsafe {
                let _ = DestroyWindow(window);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            state.done.store(true, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn read_edit(window: HWND) -> Option<String> {
    let edit = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(window), ID_EDIT as i32)
    }
    .ok()?;
    let length = unsafe { GetWindowTextLengthW(edit) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize + 1];
    let written = unsafe { GetWindowTextW(edit, &mut buffer) };
    if written <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..written as usize]))
}
