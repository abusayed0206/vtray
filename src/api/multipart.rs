use crate::core::errors::AppError;
use crate::core::hash::safe_base_name;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static BOUNDARY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_boundary() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = BOUNDARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:032x}",
        nanos ^ ((std::process::id() as u128) << 64) ^ counter as u128
    )
}

pub struct MultipartFileBody {
    prefix: Vec<u8>,
    prefix_pos: usize,
    file: File,
    file_remaining: u64,
    suffix: Vec<u8>,
    suffix_pos: usize,
    total_length: u64,
    sent: Option<Arc<AtomicU64>>,
}

impl MultipartFileBody {
    pub fn open(path: &Path, boundary: &str) -> Result<Self, AppError> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        let name = safe_base_name(path);
        let mut prefix = Vec::new();
        write!(
            prefix,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .map_err(|error| AppError::Internal(error.to_string()))?;
        let suffix = format!("\r\n--{boundary}--\r\n").into_bytes();
        let total_length = prefix.len() as u64 + size + suffix.len() as u64;
        Ok(Self {
            prefix,
            prefix_pos: 0,
            file,
            file_remaining: size,
            suffix,
            suffix_pos: 0,
            total_length,
            sent: None,
        })
    }

    /// Adds every file byte read into `sent`, for upload progress.
    pub fn with_progress(mut self, sent: Arc<AtomicU64>) -> Self {
        self.sent = Some(sent);
        self
    }

    pub fn content_length(&self) -> u64 {
        self.total_length
    }
}

impl Read for MultipartFileBody {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.prefix_pos < self.prefix.len() {
            let remaining = &self.prefix[self.prefix_pos..];
            let count = remaining.len().min(buffer.len());
            buffer[..count].copy_from_slice(&remaining[..count]);
            self.prefix_pos += count;
            return Ok(count);
        }
        if self.file_remaining > 0 {
            let max = buffer.len().min(self.file_remaining as usize);
            let count = self.file.read(&mut buffer[..max])?;
            if count == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "file ended before the declared upload size",
                ));
            }
            self.file_remaining -= count as u64;
            if let Some(sent) = &self.sent {
                sent.fetch_add(count as u64, Ordering::Relaxed);
            }
            return Ok(count);
        }
        if self.suffix_pos < self.suffix.len() {
            let remaining = &self.suffix[self.suffix_pos..];
            let count = remaining.len().min(buffer.len());
            buffer[..count].copy_from_slice(&remaining[..count]);
            self.suffix_pos += count;
            return Ok(count);
        }
        Ok(0)
    }
}
