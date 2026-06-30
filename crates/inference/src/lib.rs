pub mod formula_detector;
pub mod formula_lines;
pub mod formula_recognizer;
pub mod latex_repair;
pub mod text_detector;
pub mod text_recognizer;
pub mod text_segmentation;
pub mod trocr_tokenizer;
pub mod types;

pub use types::{DetectionBox, RecognitionResult};
pub use formula_detector::{DetectionParams, detect_formulas, group_formula_detections, filter_formula_detections};
pub use formula_recognizer::{RecognitionParams, recognize_formula};
pub use latex_repair::{repair_latex, latex_quality_flags, has_severe_latex_issue};
pub use text_detector::{TextDetParams, detect_text};
pub use text_recognizer::{TextRecParams, recognize_text, recognize_text_with_keys, load_keys};
