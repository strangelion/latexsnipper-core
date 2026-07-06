use crate::model_resolver::ModelId;
use crate::RuntimeBackend;
use latexsnipper_foundation::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Task types that models can perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelTask {
    FormulaDetection,
    FormulaRecognition,
    TextDetection,
    TextRecognition,
    TableDetection,
    TableStructure,
    LayoutAnalysis,
    HandwritingRecognition,
}

/// Tensor specification for model inputs/outputs.
#[derive(Debug, Clone)]
pub struct TensorSpec {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: TensorDtype,
}

/// Supported tensor data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorDtype {
    Float32,
    Int64,
    Int32,
    UInt8,
}

/// Model descriptor — describes what a model does and its capabilities.
#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub task: ModelTask,
    pub version: String,
    pub input_spec: TensorSpec,
    pub output_spec: Vec<TensorSpec>,
    pub artifact_paths: Vec<String>,
}

/// Input to a model executor.
#[derive(Debug, Clone)]
pub struct ModelInput {
    pub name: String,
    pub data: Vec<u8>,
    pub shape: Vec<usize>,
    pub dtype: TensorDtype,
}

/// Output from a model executor.
#[derive(Debug, Clone)]
pub enum ModelOutput {
    Detections(Vec<DetectionResult>),
    Text(Vec<TextResult>),
    Formula(Vec<FormulaResult>),
    Table(TableResult),
    Layout(Vec<LayoutResult>),
    Raw(Vec<Vec<f32>>),
}

/// Optional quad coordinates for rotated text regions.
/// Four points in order: top-left, top-right, bottom-right, bottom-left.
#[derive(Debug, Clone, Copy)]
pub struct DetectionQuad {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
}

/// Detection result from a detection model.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Optional four-point quad for rotated text regions.
    pub quad: Option<DetectionQuad>,
    pub confidence: f32,
    pub class_id: usize,
    pub class_name: String,
}

/// Text recognition result.
#[derive(Debug, Clone)]
pub struct TextResult {
    pub text: String,
    pub confidence: f32,
}

/// Formula recognition result.
#[derive(Debug, Clone)]
pub struct FormulaResult {
    pub latex: String,
    pub confidence: f32,
}

/// Table structure result.
#[derive(Debug, Clone)]
pub struct TableResult {
    pub rows: Vec<TableRow>,
    pub columns: Vec<TableColumn>,
    pub cells: Vec<TableCell>,
}

/// A row in a table.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub y_start: f32,
    pub y_end: f32,
}

/// A column in a table.
#[derive(Debug, Clone)]
pub struct TableColumn {
    pub x_start: f32,
    pub x_end: f32,
}

/// A cell in a table.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub row: usize,
    pub col: usize,
    pub rowspan: u32,
    pub colspan: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Layout analysis result.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub region_type: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub confidence: f32,
}

/// Context for model execution.
pub struct InferenceContext {
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl InferenceContext {
    pub fn new() -> Self {
        Self {
            metadata: std::collections::HashMap::new(),
        }
    }
}

impl Default for InferenceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A model package — describes what a model does and how to execute it.
///
/// This is the core abstraction for the Model Package architecture.
/// Each model type (YOLOv8, TrOCR, CRNN, etc.) implements this trait.
pub trait ModelPackage: Send + Sync {
    /// Get the model descriptor (task, version, specs).
    fn descriptor(&self) -> &ModelDescriptor;

    /// Create an executor for this model.
    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>>;
}

/// A model executor — runs inference on a loaded model.
pub trait ModelExecutor: Send {
    /// Run inference with the given input.
    fn run(&mut self, input: ModelInput, ctx: &mut InferenceContext) -> Result<ModelOutput>;

    /// Get the model descriptor.
    fn descriptor(&self) -> &ModelDescriptor;
}
