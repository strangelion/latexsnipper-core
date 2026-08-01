//! Public API types for LaTeXSnipper Core.
//!
//! This crate contains the shared types used across the LaTeXSnipper ecosystem:
//! - [`RecognizeMode`] — recognition mode selector
//! - [`RecognizeRequest`] — builder-pattern request
//! - [`RecognizeResponse`] — recognition result wrapper
//! - [`StreamItem`] — streaming recognition events

pub mod readiness;
pub mod v3;

pub use readiness::{
    CoreErrorCode, EngineReadiness, EphemeralProviderKey, ModeReadiness, ModelQualityReadiness,
    ModelQualityStatus, ModelReadiness, ProviderValidationKey, ProviderValidationLevel,
    ProviderValidationPolicy, ProviderValidationReport, ProviderValidationRequest,
    RecognitionAcceptance, RecognitionAction, RuntimeReadiness, TaskReadiness, ValidationScope,
    READINESS_SCHEMA_VERSION,
};
pub use v3::{
    ApiContractVersionsV3, ApiEnvelopeV3, ApiErrorV3, API_ENVELOPE_VERSION_V3,
    CAPABILITY_SCHEMA_VERSION_V3, DIAGNOSTIC_SCHEMA_VERSION_V3,
};

use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use serde::{Deserialize, Serialize};

// ============================================================================
// RecognizeMode
// ============================================================================

/// Recognition mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecognizeMode {
    Formula,
    CroppedFormula,
    Text,
    Mixed,
    Handwriting,
    Table,
    FormulaLayout,
}

/// Stable, application-facing recognition profile.
///
/// Unlike the legacy [`RecognizeMode`], this type has an explicit wire
/// representation that does not depend on Rust debug formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecognitionProfile {
    Formula,
    #[serde(
        rename = "croppedFormula",
        alias = "cropped_formula",
        alias = "cropped-formula"
    )]
    CroppedFormula,
    Text,
    Mixed,
    Table,
    Handwriting,
    FormulaLayout,
}

impl RecognitionProfile {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Formula,
            Self::CroppedFormula,
            Self::Text,
            Self::Mixed,
            Self::Table,
            Self::Handwriting,
            Self::FormulaLayout,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Formula => "formula",
            Self::CroppedFormula => "croppedFormula",
            Self::Text => "text",
            Self::Mixed => "mixed",
            Self::Table => "table",
            Self::Handwriting => "handwriting",
            Self::FormulaLayout => "formula_layout",
        }
    }
}

impl From<RecognitionProfile> for RecognizeMode {
    fn from(profile: RecognitionProfile) -> Self {
        match profile {
            RecognitionProfile::Formula => Self::Formula,
            RecognitionProfile::CroppedFormula => Self::CroppedFormula,
            RecognitionProfile::Text => Self::Text,
            RecognitionProfile::Mixed => Self::Mixed,
            RecognitionProfile::Table => Self::Table,
            RecognitionProfile::Handwriting => Self::Handwriting,
            RecognitionProfile::FormulaLayout => Self::FormulaLayout,
        }
    }
}

impl From<RecognizeMode> for RecognitionProfile {
    fn from(mode: RecognizeMode) -> Self {
        match mode {
            RecognizeMode::Formula => Self::Formula,
            RecognizeMode::CroppedFormula => Self::CroppedFormula,
            RecognizeMode::Text => Self::Text,
            RecognizeMode::Mixed => Self::Mixed,
            RecognizeMode::Table => Self::Table,
            RecognizeMode::Handwriting => Self::Handwriting,
            RecognizeMode::FormulaLayout => Self::FormulaLayout,
        }
    }
}

impl RecognizeMode {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Formula,
            Self::CroppedFormula,
            Self::Text,
            Self::Mixed,
            Self::Handwriting,
            Self::Table,
            Self::FormulaLayout,
        ]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Formula => "formula",
            Self::CroppedFormula => "cropped-formula",
            Self::Text => "text",
            Self::Mixed => "mixed",
            Self::Handwriting => "handwriting",
            Self::Table => "table",
            Self::FormulaLayout => "formula-layout",
        }
    }

    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Formula => &["math"],
            Self::CroppedFormula => &["cropped_formula", "croppedFormula"],
            Self::Text => &["ocr"],
            Self::Mixed => &["document"],
            Self::Handwriting => &["handwrite"],
            Self::Table => &[],
            Self::FormulaLayout => &["formula_layout", "layout"],
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        let normalized = label.trim().to_ascii_lowercase();
        Self::all().iter().copied().find(|mode| {
            mode.label() == normalized || mode.aliases().contains(&normalized.as_str())
        })
    }
}

// ============================================================================
// RecognizeRequest
// ============================================================================

/// A request to recognize content in an image.
/// Supports Builder pattern for flexible configuration.
pub struct RecognizeRequest {
    pub image: SnipperImage,
    pub mode: RecognizeMode,
    pub max_regions: usize,
    pub min_confidence: f32,
}

impl RecognizeRequest {
    /// Create a new request with an image and default settings.
    pub fn new(image: SnipperImage) -> Self {
        Self {
            image,
            mode: RecognizeMode::Formula,
            max_regions: 100,
            min_confidence: 0.25,
        }
    }

    /// Set the recognition mode.
    pub fn mode(mut self, mode: RecognizeMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the maximum number of regions to process.
    pub fn max_regions(mut self, max: usize) -> Self {
        self.max_regions = max;
        self
    }

    /// Set the minimum confidence threshold.
    pub fn min_confidence(mut self, threshold: f32) -> Self {
        self.min_confidence = threshold;
        self
    }
}

// ============================================================================
// RecognizeResponse
// ============================================================================

/// The result of a recognition operation.
pub struct RecognizeResponse {
    pub document: Document,
    pub mode: RecognizeMode,
    pub region_count: usize,
    pub elapsed_ms: u64,
}

impl RecognizeResponse {
    pub fn new(
        document: Document,
        mode: RecognizeMode,
        region_count: usize,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            document,
            mode,
            region_count,
            elapsed_ms,
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn region_count(&self) -> usize {
        self.region_count
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
}

// ============================================================================
// StreamItem
// ============================================================================

/// A single item in a streaming recognition response.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StreamItem {
    /// A region has been detected.
    RegionDetected {
        index: usize,
        class: String,
        confidence: f32,
    },
    /// A region has been recognized.
    RegionRecognized {
        index: usize,
        text: String,
        confidence: f32,
    },
    /// The full document is ready.
    Completed {
        document: Document,
        total_regions: usize,
        elapsed_ms: u64,
    },
    /// An error occurred during processing.
    Error { message: String },
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn recognition_profile_has_stable_snake_case_values() {
        let values = RecognitionProfile::all()
            .iter()
            .map(|profile| serde_json::to_string(profile).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            vec![
                "\"formula\"",
                "\"croppedFormula\"",
                "\"text\"",
                "\"mixed\"",
                "\"table\"",
                "\"handwriting\"",
                "\"formula_layout\"",
            ]
        );
    }

    #[test]
    fn recognition_profile_and_legacy_mode_are_lossless() {
        for profile in RecognitionProfile::all() {
            let mode = RecognizeMode::from(*profile);
            assert_eq!(RecognitionProfile::from(mode), *profile);
        }
    }
}
