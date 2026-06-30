use async_trait::async_trait;
use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_ast::*;
use latexsnipper_image::operations;
use latexsnipper_runtime::{RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_model::ModelManager;
use latexsnipper_inference::{RecognitionParams, TextRecParams, recognize_formula, recognize_text};

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Recognizes content in cropped regions stored in context metadata.
pub struct RecognizerNode {
    name: String,
    recognizer_type: RecognizerType,
}

pub enum RecognizerType {
    Formula,
    Text,
}

impl RecognizerNode {
    pub fn formula() -> Self {
        Self { name: "recognize_formula".into(), recognizer_type: RecognizerType::Formula }
    }

    pub fn text() -> Self {
        Self { name: "recognize_text".into(), recognizer_type: RecognizerType::Text }
    }
}

#[async_trait]
impl PipelineNode for RecognizerNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        match &self.recognizer_type {
            RecognizerType::Formula => self.recognize_formulas(ctx, &models).await,
            RecognizerType::Text => self.recognize_texts(ctx, &models).await,
        }
    }
}

impl RecognizerNode {
    async fn recognize_formulas(&self, ctx: &mut PipelineContext, models: &std::path::Path) -> Result<()> {
        let crop_key = "formula_crops";
        let crops = match ctx.get(crop_key) {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let crop_array = match crops.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if crop_array.is_empty() { return Ok(()); }

        let rec_config = match load_config(models, "formula-rec") {
            Ok(c) => c,
            Err(_) => { log::warn!("Formula rec model not found"); return Ok(()); }
        };

        let rec_dir = models.join("formula-rec/trocr-deit");
        let encoder_path = rec_config.find_encoder_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Encoder not found".into()))?;
        let decoder_path = rec_config.find_decoder_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Decoder not found".into()))?;
        let tokenizer_path = rec_config.find_tokenizer_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Tokenizer not found".into()))?;

        // Create runtime and sessions
        let runtime = latexsnipper_runtime::StubRuntime::new();
        let enc_handle = ModelHandle::with_path("encoder", encoder_path);
        let dec_handle = ModelHandle::with_path("decoder", decoder_path);
        let enc_session = runtime.create_session(&enc_handle, AccelerationMode::Cpu)?;
        let dec_session = runtime.create_session(&dec_handle, AccelerationMode::Cpu)?;

        let params = RecognitionParams::default();
        let mut blocks = Vec::new();

        for crop_val in &crop_array {
            if let Some(rect_val) = crop_val.get("rect") {
                let x = rect_val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let y = rect_val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let w = rect_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let h = rect_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;

                if let Some(ref image) = ctx.image {
                    if w >= 4 && h >= 4 {
                        let cropped = operations::crop(image, latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32));
                        match recognize_formula(&cropped, &*enc_session, &*dec_session, &tokenizer_path, &params) {
                            Ok(result) => {
                                blocks.push(Block::Formula(FormulaBlock {
                                    formula: Formula {
                                        source: FormulaSource::Latex(result.text),
                                        display_mode: true,
                                        confidence: result.confidence,
                                    },
                                    geometry: Some(latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32)),
                                    source: Some(SourceInfo::new()),
                                }));
                            }
                            Err(e) => log::warn!("Formula rec failed: {}", e),
                        }
                    }
                }
            }
        }

        ctx.set("formula_blocks", serde_json::to_value(&blocks).unwrap_or_default());
        log::info!("Recognized {} formula blocks", blocks.len());
        Ok(())
    }

    async fn recognize_texts(&self, ctx: &mut PipelineContext, models: &std::path::Path) -> Result<()> {
        let crop_key = "text_crops";
        let crops = match ctx.get(crop_key) {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let crop_array = match crops.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if crop_array.is_empty() { return Ok(()); }

        let rec_config = match load_config(models, "text-rec") {
            Ok(c) => c,
            Err(_) => { log::warn!("Text rec model not found"); return Ok(()); }
        };

        let rec_dir = models.join("text-rec/ppocrv5-mobile");
        let model_path = rec_config.find_model_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Text rec model not found".into()))?;
        let keys_path = rec_config.find_tokenizer_file(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Text keys not found".into()))?;

        let runtime = latexsnipper_runtime::StubRuntime::new();
        let handle = ModelHandle::with_path("text-rec", model_path);
        let session = runtime.create_session(&handle, AccelerationMode::Cpu)?;

        let params = TextRecParams::default();
        let mut blocks = Vec::new();

        for crop_val in &crop_array {
            if let Some(rect_val) = crop_val.get("rect") {
                let x = rect_val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let y = rect_val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let w = rect_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let h = rect_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;

                if let Some(ref image) = ctx.image {
                    if w >= 4 && h >= 4 {
                        let cropped = operations::crop(image, latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32));
                        match recognize_text(&cropped, &*session, &keys_path, &params) {
                            Ok(result) => {
                                if !result.text.is_empty() {
                                    blocks.push(Block::Paragraph(ParagraphBlock {
                                        inlines: vec![Inline::Text(TextRun {
                                            text: result.text,
                                            bold: None,
                                            italic: None,
                                        })],
                                        geometry: Some(latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32)),
                                        source: Some(SourceInfo::new()),
                                    }));
                                }
                            }
                            Err(e) => log::warn!("Text rec failed: {}", e),
                        }
                    }
                }
            }
        }

        ctx.set("text_blocks", serde_json::to_value(&blocks).unwrap_or_default());
        log::info!("Recognized {} text blocks", blocks.len());
        Ok(())
    }
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
