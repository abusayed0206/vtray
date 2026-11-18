use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use vt_tray::platform::windows::ipc;
use vt_tray::platform::windows::single_instance::SingleInstance;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;

fn wide(path: &str) -> Vec<u16> {
    OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn parse(data: &mut COPYDATASTRUCT) -> Option<std::path::PathBuf> {
    unsafe { ipc::read_check_path(LPARAM(data as *const COPYDATASTRUCT as isize)) }
}

#[test]
fn accepts_an_absolute_utf16_path() {
    let mut data = wide(r"C:\temp\sample.exe");
    let mut copy = COPYDATASTRUCT {
        dwData: 1,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_mut_ptr().cast(),
    };
    assert_eq!(
        parse(&mut copy).unwrap(),
        std::path::PathBuf::from(r"C:\temp\sample.exe")
    );
}

#[test]
fn rejects_malformed_copy_data() {
    let mut data = wide(r"C:\temp\sample.exe");
    let null_terminated = COPYDATASTRUCT {
        dwData: 1,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_mut_ptr().cast(),
    };
    let mut wrong_id = COPYDATASTRUCT {
        dwData: 2,
        ..null_terminated
    };
    assert!(parse(&mut wrong_id).is_none());
    let mut no_terminator = COPYDATASTRUCT {
        cbData: ((data.len() - 1) * 2) as u32,
        ..null_terminated
    };
    assert!(parse(&mut no_terminator).is_none());
    let invalid_utf16 = [0x66_u16, 0xd800, 0];
    let mut invalid = COPYDATASTRUCT {
        dwData: 1,
        cbData: (invalid_utf16.len() * 2) as u32,
        lpData: invalid_utf16.as_ptr() as *mut _,
    };
    assert!(parse(&mut invalid).is_none());
    let mut odd = COPYDATASTRUCT {
        dwData: 1,
        cbData: 3,
        lpData: data.as_mut_ptr().cast(),
    };
    assert!(parse(&mut odd).is_none());
    let mut null_data = COPYDATASTRUCT {
        dwData: 1,
        cbData: 4,
        lpData: std::ptr::null_mut(),
    };
    assert!(parse(&mut null_data).is_none());
    let mut too_large = COPYDATASTRUCT {
        dwData: 1,
        cbData: 64 * 1024 + 2,
        lpData: data.as_mut_ptr().cast(),
    };
    assert!(parse(&mut too_large).is_none());
    let mut relative = wide(r"relative.exe");
    let mut relative_data = COPYDATASTRUCT {
        dwData: 1,
        cbData: (relative.len() * 2) as u32,
        lpData: relative.as_mut_ptr().cast(),
    };
    assert!(parse(&mut relative_data).is_none());
}

#[test]
fn a_second_instance_is_rejected_and_released_on_drop() {
    let name = format!(
        r"Local\VTTray.Test.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let first = SingleInstance::acquire(&name).unwrap();
    assert!(first.is_some());
    assert!(SingleInstance::acquire(&name).unwrap().is_none());
    drop(first);
    assert!(SingleInstance::acquire(&name).unwrap().is_some());
}

#[test]
fn pending_file_round_trips_and_is_consumed_once() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pending-check.txt");
    let target = std::path::PathBuf::from(r"C:\temp\sample.exe");
    ipc::write_pending(&target, &file).unwrap();
    assert_eq!(ipc::take_pending(&file).unwrap(), target);
    assert!(ipc::take_pending(&file).is_none());
}
