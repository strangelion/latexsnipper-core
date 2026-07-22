use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
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
    AccelerationMode, ModelPackage, ModelRegistry, ModelSelectionDecision, ModelSelectionPolicy,
    ModelSelectionRequest, ModelTask, RegistryRuntimeBackend, ResolvedRuntimeVariant,
    RuntimeBackend, RuntimeFactory, RuntimeKind, RuntimeRegistry, RuntimeSession,
    SharedModelResolver,
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
    /// Registered model packages for type-safe inference.
    model_packages: HashMap<ModelTask, Arc<dyn ModelPackage>>,
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

impl SnipperEngine {
    /// Create a new engine with the given config and runtime backend.
    pub fn new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Self {
        #[cfg(feature = "native")]
        let model_manager = ModelManager::new(config.models_dir.clone());
        #[cfg(feature = "native")]
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        #[cfg(not(feature = "native"))]
        let model_resolver = None;

        let (runtime_registry, default_runtime) = legacy_runtime_registry(runtime);

        Self {
            config,
            runtime_registry,
            default_runtime,
            model_resolver,
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: HashMap::new(),
            model_selection: ModelSelectionPolicy::default(),
            model_registry: ModelRegistry::new(),
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

        Self {
            config,
            runtime_registry,
            default_runtime,
            model_resolver: Some(resolver),
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: HashMap::new(),
            model_selection: ModelSelectionPolicy::default(),
            model_registry: ModelRegistry::new(),
        }
    }

    /// Construct an engine directly from the canonical runtime registry.
    pub fn with_runtime_registry(config: EngineConfig, registry: RuntimeRegistry) -> Result<Self> {
        let default_runtime = if registry.is_available(&RuntimeKind::OnnxRuntime) {
            RuntimeKind::OnnxRuntime
        } else {
            registry
                .available_runtimes()
                .into_iter()
                .next()
                .ok_or_else(|| {
                    latexsnipper_foundation::SnipperError::Runtime(
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

        Ok(Self {
            config,
            runtime_registry: Arc::new(registry),
            default_runtime,
            model_resolver,
            #[cfg(feature = "native")]
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: HashMap::new(),
            model_selection: ModelSelectionPolicy::default(),
            model_registry: ModelRegistry::new(),
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

    /// Register a model package for a specific task.
    pub fn register_model_package(&mut self, task: ModelTask, package: Arc<dyn ModelPackage>) {
        self.model_packages.insert(task, package);
    }

    /// Get a registered model package for a specific task.
    pub fn get_model_package(&self, task: &ModelTask) -> Option<Arc<dyn ModelPackage>> {
        self.model_packages.get(task).cloned()
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

    /// Select the best model for a given task using the ModelSelectionPolicy.
    ///
    /// This is the bridge between the declarative selection policy and the
    /// engine's runtime model packages. It queries the registry for candidates,
    /// applies the selection policy, and returns the decision with explanations.
    ///
    /// # Example
    /// ```ignore
    /// let decision = engine.select_model(ModelTask::FormulaDetection, None, None, None);
    /// if let Some(selected) = &decision.selected {
    ///     // Use the selected model ID
    /// }
    /// ```
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
    ) -> Option<String> {
        let decision = self.select_model(task, None, None, None);
        if let Some(ref model_id) = decision.selected {
            // Check if we have a pre-registered package for this task
            if let Some(package) = self.model_packages.get(&task) {
                ctx.register_model_package(task, package.clone());
            }
            // Also set the model variant hint in context for nodes that use
            // model_variants for model discovery
            let category = format!("{:?}", task).to_lowercase();
            ctx.model_variants.insert(category, model_id.clone());
        }
        decision.selected.clone()
    }

    // ========================================================================
    // Model Hot-Reload API
    // ========================================================================

    /// Reload a specific model by clearing all cached sessions.
    /// Next inference call will create fresh sessions with the new model files.
    pub fn reload_model(&self, session_key: &str) -> Result<()> {
        info!("Reloading model: {}", session_key);
        self.runtime_registry.clear_sessions();
        Ok(())
    }

    /// Reload all cached sessions, forcing fresh model loads on next inference.
    pub fn reload_all_models(&self) -> Result<()> {
        info!("Reloading all models");
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
                // Check if OpenDocHybrid mode is requested
                if self.config.parse_mode == latexsnipper_pipeline::DocumentParseMode::OpenDocHybrid
                {
                    // OpenDocHybrid pipeline: layout → region resolve → specialized recognizers
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
            _ => {
                // Future modes can be added here
            }
        }

        graph
    }

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
                        let text = match block {
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
                        };

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
    /// Engine only assembles the graph and runs it. All logic lives in Nodes.
    pub async fn recognize(&self, image: SnipperImage, mode: RecognizeMode) -> Result<Document> {
        info!(
            "Recognizing image ({}, {}) in {:?} mode",
            image.width(),
            image.height(),
            mode
        );

        let graph = self.build_pipeline(mode);
        let mut ctx = self.configure_context(PipelineContext::with_image(image));

        // Register model packages with the context
        for (task, package) in &self.model_packages {
            ctx.register_model_package(*task, package.clone());
        }

        graph.run(&mut ctx).await?;

        // Extract blocks from artifacts (already sorted by PostprocessNode)
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
            .map_err(|e| latexsnipper_foundation::SnipperError::Image(e.to_string()))?;

        info!("PDF loaded: {} pages", pages.len());

        let graph = self.build_pipeline(mode);
        let mut doc_pages = Vec::new();
        let mut diagnostics = Vec::new();

        for (page_idx, page_img) in pages.iter().enumerate() {
            if page_idx > 0 {
                info!("Processing page {}/{}", page_idx + 1, pages.len());
            }

            let mut ctx = self.configure_context(PipelineContext::with_image(page_img.clone()));

            // Register model packages with the context
            for (task, package) in &self.model_packages {
                ctx.register_model_package(*task, package.clone());
            }

            graph.run(&mut ctx).await?;

            // Collect blocks (already sorted by PostprocessNode)
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
            ctx.model_variants.insert("formula-det".into(), v.clone());
        }
        if let Some(v) = &self.config.formula_rec_model {
            ctx.model_variants.insert("formula-rec".into(), v.clone());
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
            ctx.model_variants.insert("table-struct".into(), v.clone());
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
                        log::warn!("Failed to parse manifest for layout registration: {}", e);
                        return;
                    }
                }
            }
            Err(e) => {
                log::warn!("Cannot read manifest: {}", e);
                return;
            }
        };

        // Look for a layout category
        let layout_info = manifest.categories.iter().find(|(cat, _)| *cat == "layout");
        let (_cat_name, layout_cat) = match layout_info {
            Some(v) => v,
            None => {
                log::info!("No layout category in manifest, skipping layout registration");
                return;
            }
        };

        // Find the default variant
        let default_id = layout_cat.default.as_deref().unwrap_or("pp-layout-cdla");

        let variant = layout_cat.variants.iter().find(|v| v.id == default_id);

        if let Some(variant) = variant {
            let variant_dir = self.config.models_dir.join("layout").join(&variant.id);

            // Check if layout model directory exists and has files
            if !variant_dir.is_dir() {
                log::info!(
                    "Layout model directory not found: {}, skipping layout registration",
                    variant_dir.display()
                );
                return;
            }

            let model_path =
                variant_dir.join(variant.files.first().unwrap_or(&"model.onnx".into()));
            if !model_path.exists() {
                log::info!(
                    "Layout model file not found: {}, skipping",
                    model_path.display()
                );
                return;
            }

            log::info!(
                "Auto-registering layout package: {}/{}",
                _cat_name,
                variant.id
            );
            ctx.model_variants
                .insert("layout".into(), variant.id.clone());
        }
    }
}
