#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The binary is named VTray (product name), not snake_case.
#![allow(non_snake_case)]

fn main() {
    if let Err(error) = vt_tray::app::run() {
        eprintln!("{}", error.safe_message());
    }
}
