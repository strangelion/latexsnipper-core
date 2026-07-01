pub mod formula_detector;
pub mod formula_lines;
pub mod formula_recognizer;
pub mod latex_repair;
pub mod text_detector;
pub mod text_recognizer;
pub mod text_segmentation;
pub mod trocr_tokenizer;
pub mod types;

pub use formula_detector::{
    detect_formulas, filter_formula_detections, group_formula_detections, DetectionParams,
};
pub use formula_recognizer::{recognize_formula, RecognitionParams};
pub use latex_repair::{has_severe_latex_issue, latex_quality_flags, repair_latex};
pub use text_detector::{detect_text, TextDetParams};
pub use text_recognizer::{load_keys, recognize_text, recognize_text_with_keys, TextRecParams};
pub use types::{DetectionBox, RecognitionResult};
