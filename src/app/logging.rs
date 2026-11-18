use crate::core::errors::AppError;
use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};
use std::path::Path;

pub fn init(log_dir: &Path, level: &str) -> Result<(), AppError> {
    std::fs::create_dir_all(log_dir)?;
    Logger::try_with_str(level)
        .map_err(|error| AppError::Config(error.to_string()))?
        .log_to_file(FileSpec::default().directory(log_dir))
        .rotate(
            Criterion::Size(10 * 1024 * 1024),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(7),
        )
        .start()
        .map_err(|error| AppError::Config(error.to_string()))?;
    Ok(())
}
