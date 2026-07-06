pub mod artifacts;
pub mod context;
pub mod graph;
pub mod manifest;
pub mod node;
pub mod nodes;
pub mod reading_order;
pub mod text_recognition_service;
#[deprecated(
    since = "1.1.0",
    note = "Use latexsnipper_engine::sdk::Snipper instead"
)]
pub mod sdk;
pub mod simple;

pub use artifacts::{CropRegion, PipelineArtifacts, RecognizedTable};
pub use context::PipelineContext;
pub use graph::PipelineGraph;
pub use manifest::PipelineManifest;
pub use node::{PipelineNode, TransformNode};
pub use nodes::crop_node::CropNode;
pub use nodes::detector_node::DetectorNode;
pub use nodes::formula_layout_node::FormulaLayoutNode;
pub use nodes::handwriting_recognizer_node::HandwritingRecognizerNode;
pub use nodes::normalize_node::NormalizeNode;
pub use nodes::page_assembly_node::PageAssemblyNode;
pub use nodes::page_iterator_node::PageIteratorNode;
pub use nodes::postprocess_node::PostprocessNode;
pub use nodes::recognizer_node::RecognizerNode;
pub use nodes::resize_node::ResizeNode;
pub use nodes::table_recognizer_node::TableRecognizerNode;
pub use nodes::table_structure_node::TableStructureNode;
#[allow(deprecated)]
pub use sdk::Snipper;
pub use simple::{MockCropper, MockDetector, MockRecognizer, SimpleContext, SimplePipeline, Stage};

// Re-export core types for downstream users
pub use latexsnipper_foundation::SnipperError;

// Re-export commonly used AST types (used in pipeline output)
pub use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, SourceInfo, TableBlock,
    TableCell, TextRun,
};
