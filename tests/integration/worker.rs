use crate::mock_server::{MockResponse, TestEnv};
use std::time::{Duration, Instant};
use vt_tray::worker::{Worker, WorkerCommand, WorkerEvent};

fn check_result(offer_upload: bool) -> vt_tray::worker::CheckResult {
    let env = TestEnv::new(vec![MockResponse::json(404, "{}")]);
    let path = env.file("unknown.exe", b"MZ never seen");
    let mut worker = Worker::spawn(env.cache.clone(), env.client.clone(), env.quota.clone(), 0);
    worker
        .send(WorkerCommand::Check {
            path,
            manual: true,
            offer_upload,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for event in worker.drain_events() {
            if let WorkerEvent::CheckReady(result) = event {
                worker.shutdown();
                return result;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no check result");
}

#[test]
fn explorer_quick_check_never_carries_an_upload_offer() {
    assert!(
        !check_result(false).offer_upload,
        "FR-020: quick check is hash-only"
    );
    assert!(
        check_result(true).offer_upload,
        "the tray's own picker may offer upload"
    );
}

#[test]
fn worker_reports_failures_and_shuts_down_promptly() {
    let env = TestEnv::new(vec![MockResponse::json(500, "{}")]);
    let mut worker = Worker::spawn(env.cache.clone(), env.client.clone(), env.quota.clone(), 0);
    let missing = env.dir.path().join("missing.exe");
    worker
        .send(WorkerCommand::Check {
            path: missing.clone(),
            manual: true,
            offer_upload: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while events.is_empty() && Instant::now() < deadline {
        events.extend(worker.drain_events());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(matches!(
        events.as_slice(),
        [WorkerEvent::CheckFailed { path, .. }] if *path == missing
    ));
    let started = Instant::now();
    assert!(worker.shutdown());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        worker
            .send(WorkerCommand::Fingerprint { path: missing })
            .is_err()
    );
}
