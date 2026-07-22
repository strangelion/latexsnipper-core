use crate::artifacts::PipelineArtifacts;
use crate::opendoc_hybrid::DocumentParseMode;
use crate::text_recognition_service::TextRecognitionService;
use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::{
    InferenceSession, ModelPackage, ModelTask, RuntimeBackend, RuntimeRegistry, SharedModelResolver,
};
use std::collections::HashMap;
use std::sync::Arc;

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
            models_dir: None,
            backend: None,
            runtime_registry: None,
            model_resolver: None,
            model_packages: HashMap::new(),
            model_variants: HashMap::new(),
            acceleration: latexsnipper_runtime::AccelerationMode::Cpu,
            max_threads: 4,
            sessions: HashMap::new(),
            diagnostics: Vec::new(),
            parse_mode: DocumentParseMode::default(),
            text_rec_service: None,
        }
    }

    /// Get or initialize the shared text recognition service.
    /// Uses the model resolver first and falls back to the native filesystem.
    pub fn get_or_init_text_rec_service(&mut self) -> Option<Arc<TextRecognitionService>> {
        if self.text_rec_service.is_some() {
            return self.text_rec_service.clone();
        }
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
