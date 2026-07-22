//! Model Package adapters for built-in models.
//!
//! Each adapter wraps existing model-specific logic and implements
//! the ModelPackage/ModelExecutor traits.

pub mod crnn_text_recognizer;
pub mod dbnet_text_detector;
pub mod formula_backend_adapter;
pub mod layout_detector;
pub mod trocr_formula_recognizer;
pub mod yolo_v8_detector;

pub use crnn_text_recognizer::CrnnTextRecognizerPackage;
pub use dbnet_text_detector::DbNetTextDetectorPackage;
pub use formula_backend_adapter::FormulaBackendPackage;
pub use layout_detector::LayoutDetectorPackage;
pub use trocr_formula_recognizer::TrOcrFormulaPackage;
pub use yolo_v8_detector::YoloV8DetectorPackage;
