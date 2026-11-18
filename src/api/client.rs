use crate::api::models::{VtAnalysisData, VtEnvelope, VtErrorEnvelope, VtFileData};
use crate::api::multipart::{MultipartFileBody, new_boundary};
use crate::core::errors::{ApiError, AppError, redact_error_text};
use crate::core::policy::{UploadRoute, upload_route};
use crate::core::types::{LookupOutcome, LookupSummary};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use ureq::SendBody;
use zeroize::Zeroizing;

pub trait ApiKeyProvider: Send + Sync {
    fn get_key(&self) -> Result<Option<Zeroizing<String>>, AppError>;
}

pub struct StaticKeyProvider {
    key: Option<String>,
}

impl StaticKeyProvider {
    pub fn new(key: Option<String>) -> Self {
        Self { key }
    }
}

impl ApiKeyProvider for StaticKeyProvider {
    fn get_key(&self) -> Result<Option<Zeroizing<String>>, AppError> {
        Ok(self.key.clone().map(Zeroizing::new))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadApiResult {
    pub analysis_id: String,
    pub used_upload_url: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisProgress {
    pub complete: bool,
    pub status: String,
    pub summary: Option<LookupSummary>,
}

pub struct VtClient {
    agent: ureq::Agent,
    upload_agent: ureq::Agent,
    base_url: String,
    keys: Arc<dyn ApiKeyProvider>,
    upload_sent: Arc<AtomicU64>,
}

impl VtClient {
    pub fn new(keys: Arc<dyn ApiKeyProvider>, base_url: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent("VTTray/0.1")
            .build();
        let upload_config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(1800)))
            .user_agent("VTTray/0.1")
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            upload_agent: ureq::Agent::new_with_config(upload_config),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            keys,
            upload_sent: Arc::new(AtomicU64::new(0)),
        }
    }

    /// File bytes sent by the current or last upload; reset when an upload starts.
    pub fn upload_progress(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.upload_sent)
    }

    pub fn lookup(&self, sha256: &str, size_bytes: u64) -> Result<LookupOutcome, AppError> {
        let url = format!("{}/files/{sha256}", self.base_url);
        let response = self.get(&url)?;
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(LookupOutcome::Unknown {
                sha256: sha256.to_string(),
            });
        }
        let body = self.response_body(response, status)?;
        let envelope: VtEnvelope<VtFileData> = serde_json::from_slice(&body)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        Ok(LookupOutcome::Known(envelope.data.into_summary(size_bytes)))
    }

    pub fn rescan(&self, sha256: &str) -> Result<UploadApiResult, AppError> {
        let url = format!("{}/files/{sha256}/analyse", self.base_url);
        let response = self.post_empty(&url)?;
        let status = response.status().as_u16();
        let body = self.response_body(response, status)?;
        let envelope: VtEnvelope<VtAnalysisData> = serde_json::from_slice(&body)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        Ok(UploadApiResult {
            analysis_id: envelope.data.id,
            used_upload_url: false,
        })
    }

    pub fn upload_file(
        &self,
        path: &std::path::Path,
        size_bytes: u64,
    ) -> Result<UploadApiResult, AppError> {
        match upload_route(size_bytes)? {
            UploadRoute::Direct => self.upload_direct(path, size_bytes),
            UploadRoute::UploadUrl => self.upload_via_url(path, size_bytes),
        }
    }

    fn upload_direct(
        &self,
        path: &std::path::Path,
        size_bytes: u64,
    ) -> Result<UploadApiResult, AppError> {
        let url = format!("{}/files", self.base_url);
        let boundary = new_boundary();
        let body = MultipartFileBody::open(path, &boundary)?;
        let length = body.content_length();
        if length < size_bytes {
            return Err(AppError::Validation(
                "upload body was shorter than the file".into(),
            ));
        }
        let response = self.send_multipart(&url, body, &boundary)?;
        let status = response.status().as_u16();
        let data = self.response_body(response, status)?;
        let envelope: VtEnvelope<VtAnalysisData> = serde_json::from_slice(&data)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        Ok(UploadApiResult {
            analysis_id: envelope.data.id,
            used_upload_url: false,
        })
    }

    fn upload_via_url(
        &self,
        path: &std::path::Path,
        size_bytes: u64,
    ) -> Result<UploadApiResult, AppError> {
        let url = self.request_upload_url()?;
        let boundary = new_boundary();
        let body = MultipartFileBody::open(path, &boundary)?;
        if body.content_length() < size_bytes {
            return Err(AppError::Validation(
                "upload body was shorter than the file".into(),
            ));
        }
        let response = self.send_multipart(&url, body, &boundary)?;
        let status = response.status().as_u16();
        let data = self.response_body(response, status)?;
        let envelope: VtEnvelope<VtAnalysisData> = serde_json::from_slice(&data)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        Ok(UploadApiResult {
            analysis_id: envelope.data.id,
            used_upload_url: true,
        })
    }

    pub fn request_upload_url(&self) -> Result<String, AppError> {
        let url = format!("{}/files/upload_url", self.base_url);
        let response = self.get(&url)?;
        let status = response.status().as_u16();
        let body = self.response_body(response, status)?;
        let envelope: VtEnvelope<String> = serde_json::from_slice(&body)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        Ok(envelope.data)
    }

    pub fn poll_analysis(
        &self,
        analysis_id: &str,
        sha256: &str,
        size_bytes: u64,
    ) -> Result<AnalysisProgress, AppError> {
        let url = format!("{}/analyses/{analysis_id}", self.base_url);
        let response = self.get(&url)?;
        let status = response.status().as_u16();
        let body = self.response_body(response, status)?;
        let envelope: VtEnvelope<VtAnalysisData> = serde_json::from_slice(&body)
            .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())))?;
        let complete = envelope.data.is_complete();
        let state = envelope.data.attributes.status.clone();
        let summary = if complete {
            Some(envelope.data.into_summary(sha256, size_bytes))
        } else {
            None
        };
        Ok(AnalysisProgress {
            complete,
            status: state,
            summary,
        })
    }

    fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, AppError> {
        let key = self.key()?;
        let request = self.agent.get(url).header("x-apikey", key.as_str());
        request.call().map_err(map_transport_error)
    }

    fn post_empty(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, AppError> {
        let key = self.key()?;
        let request = self
            .agent
            .post(url)
            .header("x-apikey", key.as_str())
            .header("Content-Length", "0");
        request.send_empty().map_err(map_transport_error)
    }

    fn send_multipart(
        &self,
        url: &str,
        body: MultipartFileBody,
        boundary: &str,
    ) -> Result<ureq::http::Response<ureq::Body>, AppError> {
        let key = self.key()?;
        let length = body.content_length();
        let content_type = format!("multipart/form-data; boundary={boundary}");
        self.upload_sent.store(0, Ordering::Relaxed);
        let body = body.with_progress(Arc::clone(&self.upload_sent));
        self.upload_agent
            .post(url)
            .header("x-apikey", key.as_str())
            .header("Content-Type", content_type)
            .header("Content-Length", length.to_string())
            .send(SendBody::from_owned_reader(body))
            .map_err(map_transport_error)
    }

    fn key(&self) -> Result<Zeroizing<String>, AppError> {
        self.keys
            .get_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::Credential("a VirusTotal API key is not configured".into()))
    }

    fn response_body(
        &self,
        mut response: ureq::http::Response<ureq::Body>,
        status: u16,
    ) -> Result<Vec<u8>, AppError> {
        if (200..300).contains(&status) {
            return response
                .body_mut()
                .with_config()
                .limit(4 * 1024 * 1024)
                .read_to_vec()
                .map_err(|error| AppError::Api(ApiError::Protocol(error.to_string())));
        }
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let body = response
            .body_mut()
            .with_config()
            .limit(8192)
            .read_to_vec()
            .unwrap_or_default();
        if status == 401 || status == 403 {
            return Err(AppError::Api(ApiError::Unauthorized));
        }
        if status == 429 {
            return Err(AppError::Api(ApiError::RateLimited { retry_after }));
        }
        if status == 404 {
            return Err(AppError::Api(ApiError::NotFound));
        }
        let message = serde_json::from_slice::<VtErrorEnvelope>(&body)
            .ok()
            .map(|envelope| {
                let text = if envelope.error.message.is_empty() {
                    envelope.error.code
                } else {
                    envelope.error.message
                };
                redact_error_text(&text)
                    .chars()
                    .take(240)
                    .collect::<String>()
            })
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| format!("request failed with HTTP {status}"));
        Err(AppError::Api(ApiError::Http { status, message }))
    }
}

fn map_transport_error(error: ureq::Error) -> AppError {
    let text = redact_error_text(&error.to_string());
    AppError::Api(ApiError::Network(text))
}
