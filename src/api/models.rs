use crate::core::types::LookupSummary;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VtEnvelope<T> {
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct VtFileData {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub attributes: VtFileAttributes,
}

#[derive(Debug, Default, Deserialize)]
pub struct VtFileAttributes {
    #[serde(default)]
    pub last_analysis_stats: VtStats,
    pub reputation: Option<i64>,
    pub type_description: Option<String>,
    pub meaningful_name: Option<String>,
    pub first_submission_date: Option<i64>,
    pub last_analysis_date: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct VtStats {
    #[serde(default)]
    pub malicious: u32,
    #[serde(default)]
    pub suspicious: u32,
    #[serde(default)]
    pub undetected: u32,
    #[serde(default)]
    pub harmless: u32,
    #[serde(default)]
    pub timeout: u32,
}

#[derive(Debug, Deserialize)]
pub struct VtAnalysisData {
    pub id: String,
    #[serde(default)]
    pub attributes: VtAnalysisAttributes,
}

#[derive(Debug, Default, Deserialize)]
pub struct VtAnalysisAttributes {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub stats: VtStats,
    pub sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VtStringData {
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct VtErrorEnvelope {
    pub error: VtErrorBody,
}

#[derive(Debug, Deserialize)]
pub struct VtErrorBody {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

impl VtFileData {
    pub fn into_summary(self, size_bytes: u64) -> LookupSummary {
        LookupSummary {
            sha256: self.id,
            size_bytes,
            malicious: self.attributes.last_analysis_stats.malicious,
            suspicious: self.attributes.last_analysis_stats.suspicious,
            undetected: self.attributes.last_analysis_stats.undetected,
            harmless: self.attributes.last_analysis_stats.harmless,
            timeout: self.attributes.last_analysis_stats.timeout,
            reputation: self.attributes.reputation,
            type_description: self.attributes.type_description,
            meaningful_name: self.attributes.meaningful_name,
            first_submission_at: self.attributes.first_submission_date,
            last_analysis_at: self.attributes.last_analysis_date,
        }
    }
}

impl VtAnalysisData {
    pub fn is_complete(&self) -> bool {
        self.attributes.status == "completed"
    }

    pub fn into_summary(self, fallback_sha256: &str, size_bytes: u64) -> LookupSummary {
        LookupSummary {
            sha256: self
                .attributes
                .sha256
                .unwrap_or_else(|| fallback_sha256.to_string()),
            size_bytes,
            malicious: self.attributes.stats.malicious,
            suspicious: self.attributes.stats.suspicious,
            undetected: self.attributes.stats.undetected,
            harmless: self.attributes.stats.harmless,
            timeout: self.attributes.stats.timeout,
            reputation: None,
            type_description: None,
            meaningful_name: None,
            first_submission_at: None,
            last_analysis_at: None,
        }
    }
}
