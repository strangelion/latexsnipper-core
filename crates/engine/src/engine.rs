use log::{info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
#[cfg(feature = "native")]
use latexsnipper_image::pdf::{decode_pdf, PdfSource};
use latexsnipper_image::SnipperImage;
#[cfg(feature = "native")]
use latexsnipper_model::ModelManager;
use latexsnipper_pipeline::{
    DocumentParseMode, PipelineContext, PipelineGraph, PipelinePlanner, PipelineProfile,
};
#[cfg(feature = "native")]
use latexsnipper_runtime::FsModelResolver;
use latexsnipper_runtime::{
    AccelerationMode, ModelPackage, ModelRegistry, ModelScanReport, ModelSelectionDecision,
    ModelSelectionPolicy, ModelSelectionRequest, ModelTask, RegistryRuntimeBackend,
    ResolvedRuntimeVariant, RuntimeBackend, RuntimeFactory, RuntimeKind, RuntimeRegistry,
    RuntimeSession, SharedModelResolver,
};

use crate::config::EngineConfig;
use crate::job::JobQueue;

pub use latexsnipper_api_types::{RecognizeMode, RecognizeRequest, RecognizeResponse, StreamItem};

/// The main engine that orchestrates all LaTeXSnipper capabilities.
/// Engine only assembles PipelineGraph and runs it — all logic lives in Nodes.
pub struct SnipperEngine {
    config: EngineConfig,
    /// Canonical and only runtime ownership graph.
    runtime_registry: Arc<RuntimeRegistry>,
    /// Runtime selected for legacy ModelHandle calls.
    default_runtime: RuntimeKind,
    model_resolver: Option<SharedModelResolver>,
    #[cfg(feature = "native")]
    model_manager: ModelManager,
    job_queue: JobQueue,
    /// Registered model packages keyed by model ID (category/variant).
    model_packages: RwLock<HashMap<String, Arc<dyn ModelPackage>>>,
    /// Model selection policy for choosing the best model per task.
    model_selection: ModelSelectionPolicy,
    /// Model registry for discovering available models.
    model_registry: ModelRegistry,
}

fn legacy_runtime_registry(
    runtime: Box<dyn RuntimeBackend>,
) -> (Arc<RuntimeRegistry>, RuntimeKind) {
    let runtime: Arc<dyn RuntimeBackend> = Arc::from(runtime);
    let default_runtime = RuntimeKind::from_id(runtime.name());
    let registry = RuntimeRegistry::with_factory(
        latexsnipper_runtime::providers::legacy_adapter::LegacyRuntimeAdapter::new(runtime),
    );
    (Arc::new(registry), default_runtime)
}

// ============================================================================
// Model registry initialization helpers
// ============================================================================

/// Initialize a ModelRegistry with built-in adapters and scan the models directory.
fn initialize_model_registry(
    models_dir: &Path,
) -> Result<(ModelRegistry, ModelScanReport)> {
    let mut registry = ModelRegistry::new();

    // Register built-in adapters from the inference crate.
    latexsnipper_inference::register_builtin_adapters(&mut registry);

    // Scan the models directory for category/variant layout.
    let report = registry.register_models_root(models_dir)?;

    Ok((registry, report))
}

// ============================================================================
// Pipeline task resolution
// ============================================================================

/// Return the list of ModelTask that a RecognizeMode requires.
fn required_tasks(mode: RecognizeMode, parse_mode: DocumentParseMode) -> Vec<ModelTask> {
    match mode {
        RecognizeMode::Formula => vec![
            ModelTask::FormulaDetection,
            ModelTask::FormulaRecognition,
        ],

        RecognizeMode::Text => vec![
            ModelTask::TextDetection,
            ModelTask::TextRecognition,
        ],

        RecognizeMode::Table => vec![
            ModelTask::TableDetection,
            ModelTask::TableStructure,
        ],

        RecognizeMode::Handwriting => vec![ModelTask::HandwritingRecognition],

        RecognizeMode::FormulaLayout => vec![
            ModelTask::FormulaDetection,
            ModelTask::FormulaRecognition,
        ],

        RecognizeMode::Mixed => {
            if parse_mode == DocumentParseMode::OpenDocHybrid {
                vec![
                    ModelTask::LayoutAnalysis,
                    ModelTask::FormulaDetection,
                    ModelTask::FormulaRecognition,
                    ModelTask::TextDetection,
                    ModelTask::TextRecognition,
                    ModelTask::TableDetection,
                    ModelTask::TableStructure,
                ]
            } else {
                vec![
                    ModelTask::FormulaDetection,
                    ModelTask::FormulaRecognition,
                    ModelTask::TextDetection,
                    ModelTask::TextRecognition,
                ]
            }
        }

        _ => vec![],
    }
}

impl SnipperEngine {
    /// Create a new engine with the given config and runtime backend.
    ///
    /// This constructor attempts to auto-scan the models directory and register
    /// built-in adapters. Failures are logged but do not prevent construction.
    pub fn new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Self {
        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());
        #[cfg(feature = "native")]
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        #[cfg(not(feature = "native"))]
        let model_resolver = None;

        let (runtime_registry, default_runtime) = legacy_runtime_registry(runtime);

        // Auto-scan models directory
        let mut model_registry = ModelRegistry::new();
        latexsnipper_inference::register_builtin_adapters(&mut model_registry);
        if let Err(error) = model_registry.register_models_root(&config.models_dir) {
            warn!("Failed to initialize model registry: {}", error);
        }

        Self {
            config,
            runtime_registry,
            default_runtime,
            model_resolver,
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: RwLock::new(HashMap::new()),
            model_selection: ModelSelectionPolicy::default(),
            model_registry,
        }
    }

    /// Create with a custom model resolver.
    pub fn with_model_resolver(
        config: EngineConfig,
        runtime: Box<dyn RuntimeBackend>,
        resolver: SharedModelResolver,
    ) -> Self {
        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());

        let (runtime_registry, default_runtime) = legacy_runtime_registry(runtime);

        // Auto-scan models directory
        let mut model_registry = ModelRegistry::new();
        latexsnipper_inference::register_builtin_adapters(&mut model_registry);
        if let Err(error) = model_registry.register_models_root(&config.models_dir) {
            warn!("Failed to initialize model registry: {}", error);
        }

        Self {
            config,
            runtime_registry,
            default_runtime,
            model_resolver: Some(resolver),
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: RwLock::new(HashMap::new()),
            model_selection: ModelSelectionPolicy::default(),
            model_registry,
        }
    }

    /// Construct an engine directly from the canonical runtime registry.
    ///
    /// This is the preferred constructor for new code. It automatically
    /// registers built-in adapters and scans the models directory.
    pub fn with_runtime_registry(
        config: EngineConfig,
        registry: RuntimeRegistry,
    ) -> Result<Self> {
        let default_runtime = if registry.is_available(&RuntimeKind::OnnxRuntime) {
            RuntimeKind::OnnxRuntime
        } else {
            registry
                .available_runtimes()
                .into_iter()
                .next()
                .ok_or_else(|| {
                    SnipperError::Runtime(
                        "cannot create engine: runtime registry has no available runtime"
                            .to_owned(),
                    )
                })?
        };

        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());
        #[cfg(feature = "native")]
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        #[cfg(not(feature = "native"))]
        let model_resolver = None;

        // Auto-scan models and register adapters
        let (model_registry, scan_report) =
            initialize_model_registry(&config.models_dir)?;

        for issue in &scan_report.issues {
            warn!(
                "Model scan issue at '{}': {}",
                issue.path.display(),
                issue.message
            );
        }

        info!(
            "Model registry initialized: {} models loaded, {} issues",
            scan_report.loaded_count(),
            scan_report.issues.len()
        );

        Ok(Self {
            config,
            runtime_registry: Arc::new(registry),
            default_runtime,
            model_resolver,
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: RwLock::new(HashMap::new()),
            model_selection: ModelSelectionPolicy::default(),
            model_registry,
        })
    }

    /// Strict constructor that fails if model scanning fails.
    ///
    /// Unlike [`new`] which logs warnings and continues, this returns an
    /// error if the models directory cannot be scanned.
    pub fn try_new(
        config: EngineConfig,
        runtime: Box<dyn RuntimeBackend>,
    ) -> Result<Self> {
        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());
        #[cfg(feature = "native")]
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        #[cfg(not(feature = "native"))]
        let model_resolver = None;

        let (runtime_registry, default_runtime) = legacy_runtime_registry(runtime);

        let (model_registry, scan_report) =
            initialize_model_registry(&config.models_dir)?;

        for issue in &scan_report.issues {
            warn!(
                "Model scan issue at '{}': {}",
                issue.path.display(),
                issue.message
            );
        }

        Ok(Self {
            config,
            runtime_registry,
            default_runtime,
            model_resolver,
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: RwLock::new(HashMap::new()),
            model_selection: ModelSelectionPolicy::default(),
            model_registry,
        })
    }

    /// Compatibility view. The returned backend delegates every operation to
    /// the canonical RuntimeRegistry and does not form a second execution path.
    pub fn runtime(&self) -> Arc<dyn RuntimeBackend> {
        Arc::new(RegistryRuntimeBackend::new(
            self.runtime_registry.clone(),
            self.default_runtime.clone(),
        ))
    }

    /// Access the runtime registry (for registering additional runtimes).
    pub fn runtime_registry(&self) -> &RuntimeRegistry {
        &self.runtime_registry
    }

    /// Mutably access the runtime registry.
    pub fn runtime_registry_mut(&mut self) -> &mut RuntimeRegistry {
        Arc::make_mut(&mut self.runtime_registry)
    }

    /// Register a runtime factory.
    pub fn register_runtime(&mut self, factory: impl RuntimeFactory + 'static) -> Result<()> {
        Arc::make_mut(&mut self.runtime_registry).register(factory)
    }

    /// Resolve a model manifest through the same resolver used by execution.
    pub fn resolve_model_runtime(
        &self,
        manifest: &latexsnipper_runtime::ModelManifest,
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<ResolvedRuntimeVariant> {
        manifest.resolve_runtime_variant(&self.runtime_registry, model_dir, preferred_variant)
    }

    /// Resolve and create a canonical named-tensor session in one operation.
    pub fn create_model_runtime_session(
        &self,
        manifest: &latexsnipper_runtime::ModelManifest,
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<(ResolvedRuntimeVariant, Box<dyn RuntimeSession>)> {
        let mut resolved = self.resolve_model_runtime(manifest, model_dir, preferred_variant)?;
        if resolved.options.max_threads == 0 {
            resolved.options.max_threads = self.config.max_threads;
        }
        if resolved.options.providers.is_empty()
            && resolved.options.device == latexsnipper_runtime::DeviceKind::Auto
        {
            let compatibility_options =
                latexsnipper_runtime::RuntimeOptions::from(self.config.acceleration);
            resolved.options.device = compatibility_options.device;
            if resolved.runtime == RuntimeKind::OnnxRuntime {
                resolved.options.providers = compatibility_options.providers;
            }
        }
        let session = self.runtime_registry.create_resolved_session(&resolved)?;
        Ok((resolved, session))
    }

    #[cfg(feature = "native")]
    pub fn model_manager(&self) -> &ModelManager {
        &self.model_manager
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn job_queue(&self) -> &JobQueue {
        &self.job_queue
    }

    pub fn job_queue_mut(&mut self) -> &mut JobQueue {
        &mut self.job_queue
    }

    // ========================================================================
    // Model Package Management (by model ID)
    // ========================================================================

    /// Register a model package for a specific task (legacy API).
    ///
    /// Prefer [`get_or_create_model_package`] for new code.
    pub fn register_model_package(&mut self, _task: ModelTask, package: Arc<dyn ModelPackage>) {
        let id = package.descriptor().id.composite_key();
        if let Ok(mut cache) = self.model_packages.write() {
            cache.insert(id, package);
        }
    }

    /// Get a registered model package by model ID.
    pub fn get_model_package_by_id(&self, model_id: &str) -> Option<Arc<dyn ModelPackage>> {
        self.model_packages
            .read()
            .ok()
            .and_then(|cache| cache.get(model_id).cloned())
    }

    /// Get or create a ModelPackage for the given model ID, caching it.
    ///
    /// This is the primary entry point for obtaining a `ModelPackage` for
    /// pipeline execution. It checks the cache first, then constructs the
    /// package via the registered adapter.
    pub fn get_or_create_model_package(
        &self,
        model_id: &str,
    ) -> Result<Arc<dyn ModelPackage>> {
        // Check cache first
        {
            let cache = self.model_packages.read().map_err(|_| {
                SnipperError::Model("Model package cache poisoned".into())
            })?;

            if let Some(package) = cache.get(model_id) {
                return Ok(package.clone());
            }
        }

        // Look up manifest and directory from registry
        let manifest = self
            .model_registry
            .get(model_id)
            .ok_or_else(|| {
                SnipperError::Model(format!(
                    "Model '{}' is not registered",
                    model_id
                ))
            })?;

        let model_dir = self
            .model_registry
            .get_dir(model_id)
            .ok_or_else(|| {
                SnipperError::Model(format!(
                    "Model '{}' has no model directory",
                    model_id
                ))
            })?;

        // Create package via adapter
        let package = self
            .model_registry
            .create_package(manifest, model_dir)?
            .ok_or_else(|| {
                SnipperError::Model(format!(
                    "Unsupported model adapter '{}' for model '{}'",
                    manifest.adapter, model_id
                ))
            })?;

        let package: Arc<dyn ModelPackage> = Arc::from(package);

        // Cache it
        let mut cache = self.model_packages.write().map_err(|_| {
            SnipperError::Model("Model package cache poisoned".into())
        })?;

        cache.insert(model_id.to_string(), package.clone());

        Ok(package)
    }

    // ========================================================================
    // Model Selection API
    // ========================================================================

    /// Get a mutable reference to the model selection policy.
    pub fn model_selection_mut(&mut self) -> &mut ModelSelectionPolicy {
        &mut self.model_selection
    }

    /// Get a reference to the model registry.
    pub fn model_registry(&self) -> &ModelRegistry {
        &self.model_registry
    }

    /// Get a mutable reference to the model registry.
    pub fn model_registry_mut(&mut self) -> &mut ModelRegistry {
        &mut self.model_registry
    }

    /// Unified model selection: returns the best model ID for a task.
    ///
    /// Priority order:
    /// 1. User explicit override from EngineConfig
    /// 2. ModelSelectionPolicy via registry
    pub fn select_model_id(&self, task: ModelTask) -> Result<Option<String>> {
        // 1. Check explicit user override
        if let Some(explicit) = self.config.model_override(task) {
            if self.model_registry.has(explicit) {
                return Ok(Some(explicit.to_string()));
            }

            return Err(SnipperError::Model(format!(
                "Configured model '{}' for {:?} is not installed",
                explicit, task
            )));
        }

        // 2. Use selection policy
        let decision = self.select_model(task, None, Some(self.config.acceleration), None);
        Ok(decision.selected)
    }

    /// Select the best model for a given task using the ModelSelectionPolicy.
    ///
    /// This is the bridge between the declarative selection policy and the
    /// engine's runtime model packages. It queries the registry for candidates,
    /// applies the selection policy, and returns the decision with explanations.
    pub fn select_model(
        &self,
        task: ModelTask,
        backend: Option<latexsnipper_runtime::ModelBackend>,
        acceleration: Option<AccelerationMode>,
        language: Option<String>,
    ) -> ModelSelectionDecision {
        let request = ModelSelectionRequest {
            task,
            backend,
            acceleration,
            language,
            preference: self.config.model_selection_preference(),
        };
        self.model_selection
            .select_registry(&self.model_registry, &request)
    }

    /// Select and register the best model for a task into the pipeline context.
    ///
    /// Combines model selection with model package registration, so pipeline
    /// nodes can immediately use the selected model via `ctx.get_model_package()`.
    pub fn select_and_register_model(
        &self,
        ctx: &mut PipelineContext,
        task: ModelTask,
    ) -> Result<Option<String>> {
        let decision = self.select_model(task, None, Some(self.config.acceleration), None);

        let Some(model_id) = &decision.selected else {
            return Ok(None);
        };

        // Create and cache the package
        let package = self.get_or_create_model_package(model_id)?;

        // Register with the pipeline context so nodes can use it
        ctx.register_model_package(task, package);

        // Set model variant hints for nodes that use model_variants
        if let Some((category, variant)) = model_id.split_once('/') {
            ctx.model_variants
                .insert(category.to_string(), variant.to_string());
        }

        Ok(Some(model_id.clone()))
    }

    /// Prepare models for all required tasks in a recognition mode.
    ///
    /// This selects and registers models for every task needed by the given
    /// mode, populating the pipeline context appropriately.
    fn prepare_pipeline_models(
        &self,
        ctx: &mut PipelineContext,
        mode: RecognizeMode,
    ) -> Result<()> {
        for task in required_tasks(mode, self.config.parse_mode) {
            match self.select_and_register_model(ctx, task) {
                Ok(Some(ref model_id)) => {
                    info!(
                        "Selected model '{}' for {:?}",
                        model_id, task
                    );
                }
                Ok(None) => {
                    warn!("No model selected for {:?}", task);
                }
                Err(error) => {
                    warn!(
                        "Failed to prepare model for {:?}: {}",
                        task, error
                    );
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Model Hot-Reload API
    // ========================================================================

    /// Clear all cached runtime sessions without rescanning models.
    ///
    /// Next inference call will create fresh sessions.
    pub fn clear_runtime_sessions(&self) {
        self.runtime_registry.clear_sessions();
    }

    /// Rescan the models directory for new or removed models.
    ///
    /// This clears the model cache and re-registers everything from the
    /// configured models directory. Built-in adapters are preserved.
    ///
    /// Returns a scan report describing what was loaded and any issues.
    pub fn rescan_models(&mut self) -> Result<ModelScanReport> {
        self.model_registry.clear_models();

        let report = self
            .model_registry
            .register_models_root(&self.config.models_dir)?;

        // Clear the package cache so stale packages are not reused
        self.model_packages
            .write()
            .map_err(|_| SnipperError::Model("Model package cache poisoned".into()))?
            .clear();

        Ok(report)
    }

    /// Full hot reload: clear sessions and rescan models.
    ///
    /// Use this when models have been added, removed, or updated on disk.
    /// After calling this, the next recognition will use the updated models.
    pub fn reload_all_models(&mut self) -> Result<ModelScanReport> {
        self.runtime_registry.clear_sessions();
        self.rescan_models()
    }

    /// Reload a specific model by clearing all cached sessions.
    /// Next inference call will create fresh sessions with the new model files.
    #[deprecated(note = "Use clear_runtime_sessions() or reload_all_models() instead")]
    pub fn reload_model(&self, session_key: &str) -> Result<()> {
        info!("Reloading model: {}", session_key);
        self.runtime_registry.clear_sessions();
        Ok(())
    }

    /// Get the model resolver.
    pub fn model_resolver(&self) -> Option<&SharedModelResolver> {
        self.model_resolver.as_ref()
    }

    /// Set a new model resolver (for hot-swapping model sources).
    pub fn set_model_resolver(&mut self, resolver: SharedModelResolver) {
        self.model_resolver = Some(resolver);
    }

    /// Check if a model is available via the resolver.
    pub fn has_model(&self, category: &str, variant: &str) -> bool {
        if let Some(resolver) = &self.model_resolver {
            let id = latexsnipper_runtime::ModelId::new(category, variant);
            resolver.is_available(&id)
        } else {
            false
        }
    }

    // ========================================================================
    // Pipeline Construction
    // ========================================================================

    /// Build a PipelineGraph for the given recognition mode.
    pub fn build_pipeline(&self, mode: RecognizeMode) -> PipelineGraph {
        let profile = match mode {
            RecognizeMode::Formula => PipelineProfile::Formula,
            RecognizeMode::Text => PipelineProfile::Text,
            RecognizeMode::Mixed => PipelineProfile::Mixed,
            RecognizeMode::Handwriting => PipelineProfile::Handwriting,
            RecognizeMode::Table => PipelineProfile::Table,
            RecognizeMode::FormulaLayout => PipelineProfile::FormulaLayout,
            _ => return PipelineGraph::new(format!("{:?}_pipeline", mode)),
        };
        PipelinePlanner
            .plan(profile, self.config.parse_mode)
            .build_graph()
    }

    /// Legacy graph assembly retained temporarily as a behavior oracle while
    /// profiles are migrated to the declarative planner.
    #[allow(dead_code)]
    fn build_pipeline_legacy(&self, mode: RecognizeMode) -> PipelineGraph {
        let mut graph = PipelineGraph::new(format!("{:?}_pipeline", mode));

        match mode {
            RecognizeMode::Formula => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::CropNode::default()),
                    vec!["detect_formula".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::RecognizerNode::formula()),
                    vec!["crop".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                    vec!["recognize_formula".into()],
                );
            }
            RecognizeMode::Text => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::text()));
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::CropNode::default()),
                    vec!["detect_text".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::RecognizerNode::text()),
                    vec!["crop".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                    vec!["recognize_text".into()],
                );
            }
            RecognizeMode::Mixed => {
                if self.config.parse_mode == DocumentParseMode::OpenDocHybrid {
                    graph.add_node(Box::new(latexsnipper_pipeline::LayoutNode::new()));
                    graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                    graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::text()));
                    graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::table()));
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::CropNode::default()),
                        vec!["detect_formula".into(), "detect_text".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::RegionResolveNode::new()),
                        vec![
                            "layout_analysis".into(),
                            "crop".into(),
                            "detect_formula".into(),
                            "detect_text".into(),
                            "detect_table".into(),
                        ],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::RecognizerNode::formula()),
                        vec!["region_resolve".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::RecognizerNode::text()),
                        vec!["region_resolve".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::TableStructureNode::new()),
                        vec!["region_resolve".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::TableRecognizerNode::new()),
                        vec!["table_structure".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                        vec![
                            "recognize_formula".into(),
                            "recognize_text".into(),
                            "recognize_table".into(),
                        ],
                    );
                } else {
                    graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                    graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::text()));
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::CropNode::default()),
                        vec!["detect_formula".into(), "detect_text".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::RecognizerNode::formula()),
                        vec!["crop".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::RecognizerNode::text()),
                        vec!["crop".into()],
                    );
                    graph.add_node_with_deps(
                        Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                        vec!["recognize_formula".into(), "recognize_text".into()],
                    );
                }
            }
            RecognizeMode::Handwriting => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::handwriting()));
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::CropNode::default()),
                    vec!["detect_handwriting".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::HandwritingRecognizerNode::new()),
                    vec!["crop".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                    vec!["recognize_handwriting".into()],
                );
            }
            RecognizeMode::Table => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::table()));
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::TableStructureNode::new()),
                    vec!["detect_table".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::TableRecognizerNode::new()),
                    vec!["table_structure".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                    vec!["recognize_table".into()],
                );
            }
            RecognizeMode::FormulaLayout => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::CropNode::default()),
                    vec!["detect_formula".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::RecognizerNode::formula()),
                    vec!["crop".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::FormulaLayoutNode::new()),
                    vec!["recognize_formula".into()],
                );
                graph.add_node_with_deps(
                    Box::new(latexsnipper_pipeline::PostprocessNode::new()),
                    vec!["formula_layout".into()],
                );
            }
            _ => {}
        }

        graph
    }

    // ========================================================================
    // Recognition
    // ========================================================================

    /// Recognize with a Request object (Builder pattern).
    pub async fn recognize_with_request(
        &self,
        request: RecognizeRequest,
    ) -> Result<RecognizeResponse> {
        let start = std::time::Instant::now();
        let mode = request.mode;
        let doc = self.recognize(request.image, mode).await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let region_count = doc.block_count();
        Ok(RecognizeResponse::new(doc, mode, region_count, elapsed))
    }

    /// Recognize with streaming results.
    pub async fn recognize_streaming(&self, request: RecognizeRequest) -> Result<Vec<StreamItem>> {
        let start = std::time::Instant::now();
        let mut items = Vec::new();

        match self.recognize(request.image, request.mode).await {
            Ok(doc) => {
                let mut idx = 0;
                for page in &doc.pages {
                    for block in &page.blocks {
                        let text = extract_block_text(block);
                        let confidence = match block {
                            Block::Formula(f) => f.formula.confidence,
                            Block::Handwriting(hw) => hw.confidence,
                            _ => 1.0,
                        };

                        if !text.is_empty() {
                            items.push(StreamItem::RegionRecognized {
                                index: idx,
                                text,
                                confidence,
                            });
                        }
                        idx += 1;
                    }
                }
                let elapsed = start.elapsed().as_millis() as u64;
                items.push(StreamItem::Completed {
                    document: doc,
                    total_regions: idx,
                    elapsed_ms: elapsed,
                });
            }
            Err(e) => {
                items.push(StreamItem::Error {
                    message: e.to_string(),
                });
            }
        }

        Ok(items)
    }

    /// Recognize content in an image — Pipeline First.
    ///
    /// Engine assembles the graph, auto-selects models, and runs the pipeline.
    /// All logic lives in Nodes.
    pub async fn recognize(&self, image: SnipperImage, mode: RecognizeMode) -> Result<Document> {
        info!(
            "Recognizing image ({}, {}) in {:?} mode",
            image.width(),
            image.height(),
            mode
        );

        let graph = self.build_pipeline(mode);
        let mut ctx = self.configure_context(PipelineContext::with_image(image));

        // Auto-select and register models for this mode
        self.prepare_pipeline_models(&mut ctx, mode)?;

        graph.run(&mut ctx).await?;

        let blocks = Self::collect_blocks_from_context(&ctx);
        let diagnostics = ctx.diagnostics.into_iter().map(Into::into).collect();

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: ctx.image.as_ref().map_or(0.0, |i| i.width() as f32),
                height: ctx.image.as_ref().map_or(0.0, |i| i.height() as f32),
                blocks,
                page_number: Some(1),
                layout: None,
                background_asset_id: None,
            }],
            assets: Vec::new(),
            diagnostics,
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        })
    }

    /// Recognize content in a PDF file — Multi-page support.
    ///
    /// Each page is processed independently through the pipeline.
    /// Uses `pdftoppm` (poppler) or `mutool` (MuPDF) for rendering.
    #[cfg(feature = "native")]
    pub async fn recognize_pdf(&self, pdf_path: &Path, mode: RecognizeMode) -> Result<Document> {
        info!("Recognizing PDF {:?} in {:?} mode", pdf_path, mode);

        let pages = decode_pdf(PdfSource::File(pdf_path), 300)
            .map_err(|e| SnipperError::Image(e.to_string()))?;

        info!("PDF loaded: {} pages", pages.len());

        let graph = self.build_pipeline(mode);
        let mut doc_pages = Vec::new();
        let mut diagnostics = Vec::new();

        for (page_idx, page_img) in pages.iter().enumerate() {
            if page_idx > 0 {
                info!("Processing page {}/{}", page_idx + 1, pages.len());
            }

            let mut ctx = self.configure_context(PipelineContext::with_image(page_img.clone()));

            // Auto-select and register models for this mode
            self.prepare_pipeline_models(&mut ctx, mode)?;

            graph.run(&mut ctx).await?;

            let blocks = Self::collect_blocks_from_context(&ctx);
            diagnostics.extend(ctx.diagnostics.into_iter().map(Into::into));

            doc_pages.push(Page {
                width: page_img.width() as f32,
                height: page_img.height() as f32,
                blocks,
                page_number: Some((page_idx + 1) as u32),
                layout: None,
                background_asset_id: None,
            });
        }

        info!(
            "PDF recognition complete: {} pages, {} total blocks",
            doc_pages.len(),
            doc_pages.iter().map(|p| p.blocks.len()).sum::<usize>()
        );

        Ok(Document {
            metadata: Metadata::default(),
            pages: doc_pages,
            assets: Vec::new(),
            diagnostics,
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        })
    }

    /// Collect blocks from artifacts in the pipeline context.
    fn collect_blocks_from_context(ctx: &PipelineContext) -> Vec<Block> {
        ctx.artifacts.all_blocks()
    }

    /// Configure a pipeline context with engine config (shared by image and PDF paths).
    fn configure_context(&self, mut ctx: PipelineContext) -> PipelineContext {
        ctx.models_dir = Some(self.config.models_dir.clone());
        ctx.runtime_registry = Some(self.runtime_registry.clone());
        ctx.backend = Some(self.runtime());
        ctx.model_resolver = self.model_resolver.clone();
        ctx.acceleration = self.config.acceleration;
        ctx.max_threads = self.config.max_threads;
        ctx.parse_mode = self.config.parse_mode;

        // Apply explicit model variant overrides from config
        if let Some(v) = &self.config.formula_det_model {
            ctx.model_variants
                .insert("formula-det".into(), v.clone());
        }
        if let Some(v) = &self.config.formula_rec_model {
            ctx.model_variants
                .insert("formula-rec".into(), v.clone());
        }
        if let Some(v) = &self.config.text_det_model {
            ctx.model_variants.insert("text-det".into(), v.clone());
        }
        if let Some(v) = &self.config.text_rec_model {
            ctx.model_variants.insert("text-rec".into(), v.clone());
        }
        if let Some(v) = &self.config.table_det_model {
            ctx.model_variants.insert("table-det".into(), v.clone());
        }
        if let Some(v) = &self.config.table_struct_model {
            ctx.model_variants
                .insert("table-struct".into(), v.clone());
        }
        if let Some(v) = &self.config.handwriting_det_model {
            ctx.model_variants
                .insert("handwriting-det".into(), v.clone());
        }

        // Mode-specific defaults
        match self.config.parse_mode {
            DocumentParseMode::OpenOcrText => {
                self.prefer_variant_if_installed(&mut ctx, "text-det", "openocr-mobile");
                self.prefer_variant_if_installed(&mut ctx, "text-rec", "openocr-mobile");
                self.prefer_variant_if_installed(&mut ctx, "table-struct", "slanet-plus");
            }
            DocumentParseMode::OpenDocHybrid => {
                self.prefer_variant_if_installed(&mut ctx, "table-struct", "slanet-plus");
                // Auto-register layout package from manifest if available
                self.try_register_layout_package(&mut ctx);
            }
            DocumentParseMode::SpecializedStable => {}
        }

        ctx
    }

    fn prefer_variant_if_installed(
        &self,
        ctx: &mut PipelineContext,
        category: &str,
        variant: &str,
    ) {
        if ctx.model_variants.contains_key(category) {
            return;
        }

        let variant_dir = self.config.models_dir.join(category).join(variant);
        if variant_dir.is_dir() {
            ctx.model_variants
                .insert(category.to_string(), variant.to_string());
        }
    }

    /// Try to auto-register layout analysis package from the model manifest.
    fn try_register_layout_package(&self, ctx: &mut PipelineContext) {
        let manifest_path = self.config.models_dir.join("model-manifest.json");

        let manifest = match std::fs::read_to_string(&manifest_path) {
            Ok(content) => {
                match serde_json::from_str::<latexsnipper_model::manifest::ModelManifest>(&content)
                {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to parse manifest for layout registration: {}", e);
                        return;
                    }
                }
            }
            Err(e) => {
                warn!("Cannot read manifest: {}", e);
                return;
            }
        };

        // Look for a layout category
        let layout_info = manifest.categories.iter().find(|(cat, _)| *cat == "layout");
        let (_cat_name, layout_cat) = match layout_info {
            Some(v) => v,
            None => {
                info!("No layout category in manifest, skipping layout registration");
                return;
            }
        };

        // Find the default variant
        let default_id = layout_cat.default.as_deref().unwrap_or("pp-layout-cdla");

        let variant = layout_cat.variants.iter().find(|v| v.id == default_id);

        if let Some(variant) = variant {
            let variant_dir = self.config.models_dir.join("layout").join(&variant.id);

            if !variant_dir.is_dir() {
                info!(
                    "Layout model directory not found: {}, skipping layout registration",
                    variant_dir.display()
                );
                return;
            }

            let model_path =
                variant_dir.join(variant.files.first().unwrap_or(&"model.onnx".into()));
            if !model_path.exists() {
                info!(
                    "Layout model file not found: {}, skipping",
                    model_path.display()
                );
                return;
            }

            info!(
                "Auto-registering layout package: {}/{}",
                _cat_name, variant.id
            );
            ctx.model_variants
                .insert("layout".into(), variant.id.clone());
        }
    }
}

// ============================================================================
// Block text extraction helper (shared by recognize_streaming)
// ============================================================================

fn extract_block_text(block: &Block) -> String {
    match block {
        Block::Formula(f) => f.formula.as_latex().to_string(),
        Block::Paragraph(p) => p
            .inlines
            .iter()
            .filter_map(|i| {
                if let Inline::Text(t) = i {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<String>(),
        Block::Heading(h) => h
            .inlines
            .iter()
            .filter_map(|i| {
                if let Inline::Text(t) = i {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<String>(),
        Block::Table(t) => {
            let mut buf = String::new();
            for row in &t.rows {
                for cell in &row.cells {
                    let inlines = cell.collect_inlines();
                    for inline in &inlines {
                        if let Inline::Text(txt) = inline {
                            buf.push_str(&txt.text);
                            buf.push(' ');
                        }
                    }
                    buf.push('\t');
                }
                buf.push('\n');
            }
            buf
        }
        Block::Handwriting(hw) => hw
            .inlines
            .iter()
            .filter_map(|i| {
                if let Inline::Text(t) = i {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<String>(),
        Block::Code(c) => c.code.clone(),
        Block::Figure(f) => f.caption.clone().unwrap_or_default(),
        Block::List(l) => l
            .items
            .iter()
            .filter_map(|item| {
                let t: String = item
                    .content
                    .iter()
                    .flat_map(|b| b.inlines())
                    .filter_map(|i| {
                        if let Inline::Text(txt) = i {
                            Some(txt.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
        Block::Quote(q) => q
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => Some(
                    p.inlines
                        .iter()
                        .filter_map(|i| {
                            if let Inline::Text(t) = i {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        Block::HorizontalRule(_) => "---".to_string(),
        Block::DescriptionList(dl) => {
            let mut buf = String::new();
            for item in &dl.items {
                if let Some(label) = &item.label {
                    for inline in label {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                    buf.push_str(": ");
                }
                for block in &item.content {
                    if let Block::Paragraph(p) = block {
                        for inline in &p.inlines {
                            if let Inline::Text(t) = inline {
                                buf.push_str(&t.text);
                            }
                        }
                    }
                }
                buf.push('\n');
            }
            buf
        }
        Block::TableOfContents => "目录".to_string(),
        Block::Theorem(t) => {
            let mut buf = format!("{}: ", t.name);
            for block in &t.content {
                if let Block::Paragraph(p) = block {
                    for inline in &p.inlines {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                }
            }
            buf
        }
        Block::Proof(p) => {
            let mut buf = "Proof: ".to_string();
            for block in &p.content {
                if let Block::Paragraph(p) = block {
                    for inline in &p.inlines {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                }
            }
            buf
        }
        Block::Minipage(m) => {
            let mut buf = String::new();
            for block in &m.content {
                if let Block::Paragraph(p) = block {
                    for inline in &p.inlines {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                }
            }
            buf
        }
        Block::Float(f) => {
            let mut buf = String::new();
            for block in &f.content {
                if let Block::Paragraph(p) = block {
                    for inline in &p.inlines {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                }
            }
            buf
        }
        Block::TextBox(tb) => {
            let mut buf = String::new();
            for block in &tb.content {
                if let Block::Paragraph(p) = block {
                    for inline in &p.inlines {
                        if let Inline::Text(t) = inline {
                            buf.push_str(&t.text);
                        }
                    }
                }
            }
            buf
        }
        Block::Chart(_)
        | Block::Shape(_)
        | Block::EmbeddedObject(_)
        | Block::Annotation(_)
        | Block::PageBreak(_)
        | Block::SectionBreak(_)
        | Block::HeaderFooter(_)
        | Block::Bibliography(_)
        | Block::FormField(_)
        | Block::Revision(_)
        | Block::ChemicalFormula(_)
        | Block::QrCode(_)
        | Block::Graph(_) => String::new(),
    }
}
