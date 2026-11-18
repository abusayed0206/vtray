//! Every user-facing string in one place, for review (T033, T083).
//! Log lines and internal error details stay at their call sites.
use crate::core::errors::{ApiError, AppError};
use crate::core::quota::{DAY_LIMIT, MANUAL_RESERVE, QuotaWarning};
use crate::core::types::{LookupSummary, QuotaDenial};

pub const APP: &str = "VTTray";

// Tray tooltip
pub const TIP_DEFAULT: &str = "VTTray - check files with VirusTotal";
pub const TIP_KEY_NEEDED: &str = "API key needed";
pub const TIP_PAUSED: &str = "monitoring paused";
pub const TIP_READY: &str = "ready";
pub const TIP_ANALYZING: &str = "upload sent, VirusTotal is analyzing";

pub fn tooltip(state: &str) -> String {
    format!("{APP} - {state}")
}

pub fn tip_uploading(percent: u64) -> String {
    tooltip(&format!("uploading {percent}%"))
}

pub fn tip_checking(name: &str) -> String {
    tooltip(&format!("checking {name}"))
}

// Menu
pub const MENU_CHECK: &str = "Check a file...";
pub const MENU_ADD_FOLDER: &str = "Add watched folder...";
pub const MENU_WATCHED_FOLDERS: &str = "Watched folders";
pub const MENU_HISTORY: &str = "Recent checks...";
pub const MENU_SETTINGS: &str = "Settings";
pub const MENU_AUTOSTART: &str = "Start with Windows";
pub const MENU_NOTIFICATIONS: &str = "Notifications";
pub const MENU_CONTEXT_MENU: &str = "Explorer context menu";
pub const MENU_AUTO_CHECK: &str = "Automatic hash checks";
pub const MENU_PAUSE: &str = "Pause monitoring";
pub const MENU_SET_KEY: &str = "Set or replace API key...";
pub const MENU_REMOVE_KEY: &str = "Remove API key...";
pub const MENU_OPEN_LOGS: &str = "Open log folder";
pub const MENU_CLEAR_DATA: &str = "Clear local data...";
pub const MENU_EXIT: &str = "Exit";
pub const MENU_QUOTA_PAUSED: &str = "Quota: online requests paused";
pub const MENU_QUOTA_UNAVAILABLE: &str = "Quota: status unavailable";
pub const MENU_KEY_CONFIGURED: &str = "API key: configured";
pub const MENU_KEY_MISSING: &str = "API key: not configured";

pub fn menu_quota_today(
    used: impl std::fmt::Display,
    limit: impl std::fmt::Display,
    auto_paused: bool,
) -> String {
    if auto_paused {
        format!("Quota: {used}/{limit} requests today (automatic checks paused)")
    } else {
        format!("Quota: {used}/{limit} requests today")
    }
}

// Quota warnings
pub const QUOTA_WARNING_TITLE: &str = "VirusTotal quota";

pub fn quota_warning_body(warning: QuotaWarning) -> String {
    match warning {
        QuotaWarning::AutoPaused => format!(
            "{} of today's {DAY_LIMIT} requests are used. Automatic checks are paused until 00:00 UTC; {MANUAL_RESERVE} remain for manual checks.",
            DAY_LIMIT - MANUAL_RESERVE
        ),
        QuotaWarning::DayLow { remaining: 0 } => {
            "Today's VirusTotal requests are used up. Checks resume at 00:00 UTC.".to_string()
        }
        QuotaWarning::DayLow { remaining } => format!(
            "Only {remaining} VirusTotal requests are left today. The limit resets at 00:00 UTC."
        ),
        QuotaWarning::MonthLow { remaining } => format!(
            "Only {remaining} VirusTotal requests are left this month. The limit resets on the 1st (UTC)."
        ),
    }
}

pub fn menu_remove_folder(path: &std::path::Path, unavailable: bool) -> String {
    let shown = display_path(path);
    if unavailable {
        format!("Remove: {shown} (unavailable)")
    } else {
        format!("Remove: {shown}")
    }
}

/// Paths as users know them: without the `\\?\` long-path prefix Windows adds on canonicalize.
pub fn display_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => text
            .strip_prefix(r"\\?\")
            .map(str::to_string)
            .unwrap_or(text),
    }
}

// API key
pub const KEY_TITLE: &str = "VirusTotal API key";
pub const KEY_PROMPT: &str = "Paste your VirusTotal API key. The key is stored only in Windows Credential Manager and is never written to logs or configuration files.";
pub const KEY_HINT: &str = "Find it on virustotal.com: open your profile menu and choose API key.";
pub const KEY_SAVED: &str = "VirusTotal API key saved.";
pub const KEY_REJECTED: &str =
    "VirusTotal rejected the saved API key. Replace it from the tray menu.";
pub const KEY_NEEDED_TITLE: &str = "VirusTotal API key needed";
pub const REMOVE_KEY_TITLE: &str = "Remove VirusTotal API key?";
pub const REMOVE_KEY_BODY: &str = "Online checks and uploads will stop until a key is saved again. Local monitoring and history remain.";

pub fn key_read_failed(error: &str) -> String {
    format!("Could not read the saved API key: {error}")
}

pub fn key_save_failed(error: &str) -> String {
    format!("Could not save the key: {error}")
}

pub fn key_needed_fingerprint(name: &str, size: &str, sha256: &str) -> String {
    format!(
        "No VirusTotal API key is configured, so this file was fingerprinted locally but not checked online.\n\nFile: {name}\nSize: {size}\nSHA-256: {sha256}\n\nAdd a key from the tray menu to enable online checks. No upload was performed."
    )
}

// Errors

/// What the user sees when a check or upload fails: plain language and what to do next.
/// Technical details (status codes, transport errors) are logged, not shown.
pub fn error_message(error: &AppError) -> String {
    match error {
        AppError::QuotaDenied(denial) => denial_message(*denial).to_string(),
        AppError::Api(ApiError::RateLimited { retry_after }) => match retry_after {
            Some(seconds) => format!(
                "VirusTotal asked VTTray to slow down. Online checks resume in about {seconds} seconds."
            ),
            None => "VirusTotal asked VTTray to slow down. Online checks resume in about a minute."
                .to_string(),
        },
        AppError::Api(ApiError::Unauthorized) => "VirusTotal rejected the API key.".to_string(),
        AppError::Api(ApiError::Network(_)) => "VTTray could not reach VirusTotal. Check the internet connection, proxy, or firewall, then try again.".to_string(),
        AppError::Api(ApiError::Http { status, .. }) if *status >= 500 => {
            "VirusTotal is having problems right now. Try again later.".to_string()
        }
        AppError::Credential(_) => {
            "No VirusTotal API key is configured. Add one from the tray menu.".to_string()
        }
        _ => error.safe_message(),
    }
}

pub fn denial_message(denial: QuotaDenial) -> &'static str {
    match denial {
        QuotaDenial::Minute => {
            "VirusTotal's limit of four requests per minute is in use. Try again in a minute."
        }
        QuotaDenial::Day => {
            "VirusTotal's daily request limit has been reached. It resets at 00:00 UTC."
        }
        QuotaDenial::Month => {
            "VirusTotal's monthly request limit has been reached. It resets on the 1st (UTC)."
        }
        QuotaDenial::ManualReserve => {
            "Automatic checks are paused to keep capacity for manual checks."
        }
        QuotaDenial::Paused => {
            "Online requests are paused after VirusTotal asked VTTray to slow down."
        }
    }
}

pub fn could_not_fingerprint(name: &str) -> String {
    format!("Could not fingerprint {name}")
}

pub fn could_not_check(name: &str) -> String {
    format!("Could not check {name}")
}

pub fn could_not_upload(name: &str) -> String {
    format!("Could not upload {name}")
}

pub fn file_not_accessible(error: impl std::fmt::Display) -> String {
    format!("The selected file is not accessible: {error}")
}

// Checking and results
pub const PICK_FILE_TITLE: &str = "Check a file with VirusTotal";
pub const UNKNOWN_FILE: &str = "Unknown file";
pub const UNKNOWN_TIME: &str = "unknown time";
pub const RESULT_TITLE: &str = "VirusTotal result";

pub fn result_body(
    summary: &LookupSummary,
    name: &str,
    size: &str,
    analyzed: &str,
    cached: bool,
) -> String {
    let verdict = if summary.flagged() > 0 {
        "VirusTotal found detections"
    } else {
        "VirusTotal found no detections"
    };
    let source = if cached {
        "Source: local cache"
    } else {
        "Source: VirusTotal"
    };
    format!(
        "{verdict}\n\nFile: {name}\nSize: {size}\nSHA-256: {}\nDetections: {} malicious, {} suspicious, {} undetected\nLast analysis: {analyzed}\n{source}\n\nThis is advisory reputation data, not a guarantee that the file is safe or unsafe.\n\nOpen the report in your browser?",
        summary.sha256, summary.malicious, summary.suspicious, summary.undetected,
    )
}

// Uploading
pub const CONSENT_TITLE: &str = "Upload to VirusTotal?";
pub const UPLOAD_STARTED_TITLE: &str = "Upload started";
pub const UPLOAD_STARTED_BODY: &str =
    "The file is being sent to VirusTotal. Analysis can take a few minutes.";
pub const UPLOAD_COMPLETE_TITLE: &str = "Upload complete";
pub const UPLOAD_COMPLETE_BODY: &str = "The VirusTotal report is ready.";
pub const ANALYSIS_RUNNING_TITLE: &str = "Analysis still running";
pub const ANALYSIS_RUNNING_BODY: &str = "VirusTotal accepted the file and is still analyzing it. Use Check a file to retrieve the report later.";
/// Stored as the upload's last error; shown in history.
pub const UPLOAD_INTERRUPTED: &str =
    "VTTray closed before the upload finished; check the file again";

pub fn consent_body(name: &str, size: &str, sha256: &str, large: bool) -> String {
    let mut text = format!(
        "VirusTotal does not know this file.\n\nFile: {name}\nSize: {size}\nSHA-256: {sha256}\n\nUploading sends the file content to VirusTotal and shares it with the security community. Only upload files you are allowed to share.\n\nUpload this file?"
    );
    if large {
        text.push_str("\n\nThis file is larger than 200 MB and may take a long time to upload.");
    }
    text
}

/// Explorer quick checks are hash-only (FR-020): no upload offer, only where to go for one.
pub fn quick_check_unknown(name: &str, sha256: &str) -> String {
    format!(
        "VirusTotal does not know this file.\n\nFile: {name}\nSHA-256: {sha256}\n\nNothing was uploaded. To upload it, use Check a file in the VTTray tray menu."
    )
}

pub fn cannot_upload(name: &str) -> String {
    format!(
        "VirusTotal does not know this file.\n\n{name}\n\nThis file cannot be uploaded because it is empty or larger than the 650 MB limit."
    )
}

// Monitoring
pub const PICK_FOLDER_TITLE: &str = "Choose a folder to watch";
pub const FOLDER_ADDED: &str = "The folder was added. New executable and script files will be detected locally; uploading still requires your consent for each file.";
pub const MONITORING_TITLE: &str = "VTTray monitoring";
pub const DETECTIONS_TITLE: &str = "VirusTotal detections";
pub const NEW_FILE_TITLE: &str = "New file detected";

pub fn monitoring_failed(error: &str) -> String {
    format!("Monitoring did not start: {error}")
}

pub fn detections_body(name: &str, flagged: u32) -> String {
    format!("{name}: {flagged} engines flagged this file. Open Recent checks for details.")
}

pub fn new_file_body(name: &str) -> String {
    format!("{name} was added to a watched folder.")
}

// History and settings
pub const HISTORY_TITLE: &str = "Recent checks";
pub const HISTORY_EMPTY: &str = "No checks have been recorded yet.";
pub const CONTEXT_MENU_UPDATED: &str = "The Explorer context menu setting was updated.";
pub const CLEAR_TITLE: &str = "Clear local data?";
pub const CLEAR_BODY: &str = "This removes cached VirusTotal results, detections, and upload history. Watched folders, preferences, and quota counters are kept, so VirusTotal's limits are still respected.";
pub const CLEARED: &str = "Local history and cached results were cleared.";

// Removing VTTray from the PC (constitution V: removal leaves nothing registered behind)
pub const MENU_REMOVE_APP: &str = "Remove VTTray from this PC...";
pub const REMOVE_APP_TITLE: &str = "Remove VTTray from this PC?";
pub const REMOVE_APP_BODY: &str = "This will:\n\n- remove VTTray from Windows startup\n- remove the \"Check with VT Tray\" Explorer menu entry\n- delete the saved VirusTotal API key\n- delete all local data, settings, and history\n\nVTTray then exits. Files you checked or uploaded are not touched.\n\nRemove VTTray?";

pub fn removed_summary(problems: &[String], leftover: &std::path::Path) -> String {
    let mut text = if problems.is_empty() {
        "VTTray was removed from startup and the Explorer menu, and its API key and data were deleted.".to_string()
    } else {
        format!(
            "VTTray was removed, but some steps failed:\n\n- {}\n\nYou can retry them after starting VTTray again.",
            problems.join("\n- ")
        )
    };
    text.push_str(&format!(
        "\n\nTo finish, delete the program file and this folder (it holds the log files): {}",
        leftover.display()
    ));
    text
}
