//! Model Package adapters for built-in models.
//!
//! Each adapter wraps existing model-specific logic and implements
//! the ModelPackage/ModelExecutor traits.

pub mod yolo_v8_detector;
pub mod trocr_formula_recognizer;
pub mod crnn_text_recognizer;

pub use yolo_v8_detector::YoloV8DetectorPackage;
pub use trocr_formula_recognizer::TrOcrFormulaPackage;
pub use crnn_text_recognizer::CrnnTextRecognizerPackage;
