use crate::core::candidates::is_candidate;
use crate::core::errors::AppError;
use crate::core::settings::WatchedFolder;
use notify::RecursiveMode;
use notify_debouncer_full::{DebouncedEvent, new_debouncer};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// How often missing folders are re-checked; cheap (one `exists()` per folder).
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const QUEUE_LIMIT: usize = 100;
/// Margin added before the last event batch when rescanning after an overflow.
const RESCAN_MARGIN: Duration = Duration::from_secs(10);

type PathQueue = Arc<Mutex<VecDeque<PathBuf>>>;

pub struct WatcherService {
    queue: PathQueue,
    unavailable: Arc<Mutex<Vec<PathBuf>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WatcherService {
    pub fn start(folders: &[WatchedFolder], window: usize) -> Result<Self, AppError> {
        Self::start_with(folders, window, HEALTH_INTERVAL)
    }

    /// Watches every enabled folder that exists; missing ones are retried every
    /// `health_interval` and scanned for missed files when they return.
    pub fn start_with(
        folders: &[WatchedFolder],
        window: usize,
        health_interval: Duration,
    ) -> Result<Self, AppError> {
        let (sender, receiver) = mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(2), None, sender)
            .map_err(|error| AppError::Internal(format!("could not start the watcher: {error}")))?;
        let started = SystemTime::now();
        let mut watched = Vec::new();
        let mut lost = Vec::new();
        for folder in folders.iter().filter(|folder| folder.enabled) {
            match debouncer.watch(&folder.path, RecursiveMode::Recursive) {
                Ok(()) => watched.push(folder.path.clone()),
                Err(error) => {
                    log::warn!("a watched folder is unavailable; will retry: {error}");
                    lost.push((folder.path.clone(), started));
                }
            }
        }
        let queue: PathQueue = Arc::new(Mutex::new(VecDeque::new()));
        let unavailable = Arc::new(Mutex::new(lost_paths(&lost)));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_queue, thread_unavailable, thread_stop) = (
            Arc::clone(&queue),
            Arc::clone(&unavailable),
            Arc::clone(&stop),
        );
        let thread = thread::spawn(move || {
            let mut last_health = Instant::now();
            let mut last_batch = started;
            while !thread_stop.load(Ordering::Relaxed) {
                let mut added = false;
                match receiver.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(events)) => {
                        let mut rescan = false;
                        for event in &events {
                            rescan |= event.need_rescan();
                            added |= enqueue_event(event, &thread_queue);
                        }
                        if rescan {
                            log::info!("watcher event buffer overflowed; rescanning folders");
                            let since = last_batch.checked_sub(RESCAN_MARGIN).unwrap_or(started);
                            let missed = changed_since(&watched, since, QUEUE_LIMIT);
                            added |= enqueue_paths(missed, &thread_queue);
                        }
                        last_batch = SystemTime::now();
                    }
                    Ok(Err(errors)) => {
                        for error in errors {
                            log::debug!("watcher error: {:?}", error.kind);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if last_health.elapsed() >= health_interval {
                    last_health = Instant::now();
                    let now = SystemTime::now();
                    watched.retain(|root| {
                        if root.exists() {
                            return true;
                        }
                        log::warn!("a watched folder became unavailable; will retry");
                        let _ = debouncer.unwatch(root);
                        lost.push((root.clone(), now));
                        false
                    });
                    let mut returned = Vec::new();
                    lost.retain(|(root, since)| {
                        let back = root.exists()
                            && debouncer.watch(root, RecursiveMode::Recursive).is_ok();
                        if back {
                            returned.push((root.clone(), *since));
                        }
                        !back
                    });
                    for (root, since) in returned {
                        log::info!("a watched folder is available again; scanning it");
                        let missed = changed_since(std::slice::from_ref(&root), since, QUEUE_LIMIT);
                        added |= enqueue_paths(missed, &thread_queue);
                        watched.push(root);
                    }
                    if let Ok(mut unavailable) = thread_unavailable.lock() {
                        *unavailable = lost_paths(&lost);
                    }
                }
                if added {
                    notify_window(window);
                }
            }
            drop(debouncer);
        });
        Ok(Self {
            queue,
            unavailable,
            stop,
            thread: Some(thread),
        })
    }

    pub fn drain(&self) -> Vec<PathBuf> {
        let mut queue = self.queue.lock().unwrap();
        queue.drain(..).collect()
    }

    /// Folders that are configured but currently missing or disconnected (FR-015).
    pub fn unavailable(&self) -> Vec<PathBuf> {
        self.unavailable
            .lock()
            .map(|paths| paths.clone())
            .unwrap_or_default()
    }
}

impl Drop for WatcherService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn lost_paths(lost: &[(PathBuf, SystemTime)]) -> Vec<PathBuf> {
    lost.iter().map(|(path, _)| path.clone()).collect()
}

fn notify_window(window: usize) {
    if window == 0 {
        return;
    }
    let hwnd = HWND(window as *mut core::ffi::c_void);
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            crate::app::window::WM_APP_WATCH,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

/// Candidate files under `roots` modified at or after `since`; used after resume, overflow,
/// and a folder coming back.
/// shortcut: capped at `limit` files and skips symlinked dirs; switch to a USN scan if folders get huge.
pub fn changed_since(roots: &[PathBuf], since: SystemTime, limit: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && is_candidate(&path)
                && entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .is_ok_and(|modified| modified >= since)
            {
                found.push(path);
                if found.len() >= limit {
                    return found;
                }
            }
        }
    }
    found
}

fn enqueue_event(event: &DebouncedEvent, queue: &PathQueue) -> bool {
    if !matches!(
        event.kind,
        notify::EventKind::Create(_) | notify::EventKind::Modify(_)
    ) {
        return false;
    }
    enqueue_paths(
        event
            .paths
            .iter()
            .filter(|path| is_candidate(path))
            .cloned(),
        queue,
    )
}

/// Adds paths not already queued, dropping the oldest past the limit. True if any were added.
fn enqueue_paths(paths: impl IntoIterator<Item = PathBuf>, queue: &PathQueue) -> bool {
    let Ok(mut queue) = queue.lock() else {
        return false;
    };
    let mut added = false;
    for path in paths {
        if queue.iter().any(|queued| queued == &path) {
            continue;
        }
        if queue.len() >= QUEUE_LIMIT {
            queue.pop_front();
        }
        queue.push_back(path);
        added = true;
    }
    added
}
