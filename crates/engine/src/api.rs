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
    pub fn new(document: Document, mode: RecognizeMode, region_count: usize, elapsed_ms: u64) -> Self {
        Self { document, mode, region_count, elapsed_ms }
    }

    pub fn document(&self) -> &Document { &self.document }
    pub fn region_count(&self) -> usize { self.region_count }
    pub fn elapsed_ms(&self) -> u64 { self.elapsed_ms }
}

/// A single item in a streaming recognition response.
#[derive(Debug, Clone)]
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
    Error {
        message: String,
    },
}
