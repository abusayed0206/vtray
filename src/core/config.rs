use crate::core::errors::AppError;
use crate::core::settings::Preferences;
use directories::BaseDirs;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub config_file: PathBuf,
    pub db_file: PathBuf,
    pub pending_check_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, AppError> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AppError::Config("could not resolve application data folders".into()))?;
        Ok(Self::from_dirs(
            &dirs.config_dir().join("VTTray"),
            &dirs.data_local_dir().join("VTTray"),
        ))
    }

    pub fn under(root: &Path) -> Self {
        Self::from_dirs(&root.join("config"), &root.join("data"))
    }

    fn from_dirs(config_dir: &Path, data_dir: &Path) -> Self {
        Self {
            config_dir: config_dir.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            log_dir: data_dir.join("logs"),
            config_file: config_dir.join("config.json"),
            db_file: data_dir.join("cache.db"),
            pending_check_file: data_dir.join("pending-check.txt"),
        }
    }

    pub fn ensure(&self) -> Result<(), AppError> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.data_dir)?;
        fs::create_dir_all(&self.log_dir)?;
        Ok(())
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Preferences, AppError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Preferences::default());
            }
            Err(error) => return Err(error.into()),
        };
        match serde_json::from_slice::<Preferences>(&bytes) {
            Ok(mut preferences) => {
                preferences.normalize();
                Ok(preferences)
            }
            Err(error) => {
                let backup = self
                    .path
                    .with_extension(format!("corrupt-{}", crate::core::config::now_stamp()));
                let _ = fs::rename(&self.path, &backup);
                log::warn!("configuration was unreadable and a backup was written");
                log::warn!("configuration parse error: {error}");
                Ok(Preferences::default())
            }
        }
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(preferences)
            .map_err(|error| AppError::Config(error.to_string()))?;
        let temp = self.path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

pub fn now_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
