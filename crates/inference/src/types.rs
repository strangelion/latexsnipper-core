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
