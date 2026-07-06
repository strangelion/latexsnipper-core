use latexsnipper_ast::{Quad, Rect};

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
