use log::{info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use latexsnipper_api_types::{
    CoreErrorCode, EngineReadiness, ModeReadiness, ModelQualityReadiness, ModelQualityStatus,
    ModelReadiness as ApiModelReadiness, ProviderValidationKey, ProviderValidationLevel,
    ProviderValidationPolicy, ProviderValidationReport, ProviderValidationRequest,
    RuntimeReadiness, TaskReadiness, ValidationScope, READINESS_SCHEMA_VERSION,
};
use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
#[cfg(feature = "native")]
use latexsnipper_image::pdf::{decode_pdf, PdfSource};
use latexsnipper_image::SnipperImage;
#[cfg(feature = "native")]
use latexsnipper_model::ModelManager;
use latexsnipper_pipeline::{
    DocumentParseMode, PipelineCancellationToken, PipelineContext, PipelineGraph, PipelinePlanner,
    PipelineProfile, PipelineProgressObserver,
};
#[cfg(feature = "native")]
use latexsnipper_runtime::FsModelResolver;
use latexsnipper_runtime::{
    AccelerationMode, ExecutionProviderSpec, ModelPackage, ModelRegistry, ModelRuntimeEvent,
    ModelRuntimeObservation, ModelRuntimeObserver, ModelScanIssue, ModelScanReport,
    ModelSelectionDecision, ModelSelectionPolicy, ModelSelectionRequest, ModelTask, PreparedModel,
    ProviderEnvironmentFingerprint, ProviderSmokeFixture, RegistryRuntimeBackend,
    ResolvedRuntimeVariant, RuntimeArtifacts, RuntimeBackend, RuntimeFactory, RuntimeKind,
    RuntimeOptions, RuntimeProbe, RuntimeRegistry, RuntimeSession, SharedModelResolver,
};

use crate::config::EngineConfig;
use crate::job::JobQueue;
use crate::provider_validation::ProviderValidationStore;
use crate::quality::{ModelQualityRegistry, ModelQualityValidation};

pub use latexsnipper_api_types::{RecognizeMode, RecognizeRequest, RecognizeResponse, StreamItem};

/// Outcome of preparing one model task during application warmup.
#[derive(Debug, Clone)]
pub struct EngineWarmupEntry {
    pub task: ModelTask,
    pub model_id: Option<String>,
    pub loaded: bool,
    pub message: Option<String>,
}

struct EngineModelRuntimeObserver {
    states: Arc<RwLock<HashMap<String, ModelRuntimeObservation>>>,
}

impl ModelRuntimeObserver for EngineModelRuntimeObserver {
    fn observe(&self, model_id: &str, event: ModelRuntimeEvent) {
        if let Ok(mut states) = self.states.write() {
            states.entry(model_id.to_owned()).or_default().record(event);
        }
    }
}

// ============================================================================
// Canonical category key constants
// ============================================================================

/// All `ctx.model_variants` keys should use these canonical category names.
/// The old short keys (`formula-det`, `text-rec`, etc.) are deprecated.
pub mod category {
    pub const FORMULA_DETECTION: &str = "formula-detection";
    pub const FORMULA_RECOGNITION: &str = "formula-recognition";
    pub const TEXT_DETECTION: &str = "text-detection";
    pub const TEXT_RECOGNITION: &str = "text-recognition";
    pub const TABLE_DETECTION: &str = "table-detection";
    pub const TABLE_STRUCTURE: &str = "table-structure";
    pub const LAYOUT_ANALYSIS: &str = "layout-analysis";
    pub const HANDWRITING_RECOGNITION: &str = "handwriting-recognition";
}

/// Map ModelTask to its canonical category key.
fn task_category_key(task: ModelTask) -> &'static str {
    task.id()
}

/// Map a canonical category name to its legacy short form.
///
/// WASM builds use short category names (`text-rec`, `formula-det`) while
/// the canonical names use the full form (`text-recognition`, `formula-detection`).
fn legacy_category(canonical: &str) -> &str {
    match canonical {
        "text-recognition" => "text-rec",
        "text-detection" => "text-det",
        "formula-recognition" => "formula-rec",
        "formula-detection" => "formula-det",
        "table-recognition" => "table-rec",
        "table-detection" => "table-det",
        "table-structure" => "table-struct",
        "layout-analysis" => "layout",
        "handwriting-recognition" => "handwriting-det",
        // If it doesn't match any canonical name, return as-is
        other => other,
    }
}

/// Check whether a model ID represents a built-in rule-based strategy
/// (not a real model). These skip Runtime validation.
fn is_builtin_model_strategy(model_id: &str) -> bool {
    matches!(
        model_id,
        "table-structure/projection" | "table-struct/projection"
    )
}

fn unavailable_provider_code(reason: &str) -> CoreErrorCode {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("library")
        || reason.contains(".dll")
        || reason.contains(".so")
        || reason.contains(".dylib")
    {
        CoreErrorCode::ProviderLibraryMissing
    } else {
        CoreErrorCode::ProviderUnavailable
    }
}

fn readiness_error_code(error: &SnipperError) -> CoreErrorCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("checksum") || message.contains("hash") {
        CoreErrorCode::ModelArtifactHashMismatch
    } else if message.contains("artifact") && message.contains("does not exist") {
        CoreErrorCode::ModelArtifactMissing
    } else if message.contains("manifest") || message.contains("runtime variant") {
        CoreErrorCode::ModelManifestInvalid
    } else {
        match error {
            SnipperError::Model(_) => CoreErrorCode::ModelNotFound,
            SnipperError::Runtime(_) => unavailable_provider_code(&message),
            SnipperError::Inference(_) => CoreErrorCode::SessionCreateFailed,
            _ => CoreErrorCode::OutputValidationFailed,
        }
    }
}

fn current_provider_key(
    provider: &str,
    probe: &RuntimeProbe,
    smoke_model_sha256: Option<&str>,
) -> ProviderValidationKey {
    let fingerprint = ProviderEnvironmentFingerprint::collect(
        env!("CARGO_PKG_VERSION"),
        provider,
        probe,
        smoke_model_sha256,
    );
    ProviderValidationKey {
        core_version: fingerprint.core_version,
        runtime_version: fingerprint.runtime_version,
        provider: fingerprint.provider,
        provider_library_fingerprint: fingerprint.provider_library_fingerprint,
        os: fingerprint.os,
        architecture: fingerprint.architecture,
        device_driver_fingerprint: fingerprint.device_driver_fingerprint,
        smoke_model_sha256: fingerprint.smoke_model_sha256,
        runtime_binary_sha256: fingerprint.runtime_binary_sha256,
        provider_library_sha256: fingerprint.provider_library_sha256,
        device_identity: fingerprint.device_identity,
    }
}

fn declared_model_sha256(manifest: &latexsnipper_runtime::ModelManifest) -> String {
    if manifest.checksums.len() == 1 {
        return manifest
            .checksums
            .values()
            .next()
            .cloned()
            .unwrap_or_else(|| "0".repeat(64));
    }
    if manifest.checksums.is_empty() {
        return "0".repeat(64);
    }
    let mut checksums = manifest.checksums.iter().collect::<Vec<_>>();
    checksums.sort_by_key(|(name, _)| *name);
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    for (name, checksum) in checksums {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(checksum.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

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
    /// Scan issues retained for the public readiness/diagnostics snapshot.
    model_scan_issues: Vec<ModelScanIssue>,
    /// Runtime facts observed by explicit warmup and real recognition calls.
    model_technical_state: Arc<RwLock<HashMap<String, ModelRuntimeObservation>>>,
    /// Release-owned quality evidence. Model packages cannot mutate it.
    model_quality_registry: ModelQualityRegistry,
    /// Environment-bound provider validation results.
    provider_validation_store: ProviderValidationStore,
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
fn initialize_model_registry(models_dir: &Path) -> Result<(ModelRegistry, ModelScanReport)> {
    let mut registry = ModelRegistry::new();

    // Register built-in adapters from the inference crate.
    latexsnipper_inference::register_builtin_adapters(&mut registry);

    // Scan the models directory for category/variant layout.
    let report = registry.register_models_root(models_dir)?;

    Ok((registry, report))
}

fn quality_baselines_root(config: &EngineConfig) -> std::path::PathBuf {
    config.quality_baselines_dir.clone().unwrap_or_else(|| {
        config
            .models_dir
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(
                || std::path::PathBuf::from("quality").join("baselines"),
                |parent| parent.join("quality").join("baselines"),
            )
    })
}

fn load_model_quality_registry(config: &EngineConfig) -> ModelQualityRegistry {
    let root = quality_baselines_root(config);
    match ModelQualityRegistry::load(&root) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "Failed to load trusted model quality baselines from '{}': {}",
                root.display(),
                error
            );
            let message = error.to_string();
            let lower = message.to_ascii_lowercase();
            let code = if lower.contains("hash mismatch") {
                CoreErrorCode::QualityBaselineHashMismatch
            } else {
                CoreErrorCode::QualityBaselineIndexInvalid
            };
            ModelQualityRegistry::unavailable(code, message)
        }
    }
}

// ============================================================================
// Pipeline task resolution
// ============================================================================

/// Return the list of ModelTask that a RecognizeMode requires.
fn required_tasks(mode: RecognizeMode, parse_mode: DocumentParseMode) -> Vec<ModelTask> {
    match mode {
        RecognizeMode::Formula => vec![ModelTask::FormulaDetection, ModelTask::FormulaRecognition],
        RecognizeMode::CroppedFormula => vec![ModelTask::FormulaRecognition],

        RecognizeMode::Text => vec![ModelTask::TextDetection, ModelTask::TextRecognition],

        RecognizeMode::Table => vec![
            ModelTask::TableDetection,
            ModelTask::TableStructure,
            // Cell content recognition
            ModelTask::TextRecognition,
            ModelTask::FormulaDetection,
            ModelTask::FormulaRecognition,
        ],

        RecognizeMode::Handwriting => vec![ModelTask::HandwritingRecognition],

        RecognizeMode::FormulaLayout => {
            vec![ModelTask::FormulaDetection, ModelTask::FormulaRecognition]
        }

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
        let model_scan_issues = match model_registry.register_models_root(&config.models_dir) {
            Ok(report) => report.issues,
            Err(error) => {
                warn!("Failed to initialize model registry: {}", error);
                vec![ModelScanIssue {
                    path: config.models_dir.clone(),
                    message: error.to_string(),
                }]
            }
        };

        let model_quality_registry = load_model_quality_registry(&config);
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
            model_scan_issues,
            model_technical_state: Arc::new(RwLock::new(HashMap::new())),
            model_quality_registry,
            provider_validation_store: ProviderValidationStore::default(),
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
        let model_scan_issues = match model_registry.register_models_root(&config.models_dir) {
            Ok(report) => report.issues,
            Err(error) => {
                warn!("Failed to initialize model registry: {}", error);
                vec![ModelScanIssue {
                    path: config.models_dir.clone(),
                    message: error.to_string(),
                }]
            }
        };

        let model_quality_registry = load_model_quality_registry(&config);
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
            model_scan_issues,
            model_technical_state: Arc::new(RwLock::new(HashMap::new())),
            model_quality_registry,
            provider_validation_store: ProviderValidationStore::default(),
        }
    }

    /// Construct an engine directly from the canonical runtime registry.
    ///
    /// This is the preferred constructor for new code. It automatically
    /// registers built-in adapters and scans the models directory.
    pub fn with_runtime_registry(config: EngineConfig, registry: RuntimeRegistry) -> Result<Self> {
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
        let (model_registry, scan_report) = initialize_model_registry(&config.models_dir)?;

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

        let model_quality_registry = load_model_quality_registry(&config);
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
            model_scan_issues: scan_report.issues,
            model_technical_state: Arc::new(RwLock::new(HashMap::new())),
            model_quality_registry,
            provider_validation_store: ProviderValidationStore::default(),
        })
    }

    /// Strict constructor that fails if model scanning fails.
    ///
    /// Unlike [`new`] which logs warnings and continues, this returns an
    /// error if the models directory cannot be scanned.
    pub fn try_new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Result<Self> {
        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());
        #[cfg(feature = "native")]
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        #[cfg(not(feature = "native"))]
        let model_resolver = None;

        let (runtime_registry, default_runtime) = legacy_runtime_registry(runtime);

        let (model_registry, scan_report) = initialize_model_registry(&config.models_dir)?;

        for issue in &scan_report.issues {
            warn!(
                "Model scan issue at '{}': {}",
                issue.path.display(),
                issue.message
            );
        }

        let model_quality_registry = load_model_quality_registry(&config);
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
            model_scan_issues: scan_report.issues,
            model_technical_state: Arc::new(RwLock::new(HashMap::new())),
            model_quality_registry,
            provider_validation_store: ProviderValidationStore::default(),
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

    /// Resolve and fully prepare a runtime variant for execution.
    ///
    /// This is the single source of truth for runtime preparation.
    /// It resolves the variant and fills in `max_threads`, acceleration
    /// providers, and other engine-config-driven defaults so that every
    /// code path (validation, registration, session creation) uses the
    /// same options.
    fn prepare_model_runtime(
        &self,
        manifest: &latexsnipper_runtime::ModelManifest,
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<ResolvedRuntimeVariant> {
        let mut resolved = self.resolve_model_runtime(manifest, model_dir, preferred_variant)?;

        // Apply engine-level defaults
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

        Ok(resolved)
    }

    /// Resolve and create a canonical named-tensor session in one operation.
    pub fn create_model_runtime_session(
        &self,
        manifest: &latexsnipper_runtime::ModelManifest,
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<(ResolvedRuntimeVariant, Box<dyn RuntimeSession>)> {
        let resolved = self.prepare_model_runtime(manifest, model_dir, preferred_variant)?;
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

    /// Return a stable, serializable snapshot for desktop, Office, SDK, and
    /// FFI callers. This probes and resolves capabilities but never creates a
    /// session or exposes internal runtime/model objects.
    pub fn readiness(&self) -> EngineReadiness {
        let mut diagnostics = self
            .model_scan_issues
            .iter()
            .map(|issue| {
                Diagnostic::new(
                    DiagnosticLevel::Error,
                    CoreErrorCode::ModelManifestInvalid.as_str(),
                    format!("{}: {}", issue.path.display(), issue.message),
                )
                .with_recoverable(true)
            })
            .collect::<Vec<_>>();
        let configured_smoke_fixture = self
            .config
            .provider_smoke_fixture
            .as_deref()
            .map(ProviderSmokeFixture::load)
            .transpose();
        if let Err(error) = &configured_smoke_fixture {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    CoreErrorCode::ProviderValidationRequired.as_str(),
                    error.to_string(),
                )
                .with_recoverable(true),
            );
        }
        let smoke_model_sha256 = configured_smoke_fixture
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .map(ProviderSmokeFixture::model_sha256);

        let runtimes = self
            .runtime_registry
            .probe_all()
            .into_iter()
            .map(|(kind, probe)| {
                let code = (!probe.available).then(|| {
                    unavailable_provider_code(probe.reason_unavailable.as_deref().unwrap_or(""))
                });
                if let Some(code) = code {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            code.as_str(),
                            format!(
                                "runtime '{}' is unavailable: {}",
                                kind,
                                probe
                                    .reason_unavailable
                                    .as_deref()
                                    .unwrap_or("no reason reported")
                            ),
                        )
                        .with_recoverable(true),
                    );
                }
                let providers = probe.capabilities.execution_providers.clone();
                let provider_validations = providers
                    .iter()
                    .map(|provider| {
                        let key = current_provider_key(provider, &probe, smoke_model_sha256);
                        self.provider_validation_store
                            .lookup(&key)
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| ProviderValidationReport {
                                provider: provider.clone(),
                                validation_level: ProviderValidationLevel::ProbePassed,
                                library_detected: true,
                                probe_passed: true,
                                session_created: false,
                                smoke_inference_passed: false,
                                benchmark_measured: false,
                                benchmark_validated: false,
                                scope: ValidationScope::CurrentProcess,
                                reusable_across_restart: false,
                                validated_at: 0,
                                duration_ms: 0,
                                runtime_instance_id: self
                                    .provider_validation_store
                                    .runtime_instance_id()
                                    .to_owned(),
                                session_generation: 0,
                                last_failure_code: None,
                                key: Some(key),
                                stale: false,
                                diagnostics: vec![
                                    "runtime probe passed; no model session or inference was run"
                                        .to_string(),
                                ],
                            })
                    })
                    .collect();
                RuntimeReadiness {
                    id: kind.to_string(),
                    available: probe.available,
                    version: probe.version,
                    providers: providers.into_iter().collect(),
                    provider_validations,
                    devices: probe
                        .devices
                        .into_iter()
                        .map(|device| device.name)
                        .collect(),
                    code,
                    message: probe.reason_unavailable,
                }
            })
            .collect();

        let technical_states = self
            .model_technical_state
            .read()
            .map(|states| states.clone())
            .unwrap_or_default();
        let mut model_entries: Vec<_> = self.model_registry.entries().collect();
        model_entries.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
        let model_and_quality = model_entries
            .into_iter()
            .map(|(manifest, model_dir)| {
                match self.prepare_model_runtime(manifest, model_dir, None) {
                    Ok(_resolved) => {
                        let state = technical_states
                            .get(&manifest.id)
                            .cloned()
                            .unwrap_or_default();
                        let runtime = state.runtime.clone();
                        let provider = state.effective_provider.clone();
                        let quality =
                            self.model_quality_registry.validate(ModelQualityValidation {
                                model_id: &manifest.id,
                                model_version: &manifest.version,
                                model_sha256: &declared_model_sha256(manifest),
                                dataset_version: None,
                                runtime: runtime.as_deref(),
                                provider: provider.as_deref(),
                            });
                        let technical_ready = state.executor_created
                            && state.session_created
                            && state.latest_inference_succeeded()
                            && provider.is_some();
                        let code = if technical_ready {
                            quality.code
                        } else if provider.is_none() {
                            Some(CoreErrorCode::ModelEffectiveProviderUnknown)
                        } else {
                            Some(CoreErrorCode::ProviderValidationRequired)
                        };
                        let message = if technical_ready {
                            quality.message.clone()
                        } else {
                            Some(
                                "model artifacts resolve, but executor/session/smoke readiness has not been observed"
                                    .to_owned(),
                            )
                        };
                        (
                            ApiModelReadiness {
                                id: manifest.id.clone(),
                                task: manifest.task.id().to_owned(),
                                version: manifest.version.clone(),
                                manifest_valid: true,
                                artifacts_valid: true,
                                runtime_resolved: true,
                                executor_created: state.executor_created,
                                session_created: state.session_created,
                                smoke_inference_passed: state.latest_inference_succeeded(),
                                technical_ready,
                                quality_status: quality.status,
                                runtime,
                                provider,
                                code,
                                message,
                            },
                            quality,
                        )
                    }
                    Err(error) => {
                        let code = readiness_error_code(&error);
                        (
                            ApiModelReadiness {
                                id: manifest.id.clone(),
                                task: manifest.task.id().to_owned(),
                                version: manifest.version.clone(),
                                manifest_valid: true,
                                artifacts_valid: false,
                                runtime_resolved: false,
                                executor_created: false,
                                session_created: false,
                                smoke_inference_passed: false,
                                technical_ready: false,
                                quality_status: ModelQualityStatus::Unknown,
                                runtime: None,
                                provider: None,
                                code: Some(code),
                                message: Some(error.to_string()),
                            },
                            ModelQualityReadiness {
                                model_id: manifest.id.clone(),
                                model_version: manifest.version.clone(),
                                task: manifest.task.id().to_owned(),
                                status: ModelQualityStatus::Unknown,
                                dataset_version: None,
                                runtime: None,
                                provider: None,
                                baseline_sha256: None,
                                code: Some(CoreErrorCode::ModelQualityNotValidated),
                                message: Some(
                                    "technical model resolution failed before quality evidence could be matched"
                                        .to_owned(),
                                ),
                            },
                        )
                    }
                }
            })
            .collect::<Vec<_>>();
        let models = model_and_quality
            .iter()
            .map(|(model, _)| model.clone())
            .collect::<Vec<_>>();
        let quality = model_and_quality
            .into_iter()
            .map(|(_, quality)| quality)
            .collect::<Vec<_>>();

        let modes = RecognizeMode::all()
            .iter()
            .copied()
            .map(|mode| {
                let tasks = required_tasks(mode, self.config.parse_mode)
                    .into_iter()
                    .map(|task| match self.select_runnable_model(task) {
                        Ok(Some(model_id)) => {
                            let model = models.iter().find(|model| model.id == model_id);
                            let technical_ready = model.is_some_and(|model| model.technical_ready);
                            let quality_status = model
                                .map_or(ModelQualityStatus::Unknown, |model| model.quality_status);
                            TaskReadiness {
                                task: task.id().to_owned(),
                                technical_ready,
                                quality_ready: quality_status.is_quality_ready(),
                                selected_model: Some(model_id),
                                code: model.and_then(|model| model.code),
                                message: model.and_then(|model| model.message.clone()),
                            }
                        }
                        Ok(None) => TaskReadiness {
                            task: task.id().to_owned(),
                            technical_ready: false,
                            quality_ready: false,
                            selected_model: None,
                            code: Some(if mode == RecognizeMode::CroppedFormula {
                                CoreErrorCode::CroppedFormulaModelMissing
                            } else {
                                CoreErrorCode::ModelNotFound
                            }),
                            message: Some("no runnable model is installed".to_owned()),
                        },
                        Err(error) => TaskReadiness {
                            task: task.id().to_owned(),
                            technical_ready: false,
                            quality_ready: false,
                            selected_model: None,
                            code: Some(readiness_error_code(&error)),
                            message: Some(error.to_string()),
                        },
                    })
                    .collect::<Vec<_>>();
                let technical_ready = tasks.iter().all(|task| task.technical_ready);
                let quality_ready = tasks.iter().all(|task| task.quality_ready);
                let production_recommended = technical_ready
                    && tasks.iter().all(|task| {
                        task.selected_model.as_ref().is_some_and(|selected| {
                            models.iter().any(|model| {
                                model.id == *selected
                                    && model.quality_status == ModelQualityStatus::Validated
                            })
                        })
                    });
                ModeReadiness {
                    mode: mode.label().to_owned(),
                    technical_ready,
                    quality_ready,
                    production_recommended,
                    tasks,
                }
            })
            .collect();

        EngineReadiness {
            schema_version: READINESS_SCHEMA_VERSION,
            core_version: env!("CARGO_PKG_VERSION").to_owned(),
            modes,
            runtimes,
            models,
            quality,
            diagnostics,
        }
    }

    /// Record externally produced validation only after the caller has
    /// authenticated the report and supplied its full environment key.
    pub fn record_provider_validation(&self, mut report: ProviderValidationReport) -> Result<()> {
        let supplied_smoke_sha = report
            .key
            .as_ref()
            .map(|key| key.smoke_model_sha256.clone());
        let configured_fixture = self
            .config
            .provider_smoke_fixture
            .as_deref()
            .map(ProviderSmokeFixture::load)
            .transpose()?;
        let smoke_sha = configured_fixture
            .as_ref()
            .map(ProviderSmokeFixture::model_sha256)
            .or(supplied_smoke_sha.as_deref());
        let probe = self
            .runtime_registry
            .probe_all()
            .into_values()
            .find(|probe| {
                probe
                    .capabilities
                    .execution_providers
                    .iter()
                    .any(|provider| provider.eq_ignore_ascii_case(&report.provider))
            })
            .ok_or_else(|| {
                SnipperError::Runtime(format!(
                    "cannot collect environment fingerprint for unavailable provider '{}'",
                    report.provider
                ))
            })?;
        report.key = Some(current_provider_key(&report.provider, &probe, smoke_sha));
        self.provider_validation_store.record(report)
    }

    /// Validate a provider according to an explicit policy. Readiness itself
    /// never starts a session, smoke inference, or benchmark.
    pub fn validate_provider(
        &self,
        request: ProviderValidationRequest,
    ) -> Result<ProviderValidationReport> {
        let validation_started = std::time::Instant::now();
        let provider = request.provider.to_ascii_lowercase();
        let runtime_probe =
            self.runtime_registry
                .probe_all()
                .into_iter()
                .find_map(|(kind, probe)| {
                    probe
                        .capabilities
                        .execution_providers
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(&provider))
                        .then_some((kind, probe))
                });
        let Some((runtime_kind, probe)) = runtime_probe else {
            return Ok(ProviderValidationReport {
                provider,
                validation_level: ProviderValidationLevel::Declared,
                library_detected: false,
                probe_passed: false,
                session_created: false,
                smoke_inference_passed: false,
                benchmark_measured: false,
                benchmark_validated: false,
                scope: ValidationScope::CurrentProcess,
                reusable_across_restart: false,
                validated_at: 0,
                duration_ms: 0,
                runtime_instance_id: self
                    .provider_validation_store
                    .runtime_instance_id()
                    .to_owned(),
                session_generation: 0,
                last_failure_code: Some(CoreErrorCode::ProviderUnavailable.as_str().to_owned()),
                key: request.key,
                stale: false,
                diagnostics: vec![format!(
                    "{}: provider is not available in any registered runtime",
                    CoreErrorCode::ProviderUnavailable.as_str()
                )],
            });
        };
        let fixture = self
            .config
            .provider_smoke_fixture
            .as_deref()
            .map(ProviderSmokeFixture::load)
            .transpose();
        let supplied_smoke_sha = request
            .key
            .as_ref()
            .map(|key| key.smoke_model_sha256.as_str());
        let smoke_sha = fixture
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .map(ProviderSmokeFixture::model_sha256)
            .or(supplied_smoke_sha);
        let key = current_provider_key(&provider, &probe, smoke_sha);
        if let Some(cached) = self.provider_validation_store.lookup(&key)? {
            let sufficient = match request.policy {
                ProviderValidationPolicy::ProbeOnly => cached.probe_passed,
                ProviderValidationPolicy::CreateSession => cached.session_created,
                ProviderValidationPolicy::SmokeInference => cached.smoke_inference_passed,
                ProviderValidationPolicy::Benchmark => cached.benchmark_measured,
            };
            if sufficient && !cached.stale {
                return Ok(cached);
            }
        }

        let mut report = ProviderValidationReport {
            provider,
            validation_level: ProviderValidationLevel::ProbePassed,
            library_detected: true,
            probe_passed: true,
            session_created: false,
            smoke_inference_passed: false,
            benchmark_measured: false,
            benchmark_validated: false,
            scope: ValidationScope::CurrentProcess,
            reusable_across_restart: false,
            validated_at: 0,
            duration_ms: 0,
            runtime_instance_id: self
                .provider_validation_store
                .runtime_instance_id()
                .to_owned(),
            session_generation: 0,
            last_failure_code: None,
            key: Some(key),
            stale: false,
            diagnostics: Vec::new(),
        };
        let mut session = None;
        if request.policy != ProviderValidationPolicy::ProbeOnly {
            match &fixture {
                Ok(Some(fixture)) => {
                    let artifacts = RuntimeArtifacts::new(runtime_kind.clone())
                        .with_file("model", fixture.model_path());
                    let options = RuntimeOptions {
                        providers: vec![ExecutionProviderSpec::new(report.provider.clone())],
                        ..RuntimeOptions::default()
                    };
                    match self
                        .runtime_registry
                        .create_session(&runtime_kind, &artifacts, &options)
                    {
                        Ok(created) => {
                            if let Some(effective) = created.metadata().effective_provider.clone() {
                                report.provider = effective.to_ascii_lowercase();
                                report.key =
                                    Some(current_provider_key(&report.provider, &probe, smoke_sha));
                                report.session_created = true;
                                report.validation_level = ProviderValidationLevel::SessionCreated;
                                report
                                    .diagnostics
                                    .extend(created.metadata().fallback_diagnostics.clone());
                                session = Some(created);
                            } else {
                                report.last_failure_code = Some(
                                    CoreErrorCode::ModelEffectiveProviderUnknown
                                        .as_str()
                                        .to_owned(),
                                );
                                report.diagnostics.push(format!(
                                    "{}: runtime session did not report an effective provider",
                                    CoreErrorCode::ModelEffectiveProviderUnknown.as_str()
                                ));
                            }
                        }
                        Err(error) => {
                            report.last_failure_code = Some(
                                CoreErrorCode::ProviderSessionCreateFailed
                                    .as_str()
                                    .to_owned(),
                            );
                            report.diagnostics.push(format!(
                                "{}: {}",
                                CoreErrorCode::ProviderSessionCreateFailed.as_str(),
                                error
                            ));
                        }
                    }
                }
                Ok(None) if request.policy == ProviderValidationPolicy::CreateSession => {
                    let mut failures = Vec::new();
                    for (manifest, model_dir) in self.model_registry.entries() {
                        let mut resolved =
                            match self.prepare_model_runtime(manifest, model_dir, None) {
                                Ok(resolved) => resolved,
                                Err(error) => {
                                    failures.push(error.to_string());
                                    continue;
                                }
                            };
                        resolved.options.providers =
                            vec![ExecutionProviderSpec::new(report.provider.clone())];
                        match self.runtime_registry.create_resolved_session(&resolved) {
                            Ok(created) => {
                                if let Some(effective) =
                                    created.metadata().effective_provider.clone()
                                {
                                    report.provider = effective.to_ascii_lowercase();
                                    report.key = Some(current_provider_key(
                                        &report.provider,
                                        &probe,
                                        smoke_sha,
                                    ));
                                    report.session_created = true;
                                    report.validation_level =
                                        ProviderValidationLevel::SessionCreated;
                                    report
                                        .diagnostics
                                        .extend(created.metadata().fallback_diagnostics.clone());
                                    session = Some(created);
                                    break;
                                }
                                failures.push(format!(
                                    "{}: runtime session did not report an effective provider",
                                    CoreErrorCode::ModelEffectiveProviderUnknown.as_str()
                                ));
                            }
                            Err(error) => failures.push(error.to_string()),
                        }
                    }
                    if !report.session_created {
                        report.last_failure_code = Some(
                            CoreErrorCode::ProviderSessionCreateFailed
                                .as_str()
                                .to_owned(),
                        );
                        report.diagnostics.push(format!(
                            "{}: no installed model session could be created ({})",
                            CoreErrorCode::ProviderSessionCreateFailed.as_str(),
                            failures
                                .first()
                                .map_or("no installed model was eligible", String::as_str)
                        ));
                    }
                }
                Ok(None) => report.diagnostics.push(format!(
                    "{}: configure a versioned tensor fixture before smoke validation",
                    CoreErrorCode::ProviderValidationRequired.as_str()
                )),
                Err(error) => report.diagnostics.push(format!(
                    "{}: {}",
                    CoreErrorCode::ProviderValidationRequired.as_str(),
                    error
                )),
            }
        }
        if matches!(
            request.policy,
            ProviderValidationPolicy::SmokeInference | ProviderValidationPolicy::Benchmark
        ) {
            if let (Ok(Some(fixture)), Some(session)) = (&fixture, session.as_deref()) {
                match fixture.execute(session) {
                    Ok(outcome) => {
                        report.smoke_inference_passed = true;
                        report.validation_level = ProviderValidationLevel::SmokeInferencePassed;
                        report.diagnostics.push(format!(
                            "provider smoke output {} completed in {:.3} ms",
                            outcome.output_sha256,
                            outcome.inference_duration.as_secs_f64() * 1000.0
                        ));
                        if request.policy == ProviderValidationPolicy::Benchmark {
                            let mut samples = Vec::new();
                            let mut benchmark_failed = None;
                            for _ in 0..3 {
                                match fixture.execute(session) {
                                    Ok(sample) => samples
                                        .push(sample.inference_duration.as_secs_f64() * 1000.0),
                                    Err(error) => {
                                        benchmark_failed = Some(error);
                                        break;
                                    }
                                }
                            }
                            if let Some(error) = benchmark_failed {
                                report.diagnostics.push(format!(
                                    "{}: benchmark inference failed: {}",
                                    CoreErrorCode::ProviderSmokeInferenceFailed.as_str(),
                                    error
                                ));
                            } else {
                                samples.sort_by(f64::total_cmp);
                                report.benchmark_measured = true;
                                report.validation_level =
                                    ProviderValidationLevel::BenchmarkMeasured;
                                report.diagnostics.push(format!(
                                    "provider benchmark median over {} measured runs: {:.3} ms",
                                    samples.len(),
                                    samples[samples.len() / 2]
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        report.last_failure_code = Some(
                            CoreErrorCode::ProviderSmokeInferenceFailed
                                .as_str()
                                .to_owned(),
                        );
                        report.diagnostics.push(format!(
                            "{}: {}",
                            CoreErrorCode::ProviderSmokeInferenceFailed.as_str(),
                            error
                        ));
                    }
                }
            }
        }
        report.validated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or_default();
        report.duration_ms = validation_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        report.session_generation = self.provider_validation_store.next_session_generation();
        self.provider_validation_store.record(report.clone())?;
        Ok(report)
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
    pub fn get_or_create_model_package(&self, model_id: &str) -> Result<Arc<dyn ModelPackage>> {
        // Check cache first
        {
            let cache = self
                .model_packages
                .read()
                .map_err(|_| SnipperError::Model("Model package cache poisoned".into()))?;

            if let Some(package) = cache.get(model_id) {
                return Ok(package.clone());
            }
        }

        // Look up manifest and directory from registry
        let manifest = self.model_registry.get(model_id).ok_or_else(|| {
            SnipperError::Model(format!("Model '{}' is not registered", model_id))
        })?;

        let model_dir = self.model_registry.get_dir(model_id).ok_or_else(|| {
            SnipperError::Model(format!("Model '{}' has no model directory", model_id))
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
        let mut cache = self
            .model_packages
            .write()
            .map_err(|_| SnipperError::Model("Model package cache poisoned".into()))?;

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

    // ========================================================================
    // Runnable model validation & selection
    // ========================================================================

    /// Normalize a user-supplied model override to a full "category/variant" ID.
    ///
    /// Accepts both short variant names ("ppocrv5-mobile") and full IDs
    /// ("text-recognition/ppocrv5-mobile"). Short names are expanded using the
    /// canonical task category.
    fn normalize_model_override(&self, task: ModelTask, value: &str) -> String {
        if value.contains('/') {
            value.to_string()
        } else {
            format!("{}/{}", task.id(), value)
        }
    }

    /// Extract the variant portion from a "category/variant" string.
    ///
    /// Returns the part after the `/`, or the whole string if no `/` is present.
    fn variant_part(value: &str) -> &str {
        value.split_once('/').map(|(_, v)| v).unwrap_or(value)
    }

    /// Validate that a model can actually run using the real RuntimeResolver.
    ///
    /// Checks: manifest exists, adapter is registered, package is constructable,
    /// and at least one runtime variant resolves successfully via the full
    /// RuntimeResolver (platform, artifacts, capabilities, runtime availability).
    ///
    /// Falls back to `model_resolver` when the model is not in the registry
    /// (e.g. WASM builds that use `MemoryModelResolver`).
    fn validate_model_runnable(&self, model_id: &str) -> Result<()> {
        // Built-in rule-based strategies (e.g. table projection) don't
        // need a real model artifact or runtime.
        if is_builtin_model_strategy(model_id) {
            return Ok(());
        }

        // If the model isn't in the registry, check via model_resolver
        // (WASM builds use MemoryModelResolver, not ModelRegistry).
        // Use list_artifacts() to detect both single-model and
        // multi-artifact packages (encoder-decoder, Paddle, etc.).
        if !self.model_registry.has(model_id) {
            if let Some(ref resolver) = self.model_resolver {
                let available = |id: &latexsnipper_runtime::ModelId| {
                    resolver.is_available(id) || !resolver.list_artifacts(id).is_empty()
                };

                let canonical_id = latexsnipper_runtime::ModelId::from_composite_key(model_id);

                if available(&canonical_id) {
                    return Ok(());
                }

                // Also try the legacy category name
                if let Some((cat, var)) = model_id.split_once('/') {
                    let legacy_id = latexsnipper_runtime::ModelId::new(legacy_category(cat), var);
                    if available(&legacy_id) {
                        return Ok(());
                    }
                }

                return Err(SnipperError::Model(format!(
                    "Model '{}' is not available in the resolver",
                    model_id
                )));
            }

            return Err(SnipperError::Model(format!(
                "Model '{}' is not registered and no resolver is configured",
                model_id
            )));
        }

        let manifest = self.model_registry.get(model_id).ok_or_else(|| {
            SnipperError::Model(format!("Model '{}' is not registered", model_id))
        })?;

        let model_dir = self.model_registry.get_dir(model_id).ok_or_else(|| {
            SnipperError::Model(format!("Model '{}' has no model directory", model_id))
        })?;

        // Check adapter is registered and package is constructable
        self.model_registry
            .create_package(manifest, model_dir)?
            .ok_or_else(|| {
                SnipperError::Model(format!(
                    "Unsupported model adapter '{}' for model '{}'",
                    manifest.adapter, model_id
                ))
            })?;

        // Use prepare_model_runtime: resolves + fills max_threads/acceleration
        self.prepare_model_runtime(manifest, model_dir, None)?;

        Ok(())
    }

    /// Select a model that is confirmed runnable on the current system.
    ///
    /// This extends [`select_model_id`] with runtime probing:
    ///
    /// 1. Explicit user override → normalized to full ID, validated, fails hard
    ///    if not runnable
    /// 2. Auto-selection → tries selected + fallbacks in order, picks the
    ///    first one that is actually runnable
    ///
    /// Returns the selected model ID, or `None` if no runnable candidate exists.
    pub fn select_runnable_model(&self, task: ModelTask) -> Result<Option<String>> {
        // 1. Explicit user override
        if let Some(explicit) = self.config.model_override(task) {
            let model_id = self.normalize_model_override(task, explicit);
            self.validate_model_runnable(&model_id)?;
            return Ok(Some(model_id));
        }

        // 2. Auto-selection with fallback probing
        let decision = self.select_model(task, None, Some(self.config.acceleration), None);

        let candidates = decision.selected.into_iter().chain(decision.fallbacks);

        let mut failures = Vec::new();

        for model_id in candidates {
            match self.validate_model_runnable(&model_id) {
                Ok(()) => {
                    info!("Selected runnable model '{}' for {:?}", model_id, task);
                    return Ok(Some(model_id));
                }
                Err(error) => {
                    warn!("Model '{}' not runnable: {}", model_id, error);
                    failures.push(format!("{}: {}", model_id, error));
                }
            }
        }

        if failures.is_empty() {
            info!("No model candidates for {:?}", task);
            Ok(None)
        } else {
            warn!("No runnable model for {:?}: {}", task, failures.join("; "));
            // Return None rather than Err to allow degraded operation
            Ok(None)
        }
    }

    /// Select and register the best *runnable* model for a task into the
    /// pipeline context.
    ///
    /// This is the authoritative entry point for model assignment in
    /// pipeline execution. It respects user overrides, validates
    /// runtime availability, resolves the runtime variant, and registers
    /// a [`PreparedModel`] that binds the model package to its resolved
    /// runtime — ensuring the executor uses the correct runtime.
    pub fn select_and_register_model(
        &self,
        ctx: &mut PipelineContext,
        task: ModelTask,
    ) -> Result<Option<String>> {
        let Some(model_id) = self.select_runnable_model(task)? else {
            return Ok(None);
        };

        // Built-in strategy (e.g. table projection): set variant hints,
        // skip ModelRegistry/PreparedModel.
        if is_builtin_model_strategy(&model_id) {
            self.set_model_variant_hints(ctx, task, &model_id);
            return Ok(Some(model_id));
        }

        // WASM / resolver-only model: set hints, don't attempt Package creation.
        if !self.model_registry.has(&model_id) {
            if self.model_resolver.is_some() {
                self.set_model_variant_hints(ctx, task, &model_id);
                return Ok(Some(model_id));
            }
            return Ok(None);
        }

        // Native registry-backed path: create PreparedModel.
        let package = self.get_or_create_model_package(&model_id)?;

        let manifest = self.model_registry.get(&model_id).ok_or_else(|| {
            SnipperError::Model(format!("Model '{}' vanished during registration", model_id))
        })?;
        let model_dir = self.model_registry.get_dir(&model_id).ok_or_else(|| {
            SnipperError::Model(format!(
                "Model '{}' has no directory during registration",
                model_id
            ))
        })?;
        let resolved = self.prepare_model_runtime(manifest, model_dir, None)?;

        let prepared = PreparedModel::new(model_id.clone(), package, resolved);
        ctx.register_prepared_model(task, prepared);

        // Backward compat
        ctx.register_model_package(task, self.get_or_create_model_package(&model_id)?);

        self.set_model_variant_hints(ctx, task, &model_id);

        Ok(Some(model_id))
    }

    /// Set both canonical and legacy model variant hints in the context.
    fn set_model_variant_hints(&self, ctx: &mut PipelineContext, task: ModelTask, model_id: &str) {
        if let Some((_category, variant)) = model_id.split_once('/') {
            let key = task_category_key(task);
            ctx.model_variants
                .insert(key.to_string(), variant.to_string());
            let legacy = legacy_category(key);
            ctx.model_variants
                .insert(legacy.to_string(), variant.to_string());
        }
    }

    /// Prepare models for all required tasks in a recognition mode.
    ///
    /// For tasks with explicit user overrides, failures are propagated
    /// immediately. For auto-selected tasks, failures are logged and the
    /// pipeline continues in degraded mode.
    fn prepare_pipeline_models(
        &self,
        ctx: &mut PipelineContext,
        mode: RecognizeMode,
        parse_mode: DocumentParseMode,
    ) -> Result<()> {
        for task in required_tasks(mode, parse_mode) {
            let has_explicit_override = self.config.model_override(task).is_some();

            match self.select_and_register_model(ctx, task) {
                Ok(Some(ref model_id)) => {
                    info!("Selected model '{}' for {:?}", model_id, task);
                }
                Ok(None) => {
                    if has_explicit_override {
                        // User explicitly configured a model but it couldn't be
                        // selected or wasn't runnable → fail hard.
                        return Err(SnipperError::Model(format!(
                            "Explicitly configured model for {:?} could not be loaded",
                            task
                        )));
                    }
                    warn!("No model selected for {:?}", task);
                }
                Err(error) => {
                    if has_explicit_override {
                        return Err(error);
                    }
                    warn!("Failed to prepare model for {:?}: {}", task, error);
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
        self.provider_validation_store.clear_ephemeral();
        if let Ok(mut states) = self.model_technical_state.write() {
            states.clear();
        }
    }

    /// Rescan the models directory for new or removed models.
    ///
    /// This clears the model cache and re-registers everything from the
    /// configured models directory. Built-in adapters are preserved.
    ///
    /// Returns a scan report describing what was loaded and any issues.
    pub fn rescan_models(&mut self) -> Result<ModelScanReport> {
        self.provider_validation_store.clear_ephemeral();
        self.model_registry.clear_models();

        let report = self
            .model_registry
            .register_models_root(&self.config.models_dir)?;
        self.model_scan_issues = report.issues.clone();
        self.model_quality_registry = load_model_quality_registry(&self.config);

        // Clear the package cache so stale packages are not reused
        self.model_packages
            .write()
            .map_err(|_| SnipperError::Model("Model package cache poisoned".into()))?
            .clear();
        if let Ok(mut states) = self.model_technical_state.write() {
            states.clear();
        }

        Ok(report)
    }

    /// Full hot reload: clear sessions and rescan models.
    ///
    /// Use this when models have been added, removed, or updated on disk.
    /// After calling this, the next recognition will use the updated models.
    pub fn reload_all_models(&mut self) -> Result<ModelScanReport> {
        self.runtime_registry.clear_sessions();
        self.provider_validation_store.clear_ephemeral();
        self.rescan_models()
    }

    /// Reload a specific model by clearing all cached sessions.
    /// Next inference call will create fresh sessions with the new model files.
    #[deprecated(note = "Use clear_runtime_sessions() or reload_all_models() instead")]
    pub fn reload_model(&self, _session_key: &str) -> Result<()> {
        info!("Clearing cached model runtime sessions");
        self.runtime_registry.clear_sessions();
        self.provider_validation_store.clear_ephemeral();
        Ok(())
    }

    /// Get the model resolver.
    pub fn model_resolver(&self) -> Option<&SharedModelResolver> {
        self.model_resolver.as_ref()
    }

    /// Set a new model resolver (for hot-swapping model sources).
    pub fn set_model_resolver(&mut self, resolver: SharedModelResolver) {
        self.provider_validation_store.clear_ephemeral();
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
        self.build_pipeline_with_parse_mode(mode, self.config.parse_mode)
    }

    /// Build a pipeline using a request-scoped document parse mode.
    pub fn build_pipeline_with_parse_mode(
        &self,
        mode: RecognizeMode,
        parse_mode: DocumentParseMode,
    ) -> PipelineGraph {
        let profile = match mode {
            RecognizeMode::Formula => PipelineProfile::Formula,
            RecognizeMode::CroppedFormula => PipelineProfile::CroppedFormula,
            RecognizeMode::Text => PipelineProfile::Text,
            RecognizeMode::Mixed => PipelineProfile::Mixed,
            RecognizeMode::Handwriting => PipelineProfile::Handwriting,
            RecognizeMode::Table => PipelineProfile::Table,
            RecognizeMode::FormulaLayout => PipelineProfile::FormulaLayout,
            _ => return PipelineGraph::new(format!("{:?}_pipeline", mode)),
        };
        PipelinePlanner.plan(profile, parse_mode).build_graph()
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
        self.recognize_controlled(image, mode, self.config.parse_mode, None, None, None)
            .await
    }

    /// Recognize an image with request-scoped control and parsing options.
    ///
    /// The engine and runtime registry remain owned by `self`; this method only
    /// creates the per-request pipeline context.
    pub async fn recognize_controlled(
        &self,
        image: SnipperImage,
        mode: RecognizeMode,
        parse_mode: DocumentParseMode,
        cancellation: Option<PipelineCancellationToken>,
        timeout: Option<std::time::Duration>,
        progress: Option<Arc<dyn PipelineProgressObserver>>,
    ) -> Result<Document> {
        info!(
            "Recognizing image ({}, {}) in {:?} mode",
            image.width(),
            image.height(),
            mode
        );

        let graph = self.build_pipeline_with_parse_mode(mode, parse_mode);
        let mut ctx =
            self.configure_context_with_parse_mode(PipelineContext::with_image(image), parse_mode);
        if mode == RecognizeMode::CroppedFormula {
            ctx.metadata
                .insert("croppedFormula".to_owned(), serde_json::Value::Bool(true));
        }
        if let Some(cancellation) = cancellation {
            ctx.set_cancellation_token(cancellation);
        }
        if let Some(timeout) = timeout {
            ctx.set_timeout(timeout);
        }
        if let Some(progress) = progress {
            ctx.set_progress_observer(progress);
        }
        ctx.check_control()?;

        // Auto-select and register models for this mode
        self.prepare_pipeline_models(&mut ctx, mode, parse_mode)?;
        ctx.check_control()?;

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

    /// Prepare model executors for a profile without running inference.
    ///
    /// Executor construction resolves artifacts and creates runtime sessions
    /// for adapters that support eager loading. Runtime-owned caches keep those
    /// sessions available to later recognition requests.
    pub fn warmup_profile(
        &self,
        mode: RecognizeMode,
        parse_mode: DocumentParseMode,
    ) -> Vec<EngineWarmupEntry> {
        let mut ctx = self.configure_context_with_parse_mode(PipelineContext::new(), parse_mode);
        required_tasks(mode, parse_mode)
            .into_iter()
            .map(
                |task| match self.select_and_register_model(&mut ctx, task) {
                    Ok(Some(model_id)) if is_builtin_model_strategy(&model_id) => {
                        EngineWarmupEntry {
                            task,
                            model_id: Some(model_id),
                            loaded: true,
                            message: Some(
                                "built-in strategy requires no runtime session".to_string(),
                            ),
                        }
                    }
                    Ok(Some(model_id)) => match ctx.create_model_executor(task) {
                        Ok(Some(_executor)) => EngineWarmupEntry {
                            task,
                            model_id: Some(model_id),
                            loaded: true,
                            message: None,
                        },
                        Ok(None) => EngineWarmupEntry {
                            task,
                            model_id: Some(model_id),
                            loaded: false,
                            message: Some(
                                "model resolved but no executable adapter was available"
                                    .to_string(),
                            ),
                        },
                        Err(error) => EngineWarmupEntry {
                            task,
                            model_id: Some(model_id),
                            loaded: false,
                            message: Some(error.to_string()),
                        },
                    },
                    Ok(None) => EngineWarmupEntry {
                        task,
                        model_id: None,
                        loaded: false,
                        message: Some("no runnable model found".to_string()),
                    },
                    Err(error) => EngineWarmupEntry {
                        task,
                        model_id: None,
                        loaded: false,
                        message: Some(error.to_string()),
                    },
                },
            )
            .collect::<Vec<_>>()
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
            self.prepare_pipeline_models(&mut ctx, mode, self.config.parse_mode)?;

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
    ///
    /// Note: model variant hints (ctx.model_variants) are now set exclusively
    /// by `select_and_register_model`, which uses canonical category keys.
    /// The old short-key overrides below are retained temporarily for backward
    /// compatibility with pipeline nodes that still read the deprecated keys.
    #[cfg(feature = "native")]
    fn configure_context(&self, ctx: PipelineContext) -> PipelineContext {
        self.configure_context_with_parse_mode(ctx, self.config.parse_mode)
    }

    fn configure_context_with_parse_mode(
        &self,
        mut ctx: PipelineContext,
        parse_mode: DocumentParseMode,
    ) -> PipelineContext {
        ctx.models_dir = Some(self.config.models_dir.clone());
        ctx.runtime_registry = Some(self.runtime_registry.clone());
        ctx.backend = Some(self.runtime());
        ctx.model_resolver = self.model_resolver.clone();
        ctx.acceleration = self.config.acceleration;
        ctx.max_threads = self.config.max_threads;
        ctx.parse_mode = parse_mode;
        ctx.set_model_runtime_observer(Arc::new(EngineModelRuntimeObserver {
            states: self.model_technical_state.clone(),
        }));

        // Apply explicit model variant overrides from config, using both
        // canonical and legacy keys for backward compatibility.
        // Legacy keys receive only the variant portion (e.g. "ppocrv5-mobile"),
        // while canonical keys receive the full override value.
        //
        // TODO(p0): remove legacy short keys once all pipeline nodes read canonical keys.
        if let Some(v) = &self.config.formula_det_model {
            ctx.model_variants
                .insert(category::FORMULA_DETECTION.into(), v.clone());
            ctx.model_variants
                .insert("formula-det".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.formula_rec_model {
            ctx.model_variants
                .insert(category::FORMULA_RECOGNITION.into(), v.clone());
            ctx.model_variants
                .insert("formula-rec".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.text_det_model {
            ctx.model_variants
                .insert(category::TEXT_DETECTION.into(), v.clone());
            ctx.model_variants
                .insert("text-det".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.text_rec_model {
            ctx.model_variants
                .insert(category::TEXT_RECOGNITION.into(), v.clone());
            ctx.model_variants
                .insert("text-rec".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.table_det_model {
            ctx.model_variants
                .insert(category::TABLE_DETECTION.into(), v.clone());
            ctx.model_variants
                .insert("table-det".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.table_struct_model {
            ctx.model_variants
                .insert(category::TABLE_STRUCTURE.into(), v.clone());
            ctx.model_variants
                .insert("table-struct".into(), Self::variant_part(v).to_string());
        }
        if let Some(v) = &self.config.handwriting_det_model {
            ctx.model_variants
                .insert(category::HANDWRITING_RECOGNITION.into(), v.clone());
            ctx.model_variants
                .insert("handwriting-det".into(), Self::variant_part(v).to_string());
        }

        ctx
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

#[cfg(test)]
mod readiness_tests {
    use super::*;

    #[test]
    fn mixed_mode_observation_does_not_promote_unused_fallback_model() {
        let states = Arc::new(RwLock::new(HashMap::new()));
        let observer = EngineModelRuntimeObserver {
            states: states.clone(),
        };
        for event in [
            ModelRuntimeEvent::ExecutorCreated,
            ModelRuntimeEvent::SessionCreated {
                runtime: "onnxruntime".to_owned(),
                effective_provider: "cpu".to_owned(),
            },
            ModelRuntimeEvent::InferenceStarted,
            ModelRuntimeEvent::InferenceCompleted,
        ] {
            observer.observe("text-recognition/selected", event);
        }

        let states = states.read().unwrap();
        let selected = states.get("text-recognition/selected").unwrap();
        assert!(selected.executor_created);
        assert!(selected.session_created);
        assert!(selected.inference_completed);
        assert!(!states.contains_key("text-recognition/unused-fallback"));
        assert!(!states.contains_key("table-structure/unused-conditional"));
    }

    #[cfg(feature = "native")]
    #[test]
    fn provider_benchmark_measures_without_claiming_validation() {
        let models = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("contracts")
            .join("fixtures")
            .join("provider-smoke-v1.json");
        let config = EngineConfig::with_models_dir(models.path().to_owned())
            .with_provider_smoke_fixture(fixture);
        let registry = crate::runtime_registry::default_runtime_registry(models.path()).unwrap();
        let engine = SnipperEngine::with_runtime_registry(config, registry).unwrap();

        let report = engine
            .validate_provider(ProviderValidationRequest {
                provider: "cpu".to_owned(),
                policy: ProviderValidationPolicy::Benchmark,
                key: None,
            })
            .unwrap();
        assert!(report.session_created, "{report:?}");
        assert!(report.smoke_inference_passed, "{report:?}");
        assert!(report.benchmark_measured, "{report:?}");
        assert!(!report.benchmark_validated, "{report:?}");
        assert_eq!(
            report.validation_level,
            ProviderValidationLevel::BenchmarkMeasured
        );
        let key = report.key.expect("smoke report must be environment keyed");
        assert_eq!(
            key.smoke_model_sha256,
            "ec6ecac6a32e663f67bd3967a6579171783c7185042cc61bb7ca84a92fdc5daa"
        );
        assert!(!key.provider_library_fingerprint.contains("unverified"));
    }

    #[test]
    fn explicit_quality_directory_loads_independently_of_models_directory() {
        let models = tempfile::tempdir().unwrap();
        let quality_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("quality")
            .join("baselines");
        let config = EngineConfig::with_models_dir(models.path().join("application-models"))
            .with_quality_baselines_dir(quality_root.clone());

        assert_eq!(quality_baselines_root(&config), quality_root);
        let registry = load_model_quality_registry(&config);
        let readiness = registry.validate(ModelQualityValidation {
            model_id: "trocr-deit",
            model_version: "models-v3.1.0",
            model_sha256: "c68629f7efe6b51e05833617f630aee90551dd505064a4a2d8e2529d11bff7f8",
            dataset_version: None,
            runtime: Some("onnxruntime"),
            provider: Some("cpu"),
        });
        assert!(
            readiness.baseline_sha256.is_some(),
            "explicit trusted baseline directory was not loaded: {readiness:?}"
        );
    }

    #[test]
    fn readiness_json_reports_modes_without_exposing_runtime_objects() {
        let models = tempfile::tempdir().unwrap();
        let engine = SnipperEngine::new(
            EngineConfig::with_models_dir(models.path().to_owned()),
            Box::new(latexsnipper_runtime::StubRuntime::new()),
        );

        let readiness = engine.readiness();
        assert_eq!(readiness.schema_version, READINESS_SCHEMA_VERSION);
        assert_eq!(readiness.runtimes.len(), 1);
        assert!(readiness.runtimes[0].available);
        assert!(readiness.modes.iter().all(|mode| !mode.technical_ready));

        let json = serde_json::to_value(readiness).unwrap();
        assert!(json.get("runtime_registry").is_none());
        assert!(json.get("sessions").is_none());
        assert_eq!(json["schemaVersion"], READINESS_SCHEMA_VERSION);
    }
}
