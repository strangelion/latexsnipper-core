use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;

use crate::engine::RecognizeMode;

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

/// The result of a recognition operation.
pub struct RecognizeResponse {
    pub document: Document,
    pub mode: RecognizeMode,
    pub region_count: usize,
    pub elapsed_ms: u64,
}

impl RecognizeResponse {
    /// Create a new response.
    pub fn new(document: Document, mode: RecognizeMode, region_count: usize, elapsed_ms: u64) -> Self {
        Self { document, mode, region_count, elapsed_ms }
    }

    /// Get the recognized document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Get the number of detected regions.
    pub fn region_count(&self) -> usize {
        self.region_count
    }

    /// Get the elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
}
