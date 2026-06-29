use latexsnipper_ast::Rect;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::DetectionBox;

/// Mock detector that returns fixed detection results.
pub struct FakeDetector {
    boxes: Vec<DetectionBox>,
}

impl FakeDetector {
    pub fn new(boxes: Vec<DetectionBox>) -> Self {
        Self { boxes }
    }

    /// Create a detector that returns a single formula box.
    pub fn single_formula(confidence: f32) -> Self {
        Self {
            boxes: vec![DetectionBox {
                rect: Rect::new(10.0, 10.0, 100.0, 50.0),
                confidence,
                class_id: 1,
                class_name: "isolated".into(),
            }],
        }
    }

    /// Create a detector that returns a single text box.
    pub fn single_text(confidence: f32) -> Self {
        Self {
            boxes: vec![DetectionBox {
                rect: Rect::new(10.0, 10.0, 200.0, 30.0),
                confidence,
                class_id: 0,
                class_name: "text".into(),
            }],
        }
    }

    /// Create a detector that returns empty results.
    pub fn empty() -> Self {
        Self { boxes: vec![] }
    }

    pub fn detect(&self, _image: &SnipperImage) -> Vec<DetectionBox> {
        self.boxes.clone()
    }
}
