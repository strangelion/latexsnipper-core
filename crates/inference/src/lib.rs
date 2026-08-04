pub mod adapter_registry;
pub mod adapters;
pub mod cell_candidate;
pub mod decoder_state;
pub mod formula_backend;
pub mod formula_detector;
pub mod formula_lines;
pub mod formula_parser;
pub mod formula_recognizer;
pub mod handwriting_detector;
pub mod handwriting_postprocess;
pub mod language;
pub mod latex_repair;
pub mod postprocess;
pub mod pp_formulanet;
pub mod pp_formulanet_adapter;
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
pub use adapters::formula_backend_adapter::FormulaBackendPackage;
pub use cell_candidate::{
    cell_recognition_route, looks_like_hard_negative, select_ambiguous_cell_candidate,
    CellCandidate, CellCandidateDecision, CellCandidateKind, CellCandidateScore,
    CellGeometryEvidence, CellRecognitionRoute,
};
pub use decoder_state::{
    AttentionKind, AxisSemantic, DecoderDType, DecoderStateEntry, DecoderStateError,
    DecoderStateObservation, DecoderStateRole, DecoderStateSchema,
};
pub use formula_backend::{BackendConfig, FormulaBackend, OnnxFormulaBackend};
pub use formula_detector::{
    detect_formulas, filter_formula_detections, group_formula_detections, DetectionParams,
};
pub use formula_lines::{
    plan_formula_segmentation, split_formula_line_groups, FormulaLineCrop, FormulaLineGroup,
    FormulaSegmentPlan, FormulaSegmentPolicy, FormulaSegmentationClass,
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
pub use latexsnipper_ast::{
    PostProcessResult, RecognitionProvenance, SourcePolygon, TextDiff, TransformationEvidence,
    TransformationMode, TriggerDecision, ValidationEvidence,
};
pub use postprocess::{
    Candidate, PostProcessError, RecognitionPostProcessor, RuleBasedRecognitionPostProcessor,
};
#[allow(deprecated)]
pub use pp_formulanet::PPFormulaNetBackend;
pub use pp_formulanet_adapter::PPFormulaNetAdapter;
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
