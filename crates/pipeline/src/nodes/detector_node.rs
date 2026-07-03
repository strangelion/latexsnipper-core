use async_trait::async_trait;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_inference::{
    detect_formulas, detect_handwriting, detect_tables, detect_text, filter_formula_detections,
    filter_handwriting_detections, filter_table_detections, group_formula_detections,
    DetectionParams, HandwritingDetParams, TableDetParams, TextDetParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend};
use std::sync::Arc;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Detects regions (formulas, text, handwriting, or tables) in the image.
/// Loads models, runs detection, stores results in context metadata.
pub struct DetectorNode {
    name: String,
    detector_type: DetectorType,
}

pub enum DetectorType {
    Formula,
    Text,
    Handwriting,
    Table,
}

impl DetectorNode {
    pub fn formula() -> Self {
        Self {
            name: "detect_formula".into(),
            detector_type: DetectorType::Formula,
        }
    }

    pub fn text() -> Self {
        Self {
            name: "detect_text".into(),
            detector_type: DetectorType::Text,
        }
    }

    pub fn handwriting() -> Self {
        Self {
            name: "detect_handwriting".into(),
            detector_type: DetectorType::Handwriting,
        }
    }

    pub fn table() -> Self {
        Self {
            name: "detect_table".into(),
            detector_type: DetectorType::Table,
        }
    }
}

#[async_trait]
impl PipelineNode for DetectorNode {
    fn name(&self) -> &str {
        &self.name
    }

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
            DetectorType::Handwriting => self.detect_handwriting(ctx, &image, &models).await,
            DetectorType::Table => self.detect_tables(ctx, &image, &models).await,
        }
    }
}

impl DetectorNode {
    fn get_backend(ctx: &PipelineContext) -> Result<Arc<dyn RuntimeBackend>> {
        ctx.backend
            .clone()
            .ok_or_else(|| SnipperError::Runtime("No backend configured".into()))
    }

    async fn detect_formulas(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(models, "formula-det") {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Formula det model not found");
                return Ok(());
            }
        };

        let det_params = DetectionParams::from_config(&det_config);
        let det_model_path = det_config
            .find_model_file(&models.join("formula-det/yolov8-mfd"))
            .ok_or_else(|| SnipperError::Model("Formula detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("formula-det", det_model_path);

        // Use backend from context (injected by engine)
        let backend = Self::get_backend(ctx)?;
        let session = if let Some(s) = ctx.get_session("formula_det") {
            s
        } else {
            let s = backend.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("formula_det", s);
            ctx.get_session("formula_det").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache formula detection session".into())
            })?
        };

        let mut detections = detect_formulas(image, &*session, &det_params)?;

        // Group nearby detections into complete formulas (like LaTeXSnipper)
        group_formula_detections(&mut detections);

        // Filter by minimum area and confidence
        filter_formula_detections(&mut detections, 100.0, 0.2);

        let count = detections.len();
        log::info!(
            "Pipeline: detect_formula found {} regions after grouping",
            count
        );

        let detections_json: Vec<serde_json::Value> = detections
            .iter()
            .map(|d| {
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
            })
            .collect();

        ctx.set("formula_detections", serde_json::json!(detections_json));
        Ok(())
    }

    async fn detect_texts(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(models, "text-det") {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Text det model not found");
                return Ok(());
            }
        };

        let det_params = TextDetParams::default();

        // Try v6 models first, then fallback to v5
        let det_model_path = find_text_det_model(models, &det_config)
            .ok_or_else(|| SnipperError::Model("Text detection model not found".into()))?;

        let det_handle = ModelHandle::with_path("text-det", det_model_path);

        // Use backend from context
        let backend = Self::get_backend(ctx)?;
        let session = if let Some(s) = ctx.get_session("text_det") {
            s
        } else {
            let s = backend.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("text_det", s);
            ctx.get_session("text_det").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache text detection session".into())
            })?
        };

        let detections = detect_text(image, &*session, &det_params)?;
        let count = detections.len();
        log::info!("Pipeline: detect_text found {} regions", count);

        let detections_json: Vec<serde_json::Value> = detections
            .iter()
            .map(|d| {
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
            })
            .collect();

        ctx.set("text_detections", serde_json::json!(detections_json));
        Ok(())
    }

    async fn detect_handwriting(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(models, "handwriting-det") {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Handwriting det model not found");
                return Ok(());
            }
        };

        let det_params = HandwritingDetParams::from_config(&det_config);
        let det_model_path = det_config
            .find_model_file(&models.join("handwriting-det"))
            .ok_or_else(|| SnipperError::Model("Handwriting detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("handwriting-det", det_model_path);

        let backend = Self::get_backend(ctx)?;
        let session = if let Some(s) = ctx.get_session("handwriting_det") {
            s
        } else {
            let s = backend.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("handwriting_det", s);
            ctx.get_session("handwriting_det").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache handwriting detection session".into())
            })?
        };

        let mut detections = detect_handwriting(image, &*session, &det_params)?;

        // Filter by minimum area and confidence
        filter_handwriting_detections(&mut detections, 100.0, 0.2);

        let count = detections.len();
        log::info!("Pipeline: detect_handwriting found {} regions", count);

        let detections_json: Vec<serde_json::Value> = detections
            .iter()
            .map(|d| {
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
            })
            .collect();

        ctx.set("handwriting_detections", serde_json::json!(detections_json));
        Ok(())
    }

    async fn detect_tables(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(models, "table-det") {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Table det model not found");
                return Ok(());
            }
        };

        let det_params = TableDetParams::from_config(&det_config);
        let det_model_path = det_config
            .find_model_file(&models.join("table-det"))
            .ok_or_else(|| SnipperError::Model("Table detection model not found".into()))?;
        let det_handle = ModelHandle::with_path("table-det", det_model_path);

        let backend = Self::get_backend(ctx)?;
        let session = if let Some(s) = ctx.get_session("table_det") {
            s
        } else {
            let s = backend.create_session(&det_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("table_det", s);
            ctx.get_session("table_det").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache table detection session".into())
            })?
        };

        let mut detections = detect_tables(image, &*session, &det_params)?;

        // Filter by minimum area and confidence
        filter_table_detections(&mut detections, 400.0, 0.3);

        let count = detections.len();
        log::info!("Pipeline: detect_table found {} regions", count);

        let detections_json: Vec<serde_json::Value> = detections
            .iter()
            .map(|d| {
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
            })
            .collect();

        ctx.set("table_detections", serde_json::json!(detections_json));
        Ok(())
    }
}

/// Find text detection model, trying v6 variants first.
fn find_text_det_model(
    models: &std::path::Path,
    config: &latexsnipper_model::ModelConfig,
) -> Option<std::path::PathBuf> {
    let candidates = [
        models.join("v6_models/PP-OCRv6_medium_det_infer"),
        models.join("v6_models/PP-OCRv6_small_det_infer"),
        models.join("text-det/ppocrv5-mobile"),
    ];

    for dir in &candidates {
        if !dir.is_dir() {
            continue;
        }
        if let Some(path) = config.find_model_file(dir) {
            log::info!("Using text det model: {}", path.display());
            return Some(path);
        }
    }
    None
}

fn load_config(
    models: &std::path::Path,
    category: &str,
) -> Result<latexsnipper_model::ModelConfig> {
    let cat_dir = models.join(category);
    let variant_dir = std::fs::read_dir(&cat_dir)
        .map_err(|e| SnipperError::Model(format!("Cannot read {}: {}", cat_dir.display(), e)))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .ok_or_else(|| SnipperError::Model(format!("No variant in {}", cat_dir.display())))?;
    latexsnipper_model::ModelConfig::load(&variant_dir.path())
}
