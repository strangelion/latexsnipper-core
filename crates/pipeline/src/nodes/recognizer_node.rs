use async_trait::async_trait;
use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_ast::*;
use latexsnipper_image::operations;
use latexsnipper_runtime::{RuntimeBackend, AccelerationMode, ModelHandle, OnnxRuntimeBackend};
use latexsnipper_inference::{RecognitionParams, TextRecParams, recognize_formula, recognize_text_with_keys, load_keys};
use latexsnipper_inference::formula_lines::split_formula_line_groups;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

struct TextRecModel {
    config: latexsnipper_model::ModelConfig,
    model_path: std::path::PathBuf,
    keys_path: std::path::PathBuf,
}

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

    fn create_backend(models: &std::path::Path) -> Result<OnnxRuntimeBackend> {
        OnnxRuntimeBackend::new(models.to_path_buf())
            .map_err(|e| SnipperError::Runtime(format!("Failed to create ONNX backend: {}", e)))
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

        let backend = Self::create_backend(models)?;
        let enc_handle = ModelHandle::with_path("encoder", encoder_path);
        let dec_handle = ModelHandle::with_path("decoder", decoder_path);

        let enc_session = if let Some(s) = ctx.get_session("formula_encoder") {
            s
        } else {
            let s = backend.create_session(&enc_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("formula_encoder", s);
            ctx.get_session("formula_encoder").unwrap()
        };
        let dec_session = if let Some(s) = ctx.get_session("formula_decoder") {
            s
        } else {
            let s = backend.create_session(&dec_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("formula_decoder", s);
            ctx.get_session("formula_decoder").unwrap()
        };

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
                        let cropped = operations::crop(image, Rect::new(x as f32, y as f32, w as f32, h as f32));
                        let line_groups = split_formula_line_groups(&cropped);

                        if line_groups.is_empty() {
                            match recognize_formula(&cropped, &*enc_session, &*dec_session, &tokenizer_path, &params) {
                                Ok(result) => {
                                    let mut f = Formula::latex(result.text);
                                    f.confidence = result.confidence;
                                    blocks.push(Block::Formula(FormulaBlock {
                                        formula: f,
                                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                                        source: Some(SourceInfo::new()),
                                    }));
                                }
                                Err(e) => log::warn!("Formula rec failed: {}", e),
                            }
                        } else {
                            let mut all_results = Vec::new();
                            for group in &line_groups {
                                for crop in &group.crops {
                                    let crop_img = latexsnipper_image::SnipperImage::new(
                                        crop.width,
                                        crop.height,
                                        latexsnipper_image::color::PixelFormat::Rgb,
                                        crop.pixels.clone(),
                                    );
                                    match recognize_formula(&crop_img, &*enc_session, &*dec_session, &tokenizer_path, &params) {
                                        Ok(result) => all_results.push(result.text),
                                        Err(e) => log::warn!("Formula line rec failed: {}", e),
                                    }
                                }
                            }

                            if !all_results.is_empty() {
                                let merged = all_results.join(" ");
                                let mut f = Formula::latex(merged);
                                f.confidence = 0.9;
                                blocks.push(Block::Formula(FormulaBlock {
                                    formula: f,
                                    geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                                    source: Some(SourceInfo::new()),
                                }));
                            }
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

        let rec_model = match select_text_rec_model(models) {
            Ok(m) => m,
            Err(e) => { log::warn!("Text rec model not found: {}", e); return Ok(()); }
        };

        let backend = Self::create_backend(models)?;
        let handle = ModelHandle::with_path("text-rec", rec_model.model_path);

        let session = if let Some(s) = ctx.get_session("text_rec") {
            s
        } else {
            let s = backend.create_session(&handle, AccelerationMode::Cpu)?;
            ctx.cache_session("text_rec", s);
            ctx.get_session("text_rec").unwrap()
        };

        let params = TextRecParams::from_config(&rec_model.config);
        let (keys, first_char_id) = if let Some(chars) = session.get_character_list() {
            (chars, 0)
        } else {
            load_keys(&rec_model.keys_path).unwrap_or((Vec::new(), 1))
        };

        let mut blocks = Vec::new();

        for crop_val in &crop_array {
            if let Some(rect_val) = crop_val.get("rect") {
                let x = rect_val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let y = rect_val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let w = rect_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let h = rect_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;

                if let Some(ref image) = ctx.image {
                    if w >= 4 && h >= 4 {
                        let pad_y = (h as f32 * 0.2).max(4.0) as u32;
                        let crop_y = y.saturating_sub(pad_y);
                        let crop_h = h + pad_y * 2;
                        let crop_y_end = (crop_y + crop_h).min(image.height());
                        let final_h = crop_y_end - crop_y;
                        let cropped = operations::crop(image, Rect::new(x as f32, crop_y as f32, w as f32, final_h as f32));
                        match recognize_text_with_keys(&cropped, &*session, &keys, first_char_id, &params) {
                            Ok(result) => {
                                if !result.text.is_empty() {
                                    blocks.push(Block::Paragraph(ParagraphBlock {
                                        inlines: vec![Inline::Text(TextRun::new(result.text))],
                                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
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

fn select_text_rec_model(models: &std::path::Path) -> Result<TextRecModel> {
    let candidates = [
        models.join("v6_models/PP-OCRv6_small_rec_infer"),
        models.join("v6_models/PP-OCRv6_medium_rec_infer"),
        models.join("text-rec/ppocrv5-mobile"),
    ];

    let mut unsupported = Vec::new();
    for dir in candidates {
        if !dir.is_dir() { continue; }

        let config = match if dir.join("config.json").exists() {
            latexsnipper_model::ModelConfig::load(&dir)
        } else {
            latexsnipper_model::ModelConfig::from_paddle_inference_dir(&dir)
        } {
            Ok(config) => config,
            Err(e) => { unsupported.push(format!("{} cannot be parsed: {}", dir.display(), e)); continue; }
        };

        let Some(model_path) = config.find_model_file(&dir) else {
            unsupported.push(format!("{} has no ONNX model", dir.display()));
            continue;
        };

        let keys_path = config.find_tokenizer_file(&dir)
            .ok_or_else(|| SnipperError::Model(format!("Text keys not found in {}", dir.display())))?;

        return Ok(TextRecModel { config, model_path, keys_path });
    }

    if unsupported.is_empty() {
        Err(SnipperError::Model("No text recognition model directory found".into()))
    } else {
        Err(SnipperError::Model(format!("No supported text recognition model found ({})", unsupported.join("; "))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_models_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("latexsnipper-{}-{}", name, stamp))
    }

    fn write_text_rec_config(dir: &std::path::Path) {
        let config = r#"{
            "model_type": "crnn_ctc",
            "input": {"name": "x", "shape": [1,3,48,320], "dtype": "float32"},
            "output": {"name": "out", "shape": [1,-1,8]},
            "preprocessing": {
                "resize": {"height": 48, "width": 320, "keep_ratio": true, "pad_value": 0},
                "normalization": {"mean": [0.5,0.5,0.5], "std": [0.5,0.5,0.5]},
                "color_format": "RGB"
            },
            "decoding": {"type": "ctc_greedy", "blank_id": 0, "keys_file": "ppocr_keys.txt"}
        }"#;
        fs::write(dir.join("config.json"), config).unwrap();
    }

    #[test]
    fn select_text_rec_prefers_v6_when_onnx_exists() {
        let root = temp_models_dir("text-rec-v6");
        let v6 = root.join("v6_models/PP-OCRv6_medium_rec_infer");
        let old = root.join("text-rec/ppocrv5-mobile");
        fs::create_dir_all(&v6).unwrap();
        fs::create_dir_all(&old).unwrap();
        write_text_rec_config(&v6);
        write_text_rec_config(&old);
        fs::write(v6.join("model.onnx"), []).unwrap();
        fs::write(v6.join("ppocr_keys.txt"), "a\nb\n").unwrap();
        fs::write(old.join("ppocrv5_mobile_rec.onnx"), []).unwrap();
        fs::write(old.join("ppocrv5_keys.txt"), "x\ny\n").unwrap();

        let selected = select_text_rec_model(&root).unwrap();
        assert!(selected.model_path.ends_with("model.onnx"));
        assert!(selected.keys_path.starts_with(&v6));
    }

    #[test]
    fn select_text_rec_falls_back_when_v6_has_no_onnx() {
        let root = temp_models_dir("text-rec-fallback");
        let v6 = root.join("v6_models/PP-OCRv6_medium_rec_infer");
        let old = root.join("text-rec/ppocrv5-mobile");
        fs::create_dir_all(&v6).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(v6.join("inference.json"), "{}").unwrap();
        fs::write(v6.join("inference.pdiparams"), []).unwrap();
        write_text_rec_config(&old);
        fs::write(old.join("ppocrv5_mobile_rec.onnx"), []).unwrap();
        fs::write(old.join("ppocrv5_keys.txt"), "x\ny\n").unwrap();

        let selected = select_text_rec_model(&root).unwrap();
        assert!(selected.model_path.ends_with("ppocrv5_mobile_rec.onnx"));
        assert!(selected.keys_path.starts_with(&old));
    }
}
