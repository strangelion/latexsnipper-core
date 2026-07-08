use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
use latexsnipper_image::pdf::{decode_pdf, PdfSource};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelManager;
use latexsnipper_pipeline::{DocumentParseMode, PipelineContext, PipelineGraph};
use latexsnipper_runtime::{
    FsModelResolver, ModelPackage, ModelTask, RuntimeBackend, SharedModelResolver,
};

use crate::config::EngineConfig;
use crate::job::JobQueue;

pub use latexsnipper_api_types::{RecognizeMode, RecognizeRequest, RecognizeResponse, StreamItem};

/// The main engine that orchestrates all LaTeXSnipper capabilities.
/// Engine only assembles PipelineGraph and runs it — all logic lives in Nodes.
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Arc<dyn RuntimeBackend>,
    model_resolver: Option<SharedModelResolver>,
    model_manager: ModelManager,
    job_queue: JobQueue,
    /// Registered model packages for type-safe inference.
    model_packages: HashMap<ModelTask, Arc<dyn ModelPackage>>,
}

impl SnipperEngine {
    /// Create a new engine with the given config and runtime backend.
    pub fn new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Self {
        let model_manager = ModelManager::new(config.models_dir.clone());
        let model_resolver: Option<SharedModelResolver> =
            Some(Arc::new(FsModelResolver::new(config.models_dir.clone())));
        Self {
            config,
            runtime: Arc::from(runtime),
            model_resolver,
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: HashMap::new(),
        }
    }

    /// Create with a custom model resolver.
    pub fn with_model_resolver(
        config: EngineConfig,
        runtime: Box<dyn RuntimeBackend>,
        resolver: SharedModelResolver,
    ) -> Self {
        let model_manager = ModelManager::new(config.models_dir.clone());
        Self {
            config,
            runtime: Arc::from(runtime),
            model_resolver: Some(resolver),
            model_manager,
            job_queue: JobQueue::new(),
            model_packages: HashMap::new(),
        }
    }

    pub fn runtime(&self) -> &dyn RuntimeBackend {
        &*self.runtime
    }
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
    // Model Hot-Reload API
    // ========================================================================

    /// Reload a specific model by clearing all cached sessions.
    /// Next inference call will create fresh sessions with the new model files.
    pub fn reload_model(&self, session_key: &str) -> Result<()> {
        info!("Reloading model: {}", session_key);
        self.runtime.clear_sessions();
        Ok(())
    }

    /// Reload all cached sessions, forcing fresh model loads on next inference.
    pub fn reload_all_models(&self) -> Result<()> {
        info!("Reloading all models");
        self.runtime.clear_sessions();
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
            diagnostics: Vec::new(),
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
    pub async fn recognize_pdf(&self, pdf_path: &Path, mode: RecognizeMode) -> Result<Document> {
        info!("Recognizing PDF {:?} in {:?} mode", pdf_path, mode);

        let pages = decode_pdf(PdfSource::File(pdf_path), 300)
            .map_err(|e| latexsnipper_foundation::SnipperError::Image(e.to_string()))?;

        info!("PDF loaded: {} pages", pages.len());

        let graph = self.build_pipeline(mode);
        let mut doc_pages = Vec::new();

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
            diagnostics: Vec::new(),
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
        ctx.backend = Some(self.runtime.clone());
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
