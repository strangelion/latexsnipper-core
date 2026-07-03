pub mod context;
pub mod graph;
pub mod manifest;
pub mod node;
pub mod nodes;
pub mod sdk;
pub mod simple;

pub use context::PipelineContext;
pub use graph::PipelineGraph;
pub use manifest::PipelineManifest;
pub use node::{PipelineNode, TransformNode};
pub use nodes::crop_node::CropNode;
pub use nodes::detector_node::{DetectorNode, DetectorType};
pub use nodes::formula_layout_node::FormulaLayoutNode;
pub use nodes::handwriting_recognizer_node::HandwritingRecognizerNode;
pub use nodes::normalize_node::NormalizeNode;
pub use nodes::page_assembly_node::PageAssemblyNode;
pub use nodes::page_iterator_node::PageIteratorNode;
pub use nodes::postprocess_node::PostprocessNode;
pub use nodes::recognizer_node::{RecognizerNode, RecognizerType};
pub use nodes::resize_node::ResizeNode;
pub use nodes::table_structure_node::TableStructureNode;
pub use nodes::table_recognizer_node::TableRecognizerNode;
pub use sdk::Snipper;
pub use simple::{
    MockCropper, MockDetector, MockRecognizer, PipelineContext as SimpleContext, SimplePipeline,
    Stage,
};

// Re-export core types for downstream users
pub use latexsnipper_foundation::SnipperError;

// Re-export AST types
pub use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, FormulaSource, HeadingBlock, Inline, ListBlock,
    ListItem, Metadata, NodeId, Page, ParagraphBlock, QuoteBlock, SourceInfo, TableBlock,
    TableCell, TextRun,
};

// Re-export Conversion types
pub use latexsnipper_conversion::{DocumentConverter, OutputFormat};
