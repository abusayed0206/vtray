use crate::core::types::QuotaDenial;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("authentication failed")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited { retry_after: Option<u64> },
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("not found")]
    NotFound,
    #[error("canceled")]
    Canceled,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("credential error: {0}")]
    Credential(String),
    #[error("quota denied: {0}")]
    QuotaDenied(QuotaDenial),
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("canceled")]
    Canceled,
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn safe_message(&self) -> String {
        redact_error_text(&self.to_string())
    }

    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::Api(ApiError::Unauthorized))
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::Api(ApiError::RateLimited { .. }))
    }

    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::Api(ApiError::RateLimited { retry_after }) => *retry_after,
            _ => None,
        }
    }
}

pub fn redact_error_text(input: &str) -> String {
    let mut output = input.to_string();
    for marker in ["x-apikey", "apikey", "api_key", "authorization"] {
        let lower = output.to_ascii_lowercase();
        if let Some(index) = lower.find(marker) {
            let line_end = output[index..]
                .find(['\r', '\n'])
                .map(|offset| index + offset)
                .unwrap_or(output.len());
            output.replace_range(index..line_end, "[redacted]");
        }
    }
    output
}
