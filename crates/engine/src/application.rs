//! Long-lived, transport-neutral application integration API.
//!
//! This module owns no protocol and emits no process I/O. It is a thin
//! lifecycle wrapper around [`SnipperEngine`].

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use latexsnipper_api_types::{
    EngineReadiness, ModeReadiness, ModelQualityStatus, ProviderValidationReport,
    ProviderValidationRequest, RecognitionAcceptance, RuntimeReadiness,
};
use latexsnipper_ast::{
    Block, Diagnostic, DiagnosticLevel, Document, Formula, ImportOptions, InputFormat,
};
use latexsnipper_conversion::{DocumentConverter, DocumentImporter, OutputFormat};
use latexsnipper_foundation::SnipperError;
use latexsnipper_image::decode::{decode_with_options, ImageSource};
use latexsnipper_image::SnipperImage;
use latexsnipper_pipeline::{
    PipelineCancellationToken, PipelineProgressObserver, PipelineProgressSnapshot,
};
use latexsnipper_runtime::{AccelerationMode, RuntimeRegistry};
use serde::{Deserialize, Serialize};

use crate::{
    default_runtime_registry, DocumentParseMode, EngineConfig, EngineWarmupEntry, RecognizeMode,
    SnipperEngine,
};

pub use latexsnipper_api_types::RecognitionProfile;
pub use latexsnipper_pipeline::PipelineCancellationToken as CancellationToken;

/// Hardware preference for an application session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuntimePreference {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl From<RuntimePreference> for AccelerationMode {
    fn from(value: RuntimePreference) -> Self {
        match value {
            RuntimePreference::Auto => Self::Auto,
            RuntimePreference::Cpu => Self::Cpu,
            RuntimePreference::Gpu => Self::Gpu,
        }
    }
}

/// Owned recognition input.
pub enum RecognitionInput {
    Path(PathBuf),
    Bytes {
        data: Vec<u8>,
        format_hint: Option<InputFormat>,
    },
    Image(SnipperImage),
}

/// Request-scoped application options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionOptions {
    pub parse_mode: Option<DocumentParseMode>,
    pub include_source_asset: bool,
    pub timeout: Option<Duration>,
    pub strict: bool,
}

/// Transport-neutral recognition request.
pub struct RecognitionRequest {
    pub input: RecognitionInput,
    pub profile: RecognitionProfile,
    pub options: RecognitionOptions,
}

impl RecognitionRequest {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            input: RecognitionInput::Path(path.into()),
            profile: RecognitionProfile::Formula,
            options: RecognitionOptions::default(),
        }
    }

    pub fn from_bytes(data: impl Into<Vec<u8>>, format_hint: Option<InputFormat>) -> Self {
        Self {
            input: RecognitionInput::Bytes {
                data: data.into(),
                format_hint,
            },
            profile: RecognitionProfile::Formula,
            options: RecognitionOptions::default(),
        }
    }

    pub fn from_image(image: SnipperImage) -> Self {
        Self {
            input: RecognitionInput::Image(image),
            profile: RecognitionProfile::Formula,
            options: RecognitionOptions::default(),
        }
    }

    pub fn with_profile(mut self, profile: RecognitionProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_options(mut self, options: RecognitionOptions) -> Self {
        self.options = options;
        self
    }
}

/// Runtime facts observed for one recognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetadata {
    pub registered: Vec<String>,
    pub available: Vec<String>,
}

/// Metadata that is not part of the authoritative Document AST.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionMetadata {
    pub profile: RecognitionProfile,
    pub parse_mode: DocumentParseMode,
    pub runtime: RuntimeMetadata,
    pub image_size: Option<(u32, u32)>,
    pub elapsed: Duration,
    /// The runtime does not expose a trustworthy per-request cache-hit bit.
    pub model_cache_hit: Option<bool>,
}

/// Application recognition result. `document` remains the authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionResult {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: RecognitionMetadata,
    #[serde(default)]
    pub formulas: Vec<FormulaRecognitionResult>,
}

/// Office-facing formula payload. It keeps each transformation stage and the
/// Core-owned acceptance decision together so a host never reconstructs policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaRecognitionResult {
    pub raw: String,
    pub normalized: String,
    pub corrected: String,
    pub confidence: f32,
    pub quality_status: ModelQualityStatus,
    pub acceptance: RecognitionAcceptance,
}

impl RecognitionResult {
    pub fn to_latex(&self) -> Result<String, ApplicationError> {
        self.convert(OutputFormat::Latex)
    }

    pub fn to_markdown(&self) -> Result<String, ApplicationError> {
        self.convert(OutputFormat::MarkdownBlock)
    }

    pub fn to_typst(&self) -> Result<String, ApplicationError> {
        self.convert(OutputFormat::Typst)
    }

    pub fn to_omml(&self) -> Result<String, ApplicationError> {
        self.convert(OutputFormat::OMML)
    }

    pub fn to_format(&self, format: OutputFormat) -> Result<String, ApplicationError> {
        self.convert(format)
    }

    fn convert(&self, format: OutputFormat) -> Result<String, ApplicationError> {
        DocumentConverter::new(format)
            .convert(&self.document)
            .map_err(ApplicationError::from)
    }
}

/// Stable progress stages independent of UI and transport frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProgressStage {
    InitializingRuntime,
    ResolvingModels,
    DownloadingModel,
    VerifyingModel,
    LoadingModel,
    DecodingInput,
    DetectingLayout,
    RecognizingText,
    RecognizingFormula,
    RecognizingTable,
    ConvertingOutput,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: ProgressStage,
    pub current: Option<u64>,
    pub total: Option<u64>,
    pub message: Option<String>,
}

pub trait ProgressSink: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

/// Provisional recognition content emitted at safe pipeline boundaries.
///
/// `document` is always a Core AST rather than a derived LaTeX string. A
/// consumer must treat it as read-only until `is_final` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialRecognitionSnapshot {
    pub sequence: u64,
    pub stage: ProgressStage,
    pub current: u64,
    pub total: u64,
    pub detected_regions: usize,
    pub recognized_regions: usize,
    pub document: Option<Document>,
    pub is_final: bool,
}

pub trait PartialResultSink: Send + Sync {
    fn report(&self, snapshot: PartialRecognitionSnapshot);
}

#[derive(Default)]
pub struct RecognitionControl {
    pub progress: Option<Arc<dyn ProgressSink>>,
    pub cancellation: Option<CancellationToken>,
}

/// Additive control surface for callers that opt in to progressive Document
/// snapshots. The original [`RecognitionControl`] remains source-compatible.
#[derive(Default)]
pub struct ProgressiveRecognitionControl {
    pub progress: Option<Arc<dyn ProgressSink>>,
    pub partial_results: Option<Arc<dyn PartialResultSink>>,
    pub cancellation: Option<CancellationToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusReport {
    pub initialized: bool,
    pub runtimes: Vec<RuntimeReadiness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusReport {
    pub profile: RecognitionProfile,
    pub ready: bool,
    pub tasks: Vec<latexsnipper_api_types::TaskReadiness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCapability {
    pub profile: RecognitionProfile,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReport {
    pub profiles: Vec<ProfileCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub ready: bool,
    pub runtime: RuntimeStatusReport,
    pub models: Vec<ModelStatusReport>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelDescriptor {
    pub id: String,
    pub task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequirement {
    pub task: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmupReport {
    pub profile: RecognitionProfile,
    pub ready: bool,
    pub loaded_models: Vec<LoadedModelDescriptor>,
    pub missing_models: Vec<ModelRequirement>,
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed: Duration,
    pub already_warm: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmupRequest {
    pub profile: RecognitionProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReloadReport {
    pub loaded_models: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Stable, transport-neutral contract consumed by Office, desktop, CLI, FFI,
/// and WASM adapters. It exposes owned DTOs, never runtime registries,
/// factories, sessions, manifests, or DLL probing.
pub trait RecognitionIntegrationApi {
    fn readiness(&self) -> EngineReadiness;
    fn warmup(&mut self, request: WarmupRequest) -> Result<WarmupReport, ApplicationError>;
    fn recognize(
        &mut self,
        request: RecognitionRequest,
    ) -> Result<RecognitionResult, ApplicationError>;
    /// Opt-in progressive recognition. Implementations that have not adopted
    /// pipeline checkpoints remain compatible and emit one authoritative final
    /// snapshot instead of inventing intermediate data.
    fn recognize_progressive(
        &mut self,
        request: RecognitionRequest,
        control: ProgressiveRecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        let result = self.recognize(request)?;
        if let Some(sink) = control.partial_results {
            let document = result.document.clone();
            let regions = document.block_count();
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sink.report(PartialRecognitionSnapshot {
                    sequence: 1,
                    stage: ProgressStage::Completed,
                    current: 1,
                    total: 1,
                    detected_regions: 0,
                    recognized_regions: regions,
                    document: Some(document),
                    is_final: true,
                })
            }));
        }
        Ok(result)
    }
    fn validate_provider(
        &self,
        request: ProviderValidationRequest,
    ) -> Result<ProviderValidationReport, ApplicationError>;
    fn reload_models(&mut self) -> Result<ModelReloadReport, ApplicationError>;
}

/// Stable application error code. Wire names do not depend on Rust variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ApplicationErrorCode {
    InvalidInput,
    UnsupportedFormat,
    InputTooLarge,
    ImageDecodeFailed,
    ModelMissing,
    ModelInvalid,
    RuntimeUnavailable,
    ProviderUnavailable,
    WarmupFailed,
    RecognitionFailed,
    ConversionFailed,
    Cancelled,
    Timeout,
    Internal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationError {
    pub code: ApplicationErrorCode,
    pub message: String,
    pub detail: Option<String>,
    pub retryable: bool,
    #[serde(skip)]
    source: Option<SnipperError>,
}

impl ApplicationError {
    fn new(
        code: ApplicationErrorCode,
        message: impl Into<String>,
        detail: Option<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail,
            retryable,
            source: None,
        }
    }

    pub fn source_error(&self) -> Option<&SnipperError> {
        self.source.as_ref()
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

impl From<SnipperError> for ApplicationError {
    fn from(error: SnipperError) -> Self {
        let (code, message, retryable) = match &error {
            SnipperError::Io(_) | SnipperError::InvalidFormat(_) => (
                ApplicationErrorCode::InvalidInput,
                "The input could not be read or validated.",
                false,
            ),
            SnipperError::Config(_) => (
                ApplicationErrorCode::InvalidInput,
                "The recognition configuration is invalid.",
                false,
            ),
            SnipperError::UnsupportedFormat(_) => (
                ApplicationErrorCode::UnsupportedFormat,
                "The input format is not supported for recognition.",
                false,
            ),
            SnipperError::LimitExceeded(_) => (
                ApplicationErrorCode::InputTooLarge,
                "The input exceeds configured safety limits.",
                false,
            ),
            SnipperError::Image(_) => (
                ApplicationErrorCode::ImageDecodeFailed,
                "The image could not be decoded.",
                false,
            ),
            SnipperError::Model(message) if message.contains("manifest") => (
                ApplicationErrorCode::ModelInvalid,
                "A required model is invalid.",
                false,
            ),
            SnipperError::Model(_) => (
                ApplicationErrorCode::ModelMissing,
                "A required model is unavailable.",
                false,
            ),
            SnipperError::Runtime(message) if message.to_ascii_lowercase().contains("provider") => {
                (
                    ApplicationErrorCode::ProviderUnavailable,
                    "The requested execution provider is unavailable.",
                    true,
                )
            }
            SnipperError::Runtime(_) => (
                ApplicationErrorCode::RuntimeUnavailable,
                "The recognition runtime is unavailable.",
                true,
            ),
            SnipperError::Inference(_) | SnipperError::Pipeline(_) => (
                ApplicationErrorCode::RecognitionFailed,
                "Recognition failed.",
                true,
            ),
            SnipperError::Conversion(_) | SnipperError::Export(_) => (
                ApplicationErrorCode::ConversionFailed,
                "Document conversion failed.",
                false,
            ),
            SnipperError::Cancelled => (
                ApplicationErrorCode::Cancelled,
                "Recognition was cancelled.",
                true,
            ),
            SnipperError::Timeout(_) => (
                ApplicationErrorCode::Timeout,
                "Recognition timed out.",
                true,
            ),
            _ => (
                ApplicationErrorCode::Internal,
                "An internal recognition error occurred.",
                false,
            ),
        };
        let detail = (!matches!(error, SnipperError::Cancelled)).then(|| error.to_string());
        Self {
            code,
            message: message.to_string(),
            detail,
            retryable,
            source: Some(error),
        }
    }
}

/// Builder for a long-lived recognition session.
#[derive(Default)]
pub struct RecognitionSessionBuilder {
    config: EngineConfig,
    import_options: ImportOptions,
    runtime_registry: Option<RuntimeRegistry>,
}

impl RecognitionSessionBuilder {
    pub fn models_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.models_dir = path.into();
        self
    }

    /// Use an explicit release-owned directory for trusted quality baselines.
    pub fn quality_baselines_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.quality_baselines_dir = Some(path.into());
        self
    }

    /// Use a versioned tensor fixture for explicit provider smoke validation.
    pub fn provider_smoke_fixture(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.provider_smoke_fixture = Some(path.into());
        self
    }

    pub fn runtime_preference(mut self, preference: RuntimePreference) -> Self {
        self.config.acceleration = preference.into();
        self
    }

    pub fn parse_mode(mut self, parse_mode: DocumentParseMode) -> Self {
        self.config.parse_mode = parse_mode;
        self
    }

    pub fn max_threads(mut self, max_threads: usize) -> Self {
        self.config.max_threads = max_threads.max(1);
        self
    }

    pub fn import_options(mut self, options: ImportOptions) -> Self {
        self.import_options = options;
        self
    }

    /// Inject a preconfigured registry, primarily for embedders and tests.
    pub fn runtime_registry(mut self, registry: RuntimeRegistry) -> Self {
        self.runtime_registry = Some(registry);
        self
    }

    pub fn build(self) -> Result<RecognitionSession, ApplicationError> {
        let registry = match self.runtime_registry {
            Some(registry) => registry,
            None => {
                default_runtime_registry(&self.config.models_dir).map_err(ApplicationError::from)?
            }
        };
        let engine = SnipperEngine::with_runtime_registry(self.config, registry)
            .map_err(ApplicationError::from)?;
        RecognitionSession::from_engine_with_options(engine, self.import_options)
    }
}

/// Long-lived application integration object.
///
/// Methods take `&mut self`, making request serialization explicit. The type is
/// `Send` but deliberately not `Sync`; applications that need shared access
/// should place it behind a bounded, application-owned mutex or actor.
pub struct RecognitionSession {
    engine: SnipperEngine,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
    import_options: ImportOptions,
    warmups: HashMap<RecognitionProfile, WarmupReport>,
    closed: bool,
    _serial: PhantomData<Cell<()>>,
}

impl RecognitionSession {
    pub fn builder() -> RecognitionSessionBuilder {
        RecognitionSessionBuilder::default()
    }

    /// Wrap an already configured engine without rebuilding its registry.
    pub fn from_engine(engine: SnipperEngine) -> Result<Self, ApplicationError> {
        Self::from_engine_with_options(engine, ImportOptions::default())
    }

    /// Wrap an engine and apply caller-defined input safety limits.
    pub fn from_engine_with_import_options(
        engine: SnipperEngine,
        import_options: ImportOptions,
    ) -> Result<Self, ApplicationError> {
        Self::from_engine_with_options(engine, import_options)
    }

    fn from_engine_with_options(
        engine: SnipperEngine,
        import_options: ImportOptions,
    ) -> Result<Self, ApplicationError> {
        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            ApplicationError::new(
                ApplicationErrorCode::RuntimeUnavailable,
                "The session runtime could not be initialized.",
                Some(error.to_string()),
                true,
            )
        })?;
        Ok(Self {
            engine,
            runtime: Some(Arc::new(runtime)),
            import_options,
            warmups: HashMap::new(),
            closed: false,
            _serial: PhantomData,
        })
    }

    pub fn engine(&self) -> &SnipperEngine {
        &self.engine
    }

    pub fn recognize(
        &mut self,
        request: RecognitionRequest,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.ensure_open()?;
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "Synchronous recognition cannot run inside a Tokio runtime; use recognize_async.",
                None,
                false,
            ));
        }
        let runtime = self
            .runtime
            .as_ref()
            .cloned()
            .ok_or_else(closed_session_error)?;
        runtime.block_on(self.recognize_async(request))
    }

    pub fn recognize_with_control(
        &mut self,
        request: RecognitionRequest,
        control: RecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.ensure_open()?;
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "Synchronous recognition cannot run inside a Tokio runtime; use recognize_async_with_control.",
                None,
                false,
            ));
        }
        let runtime = self
            .runtime
            .as_ref()
            .cloned()
            .ok_or_else(closed_session_error)?;
        runtime.block_on(self.recognize_async_with_control(request, control))
    }

    pub async fn recognize_async(
        &mut self,
        request: RecognitionRequest,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.recognize_async_with_control(request, RecognitionControl::default())
            .await
    }

    pub async fn recognize_async_with_control(
        &mut self,
        request: RecognitionRequest,
        control: RecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.recognize_async_internal(request, control.progress, None, control.cancellation)
            .await
    }

    pub fn recognize_progressive(
        &mut self,
        request: RecognitionRequest,
        control: ProgressiveRecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.ensure_open()?;
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "Synchronous progressive recognition cannot run inside a Tokio runtime; use recognize_async_progressive.",
                None,
                false,
            ));
        }
        let runtime = self
            .runtime
            .as_ref()
            .cloned()
            .ok_or_else(closed_session_error)?;
        runtime.block_on(self.recognize_async_progressive(request, control))
    }

    pub async fn recognize_async_progressive(
        &mut self,
        request: RecognitionRequest,
        control: ProgressiveRecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.recognize_async_internal(
            request,
            control.progress,
            control.partial_results,
            control.cancellation,
        )
        .await
    }

    async fn recognize_async_internal(
        &mut self,
        request: RecognitionRequest,
        progress: Option<Arc<dyn ProgressSink>>,
        partial_results: Option<Arc<dyn PartialResultSink>>,
        cancellation: Option<CancellationToken>,
    ) -> Result<RecognitionResult, ApplicationError> {
        self.ensure_open()?;
        let partial_sequence = Arc::new(AtomicU64::new(0));
        let partial_counts = Arc::new(Mutex::new(None));
        let started = Instant::now();
        if request.options.include_source_asset {
            return Err(ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "include_source_asset is not supported by the recognition pipeline yet.",
                Some("The option was rejected instead of being silently ignored.".to_string()),
                false,
            ));
        }
        check_cancelled(cancellation.as_ref())?;
        report(&progress, ProgressStage::DecodingInput, None, None, None);
        let image = self.decode_input(request.input)?;
        let image_size = Some((image.width(), image.height()));
        check_cancelled(cancellation.as_ref())?;

        let parse_mode = request
            .options
            .parse_mode
            .unwrap_or(self.engine.config().parse_mode);
        let readiness = self.engine.readiness();
        let formula_quality = formula_quality_status(request.profile, &readiness);
        let runtime = runtime_metadata(&readiness);
        report(&progress, ProgressStage::ResolvingModels, None, None, None);
        let observer = if progress.is_some() || partial_results.is_some() {
            Some(Arc::new(ApplicationPipelineObserver {
                progress: progress.clone(),
                partial_results: partial_results.clone(),
                partial_sequence: partial_sequence.clone(),
                last_counts: partial_counts.clone(),
            }) as Arc<dyn PipelineProgressObserver>)
        } else {
            None
        };
        let document = self
            .engine
            .recognize_controlled(
                image,
                RecognizeMode::from(request.profile),
                parse_mode,
                cancellation,
                request.options.timeout,
                observer,
            )
            .await
            .map_err(ApplicationError::from)?;

        if request.options.strict
            && document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.level != DiagnosticLevel::Info)
        {
            return Err(ApplicationError::new(
                ApplicationErrorCode::RecognitionFailed,
                "Strict recognition rejected non-informational diagnostics.",
                Some(format!(
                    "{} diagnostic(s) were produced",
                    document.diagnostics.len()
                )),
                false,
            ));
        }
        let final_detected_regions = partial_counts
            .lock()
            .ok()
            .and_then(|counts| counts.as_ref().map(|(detected, _)| *detected))
            .unwrap_or_default();
        report_partial(
            &partial_results,
            &partial_sequence,
            PartialRecognitionSnapshot {
                sequence: 0,
                stage: ProgressStage::Completed,
                current: 1,
                total: 1,
                detected_regions: final_detected_regions,
                recognized_regions: document.block_count(),
                document: Some(document.clone()),
                is_final: true,
            },
        );
        report(&progress, ProgressStage::Completed, Some(1), Some(1), None);
        let formulas = formula_results(&document, formula_quality);
        Ok(RecognitionResult {
            diagnostics: document.diagnostics.clone(),
            metadata: RecognitionMetadata {
                profile: request.profile,
                parse_mode,
                runtime,
                image_size,
                elapsed: started.elapsed(),
                model_cache_hit: None,
            },
            formulas,
            document,
        })
    }

    pub fn health_check(&self) -> Result<HealthReport, ApplicationError> {
        self.ensure_open()?;
        let readiness = self.engine.readiness();
        let runtime = RuntimeStatusReport {
            initialized: !readiness.runtimes.is_empty(),
            runtimes: readiness.runtimes.clone(),
        };
        let models = RecognitionProfile::all()
            .iter()
            .copied()
            .map(|profile| {
                model_status_from_readiness(profile, &readiness, self.warmups.get(&profile))
            })
            .collect::<Vec<_>>();
        let ready = runtime.runtimes.iter().any(|item| item.available)
            && models.iter().any(|item| item.ready);
        Ok(HealthReport {
            ready,
            runtime,
            models,
            diagnostics: readiness.diagnostics,
        })
    }

    pub fn capabilities(&self) -> CapabilityReport {
        let readiness = self.engine.readiness();
        CapabilityReport {
            profiles: RecognitionProfile::all()
                .iter()
                .copied()
                .map(|profile| {
                    let status = model_status_from_readiness(
                        profile,
                        &readiness,
                        self.warmups.get(&profile),
                    );
                    ProfileCapability {
                        profile,
                        ready: status.ready,
                    }
                })
                .collect(),
        }
    }

    pub fn model_status(&self, profile: RecognitionProfile) -> ModelStatusReport {
        model_status_from_readiness(
            profile,
            &self.engine.readiness(),
            self.warmups.get(&profile),
        )
    }

    pub fn runtime_status(&self) -> RuntimeStatusReport {
        let readiness = self.engine.readiness();
        RuntimeStatusReport {
            initialized: !readiness.runtimes.is_empty(),
            runtimes: readiness.runtimes,
        }
    }

    pub fn warmup(
        &mut self,
        profile: RecognitionProfile,
    ) -> Result<WarmupReport, ApplicationError> {
        self.ensure_open()?;
        if let Some(report) = self.warmups.get(&profile) {
            let mut report = report.clone();
            report.already_warm = true;
            report.elapsed = Duration::ZERO;
            return Ok(report);
        }
        let started = Instant::now();
        let entries = self
            .engine
            .warmup_profile(profile.into(), self.engine.config().parse_mode);
        let report = warmup_report(profile, entries, started.elapsed());
        self.warmups.insert(profile, report.clone());
        Ok(report)
    }

    /// Release runtime-owned session caches. Calling this more than once is safe.
    pub fn close(&mut self) {
        if !self.closed {
            self.engine.clear_runtime_sessions();
            self.warmups.clear();
            self.closed = true;
            if let Some(runtime) = self.runtime.take() {
                if let Ok(runtime) = Arc::try_unwrap(runtime) {
                    // Unlike Runtime::drop, this is safe when an async caller
                    // closes or drops the Session from another Tokio runtime.
                    runtime.shutdown_background();
                }
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    fn ensure_open(&self) -> Result<(), ApplicationError> {
        if self.closed {
            Err(closed_session_error())
        } else {
            Ok(())
        }
    }

    fn decode_input(&self, input: RecognitionInput) -> Result<SnipperImage, ApplicationError> {
        match input {
            RecognitionInput::Path(path) => self.decode_path(&path),
            RecognitionInput::Bytes { data, format_hint } => {
                self.decode_bytes(&data, format_hint, None)
            }
            RecognitionInput::Image(image) => {
                validate_image(&image, &self.import_options)?;
                Ok(image)
            }
        }
    }

    fn decode_path(&self, path: &Path) -> Result<SnipperImage, ApplicationError> {
        let metadata = std::fs::metadata(path).map_err(|error| {
            ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "The input file could not be inspected.",
                Some(error.to_string()),
                false,
            )
        })?;
        if metadata.len() > self.import_options.max_input_size {
            return Err(ApplicationError::from(SnipperError::LimitExceeded(
                format!(
                    "input is {} bytes; limit is {} bytes",
                    metadata.len(),
                    self.import_options.max_input_size
                ),
            )));
        }
        let data = std::fs::read(path).map_err(|error| {
            ApplicationError::new(
                ApplicationErrorCode::InvalidInput,
                "The input file could not be read.",
                Some(error.to_string()),
                false,
            )
        })?;
        self.decode_bytes(&data, None, Some(path))
    }

    fn decode_bytes(
        &self,
        data: &[u8],
        format_hint: Option<InputFormat>,
        path_hint: Option<&Path>,
    ) -> Result<SnipperImage, ApplicationError> {
        if u64::try_from(data.len()).unwrap_or(u64::MAX) > self.import_options.max_input_size {
            return Err(ApplicationError::from(SnipperError::LimitExceeded(
                format!(
                    "input is {} bytes; limit is {} bytes",
                    data.len(),
                    self.import_options.max_input_size
                ),
            )));
        }
        let detected =
            DocumentImporter::detect_format(data, path_hint).map_err(ApplicationError::from)?;
        if let Some(hint) = format_hint {
            if hint != InputFormat::Unknown && hint != detected {
                return Err(ApplicationError::from(SnipperError::InvalidFormat(
                    "the supplied format hint does not match the input signature".to_string(),
                )));
            }
        }
        if !is_raster_format(detected) {
            return Err(ApplicationError::from(SnipperError::UnsupportedFormat(
                format!("{detected:?} cannot be recognized without a transport adapter"),
            )));
        }
        decode_with_options(ImageSource::Memory(data), &self.import_options)
            .map_err(ApplicationError::from)
    }
}

fn closed_session_error() -> ApplicationError {
    ApplicationError::new(
        ApplicationErrorCode::InvalidInput,
        "The recognition session is closed.",
        None,
        false,
    )
}

impl Drop for RecognitionSession {
    fn drop(&mut self) {
        self.close();
    }
}

impl RecognitionIntegrationApi for RecognitionSession {
    fn readiness(&self) -> EngineReadiness {
        self.engine.readiness()
    }

    fn warmup(&mut self, request: WarmupRequest) -> Result<WarmupReport, ApplicationError> {
        RecognitionSession::warmup(self, request.profile)
    }

    fn recognize(
        &mut self,
        request: RecognitionRequest,
    ) -> Result<RecognitionResult, ApplicationError> {
        RecognitionSession::recognize(self, request)
    }

    fn recognize_progressive(
        &mut self,
        request: RecognitionRequest,
        control: ProgressiveRecognitionControl,
    ) -> Result<RecognitionResult, ApplicationError> {
        RecognitionSession::recognize_progressive(self, request, control)
    }

    fn validate_provider(
        &self,
        request: ProviderValidationRequest,
    ) -> Result<ProviderValidationReport, ApplicationError> {
        self.engine
            .validate_provider(request)
            .map_err(ApplicationError::from)
    }

    fn reload_models(&mut self) -> Result<ModelReloadReport, ApplicationError> {
        self.ensure_open()?;
        let report = self
            .engine
            .reload_all_models()
            .map_err(ApplicationError::from)?;
        self.warmups.clear();
        Ok(ModelReloadReport {
            loaded_models: report.loaded,
            diagnostics: report
                .issues
                .into_iter()
                .map(|issue| {
                    Diagnostic::new(
                        DiagnosticLevel::Warning,
                        "MODEL_MANIFEST_INVALID",
                        issue.message,
                    )
                    .with_recoverable(true)
                })
                .collect(),
        })
    }
}

struct ApplicationPipelineObserver {
    progress: Option<Arc<dyn ProgressSink>>,
    partial_results: Option<Arc<dyn PartialResultSink>>,
    partial_sequence: Arc<AtomicU64>,
    last_counts: Arc<Mutex<Option<(usize, usize)>>>,
}

impl PipelineProgressObserver for ApplicationPipelineObserver {
    fn node_started(&self, node: &str, current: usize, total: usize) {
        let event = ProgressEvent {
            stage: stage_for_node(node),
            current: Some(current as u64),
            total: Some(total as u64),
            message: Some(public_node_message(node)),
        };
        report_event(&self.progress, event);
    }

    fn node_completed(&self, node: &str, current: usize, total: usize) {
        let event = ProgressEvent {
            stage: stage_for_node(node),
            current: Some(current as u64),
            total: Some(total as u64),
            message: None,
        };
        report_event(&self.progress, event);
    }

    fn wants_checkpoints(&self) -> bool {
        self.partial_results.is_some()
    }

    fn checkpoint(
        &self,
        node: &str,
        current: usize,
        total: usize,
        snapshot: &PipelineProgressSnapshot,
    ) {
        if self.partial_results.is_none()
            || (snapshot.detected_regions == 0
                && snapshot.recognized_regions == 0
                && snapshot.document.is_none())
        {
            return;
        }
        let counts = (snapshot.detected_regions, snapshot.recognized_regions);
        if let Ok(mut last_counts) = self.last_counts.lock() {
            if last_counts.as_ref() == Some(&counts) {
                return;
            }
            *last_counts = Some(counts);
        }
        report_partial(
            &self.partial_results,
            &self.partial_sequence,
            PartialRecognitionSnapshot {
                sequence: 0,
                stage: stage_for_node(node),
                current: current as u64,
                total: total as u64,
                detected_regions: snapshot.detected_regions,
                recognized_regions: snapshot.recognized_regions,
                document: snapshot.document.clone(),
                is_final: false,
            },
        );
    }
}

fn report_event(sink: &Option<Arc<dyn ProgressSink>>, event: ProgressEvent) {
    if let Some(sink) = sink {
        let sink = sink.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.report(event)));
    }
}

fn report_partial(
    sink: &Option<Arc<dyn PartialResultSink>>,
    sequence: &AtomicU64,
    mut snapshot: PartialRecognitionSnapshot,
) {
    if let Some(sink) = sink {
        snapshot.sequence = sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let sink = sink.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.report(snapshot)));
    }
}

fn report(
    sink: &Option<Arc<dyn ProgressSink>>,
    stage: ProgressStage,
    current: Option<u64>,
    total: Option<u64>,
    message: Option<String>,
) {
    if let Some(sink) = sink {
        let sink = sink.clone();
        let event = ProgressEvent {
            stage,
            current,
            total,
            message,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.report(event)));
    }
}

fn check_cancelled(token: Option<&PipelineCancellationToken>) -> Result<(), ApplicationError> {
    if token.is_some_and(PipelineCancellationToken::is_cancelled) {
        Err(ApplicationError::from(SnipperError::Cancelled))
    } else {
        Ok(())
    }
}

fn stage_for_node(node: &str) -> ProgressStage {
    if node.contains("formula") {
        ProgressStage::RecognizingFormula
    } else if node.contains("table") {
        ProgressStage::RecognizingTable
    } else if node.contains("text") || node.contains("handwriting") {
        ProgressStage::RecognizingText
    } else {
        ProgressStage::DetectingLayout
    }
}

fn public_node_message(node: &str) -> String {
    match stage_for_node(node) {
        ProgressStage::RecognizingFormula => "Processing formula content",
        ProgressStage::RecognizingTable => "Processing table content",
        ProgressStage::RecognizingText => "Processing text content",
        _ => "Processing document layout",
    }
    .to_string()
}

fn runtime_metadata(readiness: &EngineReadiness) -> RuntimeMetadata {
    RuntimeMetadata {
        registered: readiness
            .runtimes
            .iter()
            .map(|runtime| runtime.id.clone())
            .collect(),
        available: readiness
            .runtimes
            .iter()
            .filter(|runtime| runtime.available)
            .map(|runtime| runtime.id.clone())
            .collect(),
    }
}

fn formula_quality_status(
    profile: RecognitionProfile,
    readiness: &EngineReadiness,
) -> ModelQualityStatus {
    if !matches!(
        profile,
        RecognitionProfile::Formula
            | RecognitionProfile::CroppedFormula
            | RecognitionProfile::Mixed
            | RecognitionProfile::FormulaLayout
            | RecognitionProfile::Table
    ) {
        return ModelQualityStatus::Unknown;
    }
    let selected = readiness
        .modes
        .iter()
        .find(|mode| mode.mode == RecognizeMode::from(profile).label())
        .and_then(|mode| {
            mode.tasks
                .iter()
                .find(|task| task.task == "formula-recognition")
        })
        .and_then(|task| task.selected_model.as_deref());
    selected
        .and_then(|selected| readiness.models.iter().find(|model| model.id == selected))
        .map_or(ModelQualityStatus::Unknown, |model| model.quality_status)
}

fn formula_results(
    document: &Document,
    quality_status: ModelQualityStatus,
) -> Vec<FormulaRecognitionResult> {
    document
        .pages
        .iter()
        .flat_map(|page| page.blocks.iter())
        .filter_map(|block| match block {
            Block::Formula(block) => Some(&block.formula),
            _ => None,
        })
        .map(|formula| formula_result(formula, quality_status))
        .collect()
}

fn formula_result(
    formula: &Formula,
    quality_status: ModelQualityStatus,
) -> FormulaRecognitionResult {
    let (raw, normalized, corrected, parse_valid, structure_valid, review_required) =
        match formula.recognition_evidence.as_deref() {
            Some(evidence) => (
                evidence.raw.clone(),
                evidence.normalized.clone(),
                evidence.corrected.clone(),
                evidence.corrected_validation.syntax_valid(),
                evidence.corrected_validation.matrix_shape_valid,
                evidence.review_required,
            ),
            None => (
                formula.as_latex().to_owned(),
                formula.as_latex().to_owned(),
                formula.as_latex().to_owned(),
                false,
                false,
                true,
            ),
        };
    let acceptance = RecognitionAcceptance::decide(
        !corrected.trim().is_empty(),
        quality_status,
        formula.confidence,
        parse_valid,
        structure_valid,
        review_required,
    );
    FormulaRecognitionResult {
        raw,
        normalized,
        corrected,
        confidence: formula.confidence,
        quality_status,
        acceptance,
    }
}

fn model_status_from_readiness(
    profile: RecognitionProfile,
    readiness: &EngineReadiness,
    warmup: Option<&WarmupReport>,
) -> ModelStatusReport {
    let mode: RecognizeMode = profile.into();
    let mode = readiness
        .modes
        .iter()
        .find(|item| item.mode == mode.label())
        .cloned()
        .unwrap_or_else(|| ModeReadiness {
            mode: mode.label().to_string(),
            technical_ready: false,
            quality_ready: false,
            production_recommended: false,
            tasks: Vec::new(),
        });
    let Some(warmup) = warmup else {
        let tasks = mode
            .tasks
            .into_iter()
            .map(|mut task| {
                if task.technical_ready {
                    task.technical_ready = false;
                    task.code = Some(latexsnipper_api_types::CoreErrorCode::ProviderUnavailable);
                    task.message = Some(
                        "model artifacts resolve, but no application warmup has created a session"
                            .to_string(),
                    );
                }
                task
            })
            .collect();
        return ModelStatusReport {
            profile,
            ready: false,
            tasks,
        };
    };
    let tasks = mode
        .tasks
        .into_iter()
        .map(|mut task| {
            if warmup
                .loaded_models
                .iter()
                .any(|model| model.task == task.task)
            {
                task.technical_ready = true;
                task.code = None;
                task.message = None;
            } else if let Some(missing) = warmup
                .missing_models
                .iter()
                .find(|model| model.task == task.task)
            {
                task.technical_ready = false;
                task.code = Some(latexsnipper_api_types::CoreErrorCode::ProviderUnavailable);
                task.message = Some(missing.reason.clone());
            }
            task
        })
        .collect();
    ModelStatusReport {
        profile,
        ready: warmup.ready,
        tasks,
    }
}

fn warmup_report(
    profile: RecognitionProfile,
    entries: Vec<EngineWarmupEntry>,
    elapsed: Duration,
) -> WarmupReport {
    let mut loaded_models = Vec::new();
    let mut missing_models = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in entries {
        if entry.loaded {
            loaded_models.push(LoadedModelDescriptor {
                id: entry.model_id.unwrap_or_else(|| "built_in".to_string()),
                task: entry.task.id().to_string(),
            });
        } else {
            let reason = entry
                .message
                .unwrap_or_else(|| "model could not be prepared".to_string());
            missing_models.push(ModelRequirement {
                task: entry.task.id().to_string(),
                reason: reason.clone(),
            });
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    "MODEL_NOT_READY",
                    format!("{}: {reason}", entry.task.id()),
                )
                .with_recoverable(true),
            );
        }
    }
    WarmupReport {
        profile,
        ready: missing_models.is_empty(),
        loaded_models,
        missing_models,
        diagnostics,
        elapsed,
        already_warm: false,
    }
}

fn validate_image(image: &SnipperImage, options: &ImportOptions) -> Result<(), ApplicationError> {
    let pixels = u64::from(image.width())
        .checked_mul(u64::from(image.height()))
        .ok_or_else(|| {
            ApplicationError::from(SnipperError::LimitExceeded(
                "image pixel count overflow".to_string(),
            ))
        })?;
    if image.width() as usize > options.max_image_width
        || image.height() as usize > options.max_image_height
        || pixels > options.max_image_pixels
        || u64::try_from(image.pixels().len()).unwrap_or(u64::MAX) > options.max_decompressed_size
    {
        return Err(ApplicationError::from(SnipperError::LimitExceeded(
            "image dimensions or decoded memory exceed configured limits".to_string(),
        )));
    }
    Ok(())
}

fn is_raster_format(format: InputFormat) -> bool {
    matches!(
        format,
        InputFormat::ImagePng
            | InputFormat::ImageJpeg
            | InputFormat::ImageWebp
            | InputFormat::ImageBmp
            | InputFormat::ImageTiff
            | InputFormat::ImageGif
    )
}
