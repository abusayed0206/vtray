use crate::core::errors::AppError;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedFolder {
    pub path: PathBuf,
    pub enabled: bool,
    pub added_at: i64,
    pub last_checked_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub watched_folders: Vec<WatchedFolder>,
    #[serde(default)]
    pub auto_check_enabled: bool,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub autostart_enabled: bool,
    #[serde(default)]
    pub monitoring_paused: bool,
    #[serde(default = "default_retention")]
    pub cache_retention_days: u32,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_true() -> bool {
    true
}

fn default_retention() -> u32 {
    30
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            watched_folders: Vec::new(),
            auto_check_enabled: false,
            notifications_enabled: true,
            autostart_enabled: false,
            monitoring_paused: false,
            cache_retention_days: default_retention(),
            log_level: default_log_level(),
            schema_version: SCHEMA_VERSION,
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        self.cache_retention_days = self.cache_retention_days.clamp(1, 365);
        let level = self.log_level.trim().to_ascii_lowercase();
        self.log_level = if matches!(
            level.as_str(),
            "error" | "warn" | "info" | "debug" | "trace"
        ) {
            level
        } else {
            default_log_level()
        };
        let mut seen = std::collections::HashSet::new();
        self.watched_folders
            .retain(|folder| seen.insert(folder_key(&folder.path)));
        self.schema_version = SCHEMA_VERSION;
    }

    pub fn add_folder(&mut self, path: PathBuf, now: i64) -> Result<(), AppError> {
        let path = validate_watched_folder(&path)?;
        if self
            .watched_folders
            .iter()
            .any(|f| folder_key(&f.path) == folder_key(&path))
        {
            return Err(AppError::Validation(
                "folder is already watched".to_string(),
            ));
        }
        self.watched_folders.push(WatchedFolder {
            path,
            enabled: true,
            added_at: now,
            last_checked_at: None,
        });
        Ok(())
    }

    pub fn remove_folder(&mut self, path: &Path) -> bool {
        let key = folder_key(path);
        let before = self.watched_folders.len();
        self.watched_folders
            .retain(|folder| folder_key(&folder.path) != key);
        before != self.watched_folders.len()
    }
}

fn folder_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}

pub fn validate_watched_folder(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::Validation(
            "watched folder must be an absolute path".to_string(),
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| AppError::Validation(format!("folder is not accessible: {error}")))?;
    if !canonical.is_dir() {
        return Err(AppError::Validation(
            "watched path is not a folder".to_string(),
        ));
    }
    if !canonical
        .components()
        .any(|part| matches!(part, Component::Normal(_)))
    {
        return Err(AppError::Validation(
            "volume roots cannot be watched".to_string(),
        ));
    }
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::Validation(
            "reparse-point folders are not supported".to_string(),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(AppError::Validation(
                "reparse-point folders are not supported".to_string(),
            ));
        }
    }
    Ok(canonical)
}
