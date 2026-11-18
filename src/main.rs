#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = vt_tray::app::run() {
        eprintln!("{}", error.safe_message());
    }
}
