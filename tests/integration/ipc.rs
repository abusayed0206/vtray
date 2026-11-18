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

fn parse_handoff(data: &mut COPYDATASTRUCT) -> Option<ipc::Handoff> {
    unsafe { ipc::read_handoff(LPARAM(data as *const COPYDATASTRUCT as isize)) }
}

fn parse(data: &mut COPYDATASTRUCT) -> Option<std::path::PathBuf> {
    parse_handoff(data).map(|handoff| handoff.path)
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
        dwData: 99, // 1 = check and 2 = upload are the only request types
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
    for upload in [false, true] {
        let handoff = ipc::Handoff {
            path: std::path::PathBuf::from(r"C:\temp\sample.exe"),
            upload,
        };
        ipc::write_pending(&handoff, &file).unwrap();
        assert_eq!(ipc::take_pending(&file).unwrap(), handoff);
        assert!(ipc::take_pending(&file).is_none());
    }
}

#[test]
fn an_old_style_pending_file_is_a_plain_check() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pending-check.txt");
    std::fs::write(&file, r"C:\temp\old.exe").unwrap();
    let handoff = ipc::take_pending(&file).unwrap();
    assert!(!handoff.upload);
}

#[test]
fn upload_requests_use_their_own_message_type() {
    let mut data = wide(r"C:\temp\sample.exe");
    let mut copy = COPYDATASTRUCT {
        dwData: 2,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_mut_ptr().cast(),
    };
    assert!(parse_handoff(&mut copy).unwrap().upload);
    copy.dwData = 1;
    assert!(!parse_handoff(&mut copy).unwrap().upload);
    copy.dwData = 3;
    assert!(
        parse_handoff(&mut copy).is_none(),
        "unknown request types are rejected"
    );
}
