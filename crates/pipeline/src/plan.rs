use crate::{
    CropNode, DetectorNode, DocumentParseMode, FormulaLayoutNode, HandwritingRecognizerNode,
    LayoutNode, PipelineCapability, PipelineGraph, PipelineNode, PostprocessNode, RecognizerNode,
    RegionResolveNode, TableRecognizerNode, TableStructureNode,
};

/// A controlled node vocabulary that can be safely selected by a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineNodeSpec {
    Layout,
    DetectFormula,
    DetectText,
    DetectTable,
    DetectHandwriting,
    Crop,
    ResolveRegions,
    RecognizeFormula,
    RecognizeText,
    TableStructure,
    RecognizeTable,
    RecognizeHandwriting,
    FormulaLayout,
    Postprocess,
}

impl PipelineNodeSpec {
    pub fn name(self) -> &'static str {
        match self {
            Self::Layout => "layout_analysis",
            Self::DetectFormula => "detect_formula",
            Self::DetectText => "detect_text",
            Self::DetectTable => "detect_table",
            Self::DetectHandwriting => "detect_handwriting",
            Self::Crop => "crop",
            Self::ResolveRegions => "region_resolve",
            Self::RecognizeFormula => "recognize_formula",
            Self::RecognizeText => "recognize_text",
            Self::TableStructure => "table_structure",
            Self::RecognizeTable => "recognize_table",
            Self::RecognizeHandwriting => "recognize_handwriting",
            Self::FormulaLayout => "formula_layout",
            Self::Postprocess => "postprocess",
        }
    }

    fn build(self) -> Box<dyn PipelineNode> {
        match self {
            Self::Layout => Box::new(LayoutNode::new()),
            Self::DetectFormula => Box::new(DetectorNode::formula()),
            Self::DetectText => Box::new(DetectorNode::text()),
            Self::DetectTable => Box::new(DetectorNode::table()),
            Self::DetectHandwriting => Box::new(DetectorNode::handwriting()),
            Self::Crop => Box::new(CropNode::default()),
            Self::ResolveRegions => Box::new(RegionResolveNode::new()),
            Self::RecognizeFormula => Box::new(RecognizerNode::formula()),
            Self::RecognizeText => Box::new(RecognizerNode::text()),
            Self::TableStructure => Box::new(TableStructureNode::new()),
            Self::RecognizeTable => Box::new(TableRecognizerNode::new()),
            Self::RecognizeHandwriting => Box::new(HandwritingRecognizerNode::new()),
            Self::FormulaLayout => Box::new(FormulaLayoutNode::new()),
            Self::Postprocess => Box::new(PostprocessNode::new()),
        }
    }
}

/// One dependency edge in a controlled pipeline plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineDependency {
    pub node: PipelineNodeSpec,
    pub depends_on: Vec<PipelineNodeSpec>,
}

/// Declarative, inspectable description of a pipeline graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelinePlan {
    pub id: String,
    pub parse_mode: DocumentParseMode,
    pub nodes: Vec<PipelineNodeSpec>,
    pub edges: Vec<PipelineDependency>,
    pub required_capabilities: Vec<PipelineCapability>,
}

impl PipelinePlan {
    pub fn build_graph(&self) -> PipelineGraph {
        let mut graph = PipelineGraph::new(&self.id);
        for node in &self.nodes {
            let dependencies: Vec<String> = self
                .edges
                .iter()
                .find(|edge| edge.node == *node)
                .map(|edge| {
                    edge.depends_on
                        .iter()
                        .map(|dependency| dependency.name().to_string())
                        .collect()
                })
                .unwrap_or_default();
            if dependencies.is_empty() {
                graph.add_node(node.build());
            } else {
                graph.add_node_with_deps(node.build(), dependencies);
            }
        }
        graph
    }
}
