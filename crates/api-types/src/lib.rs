//! Public API types for LaTeXSnipper Core.
//!
//! This crate contains the shared types used across the LaTeXSnipper ecosystem:
//! - [`RecognizeMode`] — recognition mode selector
//! - [`RecognizeRequest`] — builder-pattern request
//! - [`RecognizeResponse`] — recognition result wrapper
//! - [`StreamItem`] — streaming recognition events

use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;

// ============================================================================
// RecognizeMode
// ============================================================================

/// Recognition mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecognizeMode {
    Formula,
    Text,
    Mixed,
    Handwriting,
    Table,
    FormulaLayout,
}

impl RecognizeMode {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Formula,
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
