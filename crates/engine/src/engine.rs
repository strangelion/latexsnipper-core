use log::info;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_ast::*;
use latexsnipper_image::SnipperImage;
use latexsnipper_image::operations;
use latexsnipper_runtime::{RuntimeBackend, AccelerationMode, ModelHandle, InferenceSession};
use latexsnipper_model::ModelManager;
use latexsnipper_inference::{
    DetectionParams, RecognitionParams,
    detect_formulas, recognize_formula,
};

use crate::config::EngineConfig;
use crate::api::{RecognizeRequest, RecognizeResponse, StreamItem};
use crate::job::JobQueue;

/// The main engine that orchestrates all LaTeXSnipper capabilities.
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Box<dyn RuntimeBackend>,
    model_manager: ModelManager,
    job_queue: JobQueue,
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
        }
    }

    pub fn runtime(&self) -> &dyn RuntimeBackend { &*self.runtime }
    pub fn model_manager(&self) -> &ModelManager { &self.model_manager }
    pub fn config(&self) -> &EngineConfig { &self.config }
    pub fn job_queue(&self) -> &JobQueue { &self.job_queue }

    /// Get a mutable reference to the job queue.
    pub fn job_queue_mut(&mut self) -> &mut JobQueue { &mut self.job_queue }

    /// Recognize content using a Request object (Builder pattern).
    pub async fn recognize_with_request(&self, request: RecognizeRequest) -> Result<RecognizeResponse> {
        let start = std::time::Instant::now();
        let mode = request.mode;

        let doc = self.recognize(request.image, mode).await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let region_count = doc.block_count();

        Ok(RecognizeResponse::new(doc, mode, region_count, elapsed))
    }

    /// Recognize with streaming results (yields items as regions are processed).
    pub async fn recognize_streaming(&self, request: RecognizeRequest) -> Result<Vec<StreamItem>> {
        let start = std::time::Instant::now();
        let mut items = Vec::new();

        // Run recognition and collect block-level results
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
                                let text: String = p.inlines.iter().filter_map(|i| {
                                    if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
                                }).collect();
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
                items.push(StreamItem::Error { message: e.to_string() });
            }
        }

        Ok(items)
    }

    /// Recognize content in an image (legacy API).
    pub async fn recognize(&self, image: SnipperImage, mode: RecognizeMode) -> Result<Document> {
        info!("Recognizing image ({}, {}) in {:?} mode", image.width(), image.height(), mode);

        match mode {
            RecognizeMode::Formula => self.recognize_formula(&image).await,
            RecognizeMode::Text => self.recognize_text(&image).await,
            RecognizeMode::Mixed => self.recognize_mixed(&image).await,
        }
    }

    async fn recognize_formula(&self, image: &SnipperImage) -> Result<Document> {
        let models = &self.config.models_dir;

        // Try to load detection model; if not available, fall back to whole-image recognition
        let det_config = match load_model_config(models, "formula-det") {
            Ok(c) => c,
            Err(_) => {
                info!("Detection model not found, falling back to whole-image recognition");
                return self.recognize_formula_region(image, models).await;
            }
        };
        let det_params = DetectionParams::from_config(&det_config);
        let det_model_path = det_config.find_model_file(&models.join("formula-det/yolov8-mfd"))
            .ok_or_else(|| SnipperError::Model("Formula detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("formula-det", det_model_path);
        let det_session = self.runtime.create_session(&det_handle, AccelerationMode::Cpu)?;

        let detections = detect_formulas(image, &*det_session, &det_params)?;
        info!("Detected {} formula regions", detections.len());

        if detections.is_empty() {
            return self.recognize_formula_region(image, models).await;
        }

        let mut blocks = Vec::new();
        for det in &detections {
            let rect = &det.rect;
            let x = (rect.x.max(0.0) as u32).min(image.width().saturating_sub(1));
            let y = (rect.y.max(0.0) as u32).min(image.height().saturating_sub(1));
            let w = (rect.width as u32).min(image.width().saturating_sub(x));
            let h = (rect.height as u32).min(image.height().saturating_sub(y));

            if w < 4 || h < 4 { continue; }

            let cropped = operations::crop(image, latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32));

            match self.recognize_formula_region(&cropped, models).await {
                Ok(doc) => {
                    for page in &doc.pages {
                        for block in &page.blocks {
                            let mut block = block.clone();
                            match &mut block {
                                Block::Formula(f) => {
                                    if let Some(geo) = &mut f.geometry {
                                        geo.x += rect.x;
                                        geo.y += rect.y;
                                    }
                                }
                                Block::Paragraph(p) => {
                                    if let Some(geo) = &mut p.geometry {
                                        geo.x += rect.x;
                                        geo.y += rect.y;
                                    }
                                }
                                _ => {}
                            }
                            blocks.push(block);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to recognize region: {}", e);
                }
            }
        }

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: image.width() as f32,
                height: image.height() as f32,
                blocks,
                page_number: Some(1),
            }],
        })
    }

    /// Recognize a single formula region (whole image or cropped).
    async fn recognize_formula_region(&self, image: &SnipperImage, models: &std::path::Path) -> Result<Document> {
        let rec_config = match load_model_config(models, "formula-rec") {
            Ok(c) => c,
            Err(_) => {
                info!("Recognition model not found, returning empty document");
                return Ok(Document::new());
            }
        };
        let rec_params = RecognitionParams::default();

        let rec_dir = models.join("formula-rec/trocr-deit");
        let encoder_path = rec_config.find_encoder_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Encoder model not found".into()))?;
        let decoder_path = rec_config.find_decoder_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Decoder model not found".into()))?;
        let tokenizer_path = rec_config.find_tokenizer_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Tokenizer not found".into()))?;

        let enc_handle = ModelHandle::with_path("encoder", encoder_path);
        let dec_handle = ModelHandle::with_path("decoder", decoder_path);

        let enc_session = self.runtime.create_session(&enc_handle, AccelerationMode::Cpu)?;
        let dec_session = self.runtime.create_session(&dec_handle, AccelerationMode::Cpu)?;

        let result = recognize_formula(image, &*enc_session, &*dec_session, &tokenizer_path, &rec_params)?;

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::Latex(result.text),
                        display_mode: true,
                        confidence: result.confidence,
                    },
                    geometry: None,
                    source: Some(SourceInfo::new()),
                })],
                page_number: Some(1),
            }],
        })
    }

    async fn recognize_text(&self, image: &SnipperImage) -> Result<Document> {
        let models = &self.config.models_dir;

        // Load text detection model
        let det_config = match load_model_config(models, "text-det") {
            Ok(c) => c,
            Err(_) => {
                info!("Text detection model not found");
                return Ok(Document::new());
            }
        };
        let det_params = latexsnipper_inference::TextDetParams::default();
        let det_model_path = det_config.find_model_file(&models.join("text-det/ppocrv5-mobile"))
            .ok_or_else(|| SnipperError::Model("Text detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("text-det", det_model_path);
        let det_session = self.runtime.create_session(&det_handle, AccelerationMode::Cpu)?;

        // Detect text regions
        let detections = latexsnipper_inference::detect_text(image, &*det_session, &det_params)?;
        info!("Detected {} text regions", detections.len());

        if detections.is_empty() {
            return Ok(Document::new());
        }

        // Load text recognition model
        let rec_config = match load_model_config(models, "text-rec") {
            Ok(c) => c,
            Err(_) => {
                info!("Text recognition model not found");
                return Ok(Document::new());
            }
        };
        let rec_params = latexsnipper_inference::TextRecParams::default();
        let rec_model_path = rec_config.find_model_file(&models.join("text-rec/ppocrv5-mobile"))
            .ok_or_else(|| SnipperError::Model("Text recognition model not found".into()))?;
        let rec_handle = ModelHandle::with_path("text-rec", rec_model_path);
        let rec_session = self.runtime.create_session(&rec_handle, AccelerationMode::Cpu)?;

        // Load keys file
        let rec_dir = models.join("text-rec/ppocrv5-mobile");
        let keys_path = rec_config.find_tokenizer_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Text keys file not found".into()))?;

        // Recognize each text region
        let mut blocks = Vec::new();
        for det in &detections {
            let rect = &det.rect;
            let x = (rect.x.max(0.0) as u32).min(image.width().saturating_sub(1));
            let y = (rect.y.max(0.0) as u32).min(image.height().saturating_sub(1));
            let w = (rect.width as u32).min(image.width().saturating_sub(x));
            let h = (rect.height as u32).min(image.height().saturating_sub(y));

            if w < 4 || h < 4 { continue; }

            let cropped = operations::crop(image, latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32));

            match latexsnipper_inference::recognize_text(&cropped, &*rec_session, &keys_path, &rec_params) {
                Ok(result) => {
                    if !result.text.is_empty() {
                        blocks.push(Block::Paragraph(ParagraphBlock {
                            inlines: vec![Inline::Text(TextRun {
                                text: result.text,
                                bold: None,
                                italic: None,
                            })],
                            geometry: Some(latexsnipper_ast::Rect::new(rect.x, rect.y, rect.width, rect.height)),
                            source: Some(SourceInfo::new()),
                        }));
                    }
                }
                Err(e) => {
                    log::warn!("Failed to recognize text region: {}", e);
                }
            }
        }

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: image.width() as f32,
                height: image.height() as f32,
                blocks,
                page_number: Some(1),
            }],
        })
    }

    async fn recognize_mixed(&self, image: &SnipperImage) -> Result<Document> {
        let models = &self.config.models_dir;

        // Load formula detection model
        let det_config = match load_model_config(models, "formula-det") {
            Ok(c) => c,
            Err(_) => {
                info!("Formula detection model not found, falling back to text-only");
                return self.recognize_text(image).await;
            }
        };
        let det_params = DetectionParams::from_config(&det_config);
        let det_model_path = det_config.find_model_file(&models.join("formula-det/yolov8-mfd"))
            .ok_or_else(|| SnipperError::Model("Formula detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("formula-det", det_model_path);
        let det_session = self.runtime.create_session(&det_handle, AccelerationMode::Cpu)?;

        // Detect formula regions
        let detections = detect_formulas(image, &*det_session, &det_params)?;
        info!("Mixed: detected {} formula regions", detections.len());

        // Load formula recognition model
        let rec_config = load_model_config(models, "formula-rec").ok();
        let rec_params = RecognitionParams::default();

        let mut blocks = Vec::new();

        if let Some(rec_cfg) = rec_config {
            let rec_dir = models.join("formula-rec/trocr-deit");
            let encoder_path = rec_cfg.find_encoder_file(&rec_dir)
                .ok_or_else(|| SnipperError::Model("Encoder not found".into()))?;
            let decoder_path = rec_cfg.find_decoder_file(&rec_dir)
                .ok_or_else(|| SnipperError::Model("Decoder not found".into()))?;
            let tokenizer_path = rec_cfg.find_tokenizer_file(&rec_dir)
                .ok_or_else(|| SnipperError::Model("Tokenizer not found".into()))?;

            let enc_handle = ModelHandle::with_path("encoder", encoder_path);
            let dec_handle = ModelHandle::with_path("decoder", decoder_path);
            let enc_session = self.runtime.create_session(&enc_handle, AccelerationMode::Cpu)?;
            let dec_session = self.runtime.create_session(&dec_handle, AccelerationMode::Cpu)?;

            // Recognize each formula region
            for det in &detections {
                let rect = &det.rect;
                let x = (rect.x.max(0.0) as u32).min(image.width().saturating_sub(1));
                let y = (rect.y.max(0.0) as u32).min(image.height().saturating_sub(1));
                let w = (rect.width as u32).min(image.width().saturating_sub(x));
                let h = (rect.height as u32).min(image.height().saturating_sub(y));

                if w < 4 || h < 4 { continue; }

                let cropped = operations::crop(image, latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32));

                match recognize_formula(&cropped, &*enc_session, &*dec_session, &tokenizer_path, &rec_params) {
                    Ok(result) => {
                        blocks.push(Block::Formula(FormulaBlock {
                            formula: Formula {
                                source: FormulaSource::Latex(result.text),
                                display_mode: true,
                                confidence: result.confidence,
                            },
                            geometry: Some(latexsnipper_ast::Rect::new(rect.x, rect.y, rect.width, rect.height)),
                            source: Some(SourceInfo::new()),
                        }));
                    }
                    Err(e) => {
                        log::warn!("Failed to recognize formula: {}", e);
                    }
                }
            }
        }

        Ok(Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: image.width() as f32,
                height: image.height() as f32,
                blocks,
                page_number: Some(1),
            }],
        })
    }
}

/// Load ModelConfig from a model category directory.
fn load_model_config(models_dir: &std::path::Path, category: &str) -> Result<latexsnipper_model::ModelConfig> {
    let cat_dir = models_dir.join(category);
    // Find the first subdirectory (variant)
    let variant_dir = std::fs::read_dir(&cat_dir)
        .map_err(|e| SnipperError::Model(format!("Cannot read {}: {}", cat_dir.display(), e)))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .ok_or_else(|| SnipperError::Model(format!("No variant found in {}", cat_dir.display())))?;

    latexsnipper_model::ModelConfig::load(&variant_dir.path())
}
