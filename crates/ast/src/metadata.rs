use serde::{Deserialize, Serialize};

/// Document-level metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    pub language: Option<String>,
    pub created_at: Option<String>,
    pub ocr_model: Option<String>,
    pub ocr_version: Option<String>,
    pub ocr_time_ms: Option<u64>,
}

/// OCR-specific metadata attached to blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrMetadata {
    pub confidence: f32,
    pub geometry: Option<crate::Rect>,
    pub rotation: Option<f32>,
    pub model: Option<String>,
    pub time_ms: Option<u64>,
}
