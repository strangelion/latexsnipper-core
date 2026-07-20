pub mod artifacts;
pub mod capability;
pub mod context;
pub mod graph;
pub mod manifest;
pub mod node;
pub mod nodes;
pub mod opendoc_hybrid;
pub mod pdf_fusion;
pub mod plan;
pub mod planner;
pub mod profile;
pub mod reading_order;
pub mod region_graph;
#[deprecated(
    since = "1.1.0",
    note = "Use latexsnipper_engine::sdk::Snipper instead"
)]
#[cfg(feature = "native")]
pub mod sdk;
pub mod simple;
pub mod text_recognition_service;

pub use crate::opendoc_hybrid::DocumentParseMode;
pub use artifacts::{CropRegion, PipelineArtifacts, RecognizedTable};
pub use capability::PipelineCapability;
pub use context::PipelineContext;
pub use graph::PipelineGraph;
pub use latexsnipper_artifact::{
    ArtifactEdge, ArtifactEdgeKind, ArtifactGraph, ArtifactId, ArtifactKind, ArtifactRecord,
};
pub use manifest::PipelineManifest;
pub use node::{PipelineNode, TransformNode};
pub use nodes::crop_node::CropNode;
pub use nodes::detector_node::DetectorNode;
pub use nodes::formula_layout_node::FormulaLayoutNode;
pub use nodes::handwriting_recognizer_node::HandwritingRecognizerNode;
pub use nodes::layout_node::LayoutNode;
pub use nodes::normalize_node::NormalizeNode;
pub use nodes::page_assembly_node::PageAssemblyNode;
pub use nodes::page_iterator_node::PageIteratorNode;
pub use nodes::postprocess_node::PostprocessNode;
pub use nodes::recognizer_node::RecognizerNode;
pub use nodes::region_resolve_node::RegionResolveNode;
pub use nodes::resize_node::ResizeNode;
pub use nodes::table_recognizer_node::TableRecognizerNode;
pub use nodes::table_structure_node::TableStructureNode;
pub use pdf_fusion::{
    fuse_pdf_regions, PdfFusionDecision, PdfFusionPolicy, PdfFusionReason, PdfRegionCandidate,
    PdfRegionSource,
};
pub use plan::{PipelineDependency, PipelineNodeSpec, PipelinePlan};
pub use planner::PipelinePlanner;
pub use profile::PipelineProfile;
#[allow(deprecated)]
#[cfg(feature = "native")]
pub use sdk::Snipper;
pub use simple::{MockCropper, MockDetector, MockRecognizer, SimpleContext, SimplePipeline, Stage};

// Re-export core types for downstream users
pub use latexsnipper_foundation::SnipperError;

// Re-export region graph types
pub use crate::region_graph::RegionKind;

// Re-export commonly used AST types (used in pipeline output)
pub use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, SourceInfo, TableBlock,
    TableCell, TextRun,
};
