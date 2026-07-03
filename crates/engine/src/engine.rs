use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
use latexsnipper_image::pdf::{decode_pdf, PdfSource};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelManager;
use latexsnipper_pipeline::{PipelineContext, PipelineGraph};
use latexsnipper_runtime::RuntimeBackend;

use crate::api::{RecognizeRequest, RecognizeResponse, StreamItem};
use crate::config::EngineConfig;
use crate::job::JobQueue;

/// Cached session wrapper.
struct CachedSession {
    _session: Box<dyn latexsnipper_runtime::InferenceSession>,
}

/// The main engine that orchestrates all LaTeXSnipper capabilities.
/// Engine only assembles PipelineGraph and runs it — all logic lives in Nodes.
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Box<dyn RuntimeBackend>,
    model_manager: ModelManager,
    job_queue: JobQueue,
    _sessions: Mutex<HashMap<String, CachedSession>>,
}

/// Recognition mode.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum RecognizeMode {
    Formula,
    Text,
    Mixed,
    Handwriting,
    Table,
    FormulaLayout,
}

impl SnipperEngine {
    /// Create a new engine with the given config and runtime backend.
    pub fn new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Self {
        let model_manager = ModelManager::new(config.models_dir.clone());
        Self {
            config,
            runtime,
            model_manager,
            job_queue: JobQueue::new(),
            _sessions: Mutex::new(HashMap::new()),
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

    /// Build a PipelineGraph for the given recognition mode.
    pub fn build_pipeline(&self, mode: RecognizeMode) -> PipelineGraph {
        let mut graph = PipelineGraph::new(format!("{:?}_pipeline", mode));

        match mode {
            RecognizeMode::Formula => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::CropNode::default()));
                graph.add_node(Box::new(latexsnipper_pipeline::RecognizerNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
            }
            RecognizeMode::Text => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::text()));
                graph.add_node(Box::new(latexsnipper_pipeline::CropNode::default()));
                graph.add_node(Box::new(latexsnipper_pipeline::RecognizerNode::text()));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
            }
            RecognizeMode::Mixed => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::text()));
                graph.add_node(Box::new(latexsnipper_pipeline::CropNode::default()));
                graph.add_node(Box::new(latexsnipper_pipeline::RecognizerNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::RecognizerNode::text()));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
            }
            RecognizeMode::Handwriting => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::handwriting()));
                graph.add_node(Box::new(latexsnipper_pipeline::CropNode::default()));
                graph.add_node(Box::new(
                    latexsnipper_pipeline::HandwritingRecognizerNode::new(),
                ));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
            }
            RecognizeMode::Table => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::table()));
                graph.add_node(Box::new(latexsnipper_pipeline::TableStructureNode::new()));
                graph.add_node(Box::new(latexsnipper_pipeline::TableRecognizerNode::new()));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
            }
            RecognizeMode::FormulaLayout => {
                graph.add_node(Box::new(latexsnipper_pipeline::DetectorNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::CropNode::default()));
                graph.add_node(Box::new(latexsnipper_pipeline::RecognizerNode::formula()));
                graph.add_node(Box::new(latexsnipper_pipeline::FormulaLayoutNode::new()));
                graph.add_node(Box::new(latexsnipper_pipeline::PostprocessNode::new()));
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
                                    for cell in row {
                                        for inline in &cell.inlines {
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
                                        .inlines
                                        .iter()
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
        let mut ctx = PipelineContext::with_image(image);
        ctx.models_dir = Some(self.config.models_dir.clone());

        graph.run(&mut ctx).await?;

        // Extract document from context metadata
        let mut blocks = Self::collect_blocks_from_context(&ctx);

        // Sort by y-coordinate (reading order)
        blocks.sort_by(|a, b| {
            let ay = a.geometry().map_or(0.0, |g| g.y);
            let by = b.geometry().map_or(0.0, |g| g.y);
            ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: ctx.image.as_ref().map_or(0.0, |i| i.width() as f32),
                height: ctx.image.as_ref().map_or(0.0, |i| i.height() as f32),
                blocks,
                page_number: Some(1),
            }],
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        })
    }

    /// Recognize content in a PDF file — Multi-page support.
    ///
    /// Each page is processed independently through the pipeline.
    ///
    /// # Note
    ///
    /// **PDF page rendering is not yet implemented.** Calling `decode_pdf` will
    /// return an error (`SnipperError::Image`) until a PDF renderer (pdfium/poppler)
    /// is integrated. Convert PDF pages to images externally (e.g. pdftoppm, pdfium)
    /// and process each page individually.
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

            let mut ctx = PipelineContext::with_image(page_img.clone());
            ctx.models_dir = Some(self.config.models_dir.clone());

            graph.run(&mut ctx).await?;

            // Collect blocks for this page (all block types)
            let mut blocks = Self::collect_blocks_from_context(&ctx);

            // Sort by geometry (y-coordinate for reading order)
            blocks.sort_by(|a, b| {
                let ay = a.geometry().map_or(0.0, |g| g.y);
                let by = b.geometry().map_or(0.0, |g| g.y);
                ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
            });

            doc_pages.push(Page {
                width: page_img.width() as f32,
                height: page_img.height() as f32,
                blocks,
                page_number: Some((page_idx + 1) as u32),
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
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        })
    }

    /// Collect blocks from all known metadata keys in the pipeline context.
    ///
    /// Handles formula_blocks, text_blocks, handwriting_blocks, and table_blocks.
    /// Used by both `recognize` and `recognize_pdf` to avoid duplication.
    fn collect_blocks_from_context(ctx: &PipelineContext) -> Vec<Block> {
        let mut blocks = Vec::new();

        for key in &[
            "formula_blocks",
            "text_blocks",
            "handwriting_blocks",
            "table_blocks",
        ] {
            if let Some(val) = ctx.get(key) {
                if let Some(arr) = val.as_array() {
                    for block_val in arr {
                        if let Ok(block) = serde_json::from_value::<Block>(block_val.clone()) {
                            blocks.push(block);
                        }
                    }
                }
            }
        }

        blocks
    }
}
