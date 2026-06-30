use latexsnipper_ast::{Document, Page, Block, FormulaBlock, Formula, ParagraphBlock, Inline, TextRun, Rect};
use latexsnipper_foundation::Result;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{DetectionBox, RecognitionResult};

use crate::fake_detector::FakeDetector;
use crate::fake_recognizer::FakeRecognizer;

/// Mock pipeline that combines fake detector and recognizer.
pub struct FakePipeline {
    detector: FakeDetector,
    recognizer: FakeRecognizer,
}

impl FakePipeline {
    pub fn new(detector: FakeDetector, recognizer: FakeRecognizer) -> Self {
        Self { detector, recognizer }
    }

    /// Create a pipeline that returns a single formula.
    pub fn formula(latex: &str, confidence: f32) -> Self {
        Self {
            detector: FakeDetector::single_formula(confidence),
            recognizer: FakeRecognizer::formula(latex, confidence),
        }
    }

    /// Create a pipeline that returns a single text line.
    pub fn text(text: &str, confidence: f32) -> Self {
        Self {
            detector: FakeDetector::single_text(confidence),
            recognizer: FakeRecognizer::text(text, confidence),
        }
    }

    /// Create a pipeline that returns mixed formula + text.
    pub fn mixed(formula_latex: &str, text: &str, confidence: f32) -> Self {
        Self {
            detector: FakeDetector::new(vec![
                DetectionBox {
                    rect: Rect::new(10.0, 10.0, 100.0, 50.0),
                    confidence,
                    class_id: 1,
                    class_name: "isolated".into(),
                },
                DetectionBox {
                    rect: Rect::new(10.0, 70.0, 200.0, 30.0),
                    confidence,
                    class_id: 0,
                    class_name: "text".into(),
                },
            ]),
            recognizer: FakeRecognizer::from_detections(
                &[
                    DetectionBox {
                        rect: Rect::new(10.0, 10.0, 100.0, 50.0),
                        confidence,
                        class_id: 1,
                        class_name: "isolated".into(),
                    },
                    DetectionBox {
                        rect: Rect::new(10.0, 70.0, 200.0, 30.0),
                        confidence,
                        class_id: 0,
                        class_name: "text".into(),
                    },
                ],
                &[formula_latex, text],
            ),
        }
    }

    /// Run the mock pipeline and return a Document.
    pub fn run(&self, _image: &SnipperImage) -> Result<Document> {
        let detections = self.detector.detect(_image);
        let results = self.recognizer.recognize_all();

        let mut blocks = Vec::new();
        for (i, detection) in detections.iter().enumerate() {
            let result = results.get(i).cloned().unwrap_or(RecognitionResult {
                text: String::new(),
                confidence: 0.0,
            });

            if detection.class_name == "isolated" || detection.class_name == "embedding" {
                blocks.push(Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: latexsnipper_ast::FormulaSource::Latex(result.text),
                        display_mode: detection.class_name == "isolated",
                        confidence: result.confidence,
                    },
                    geometry: Some(detection.rect),
                    source: None,
                }));
            } else {
                blocks.push(Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun {
                        text: result.text,
                        bold: None,
                        italic: None,
                    })],
                    geometry: Some(detection.rect),
                    source: None,
                }));
            }
        }

        Ok(Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks,
                page_number: None,
            }],
        })
    }
}
