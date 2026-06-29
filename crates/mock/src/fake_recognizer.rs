use latexsnipper_ast::Rect;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{DetectionBox, RecognitionResult};

/// Mock recognizer that returns fixed recognition results.
pub struct FakeRecognizer {
    results: Vec<RecognitionResult>,
}

impl FakeRecognizer {
    pub fn new(results: Vec<RecognitionResult>) -> Self {
        Self { results }
    }

    /// Create a recognizer that returns a single formula.
    pub fn formula(latex: &str, confidence: f32) -> Self {
        Self {
            results: vec![RecognitionResult {
                text: latex.to_string(),
                confidence,
            }],
        }
    }

    /// Create a recognizer that returns a single text line.
    pub fn text(text: &str, confidence: f32) -> Self {
        Self {
            results: vec![RecognitionResult {
                text: text.to_string(),
                confidence,
            }],
        }
    }

    /// Create a recognizer for multiple regions.
    pub fn from_detections(detections: &[DetectionBox], texts: &[&str]) -> Self {
        let results = detections.iter().zip(texts.iter()).map(|(d, t)| {
            RecognitionResult {
                text: t.to_string(),
                confidence: d.confidence,
            }
        }).collect();
        Self { results }
    }

    pub fn recognize(&self, _image: &SnipperImage, _rect: &Rect) -> RecognitionResult {
        self.results.first().cloned().unwrap_or(RecognitionResult {
            text: String::new(),
            confidence: 0.0,
        })
    }

    pub fn recognize_all(&self) -> Vec<RecognitionResult> {
        self.results.clone()
    }
}
