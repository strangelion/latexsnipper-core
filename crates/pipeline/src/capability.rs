/// Capabilities required for a pipeline plan to produce its intended result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineCapability {
    LayoutAnalysis,
    FormulaDetection,
    FormulaRecognition,
    TextDetection,
    TextRecognition,
    TableDetection,
    TableStructure,
    TableRecognition,
    HandwritingDetection,
    HandwritingRecognition,
    FormulaLayout,
}
