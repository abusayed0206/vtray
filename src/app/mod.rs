pub mod dialogs;
pub mod logging;
pub mod menu;
pub mod tray;
pub mod window;

use crate::api::client::VtClient;
use crate::core::cache::Cache;
use crate::core::config::{AppPaths, ConfigStore, now_stamp};
use crate::core::errors::AppError;
use crate::core::hash::safe_base_name;
use crate::core::policy::{requires_large_warning, upload_route};
use crate::core::quota::QuotaGate;
use crate::core::settings::Preferences;
use crate::core::types::{LookupOutcome, LookupSummary};
use crate::platform::windows::autostart;
use crate::platform::windows::context_menu;
use crate::platform::windows::credential::CredentialStore;
use crate::platform::windows::ipc;
use crate::platform::windows::setup as platform_setup;
use crate::platform::windows::shell;
use crate::platform::windows::single_instance::SingleInstance;
use crate::ui::strings as s;
use crate::watcher::WatcherService;
use crate::worker::upload::UploadOutcome;
use crate::worker::{CheckResult, DetectionResult, Worker, WorkerCommand, WorkerEvent};
use chrono::{DateTime, Utc};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
};
use zeroize::Zeroizing;

const VIRUSTOTAL_BASE_URL: &str = "https://www.virustotal.com/api/v3";

const CMD_CHECK_FILE: u32 = 1001;
const CMD_ADD_FOLDER: u32 = 1002;
const CMD_HISTORY: u32 = 1003;
const CMD_TOGGLE_AUTOSTART: u32 = 1010;
const CMD_TOGGLE_NOTIFICATIONS: u32 = 1011;
const CMD_TOGGLE_AUTO_CHECK: u32 = 1012;
const CMD_TOGGLE_PAUSE: u32 = 1013;
const CMD_SET_KEY: u32 = 1014;
const CMD_REMOVE_KEY: u32 = 1015;
const CMD_TOGGLE_CONTEXT_MENU: u32 = 1016;
const CMD_OPEN_LOGS: u32 = 1020;
const CMD_CLEAR_DATA: u32 = 1021;
const CMD_REMOVE_APP: u32 = 1022;
const CMD_REMOVE_FOLDER_BASE: u32 = 2000;
const CMD_EXIT: u32 = 1099;
const PENDING_TIMER_ID: usize = 1;
const PROGRESS_TIMER_ID: usize = 2;

#[derive(Debug, Clone)]
struct PendingCheck {
    path: PathBuf,
    /// True for the tray picker and Explorer "Upload with VT Tray...": may prompt for a key
    /// and may offer an upload (always behind the consent dialog). False for quick checks.
    allow_key_prompt: bool,
}

impl From<ipc::Handoff> for PendingCheck {
    fn from(handoff: ipc::Handoff) -> Self {
        Self {
            path: handoff.path,
            allow_key_prompt: handoff.upload,
        }
    }
}

pub fn run() -> Result<(), AppError> {
    match mode() {
        Mode::Cleanup => return run_cleanup(),
        Mode::Tray => {}
    }
    let paths = AppPaths::discover()?;
    let startup_check = handoff_argument();
    let _instance = SingleInstance::acquire_default()?;
    if _instance.is_none() {
        if let Some(handoff) = startup_check
            && !ipc::send_with_timeout(&handoff, std::time::Duration::from_secs(3))?
        {
            ipc::write_pending(&handoff, &paths.pending_check_file)?;
        }
        return Ok(());
    }
    paths.ensure()?;
    let initial = ConfigStore::new(paths.config_file.clone()).load()?;
    logging::init(&paths.log_dir, &initial.log_level)?;
    let window = window::create_hidden_window()?;
    let app = match App::new(window, paths, startup_check) {
        Ok(app) => Box::new(app),
        Err(error) => {
            unsafe {
                let _ = DestroyWindow(window);
            }
            return Err(error);
        }
    };
    window::install_app(window, app);
    window::initialize_app(window);
    log::info!("VT Tray started");
    let result = window::message_loop();
    if let Some(mut app) = window::remove_app(window) {
        app.shutdown();
        drop(app);
    }
    result
}

enum Mode {
    Tray,
    Cleanup,
}

/// `--cleanup` is run silently by the Inno Setup uninstaller; anything else starts the tray.
fn mode() -> Mode {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--cleanup")
    {
        Mode::Cleanup
    } else {
        Mode::Tray
    }
}

/// Uninstall hook: close a running tray, then remove what the installer does not track
/// (startup and Explorer entries, the saved key, settings, data, logs). No dialogs.
fn run_cleanup() -> Result<(), AppError> {
    ipc::close_running(std::time::Duration::from_secs(5));
    let paths = AppPaths::discover()?;
    let (problems, _) = remove_everything(&paths, &CredentialStore::new(), true);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(AppError::Internal(problems.join("; ")))
    }
}

/// Steps shared by "Remove VT Tray from this PC" (portable copies) and the uninstaller's
/// `--cleanup` (constitution V). Every step runs even if an earlier one fails.
/// `delete_now`: no VT Tray process holds the folders open, so delete them immediately;
/// otherwise they are deleted shortly after this process exits.
/// Returns the problems and, for a portable copy, the program file the user must delete.
fn remove_everything(
    paths: &AppPaths,
    credentials: &CredentialStore,
    delete_now: bool,
) -> (Vec<String>, Option<PathBuf>) {
    let mut problems = Vec::new();
    let executable = std::env::current_exe().unwrap_or_default();
    if let Err(error) = autostart::set_enabled(&executable, false) {
        problems.push(format!("Windows startup entry: {}", error.safe_message()));
    }
    if let Err(error) = context_menu::unregister() {
        problems.push(format!("Explorer menu entries: {}", error.safe_message()));
    }
    if let Err(error) = credentials.delete_key() {
        problems.push(format!("API key: {}", error.safe_message()));
    }
    let folders = [paths.config_dir.clone(), paths.data_dir.clone()];
    if delete_now {
        problems.extend(platform_setup::delete_now(&folders));
    } else {
        for file in [&paths.config_file, &paths.pending_check_file] {
            match std::fs::remove_file(file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => problems.push(format!("{}: {error}", safe_base_name(file))),
            }
        }
        if let Err(error) = platform_setup::schedule_delete(&folders) {
            problems.push(format!("remaining files: {}", error.safe_message()));
        }
    }
    let portable = platform_setup::uninstaller_for(&executable).is_none();
    (problems, portable.then_some(executable))
}

/// `--check <file>` (Explorer "Check with VT Tray", hash-only) or `--upload <file>`
/// (Explorer "Upload with VT Tray...", which still asks for consent).
fn handoff_argument() -> Option<ipc::Handoff> {
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        let upload = match argument.to_string_lossy().as_ref() {
            "--check" => false,
            "--upload" => true,
            _ => continue,
        };
        return arguments.next().map(|path| ipc::Handoff {
            path: PathBuf::from(path),
            upload,
        });
    }
    None
}

pub struct App {
    window: HWND,
    paths: AppPaths,
    config: ConfigStore,
    preferences: Preferences,
    cache: Arc<Cache>,
    credentials: Arc<CredentialStore>,
    quota: Arc<QuotaGate>,
    worker: Worker,
    watcher: Option<WatcherService>,
    // shortcut: in memory only, so files added while paused across a restart are not rescanned.
    paused_at: Option<std::time::SystemTime>,
    upload_sent: Arc<std::sync::atomic::AtomicU64>,
    upload_total: Option<u64>,
    tray: Option<tray::TrayIcon>,
    taskbar_message: u32,
    pending: VecDeque<PendingCheck>,
    startup_check: Option<ipc::Handoff>,
    shutting_down: bool,
    /// Set after "Remove VT Tray from this PC"; shutdown must not write settings back.
    removed: bool,
}

impl App {
    fn new(
        window: HWND,
        paths: AppPaths,
        startup_check: Option<ipc::Handoff>,
    ) -> Result<Self, AppError> {
        let config = ConfigStore::new(paths.config_file.clone());
        let preferences = config.load()?;
        let cache = Arc::new(Cache::open(&paths.db_file)?);
        let credentials = Arc::new(CredentialStore::new());
        let client = Arc::new(VtClient::new(credentials.clone(), VIRUSTOTAL_BASE_URL));
        let quota = Arc::new(QuotaGate::new(cache.clone()));
        let worker = Worker::spawn(
            cache.clone(),
            client.clone(),
            quota.clone(),
            window.0 as usize,
        );
        Ok(Self {
            window,
            paths,
            config,
            preferences,
            cache,
            credentials,
            quota,
            worker,
            watcher: None,
            paused_at: None,
            upload_sent: client.upload_progress(),
            upload_total: None,
            tray: None,
            taskbar_message: window::taskbar_created_message(),
            pending: VecDeque::new(),
            startup_check,
            shutting_down: false,
            removed: false,
        })
    }

    pub(crate) fn initialize(&mut self) {
        if let Some(handoff) = self.startup_check.take() {
            self.pending.push_back(PendingCheck::from(handoff));
        }
        if let Some(handoff) = ipc::take_pending(&self.paths.pending_check_file) {
            self.pending.push_back(PendingCheck::from(handoff));
        }
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(
                Some(self.window),
                PENDING_TIMER_ID,
                5000,
                None,
            );
        }
        match tray::TrayIcon::new(self.window) {
            Ok(tray) => self.tray = Some(tray),
            Err(error) => log::warn!(
                "tray icon was not created at startup: {}",
                error.safe_message()
            ),
        }
        if let Ok(enabled) = autostart::is_enabled() {
            self.preferences.autostart_enabled = enabled;
        }
        let now = now_stamp() as i64;
        // Uploads cut off by the last exit are never resumed automatically; only the user retries.
        for job in self.cache.resumable_uploads().unwrap_or_default() {
            let _ = self.cache.update_upload(
                job.id,
                crate::core::types::UploadStatus::Failed,
                None,
                job.upload_url_used,
                job.attempts,
                now,
                Some(s::UPLOAD_INTERRUPTED),
            );
        }
        if let Err(error) = self.cache.prune(now, self.preferences.cache_retention_days) {
            log::warn!("cache pruning failed: {}", error.safe_message());
        }
        self.refresh_tooltip();
        self.restart_watcher();
        self.process_pending_checks();
    }

    fn persist(&mut self) {
        if let Err(error) = self.config.save(&self.preferences) {
            log::error!("could not save preferences: {}", error.safe_message());
            self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
        }
    }

    fn refresh_tooltip(&mut self) {
        let has_key = self.credentials.has_key().unwrap_or(false);
        let state = if !has_key {
            s::TIP_KEY_NEEDED
        } else if self.preferences.monitoring_paused {
            s::TIP_PAUSED
        } else {
            s::TIP_READY
        };
        if let Some(tray) = self.tray.as_mut() {
            let _ = tray.set_tooltip(&s::tooltip(state));
        }
    }

    fn ensure_api_key(&mut self) -> bool {
        match self.credentials.has_key() {
            Ok(true) => true,
            Ok(false) => self.prompt_for_key(),
            Err(error) => {
                self.show_message(
                    s::KEY_TITLE,
                    &s::key_read_failed(&error.safe_message()),
                    MessageLevel::Error,
                );
                false
            }
        }
    }

    fn prompt_for_key(&mut self) -> bool {
        let Some(key) = dialogs::prompt_secret(self.window, s::KEY_TITLE, s::KEY_PROMPT) else {
            return false;
        };
        let key = Zeroizing::new(key);
        match self.credentials.set_key(key.as_str()) {
            Ok(()) => {
                self.refresh_tooltip();
                if let Some(tray) = self.tray.as_mut() {
                    let _ = tray.notify(s::APP, s::KEY_SAVED, false);
                }
                true
            }
            Err(error) => {
                self.show_message(
                    s::KEY_TITLE,
                    &s::key_save_failed(&error.safe_message()),
                    MessageLevel::Error,
                );
                false
            }
        }
    }

    pub fn taskbar_message(&self) -> u32 {
        self.taskbar_message
    }

    pub fn handle_taskbar_created(&mut self) {
        match self.tray.as_mut() {
            Some(tray) => {
                if let Err(error) = tray.add() {
                    log::warn!("could not restore the tray icon: {}", error.safe_message());
                }
            }
            None => match tray::TrayIcon::new(self.window) {
                Ok(tray) => self.tray = Some(tray),
                Err(error) => {
                    log::warn!("could not restore the tray icon: {}", error.safe_message())
                }
            },
        }
        self.refresh_tooltip();
    }

    pub fn handle_tray_event(&mut self, event: u32) {
        match event & 0xffff {
            WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU => self.show_menu(),
            WM_LBUTTONDBLCLK => self.pick_and_check(),
            _ => {}
        }
    }

    pub fn handle_command(&mut self, command: u32) {
        if (CMD_REMOVE_FOLDER_BASE..CMD_REMOVE_FOLDER_BASE + 1000).contains(&command) {
            self.remove_watched_folder((command - CMD_REMOVE_FOLDER_BASE) as usize);
            return;
        }
        match command {
            CMD_CHECK_FILE => self.pick_and_check(),
            CMD_ADD_FOLDER => self.add_watched_folder(),
            CMD_HISTORY => self.show_history(),
            CMD_TOGGLE_AUTOSTART => self.toggle_autostart(),
            CMD_TOGGLE_NOTIFICATIONS => {
                self.preferences.notifications_enabled = !self.preferences.notifications_enabled;
                self.persist();
                self.refresh_tooltip();
            }
            CMD_TOGGLE_AUTO_CHECK => {
                if !self.preferences.auto_check_enabled && !self.ensure_api_key() {
                    return;
                }
                self.preferences.auto_check_enabled = !self.preferences.auto_check_enabled;
                self.persist();
                self.refresh_tooltip();
            }
            CMD_TOGGLE_PAUSE => {
                self.preferences.monitoring_paused = !self.preferences.monitoring_paused;
                self.persist();
                self.refresh_tooltip();
                self.restart_watcher();
                if self.preferences.monitoring_paused {
                    self.paused_at = Some(std::time::SystemTime::now());
                } else if let Some(since) = self.paused_at.take() {
                    let roots: Vec<PathBuf> = self
                        .preferences
                        .watched_folders
                        .iter()
                        .filter(|folder| folder.enabled)
                        .map(|folder| folder.path.clone())
                        .collect();
                    let missed = crate::watcher::changed_since(&roots, since, 100);
                    self.queue_detections(missed);
                }
            }
            CMD_SET_KEY => {
                self.prompt_for_key();
            }
            CMD_REMOVE_KEY => self.remove_key(),
            CMD_TOGGLE_CONTEXT_MENU => self.toggle_context_menu(),
            CMD_OPEN_LOGS => {
                if let Err(error) = shell::open_path(&self.paths.log_dir) {
                    self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
                }
            }
            CMD_CLEAR_DATA => self.clear_local_data(),
            CMD_REMOVE_APP => self.remove_from_pc(),
            CMD_EXIT => unsafe {
                let _ = DestroyWindow(self.window);
            },
            _ => {}
        }
    }

    pub fn handle_copy_data(&mut self, lparam: windows::Win32::Foundation::LPARAM) -> bool {
        let handoff = unsafe { ipc::read_handoff(lparam) };
        if let Some(handoff) = handoff {
            self.pending.push_back(PendingCheck::from(handoff));
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(self.window),
                    window::WM_APP_PENDING,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
            true
        } else {
            false
        }
    }

    pub fn process_pending_checks(&mut self) {
        while let Some(pending) = self.pending.pop_front() {
            self.queue_check(pending.path, pending.allow_key_prompt);
        }
    }

    /// Shows the most urgent quota warning not yet shown this day or month.
    fn show_quota_warning(&mut self) {
        let warnings = match self.quota.take_new_warnings(now_stamp() as i64) {
            Ok(warnings) => warnings,
            Err(error) => {
                log::warn!("quota warning check failed: {}", error.safe_message());
                return;
            }
        };
        let Some(&warning) = warnings.first() else {
            return;
        };
        log::info!("quota warning: {warning:?}");
        if self.preferences.notifications_enabled
            && let Some(tray) = self.tray.as_mut()
        {
            let _ = tray.notify(
                s::QUOTA_WARNING_TITLE,
                &s::quota_warning_body(warning),
                true,
            );
        }
    }

    pub fn handle_worker_events(&mut self) {
        let events = self.worker.drain_events();
        if !events.is_empty() {
            log::info!("UI handling {} worker event(s)", events.len());
            // Requests were just made, so this is when a limit can come close (SC-005).
            self.show_quota_warning();
        }
        for event in events {
            match event {
                WorkerEvent::CheckReady(result) => self.handle_check_ready(result),
                WorkerEvent::FingerprintReady {
                    path,
                    sha256,
                    size_bytes,
                } => self.handle_fingerprint_ready(path, sha256, size_bytes),
                WorkerEvent::FingerprintFailed { path, message } => {
                    let context = s::could_not_fingerprint(&safe_base_name(&path));
                    self.handle_worker_error(&context, &message, false, None);
                }
                WorkerEvent::CheckFailed {
                    path,
                    message,
                    auth,
                    retry_after,
                } => {
                    let context = s::could_not_check(&safe_base_name(&path));
                    self.handle_worker_error(&context, &message, auth, retry_after);
                }
                WorkerEvent::Detected(result) => self.handle_detected(result),
                WorkerEvent::UploadFinished(outcome) => self.handle_upload_finished(outcome),
                WorkerEvent::UploadFailed {
                    path,
                    message,
                    auth,
                    retry_after,
                } => {
                    self.stop_upload_progress();
                    let context = s::could_not_upload(&safe_base_name(&path));
                    self.handle_worker_error(&context, &message, auth, retry_after);
                }
            }
        }
    }

    fn handle_fingerprint_ready(&mut self, path: PathBuf, sha256: String, size_bytes: u64) {
        self.show_message(
            s::KEY_NEEDED_TITLE,
            &s::key_needed_fingerprint(&safe_base_name(&path), &format_bytes(size_bytes), &sha256),
            MessageLevel::Warning,
        );
    }

    fn handle_check_ready(&mut self, result: CheckResult) {
        self.refresh_tooltip();
        match &result.outcome {
            LookupOutcome::Known(summary) => {
                self.show_summary(Some(&result.path), summary, result.cached);
            }
            LookupOutcome::Unknown { .. } if result.offer_upload => self.confirm_upload(result),
            LookupOutcome::Unknown { .. } => self.show_message(
                s::RESULT_TITLE,
                &s::quick_check_unknown(&safe_base_name(&result.path), &result.sha256),
                MessageLevel::Info,
            ),
        }
    }

    pub fn on_timer(&mut self, id: usize) {
        if id == PROGRESS_TIMER_ID {
            self.show_upload_progress();
        } else {
            self.poll_pending();
        }
    }

    fn show_upload_progress(&mut self) {
        let Some(total) = self.upload_total.filter(|total| *total > 0) else {
            return;
        };
        let sent = self
            .upload_sent
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(total);
        let text = if sent >= total {
            s::tooltip(s::TIP_ANALYZING)
        } else {
            s::tip_uploading(sent * 100 / total)
        };
        if let Some(tray) = self.tray.as_mut() {
            let _ = tray.set_tooltip(&text);
        }
    }

    fn stop_upload_progress(&mut self) {
        self.upload_total = None;
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                Some(self.window),
                PROGRESS_TIMER_ID,
            );
        }
    }

    pub fn poll_pending(&mut self) {
        if let Some(handoff) = ipc::take_pending(&self.paths.pending_check_file) {
            let pending = PendingCheck::from(handoff);
            self.queue_check(pending.path, pending.allow_key_prompt);
        }
    }

    pub fn handle_watch_events(&mut self) {
        let Some(watcher) = self.watcher.as_ref() else {
            return;
        };
        let paths = watcher.drain();
        if paths.is_empty() {
            return;
        }
        if self.preferences.monitoring_paused {
            return;
        }
        self.queue_detections(paths);
    }

    fn queue_detections(&mut self, paths: Vec<PathBuf>) {
        use crate::core::policy::{CredentialStatus, auto_check_allowed};
        let credential = if self.credentials.has_key().unwrap_or(false) {
            CredentialStatus::Present
        } else {
            CredentialStatus::Absent
        };
        let budget = self
            .quota
            .status(now_stamp() as i64)
            .map(|status| status.auto_remaining)
            .unwrap_or(0);
        let auto_check = auto_check_allowed(
            self.preferences.auto_check_enabled,
            self.preferences.monitoring_paused,
            credential,
            budget,
        );
        for path in paths {
            if let Err(error) = self.worker.send(WorkerCommand::Detect { path, auto_check }) {
                log::debug!("could not queue a watcher event: {}", error.safe_message());
                break;
            }
        }
    }

    fn handle_detected(&mut self, result: DetectionResult) {
        if result.error.is_some() {
            log::warn!(
                "automatic check failed for {}: {}",
                safe_base_name(&result.path),
                result.error.as_deref().unwrap_or_default()
            );
        }
        if !result.notify || !self.preferences.notifications_enabled {
            return;
        }
        let name = result
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| safe_base_name(&result.path));
        let flagged = result
            .summary
            .as_ref()
            .map(|summary| summary.flagged())
            .unwrap_or(0);
        if flagged > 0 {
            if let Some(tray) = self.tray.as_mut() {
                let _ = tray.notify(
                    s::DETECTIONS_TITLE,
                    &s::detections_body(&name, flagged),
                    true,
                );
            }
        } else if result.summary.is_none()
            && result.error.is_none()
            && let Some(tray) = self.tray.as_mut()
        {
            let _ = tray.notify(s::NEW_FILE_TITLE, &s::new_file_body(&name), false);
        }
    }

    fn restart_watcher(&mut self) {
        self.watcher.take();
        if self.shutting_down
            || self.preferences.monitoring_paused
            || self.preferences.watched_folders.is_empty()
        {
            return;
        }
        match WatcherService::start(&self.preferences.watched_folders, self.window.0 as usize) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => {
                log::warn!("monitoring did not start: {}", error.safe_message());
                self.show_message(
                    s::MONITORING_TITLE,
                    &s::monitoring_failed(&error.safe_message()),
                    MessageLevel::Warning,
                );
            }
        }
    }

    fn handle_upload_finished(&mut self, outcome: UploadOutcome) {
        self.stop_upload_progress();
        self.refresh_tooltip();
        if let Some(summary) = &outcome.summary {
            // The user already chose to upload, so the report opens without another question.
            let url = s::report_url(&summary.sha256);
            match shell::open_url(&url) {
                Ok(()) => {
                    log::info!("report opened for {}", safe_base_name(&outcome.path));
                    if self.preferences.notifications_enabled
                        && let Some(tray) = self.tray.as_mut()
                    {
                        let _ = tray.notify(
                            s::UPLOAD_COMPLETE_TITLE,
                            &s::upload_complete_body(
                                &safe_base_name(&outcome.path),
                                summary.flagged(),
                            ),
                            summary.flagged() > 0,
                        );
                    }
                }
                // No browser: fall back to the result dialog, which offers the report link.
                Err(error) => {
                    log::warn!("could not open the report: {}", error.safe_message());
                    self.show_summary(Some(&outcome.path), summary, false);
                }
            }
        } else {
            self.show_message(
                s::ANALYSIS_RUNNING_TITLE,
                s::ANALYSIS_RUNNING_BODY,
                MessageLevel::Info,
            );
        }
    }

    fn handle_worker_error(
        &mut self,
        context: &str,
        message: &str,
        auth: bool,
        retry_after: Option<u64>,
    ) {
        log::warn!("{context}: {message}");
        if let Some(seconds) = retry_after {
            let now = now_stamp() as i64;
            let until = now.saturating_add(seconds as i64);
            let _ = self.quota.pause(now, "service rate limit", until);
        }
        self.refresh_tooltip();
        self.show_message(
            s::APP,
            &format!("{context}\n\n{message}"),
            MessageLevel::Error,
        );
        if auth {
            self.show_message(s::KEY_TITLE, s::KEY_REJECTED, MessageLevel::Warning);
            self.prompt_for_key();
        }
    }

    fn show_menu(&mut self) {
        let has_key = self.credentials.has_key().unwrap_or(false);
        let quota_text = match self.quota.status(now_stamp() as i64) {
            Ok(status) if status.paused_until.is_some() => s::MENU_QUOTA_PAUSED.to_string(),
            Ok(status) => s::menu_quota_today(
                status.day_used,
                status.day_limit,
                status.auto_remaining == 0,
            ),
            Err(_) => s::MENU_QUOTA_UNAVAILABLE.to_string(),
        };
        let key_text = if has_key {
            s::MENU_KEY_CONFIGURED.to_string()
        } else {
            s::MENU_KEY_MISSING.to_string()
        };
        let Ok(menu) = menu::Menu::popup() else {
            return;
        };
        let _ = menu.item(CMD_CHECK_FILE, s::MENU_CHECK);
        let _ = menu.item(CMD_ADD_FOLDER, s::MENU_ADD_FOLDER);
        if !self.preferences.watched_folders.is_empty()
            && let Ok(folders) = menu::Menu::popup()
        {
            let unavailable = self
                .watcher
                .as_ref()
                .map(WatcherService::unavailable)
                .unwrap_or_default();
            for (index, folder) in self.preferences.watched_folders.iter().enumerate() {
                let label = s::menu_remove_folder(&folder.path, unavailable.contains(&folder.path));
                let _ = folders.item(CMD_REMOVE_FOLDER_BASE + index as u32, &label);
            }
            let _ = menu.submenu(s::MENU_WATCHED_FOLDERS, folders);
        }
        let _ = menu.separator();
        let _ = menu.item(CMD_HISTORY, s::MENU_HISTORY);
        let _ = menu.disabled(&quota_text);
        let _ = menu.disabled(&key_text);
        let _ = menu.separator();
        if let Ok(settings) = menu::Menu::popup() {
            let autostart = autostart::is_enabled().unwrap_or(self.preferences.autostart_enabled);
            let _ = settings.checked(CMD_TOGGLE_AUTOSTART, s::MENU_AUTOSTART, autostart);
            let _ = settings.checked(
                CMD_TOGGLE_NOTIFICATIONS,
                s::MENU_NOTIFICATIONS,
                self.preferences.notifications_enabled,
            );
            let _ = settings.checked(
                CMD_TOGGLE_CONTEXT_MENU,
                s::MENU_CONTEXT_MENU,
                context_menu::is_registered().unwrap_or(false),
            );
            let _ = settings.checked(
                CMD_TOGGLE_AUTO_CHECK,
                s::MENU_AUTO_CHECK,
                self.preferences.auto_check_enabled,
            );
            let _ = settings.checked(
                CMD_TOGGLE_PAUSE,
                s::MENU_PAUSE,
                self.preferences.monitoring_paused,
            );
            let _ = settings.separator();
            let _ = settings.item(CMD_SET_KEY, s::MENU_SET_KEY);
            if has_key {
                let _ = settings.item(CMD_REMOVE_KEY, s::MENU_REMOVE_KEY);
            }
            let _ = menu.submenu(s::MENU_SETTINGS, settings);
        }
        let _ = menu.separator();
        let _ = menu.item(CMD_OPEN_LOGS, s::MENU_OPEN_LOGS);
        let _ = menu.item(CMD_CLEAR_DATA, s::MENU_CLEAR_DATA);
        let _ = menu.item(CMD_REMOVE_APP, s::MENU_REMOVE_APP);
        let _ = menu.separator();
        let _ = menu.item(CMD_EXIT, s::MENU_EXIT);
        let command = menu.track(self.window);
        if command != 0 {
            self.handle_command(command);
        }
    }

    fn pick_and_check(&mut self) {
        log::info!("file picker opened");
        match FileDialog::new().set_title(s::PICK_FILE_TITLE).pick_file() {
            Some(path) => {
                log::info!("file picked: {}", safe_base_name(&path));
                self.queue_check(path, true);
            }
            None => log::info!("file picker closed without a file"),
        }
    }

    fn queue_check(&mut self, path: PathBuf, allow_key_prompt: bool) {
        if self.shutting_down {
            log::info!("check ignored: shutting down");
            return;
        }
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                self.show_message(s::APP, &s::file_not_accessible(error), MessageLevel::Error);
                return;
            }
        };
        let has_key = match self.credentials.has_key() {
            Ok(has_key) => has_key,
            Err(error) => {
                self.show_message(
                    s::APP,
                    &s::key_read_failed(&error.safe_message()),
                    MessageLevel::Error,
                );
                return;
            }
        };
        if !has_key {
            if allow_key_prompt {
                if !self.prompt_for_key() {
                    return;
                }
            } else {
                if let Err(error) = self.worker.send(WorkerCommand::Fingerprint { path }) {
                    self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
                }
                return;
            }
        }
        if let Some(tray) = self.tray.as_mut() {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| safe_base_name(&path));
            let _ = tray.set_tooltip(&s::tip_checking(&name));
        }
        let name = safe_base_name(&path);
        match self.worker.send(WorkerCommand::Check {
            path,
            manual: true,
            // Only the tray's own picker may lead to an upload; Explorer quick checks never do.
            offer_upload: allow_key_prompt,
        }) {
            Ok(()) => log::info!("check queued: {name}"),
            Err(error) => {
                log::warn!("check not queued for {name}: {}", error.safe_message());
                self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
            }
        }
    }

    fn confirm_upload(&mut self, result: CheckResult) {
        if upload_route(result.size_bytes).is_err() {
            let message = s::cannot_upload(&safe_base_name(&result.path));
            self.show_message(s::RESULT_TITLE, &message, MessageLevel::Warning);
            return;
        }
        let text = s::consent_body(
            &safe_base_name(&result.path),
            &format_bytes(result.size_bytes),
            &result.sha256,
            requires_large_warning(result.size_bytes),
        );
        // Native box so "No" is the default button: Enter or Esc never uploads (ui-consent contract).
        let answer = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_YESNO, MessageBoxW,
            };
            MessageBoxW(
                Some(self.window),
                &windows::core::HSTRING::from(text.as_str()),
                &windows::core::HSTRING::from(s::CONSENT_TITLE),
                MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING | MB_SETFOREGROUND,
            ) == IDYES
        };
        log::info!(
            "upload consent for {}: {}",
            safe_base_name(&result.path),
            if answer { "accepted" } else { "declined" }
        );
        if !answer {
            self.refresh_tooltip();
            return;
        }
        let consent_at = now_stamp() as i64;
        if let Err(error) = self.worker.send(WorkerCommand::Upload {
            path: result.path.clone(),
            sha256: result.sha256.clone(),
            size_bytes: result.size_bytes,
            consent_at,
        }) {
            self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
            return;
        }
        self.upload_total = Some(result.size_bytes);
        self.upload_sent
            .store(0, std::sync::atomic::Ordering::Relaxed);
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(
                Some(self.window),
                PROGRESS_TIMER_ID,
                1000,
                None,
            );
        }
        if let Some(tray) = self.tray.as_mut() {
            let _ = tray.set_tooltip(&s::tip_uploading(0));
            let _ = tray.notify(s::UPLOAD_STARTED_TITLE, s::UPLOAD_STARTED_BODY, false);
        }
    }

    fn show_summary(&self, path: Option<&std::path::Path>, summary: &LookupSummary, cached: bool) {
        let name = summary
            .meaningful_name
            .clone()
            .or_else(|| {
                path.and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
            })
            .unwrap_or_else(|| s::UNKNOWN_FILE.to_string());
        let analysis_age = summary
            .last_analysis_at
            .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0))
            .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let text = s::result_body(
            summary,
            &name,
            &format_bytes(summary.size_bytes),
            &analysis_age,
            cached,
        );
        let level = if summary.flagged() > 0 {
            MessageLevel::Warning
        } else {
            MessageLevel::Info
        };
        let answer = MessageDialog::new()
            .set_title(s::RESULT_TITLE)
            .set_description(&text)
            .set_buttons(MessageButtons::YesNo)
            .set_level(level)
            .show();
        log::info!("result dialog answer: {answer:?}");
        if matches!(answer, MessageDialogResult::Yes) {
            let url = s::report_url(&summary.sha256);
            if let Err(error) = shell::open_url(&url) {
                self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
            }
        }
    }

    fn add_watched_folder(&mut self) {
        let Some(path) = FileDialog::new()
            .set_title(s::PICK_FOLDER_TITLE)
            .pick_folder()
        else {
            return;
        };
        match self.preferences.add_folder(path, now_stamp() as i64) {
            Ok(()) => {
                self.persist();
                self.restart_watcher();
                self.show_message(s::APP, s::FOLDER_ADDED, MessageLevel::Info);
            }
            Err(error) => {
                self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
            }
        }
    }

    fn toggle_context_menu(&mut self) {
        let result = if context_menu::is_registered().unwrap_or(false) {
            context_menu::unregister()
        } else {
            std::env::current_exe()
                .map_err(AppError::Io)
                .and_then(|executable| context_menu::register(&executable))
        };
        match result {
            Ok(()) => self.show_message(s::APP, s::CONTEXT_MENU_UPDATED, MessageLevel::Info),
            Err(error) => self.show_message(s::APP, &error.safe_message(), MessageLevel::Error),
        }
    }

    fn remove_watched_folder(&mut self, index: usize) {
        let Some(path) = self
            .preferences
            .watched_folders
            .get(index)
            .map(|folder| folder.path.clone())
        else {
            return;
        };
        if self.preferences.remove_folder(&path) {
            self.persist();
            self.restart_watcher();
        }
    }

    fn show_history(&mut self) {
        let detections = match self.cache.recent_detections(12) {
            Ok(detections) => detections,
            Err(error) => {
                self.show_message(s::APP, &error.safe_message(), MessageLevel::Error);
                return;
            }
        };
        if detections.is_empty() {
            self.show_message(s::HISTORY_TITLE, s::HISTORY_EMPTY, MessageLevel::Info);
            return;
        }
        let mut text = String::new();
        for detection in detections {
            let when = DateTime::<Utc>::from_timestamp(detection.detected_at, 0)
                .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| s::UNKNOWN_TIME.to_string());
            text.push_str(&format!(
                "{}  [{}]  {}\n",
                when,
                detection.state.as_str(),
                detection.base_name
            ));
        }
        self.show_message(s::HISTORY_TITLE, &text, MessageLevel::Info);
    }

    fn toggle_autostart(&mut self) {
        let enabled = !self.preferences.autostart_enabled;
        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                self.show_message(s::APP, &error.to_string(), MessageLevel::Error);
                return;
            }
        };
        match autostart::set_enabled(&executable, enabled) {
            Ok(()) => {
                self.preferences.autostart_enabled = enabled;
                self.persist();
            }
            Err(error) => self.show_message(s::APP, &error.safe_message(), MessageLevel::Error),
        }
    }

    fn remove_key(&mut self) {
        let answer = MessageDialog::new()
            .set_title(s::REMOVE_KEY_TITLE)
            .set_description(s::REMOVE_KEY_BODY)
            .set_buttons(MessageButtons::YesNo)
            .set_level(MessageLevel::Warning)
            .show();
        if !matches!(answer, MessageDialogResult::Yes) {
            return;
        }
        match self.credentials.delete_key() {
            Ok(()) => {
                self.preferences.auto_check_enabled = false;
                self.persist();
                self.refresh_tooltip();
            }
            Err(error) => self.show_message(s::APP, &error.safe_message(), MessageLevel::Error),
        }
    }

    /// Constitution V: removal leaves no autostart, Explorer entry, key, or data behind.
    /// Every step runs even if an earlier one fails; failures are listed for the user.
    fn remove_from_pc(&mut self) {
        let confirmed = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_YESNO, MessageBoxW,
            };
            MessageBoxW(
                Some(self.window),
                &windows::core::HSTRING::from(s::REMOVE_APP_BODY),
                &windows::core::HSTRING::from(s::REMOVE_APP_TITLE),
                MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING | MB_SETFOREGROUND,
            ) == IDYES
        };
        if !confirmed {
            return;
        }
        log::info!("removing VT Tray from this PC");
        let executable = std::env::current_exe().unwrap_or_default();
        if let Some(uninstaller) = platform_setup::uninstaller_for(&executable) {
            // The uninstaller runs `--cleanup`, which closes this tray and removes the rest.
            match std::process::Command::new(&uninstaller)
                .arg("/SILENT")
                .spawn()
            {
                Ok(_) => {
                    self.removed = true;
                    return;
                }
                Err(error) => log::warn!("could not start the uninstaller: {error}"),
            }
        }
        self.watcher.take();
        // Wipe now, so the data is gone even if the delayed folder removal cannot run.
        let wiped = self.cache.wipe_all();
        let (mut problems, leftover) = remove_everything(&self.paths, &self.credentials, false);
        if let Err(error) = wiped {
            problems.push(format!("local data: {}", error.safe_message()));
        }
        self.removed = true;
        for problem in &problems {
            log::warn!("removal step failed: {problem}");
        }
        let level = if problems.is_empty() {
            MessageLevel::Info
        } else {
            MessageLevel::Warning
        };
        self.show_message(
            s::APP,
            &s::removed_summary(&problems, leftover.as_deref()),
            level,
        );
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }

    fn clear_local_data(&mut self) {
        let answer = MessageDialog::new()
            .set_title(s::CLEAR_TITLE)
            .set_description(s::CLEAR_BODY)
            .set_buttons(MessageButtons::YesNo)
            .set_level(MessageLevel::Warning)
            .show();
        if !matches!(answer, MessageDialogResult::Yes) {
            return;
        }
        let result = self
            .cache
            .clear_history()
            .and_then(|()| self.cache.clear_cache());
        match result {
            Ok(()) => self.show_message(s::APP, s::CLEARED, MessageLevel::Info),
            Err(error) => self.show_message(s::APP, &error.safe_message(), MessageLevel::Error),
        }
    }

    fn show_message(&self, title: &str, text: &str, level: MessageLevel) {
        MessageDialog::new()
            .set_title(title)
            .set_description(text)
            .set_buttons(MessageButtons::Ok)
            .set_level(level)
            .show();
    }

    pub fn shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        log::info!("VT Tray stopping");
        self.watcher.take();
        self.tray.take();
        self.worker.shutdown();
        let _ = self
            .cache
            .prune(now_stamp() as i64, self.preferences.cache_retention_days);
        if !self.removed {
            self.persist();
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.2} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{bytes} bytes")
    }
}
