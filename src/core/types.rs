use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictState {
    Unknown,
    Known,
    Stale,
    Error,
}

impl VerdictState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Known => "known",
            Self::Stale => "stale",
            Self::Error => "error",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Self::Unknown),
            "known" => Some(Self::Known),
            "stale" => Some(Self::Stale),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub sha256: String,
    pub size_bytes: u64,
    pub verdict_state: VerdictState,
    pub reputation: Option<i64>,
    pub malicious_count: u32,
    pub suspicious_count: u32,
    pub undetected_count: u32,
    pub harmless_count: u32,
    pub timeout_count: u32,
    pub type_description: Option<String>,
    pub meaningful_name: Option<String>,
    pub first_submission_at: Option<i64>,
    pub last_analysis_at: Option<i64>,
    pub last_checked_at: i64,
    pub summary_json: Option<String>,
}

impl FileRecord {
    pub fn is_stale(&self, now: i64, max_age_seconds: i64) -> bool {
        now.saturating_sub(self.last_analysis_at.unwrap_or(self.last_checked_at)) > max_age_seconds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRecord {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub mtime_ns: i128,
    pub sha256: Option<String>,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionState {
    New,
    Hashing,
    Checked,
    Unknown,
    Reported,
    Dismissed,
    Error,
}

impl DetectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Hashing => "hashing",
            Self::Checked => "checked",
            Self::Unknown => "unknown",
            Self::Reported => "reported",
            Self::Dismissed => "dismissed",
            Self::Error => "error",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "new" => Some(Self::New),
            "hashing" => Some(Self::Hashing),
            "checked" => Some(Self::Checked),
            "unknown" => Some(Self::Unknown),
            "reported" => Some(Self::Reported),
            "dismissed" => Some(Self::Dismissed),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub id: i64,
    pub path: PathBuf,
    pub sha256: Option<String>,
    pub base_name: String,
    pub size_bytes: Option<u64>,
    pub detected_at: i64,
    pub state: DetectionState,
    pub notified_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStatus {
    Pending,
    Uploading,
    Analyzing,
    Completed,
    Failed,
    Canceled,
}

impl UploadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Uploading => "uploading",
            Self::Analyzing => "analyzing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "uploading" => Some(Self::Uploading),
            "analyzing" => Some(Self::Analyzing),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadJob {
    pub id: i64,
    pub sha256: String,
    pub base_name: String,
    pub size_bytes: u64,
    pub consent_at: i64,
    pub status: UploadStatus,
    pub analysis_id: Option<String>,
    pub upload_url_used: bool,
    pub attempts: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaEndpoint {
    FileLookup,
    FileUpload,
    UploadUrl,
    AnalysisPoll,
    Rescan,
}

impl QuotaEndpoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileLookup => "file_lookup",
            Self::FileUpload => "file_upload",
            Self::UploadUrl => "upload_url",
            Self::AnalysisPoll => "analysis_poll",
            Self::Rescan => "rescan",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "file_lookup" => Some(Self::FileLookup),
            "file_upload" => Some(Self::FileUpload),
            "upload_url" => Some(Self::UploadUrl),
            "analysis_poll" => Some(Self::AnalysisPoll),
            "rescan" => Some(Self::Rescan),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaOutcome {
    Reserved,
    Ok,
    HttpError,
    NetworkError,
    Canceled,
}

impl QuotaOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Ok => "ok",
            Self::HttpError => "http_error",
            Self::NetworkError => "network_error",
            Self::Canceled => "canceled",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "reserved" => Some(Self::Reserved),
            "ok" => Some(Self::Ok),
            "http_error" => Some(Self::HttpError),
            "network_error" => Some(Self::NetworkError),
            "canceled" => Some(Self::Canceled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaDenial {
    Minute,
    Day,
    Month,
    ManualReserve,
    Paused,
}

impl QuotaDenial {
    pub fn code(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Day => "day",
            Self::Month => "month",
            Self::ManualReserve => "manual_reserve",
            Self::Paused => "paused",
        }
    }
}

impl fmt::Display for QuotaDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Minute => "the per-minute request limit is exhausted",
            Self::Day => "the daily request limit is exhausted",
            Self::Month => "the monthly request limit is exhausted",
            Self::ManualReserve => "automatic checks are paused to reserve manual capacity",
            Self::Paused => "online requests are paused",
        };
        f.write_str(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaStatus {
    pub minute_used: u32,
    pub minute_limit: u32,
    pub day_used: u32,
    pub day_limit: u32,
    pub month_used: u32,
    pub month_limit: u32,
    pub manual_remaining: u32,
    pub auto_remaining: u32,
    pub paused_reason: Option<String>,
    pub paused_until: Option<i64>,
    pub day_reset_at: i64,
    pub month_reset_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaEvent {
    pub id: i64,
    pub ts: i64,
    pub endpoint: QuotaEndpoint,
    pub outcome: QuotaOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupSummary {
    pub sha256: String,
    pub size_bytes: u64,
    pub malicious: u32,
    pub suspicious: u32,
    pub undetected: u32,
    pub harmless: u32,
    pub timeout: u32,
    pub reputation: Option<i64>,
    pub type_description: Option<String>,
    pub meaningful_name: Option<String>,
    pub first_submission_at: Option<i64>,
    pub last_analysis_at: Option<i64>,
}

impl LookupSummary {
    pub fn flagged(&self) -> u32 {
        self.malicious + self.suspicious
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupOutcome {
    Known(LookupSummary),
    Unknown { sha256: String },
}
