use crate::artifacts::PipelineArtifacts;
use crate::opendoc_hybrid::DocumentParseMode;
use crate::text_recognition_service::TextRecognitionService;
use latexsnipper_ast::{Document, Page};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::{
    InferenceContext, InferenceSession, ModelExecutionContext, ModelExecutor, ModelInput,
    ModelOutput, ModelPackage, ModelRuntimeEvent, ModelRuntimeObserver, ModelTask, PreparedModel,
    RuntimeBackend, RuntimeRegistry, SharedModelResolver,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Cloneable cancellation signal shared between an application and a running
/// pipeline. Cancellation is observed at safe pipeline boundaries.
#[derive(Debug, Clone, Default)]
pub struct PipelineCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl PipelineCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Framework-neutral observer for pipeline node progress.
pub trait PipelineProgressObserver: Send + Sync {
    fn node_started(&self, node: &str, current: usize, total: usize);
    fn node_completed(&self, node: &str, current: usize, total: usize);

    /// Opt in to potentially expensive provisional Document snapshots.
    fn wants_checkpoints(&self) -> bool {
        false
    }

    /// Receive a provisional, immutable recognition snapshot after a node has
    /// completed. Implementations may ignore it; the default preserves source
    /// compatibility for progress-only observers.
    fn checkpoint(
        &self,
        _node: &str,
        _current: usize,
        _total: usize,
        _snapshot: &PipelineProgressSnapshot,
    ) {
    }
}

/// Request-scoped recognition state exposed at safe pipeline boundaries.
///
/// The document is provisional: later nodes may normalize, reorder, or replace
/// blocks. It never becomes the authoritative result until the engine returns.
#[derive(Debug, Clone)]
pub struct PipelineProgressSnapshot {
    pub detected_regions: usize,
    pub recognized_regions: usize,
    pub document: Option<Document>,
}

/// Diagnostic event level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

/// A diagnostic event recorded during pipeline execution.
#[derive(Debug, Clone)]
pub struct DiagnosticEvent {
    pub level: DiagnosticLevel,
    pub node: String,
    pub message: String,
}

impl From<DiagnosticEvent> for latexsnipper_ast::Diagnostic {
    fn from(event: DiagnosticEvent) -> Self {
        let level = match event.level {
            DiagnosticLevel::Info => latexsnipper_ast::DiagnosticLevel::Info,
            DiagnosticLevel::Warning => latexsnipper_ast::DiagnosticLevel::Warning,
            DiagnosticLevel::Error => latexsnipper_ast::DiagnosticLevel::Error,
        };
        latexsnipper_ast::Diagnostic::new(
            level,
            format!("PIPELINE_{:?}", event.level).to_uppercase(),
            format!("[{}] {}", event.node, event.message),
        )
    }
}

/// Cached ONNX session for reuse across pipeline nodes.
pub struct CachedSession {
    pub session: Arc<Box<dyn InferenceSession>>,
    /// Model version when this session was created.
    pub version: String,
    /// When this session was created, if the target provides a monotonic clock.
    pub created_at: Option<std::time::Instant>,
}

/// Context passed through the pipeline.
/// Each node reads from and writes to this context.
pub struct PipelineContext {
    /// The input image (if any). For multi-page, this is the current page.
    pub image: Option<SnipperImage>,
    /// All page images (for multi-page PDF input).
    pub page_images: Vec<SnipperImage>,
    /// Current page index (0-based) when processing multi-page input.
    pub current_page: usize,
    /// The document being built.
    pub document: Document,
    /// Strongly-typed pipeline data (replaces string-keyed metadata).
    pub artifacts: PipelineArtifacts,
    /// Key-value metadata for passing data between nodes (kept for extensibility).
    pub metadata: HashMap<String, serde_json::Value>,
    /// Whether the pipeline was cancelled.
    pub cancelled: bool,
    /// Shared external cancellation signal.
    cancellation_token: Option<PipelineCancellationToken>,
    /// Optional request deadline.
    deadline: Option<std::time::Instant>,
    /// Original timeout used for a stable timeout error.
    timeout_ms: Option<u64>,
    /// Optional node-boundary observer.
    progress_observer: Option<Arc<dyn PipelineProgressObserver>>,
    /// Observer for actual per-model executor/session/inference events.
    model_runtime_observer: Option<Arc<dyn ModelRuntimeObserver>>,
    /// Explicitly opt-in debug/benchmark crop store. Production defaults to
    /// `None`, so source pixels are never persisted by ordinary recognition.
    crop_artifact_store: Option<Arc<latexsnipper_artifact::DebugCropStore>>,
    /// Models directory path.
    pub models_dir: Option<std::path::PathBuf>,
    /// Runtime backend for inference sessions (injected by engine).
    /// Compatibility view over `runtime_registry`; it never owns an
    /// independent runtime implementation.
    pub backend: Option<Arc<dyn RuntimeBackend>>,
    /// Canonical runtime registry used by manifest-aware model adapters.
    pub runtime_registry: Option<Arc<RuntimeRegistry>>,
    /// Model resolver for loading models (injects backend-specific loading).
    pub model_resolver: Option<SharedModelResolver>,
    /// Model packages for type-safe inference (indexed by ModelTask).
    pub model_packages: HashMap<ModelTask, Arc<dyn ModelPackage>>,
    /// Fully resolved models with their selected runtime variants.
    /// Indexed by ModelTask. Pipeline nodes should prefer this over
    /// `model_packages` or `backend` to ensure the correct runtime is used.
    pub prepared_models: HashMap<ModelTask, PreparedModel>,
    /// User-requested model variant per category (from EngineConfig).
    /// Category → variant name, e.g. "formula-det" → "custom-model".
    /// Nodes should prefer this over auto-discovery when set.
    pub model_variants: HashMap<String, String>,
    /// Acceleration mode requested by EngineConfig (injected by engine).
    pub acceleration: latexsnipper_runtime::AccelerationMode,
    /// Max intra-op threads for ORT session (injected by engine).
    pub max_threads: usize,
    /// Cached ONNX sessions for reuse across nodes.
    pub sessions: HashMap<String, CachedSession>,
    /// Diagnostic events collected during pipeline execution.
    pub diagnostics: Vec<DiagnosticEvent>,
    /// Document parsing mode (SpecializedStable, OpenOcrText, OpenDocHybrid).
    /// Controls which models and heuristics are used during pipeline execution.
    pub parse_mode: DocumentParseMode,
    /// Shared text recognition service — created once, used by both main text
    /// and table cell recognition. Respects variant selection, acceleration, and caching.
    pub text_rec_service: Option<Arc<TextRecognitionService>>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            image: None,
            page_images: Vec::new(),
            current_page: 0,
            document: Document::new(),
            artifacts: PipelineArtifacts::default(),
            metadata: HashMap::new(),
            cancelled: false,
            cancellation_token: None,
            deadline: None,
            timeout_ms: None,
            progress_observer: None,
            model_runtime_observer: None,
            crop_artifact_store: None,
            models_dir: None,
            backend: None,
            runtime_registry: None,
            model_resolver: None,
            model_packages: HashMap::new(),
            prepared_models: HashMap::new(),
            model_variants: HashMap::new(),
            acceleration: latexsnipper_runtime::AccelerationMode::Cpu,
            max_threads: 4,
            sessions: HashMap::new(),
            diagnostics: Vec::new(),
            parse_mode: DocumentParseMode::default(),
            text_rec_service: None,
        }
    }

    /// Build a progressive snapshot only when an observer explicitly asks for
    /// one. This avoids cloning blocks during ordinary recognition.
    pub fn progress_snapshot(&self) -> PipelineProgressSnapshot {
        let recognized_regions = self.artifacts.block_count();
        let document = if !self.document.pages.is_empty() {
            Some(self.document.clone())
        } else if recognized_regions > 0 {
            let mut document = Document::new();
            document.pages.push(Page {
                width: self
                    .image
                    .as_ref()
                    .map_or(0.0, |image| image.width() as f32),
                height: self
                    .image
                    .as_ref()
                    .map_or(0.0, |image| image.height() as f32),
                blocks: self.artifacts.all_blocks(),
                page_number: Some(1),
                layout: None,
                background_asset_id: None,
            });
            Some(document)
        } else {
            None
        };
        PipelineProgressSnapshot {
            detected_regions: self.artifacts.detection_count(),
            recognized_regions,
            document,
        }
    }

    /// Get or initialize the shared text recognition service.
    ///
    /// Priority order:
    /// 1. PreparedModel<TextRecognition> — resolved runtime variant
    /// 2. MemoryModelResolver (WASM)
    /// 3. Filesystem (native)
    pub fn get_or_init_text_rec_service(&mut self) -> Option<Arc<TextRecognitionService>> {
        if let Some(svc) = &self.text_rec_service {
            return Some(svc.clone());
        }

        // 1. PreparedModel path (resolved runtime)
        if let Some(prepared) = self.prepared_models.get(&ModelTask::TextRecognition) {
            let exec_ctx = match self.execution_context(prepared) {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::warn!("Prepared text rec execution context failed: {}", e);
                    return self.try_legacy_text_rec_service();
                }
            };
            match TextRecognitionService::from_context(&exec_ctx) {
                Ok(service) => {
                    self.observe_model_runtime(&prepared.id, ModelRuntimeEvent::ExecutorCreated);
                    let svc = Arc::new(service);
                    self.text_rec_service = Some(svc.clone());
                    return Some(svc);
                }
                Err(e) => {
                    log::warn!("Prepared text rec service failed: {}", e);
                }
            }
        }

        // 2-3. Legacy paths
        self.try_legacy_text_rec_service()
    }

    fn try_legacy_text_rec_service(&mut self) -> Option<Arc<TextRecognitionService>> {
        let variant = self.model_variants.get("text-rec").cloned()?;
        let backend = self.backend.clone()?;
        if let Some(resolver) = &self.model_resolver {
            if let Some(service) = TextRecognitionService::try_load_from_resolver(
                resolver,
                &variant,
                backend,
                self.acceleration,
            ) {
                let service = Arc::new(service);
                self.text_rec_service = Some(service.clone());
                return Some(service);
            }
        }

        let models_dir = self.models_dir.clone()?;
        let service = TextRecognitionService::try_load(
            &models_dir,
            Some(&variant),
            self.backend.clone(),
            self.acceleration,
        )?;
        let svc = Arc::new(service);
        self.text_rec_service = Some(svc.clone());
        Some(svc)
    }

    fn execution_context(
        &self,
        prepared: &PreparedModel,
    ) -> latexsnipper_foundation::Result<ModelExecutionContext> {
        let registry = self.runtime_registry.clone().ok_or_else(|| {
            latexsnipper_foundation::SnipperError::Runtime(
                "Runtime registry is not configured".into(),
            )
        })?;
        Ok(ModelExecutionContext {
            model_id: prepared.id.clone(),
            runtime_registry: registry,
            resolved_runtime: prepared.runtime.clone(),
            max_threads: self.max_threads,
            runtime_observer: self.model_runtime_observer.clone(),
        })
    }

    pub fn with_image(image: SnipperImage) -> Self {
        let mut ctx = Self::new();
        ctx.image = Some(image);
        ctx
    }

    /// Create context with multiple page images (for PDF input).
    pub fn with_pages(pages: Vec<SnipperImage>) -> Self {
        let mut ctx = Self::new();
        if !pages.is_empty() {
            ctx.image = Some(pages[0].clone());
        }
        ctx.page_images = pages;
        ctx
    }

    pub fn with_models_dir(models_dir: std::path::PathBuf) -> Self {
        let mut ctx = Self::new();
        ctx.models_dir = Some(models_dir);
        ctx
    }

    /// Check if this context has multiple pages.
    pub fn is_multipage(&self) -> bool {
        self.page_images.len() > 1
    }

    /// Get the total number of pages.
    pub fn page_count(&self) -> usize {
        if self.page_images.is_empty() {
            if self.image.is_some() {
                1
            } else {
                0
            }
        } else {
            self.page_images.len()
        }
    }

    /// Set the current page index and update the image reference.
    pub fn set_current_page(&mut self, index: usize) {
        if index < self.page_images.len() {
            self.current_page = index;
            self.image = Some(self.page_images[index].clone());
        }
    }

    /// Set a metadata value.
    pub fn set(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get a metadata value.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Get a cached session by key.
    pub fn get_session(&self, key: &str) -> Option<Arc<Box<dyn InferenceSession>>> {
        self.sessions.get(key).map(|c| Arc::clone(&c.session))
    }

    /// Cache a session for reuse.
    pub fn cache_session(&mut self, key: impl Into<String>, session: Box<dyn InferenceSession>) {
        self.sessions.insert(
            key.into(),
            CachedSession {
                session: Arc::new(session),
                version: String::new(),
                created_at: session_created_at(),
            },
        );
    }

    /// Cache a session with version info.
    pub fn cache_session_with_version(
        &mut self,
        key: impl Into<String>,
        session: Box<dyn InferenceSession>,
        version: impl Into<String>,
    ) {
        self.sessions.insert(
            key.into(),
            CachedSession {
                session: Arc::new(session),
                version: version.into(),
                created_at: session_created_at(),
            },
        );
    }

    /// Get session version.
    pub fn get_session_version(&self, key: &str) -> Option<&str> {
        self.sessions.get(key).map(|c| c.version.as_str())
    }

    /// Invalidate a cached session.
    pub fn invalidate_session(&mut self, key: &str) -> bool {
        self.sessions.remove(key).is_some()
    }

    /// Invalidate all cached sessions.
    pub fn invalidate_all_sessions(&mut self) {
        self.sessions.clear();
    }

    /// Get session count.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Cancel the pipeline.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Attach a cloneable external cancellation signal.
    pub fn set_cancellation_token(&mut self, token: PipelineCancellationToken) {
        self.cancellation_token = Some(token);
    }

    /// Attach an observer for actual per-model runtime events.
    pub fn set_model_runtime_observer(&mut self, observer: Arc<dyn ModelRuntimeObserver>) {
        self.model_runtime_observer = Some(observer);
    }

    fn observe_model_runtime(&self, model_id: &str, event: ModelRuntimeEvent) {
        if let Some(observer) = &self.model_runtime_observer {
            observer.observe(model_id, event);
        }
    }

    fn observed_executor(
        &self,
        model_id: String,
        executor: Box<dyn ModelExecutor>,
    ) -> Box<dyn ModelExecutor> {
        if let Some(observer) = &self.model_runtime_observer {
            Box::new(ObservedModelExecutor {
                inner: executor,
                model_id,
                observer: observer.clone(),
            })
        } else {
            executor
        }
    }

    /// Set a request deadline relative to now.
    pub fn set_timeout(&mut self, timeout: std::time::Duration) {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.deadline = std::time::Instant::now().checked_add(timeout);
        self.timeout_ms = Some(timeout_ms);
    }

    /// Attach an observer that receives node-boundary progress.
    pub fn set_progress_observer(&mut self, observer: Arc<dyn PipelineProgressObserver>) {
        self.progress_observer = Some(observer);
    }

    /// Return the currently configured progress observer.
    pub fn progress_observer(&self) -> Option<&Arc<dyn PipelineProgressObserver>> {
        self.progress_observer.as_ref()
    }

    pub fn set_crop_artifact_store(&mut self, store: Arc<latexsnipper_artifact::DebugCropStore>) {
        self.crop_artifact_store = Some(store);
    }

    pub fn crop_artifact_store(&self) -> Option<&Arc<latexsnipper_artifact::DebugCropStore>> {
        self.crop_artifact_store.as_ref()
    }

    /// Check cancellation and timeout at a safe pipeline boundary.
    pub fn check_control(&self) -> latexsnipper_foundation::Result<()> {
        if self.cancelled
            || self
                .cancellation_token
                .as_ref()
                .is_some_and(PipelineCancellationToken::is_cancelled)
        {
            return Err(latexsnipper_foundation::SnipperError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err(latexsnipper_foundation::SnipperError::Timeout(
                self.timeout_ms.unwrap_or(0),
            ));
        }
        Ok(())
    }

    /// Record a diagnostic event.
    pub fn diagnostic(
        &mut self,
        level: DiagnosticLevel,
        node: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(DiagnosticEvent {
            level,
            node: node.into(),
            message: message.into(),
        });
    }

    /// Record an info diagnostic.
    pub fn diagnostic_info(&mut self, node: impl Into<String>, message: impl Into<String>) {
        self.diagnostic(DiagnosticLevel::Info, node, message);
    }

    /// Record a warning diagnostic.
    pub fn diagnostic_warn(&mut self, node: impl Into<String>, message: impl Into<String>) {
        self.diagnostic(DiagnosticLevel::Warning, node, message);
    }

    /// Record an error diagnostic.
    pub fn diagnostic_error(&mut self, node: impl Into<String>, message: impl Into<String>) {
        self.diagnostic(DiagnosticLevel::Error, node, message);
    }

    /// Check if there are any error-level diagnostics.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error)
    }

    /// Register a model package for a specific task.
    pub fn register_model_package(&mut self, task: ModelTask, package: Arc<dyn ModelPackage>) {
        self.model_packages.insert(task, package);
    }

    /// Get a model package for a specific task.
    pub fn get_model_package(&self, task: &ModelTask) -> Option<Arc<dyn ModelPackage>> {
        self.model_packages.get(task).cloned()
    }

    /// Register a fully resolved model for a specific task.
    ///
    /// Pipeline nodes should prefer this — the [`PreparedModel`] carries the
    /// exact runtime variant that was selected, ensuring execution uses the
    /// correct runtime (ONNX, Paddle, TensorRT, etc.).
    pub fn register_prepared_model(&mut self, task: ModelTask, model: PreparedModel) {
        self.prepared_models.insert(task, model);
    }

    /// Get a fully resolved model for a specific task.
    pub fn get_prepared_model(&self, task: &ModelTask) -> Option<&PreparedModel> {
        self.prepared_models.get(task)
    }

    /// Create a model executor for the given task, preferring PreparedModel.
    ///
    /// 1. If a [`PreparedModel`] is registered for this task, creates an
    ///    executor via [`ModelPackage::create_executor_with_context`] using
    ///    the resolved runtime variant (correct runtime guaranteed).
    /// 2. Falls back to legacy [`ModelPackage::create_executor`] with
    ///    `ctx.backend` if only a bare package is registered.
    pub fn create_model_executor(
        &self,
        task: ModelTask,
    ) -> latexsnipper_foundation::Result<Option<Box<dyn latexsnipper_runtime::ModelExecutor>>> {
        // 1. PreparedModel path (preferred): uses resolved runtime variant
        if let Some(prepared) = self.prepared_models.get(&task) {
            let registry = self.runtime_registry.clone().ok_or_else(|| {
                latexsnipper_foundation::SnipperError::Runtime(
                    "Runtime registry is not configured".into(),
                )
            })?;

            let exec_ctx = ModelExecutionContext {
                model_id: prepared.id.clone(),
                runtime_registry: registry,
                resolved_runtime: prepared.runtime.clone(),
                max_threads: self.max_threads,
                runtime_observer: self.model_runtime_observer.clone(),
            };

            let executor = prepared.package.create_executor_with_context(&exec_ctx)?;
            self.observe_model_runtime(&prepared.id, ModelRuntimeEvent::ExecutorCreated);
            return Ok(Some(self.observed_executor(prepared.id.clone(), executor)));
        }

        // 2. Legacy path: bare package + backend
        if let Some(package) = self.model_packages.get(&task) {
            let backend = self.backend.clone().ok_or_else(|| {
                latexsnipper_foundation::SnipperError::Runtime(
                    "Runtime backend is not configured".into(),
                )
            })?;

            let executor = package.create_executor(backend)?;
            let model_id = package.descriptor().id.composite_key();
            self.observe_model_runtime(&model_id, ModelRuntimeEvent::ExecutorCreated);
            return Ok(Some(self.observed_executor(model_id, executor)));
        }

        Ok(None)
    }
}

struct ObservedModelExecutor {
    inner: Box<dyn ModelExecutor>,
    model_id: String,
    observer: Arc<dyn ModelRuntimeObserver>,
}

impl ModelExecutor for ObservedModelExecutor {
    fn run(
        &mut self,
        input: ModelInput,
        ctx: &mut InferenceContext,
    ) -> latexsnipper_foundation::Result<ModelOutput> {
        self.observer
            .observe(&self.model_id, ModelRuntimeEvent::InferenceStarted);
        let result = self.inner.run(input, ctx);
        let event = match &result {
            Ok(_) => ModelRuntimeEvent::InferenceCompleted,
            Err(error) => ModelRuntimeEvent::InferenceFailed {
                code: Some(error.to_string()),
            },
        };
        self.observer.observe(&self.model_id, event);
        result
    }

    fn descriptor(&self) -> &latexsnipper_runtime::ModelDescriptor {
        self.inner.descriptor()
    }
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn session_created_at() -> Option<std::time::Instant> {
    Some(std::time::Instant::now())
}

#[cfg(target_arch = "wasm32")]
fn session_created_at() -> Option<std::time::Instant> {
    None
}
