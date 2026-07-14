pub mod adapter_registry;
pub mod adapters;
pub mod formula_detector;
pub mod formula_lines;
pub mod formula_parser;
pub mod formula_recognizer;
pub mod handwriting_detector;
pub mod handwriting_postprocess;
pub mod language;
pub mod latex_repair;
pub mod symbol_detector;
pub mod table_detector;
pub mod table_structure;
pub mod table_transformer;
pub mod text_detector;
pub mod text_recognizer;
pub mod text_segmentation;
pub mod trocr_tokenizer;
pub mod types;
pub mod yolo_utils;

pub use adapter_registry::register_builtin_adapters;
pub use formula_detector::{
    detect_formulas, filter_formula_detections, group_formula_detections, DetectionParams,
};
pub use formula_parser::parse_formula_latex;
pub use formula_recognizer::{
    load_tokenizer_from_str, recognize_formula, recognize_formula_with_tokenizer, RecognitionParams,
};
pub use handwriting_detector::{
    detect_handwriting, filter_handwriting_detections, HandwritingDetParams,
};
pub use handwriting_postprocess::postprocess_handwriting;
pub use language::{Language, LanguageDetector};
pub use latex_repair::{has_severe_latex_issue, latex_quality_flags, repair_latex};
pub use symbol_detector::{detect_symbols, SymbolDetParams, SymbolDetection};
pub use table_detector::{detect_tables, filter_table_detections, TableDetParams};
pub use table_structure::{
    parse_table_structure, recognize_structure_slanet, recognize_table_structure, CellInfo,
    ColInfo, RowInfo, TableStructure,
};
pub use table_transformer::{
    build_grid_from_detections, recognize_table_transformer,
    recognize_table_transformer_with_max_edge, TableTransformerDetection, TABLE_DETECTION_LABELS,
    TABLE_STRUCTURE_LABELS,
};
pub use text_detector::{detect_text, TextDetParams};
pub use text_recognizer::{
    load_keys, load_keys_from_str, recognize_text, recognize_text_with_keys, TextRecParams,
};
pub use types::{DetectionBox, GridCell, RecognitionResult};
