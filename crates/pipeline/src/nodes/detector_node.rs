use async_trait::async_trait;
use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_runtime::{RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_inference::{DetectionParams, TextDetParams, detect_formulas, detect_text, group_formula_detections, filter_formula_detections};

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Detects regions (formulas or text) in the image.
/// Loads models, runs detection, stores results in context metadata.
pub struct DetectorNode {
    name: String,
    detector_type: DetectorType,
}

pub enum DetectorType {
    Formula,
    Text,
}

impl DetectorNode {
    pub fn formula() -> Self {
        Self { name: "detect_formula".into(), detector_type: DetectorType::Formula }
    }

    pub fn text() -> Self {
        Self { name: "detect_text".into(), detector_type: DetectorType::Text }
    }
}

#[async_trait]
impl PipelineNode for DetectorNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        match &self.detector_type {
            DetectorType::Formula => self.detect_formulas(ctx, &image, &models).await,
            DetectorType::Text => self.detect_texts(ctx, &image, &models).await,
        }
    }
}

impl DetectorNode {
    async fn detect_formulas(&self, ctx: &mut PipelineContext, image: &latexsnipper_image::SnipperImage, models: &std::path::Path) -> Result<()> {
        let det_config = match load_config(models, "formula-det") {
            Ok(c) => c,
            Err(_) => { log::warn!("Formula det model not found"); return Ok(()); }
        };

        let det_params = DetectionParams::from_config(&det_config);
        let det_model_path = det_config.find_model_file(&models.join("formula-det/yolov8-mfd"))
            .ok_or_else(|| SnipperError::Model("Formula detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("formula-det", det_model_path);

        let runtime = latexsnipper_runtime::StubRuntime::new();
        let session = if let Some(s) = ctx.get_session("formula_det") {
            s
        } else {
            let s = runtime.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("formula_det", s);
            ctx.get_session("formula_det").unwrap()
        };

        let mut detections = detect_formulas(image, &*session, &det_params)?;
        
        // Group nearby detections into complete formulas (like LaTeXSnipper)
        group_formula_detections(&mut detections);
        
        // Filter by minimum area and confidence
        filter_formula_detections(&mut detections, 100.0, 0.2);
        
        let count = detections.len();
        log::info!("Pipeline: detect_formula found {} regions after grouping", count);

        let detections_json: Vec<serde_json::Value> = detections.iter().map(|d| {
            serde_json::json!({
                "rect": {
                    "x": d.rect.x,
                    "y": d.rect.y,
                    "w": d.rect.width,
                    "h": d.rect.height
                },
                "confidence": d.confidence,
                "class_id": d.class_id,
                "class_name": d.class_name
            })
        }).collect();

        ctx.set("formula_detections", serde_json::json!(detections_json));
        Ok(())
    }

    async fn detect_texts(&self, ctx: &mut PipelineContext, image: &latexsnipper_image::SnipperImage, models: &std::path::Path) -> Result<()> {
        let det_config = match load_config(models, "text-det") {
            Ok(c) => c,
            Err(_) => { log::warn!("Text det model not found"); return Ok(()); }
        };

        let det_params = TextDetParams::default();

        // Try v6 models first, then fallback to v5
        let det_model_path = find_text_det_model(models, &det_config)
            .ok_or_else(|| SnipperError::Model("Text detection model not found".into()))?;

        let det_handle = ModelHandle::with_path("text-det", det_model_path);
        let runtime = latexsnipper_runtime::StubRuntime::new();
        let session = if let Some(s) = ctx.get_session("text_det") {
            s
        } else {
            let s = runtime.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("text_det", s);
            ctx.get_session("text_det").unwrap()
        };

        let detections = detect_text(image, &*session, &det_params)?;
        let count = detections.len();
        log::info!("Pipeline: detect_text found {} regions", count);

        let detections_json: Vec<serde_json::Value> = detections.iter().map(|d| {
            serde_json::json!({
                "rect": {
                    "x": d.rect.x,
                    "y": d.rect.y,
                    "w": d.rect.width,
                    "h": d.rect.height
                },
                "confidence": d.confidence,
                "class_id": d.class_id,
                "class_name": d.class_name
            })
        }).collect();

        ctx.set("text_detections", serde_json::json!(detections_json));
        Ok(())
    }
}

/// Find text detection model, trying v6 variants first.
fn find_text_det_model(models: &std::path::Path, config: &latexsnipper_model::ModelConfig) -> Option<std::path::PathBuf> {
    let candidates = [
        models.join("v6_models/PP-OCRv6_medium_det_infer"),
        models.join("v6_models/PP-OCRv6_small_det_infer"),
        models.join("text-det/ppocrv5-mobile"),
    ];

    for dir in &candidates {
        if !dir.is_dir() { continue; }
        if let Some(path) = config.find_model_file(dir) {
            log::info!("Using text det model: {}", path.display());
            return Some(path);
        }
    }
    None
}

fn load_config(models: &std::path::Path, category: &str) -> Result<latexsnipper_model::ModelConfig> {
    let cat_dir = models.join(category);
    let variant_dir = std::fs::read_dir(&cat_dir)
        .map_err(|e| SnipperError::Model(format!("Cannot read {}: {}", cat_dir.display(), e)))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .ok_or_else(|| SnipperError::Model(format!("No variant in {}", cat_dir.display())))?;
    latexsnipper_model::ModelConfig::load(&variant_dir.path())
}
