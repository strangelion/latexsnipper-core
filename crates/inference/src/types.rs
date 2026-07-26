use latexsnipper_ast::{Quad, Rect};

use crate::{PostProcessResult, RecognitionProvenance};

/// A detected region with bounding box and class.
#[derive(Debug, Clone)]
pub struct DetectionBox {
    pub rect: Rect,
    /// Four-point quad for rotated text regions; None for axis-aligned detectors.
    pub quad: Option<Quad>,
    pub confidence: f32,
    pub class_id: usize,
    pub class_name: String,
}

impl DetectionBox {
    /// Create a rect-only detection (e.g. from YOLO or axis-aligned detectors).
    pub fn rect(rect: Rect, confidence: f32, class_id: usize, class_name: String) -> Self {
        Self {
            rect,
            quad: None,
            confidence,
            class_id,
            class_name,
        }
    }

    /// Create a detection with a quad (e.g. from DBNet). Automatically computes rect from quad.
    pub fn quad(quad: Quad, confidence: f32, class_id: usize, class_name: String) -> Self {
        let rect = quad.bounding_rect();
        Self {
            rect,
            quad: Some(quad),
            confidence,
            class_id,
            class_name,
        }
    }
}

/// Result of recognition.
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub confidence: f32,
    pub raw_text: Option<String>,
    pub normalized_text: Option<String>,
    pub provenance: Option<RecognitionProvenance>,
    pub postprocess: Option<PostProcessResult>,
}

impl RecognitionResult {
    pub fn new(text: impl Into<String>, confidence: f32) -> Self {
        Self {
            text: text.into(),
            confidence,
            raw_text: None,
            normalized_text: None,
            provenance: None,
            postprocess: None,
        }
    }

    pub fn from_postprocess(result: PostProcessResult) -> Self {
        Self {
            text: result.corrected.clone(),
            confidence: result.normalized_confidence,
            raw_text: Some(result.raw.clone()),
            normalized_text: Some(result.normalized.clone()),
            provenance: None,
            postprocess: Some(result),
        }
    }

    pub fn with_provenance(mut self, provenance: RecognitionProvenance) -> Self {
        self.provenance = Some(provenance);
        self
    }

    /// Attach the execution identity when a backend did not already provide
    /// more specific provenance (for example PP-FormulaNet's native adapter).
    pub fn ensure_runtime_provenance(
        mut self,
        model_id: impl Into<String>,
        model_version: impl Into<String>,
        runtime: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        if self.provenance.is_none() {
            self.provenance = Some(RecognitionProvenance {
                model_id: model_id.into(),
                model_version: model_version.into(),
                runtime: runtime.into(),
                provider: provider.into(),
                source_region: None,
                raw_confidence: Some(self.confidence),
                normalized_confidence: Some(self.confidence),
                transformations: self
                    .postprocess
                    .as_ref()
                    .map(|evidence| evidence.transformations.clone())
                    .unwrap_or_default(),
            });
        }
        self
    }
}

/// A cell in a recognized table grid.
#[derive(Debug, Clone)]
pub struct GridCell {
    pub row: usize,
    pub col: usize,
    pub rowspan: u32,
    pub colspan: u32,
    pub rect: Rect,
}
