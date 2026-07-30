use crate::model_resolver::ModelId;
use crate::runtime_registry::RuntimeRegistry;
use crate::{ResolvedRuntimeVariant, RuntimeBackend};
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
    VisionLanguageRecognition,
    DocumentUnderstanding,
    FormulaCorrection,
    TextCorrection,
    TableSemanticParsing,
    DiagramUnderstanding,
    ChartUnderstanding,
    ReadingOrderAnalysis,
    StyleClassification,
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
    pub provenance: Option<latexsnipper_ast::RecognitionProvenance>,
    pub evidence: Option<latexsnipper_ast::PostProcessResult>,
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

    /// Create an executor for this model using a legacy runtime backend.
    ///
    /// Prefer [`create_executor_with_context`] for new code — it ensures the
    /// executor uses the same runtime that was resolved during model selection.
    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>>;

    /// Create an executor using the resolved runtime context.
    ///
    /// Adapters that need the resolved variant (runtime, artifacts, options)
    /// should override this. The default implementation panics with a
    /// message directing the adapter author to implement this method.
    fn create_executor_with_context(
        &self,
        ctx: &ModelExecutionContext,
    ) -> Result<Box<dyn ModelExecutor>> {
        // Fall back to legacy path for adapters that haven't migrated yet.
        // This uses ctx.backend_compat() which wraps the registry as a
        // RuntimeBackend — it won't use the resolved variant, but keeps
        // existing adapters working until they migrate.
        let backend = ctx.backend_compat();
        self.create_executor(backend)
    }
}

/// A fully resolved model ready for execution.
///
/// Created by the engine after model selection and runtime resolution.
/// Pipeline nodes use this to create executors with the exact runtime
/// that was selected.
#[derive(Clone)]
pub struct PreparedModel {
    /// Model identifier (category/variant).
    pub id: String,
    /// The model package (adapter-created).
    pub package: Arc<dyn ModelPackage>,
    /// The resolved runtime variant (specific runtime + options + artifacts).
    pub runtime: ResolvedRuntimeVariant,
}

impl PreparedModel {
    pub fn new(
        id: String,
        package: Arc<dyn ModelPackage>,
        runtime: ResolvedRuntimeVariant,
    ) -> Self {
        Self {
            id,
            package,
            runtime,
        }
    }
}

/// Context passed to [`ModelPackage::create_executor_with_context`].
///
/// Contains everything needed to create runtime sessions using the
/// exact resolved runtime variant, with role-aware artifact selection
/// (e.g. "encoder" vs "decoder" for TrOCR models).
pub struct ModelExecutionContext {
    /// Stable model identifier used by runtime observations.
    pub model_id: String,
    /// Canonical runtime registry for creating sessions.
    pub runtime_registry: Arc<RuntimeRegistry>,
    /// The resolved runtime variant to use.
    pub resolved_runtime: ResolvedRuntimeVariant,
    /// Maximum intra-op threads.
    pub max_threads: usize,
    /// Optional observer for actual executor/session/inference events.
    pub runtime_observer: Option<Arc<dyn crate::ModelRuntimeObserver>>,
}

impl ModelExecutionContext {
    /// Create a runtime session for a specific artifact role.
    ///
    /// For single-model packages (YOLOv8, DBNet, CRNN), use `"model"` or
    /// leave `artifact_role` empty. For encoder-decoder models (TrOCR),
    /// call once with `"encoder"` and once with `"decoder"`.
    ///
    /// The `artifact_role` is passed to the runtime factory via
    /// `RuntimeOptions.extra["artifact"]`, which ONNX-based factories
    /// use to select the correct file from `RuntimeArtifacts`.
    pub fn create_session(&self, artifact_role: &str) -> Result<Box<dyn crate::RuntimeSession>> {
        let mut resolved = self.resolved_runtime.clone();

        // Inject artifact role so factories can select the right file
        if !artifact_role.is_empty() {
            resolved.options.extra.insert(
                "artifact".into(),
                serde_json::Value::String(artifact_role.into()),
            );
        }

        // Apply max_threads if not already set
        if resolved.options.max_threads == 0 && self.max_threads > 0 {
            resolved.options.max_threads = self.max_threads;
        }

        let session = self.runtime_registry.create_resolved_session(&resolved)?;
        if let Some(observer) = &self.runtime_observer {
            observer.observe(&self.model_id, crate::ModelRuntimeEvent::SessionCreated);
            Ok(Box::new(ObservedRuntimeSession {
                inner: session,
                model_id: self.model_id.clone(),
                observer: observer.clone(),
            }))
        } else {
            Ok(session)
        }
    }

    /// Create a legacy [`RuntimeBackend`] from the registry for adapters
    /// that haven't migrated to [`create_executor_with_context`] yet.
    ///
    /// Prefer [`create_session`] for new code — it uses the resolved
    /// variant's runtime rather than the default.
    pub fn backend_compat(&self) -> Arc<dyn RuntimeBackend> {
        let kind = self.resolved_runtime.runtime.clone();
        Arc::new(crate::RegistryRuntimeBackend::new(
            self.runtime_registry.clone(),
            kind,
        ))
    }

    /// Get the filesystem path of a named artifact from the resolved
    /// runtime variant (e.g. `"tokenizer"`, `"config"`, `"keys"`).
    ///
    /// The paths in `ResolvedRuntimeVariant.artifacts.files` are already
    /// resolved to absolute filesystem paths by `RuntimeResolver`.
    pub fn artifact_path(&self, role: &str) -> Option<&std::path::Path> {
        self.resolved_runtime
            .artifacts
            .files
            .get(role)
            .map(|p| p.as_path())
    }

    /// Read a text artifact (tokenizer, config, keys) from the resolved
    /// runtime variant's artifacts.
    pub fn read_artifact(&self, name: &str) -> Result<String> {
        let path = self.artifact_path(name).ok_or_else(|| {
            latexsnipper_foundation::SnipperError::Model(format!(
                "Artifact '{}' not found in resolved runtime variant '{}'",
                name, self.resolved_runtime.variant_id
            ))
        })?;
        std::fs::read_to_string(path).map_err(|e| {
            latexsnipper_foundation::SnipperError::Model(format!(
                "Failed to read artifact '{}' at '{}': {}",
                name,
                path.display(),
                e
            ))
        })
    }
}

struct ObservedRuntimeSession {
    inner: Box<dyn crate::RuntimeSession>,
    model_id: String,
    observer: Arc<dyn crate::ModelRuntimeObserver>,
}

impl crate::RuntimeSession for ObservedRuntimeSession {
    fn metadata(&self) -> &crate::SessionMetadata {
        self.inner.metadata()
    }

    fn run(&self, request: crate::RunRequest) -> Result<crate::RunResponse> {
        self.observer
            .observe(&self.model_id, crate::ModelRuntimeEvent::InferenceStarted);
        let result = self.inner.run(request);
        self.observer.observe(
            &self.model_id,
            if result.is_ok() {
                crate::ModelRuntimeEvent::InferenceCompleted
            } else {
                crate::ModelRuntimeEvent::InferenceFailed
            },
        );
        result
    }
}

/// A model executor — runs inference on a loaded model.
pub trait ModelExecutor: Send {
    /// Run inference with the given input.
    fn run(&mut self, input: ModelInput, ctx: &mut InferenceContext) -> Result<ModelOutput>;

    /// Get the model descriptor.
    fn descriptor(&self) -> &ModelDescriptor;
}
