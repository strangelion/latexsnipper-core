use log::info;
use std::collections::HashMap;
use std::sync::Mutex;

use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
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
pub enum RecognizeMode {
    Formula,
    Text,
    Mixed,
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
                        match block {
                            Block::Formula(f) => {
                                items.push(StreamItem::RegionRecognized {
                                    index: idx,
                                    text: f.formula.as_latex().to_string(),
                                    confidence: f.formula.confidence,
                                });
                            }
                            Block::Paragraph(p) => {
                                let text: String = p
                                    .inlines
                                    .iter()
                                    .filter_map(|i| {
                                        if let Inline::Text(t) = i {
                                            Some(t.text.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                if !text.is_empty() {
                                    items.push(StreamItem::RegionRecognized {
                                        index: idx,
                                        text,
                                        confidence: 1.0,
                                    });
                                }
                            }
                            _ => {}
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
        let mut blocks = Vec::new();

        if let Some(val) = ctx.get("formula_blocks") {
            if let Some(arr) = val.as_array() {
                for block_val in arr {
                    if let Ok(block) = serde_json::from_value::<Block>(block_val.clone()) {
                        blocks.push(block);
                    }
                }
            }
        }

        if let Some(val) = ctx.get("text_blocks") {
            if let Some(arr) = val.as_array() {
                for block_val in arr {
                    if let Ok(block) = serde_json::from_value::<Block>(block_val.clone()) {
                        blocks.push(block);
                    }
                }
            }
        }

        // Sort by y-coordinate (reading order)
        blocks.sort_by(|a, b| {
            let ay = match a {
                Block::Formula(f) => f.geometry.as_ref().map_or(0.0, |g| g.y),
                Block::Paragraph(p) => p.geometry.as_ref().map_or(0.0, |g| g.y),
                _ => 0.0,
            };
            let by = match b {
                Block::Formula(f) => f.geometry.as_ref().map_or(0.0, |g| g.y),
                Block::Paragraph(p) => p.geometry.as_ref().map_or(0.0, |g| g.y),
                _ => 0.0,
            };
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
}
