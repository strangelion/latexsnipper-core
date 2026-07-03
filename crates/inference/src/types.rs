use latexsnipper_ast::Rect;

/// A detected region with bounding box and class.
#[derive(Debug, Clone)]
pub struct DetectionBox {
    pub rect: Rect,
    pub confidence: f32,
    pub class_id: usize,
    pub class_name: String,
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
