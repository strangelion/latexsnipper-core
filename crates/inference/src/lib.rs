pub mod formula_detector;
pub mod formula_recognizer;
pub mod text_detector;
pub mod text_recognizer;
pub mod types;

pub use types::{DetectionBox, RecognitionResult};
pub use formula_detector::{DetectionParams, detect_formulas};
pub use formula_recognizer::{RecognitionParams, recognize_formula};
pub use text_detector::{TextDetParams, detect_text};
pub use text_recognizer::{TextRecParams, recognize_text};
