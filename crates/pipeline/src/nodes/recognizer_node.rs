use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::operations;
use latexsnipper_inference::formula_lines::{plan_formula_segmentation, FormulaSegmentationClass};
use latexsnipper_inference::{
    load_keys, load_tokenizer_from_str, recognize_formula, recognize_formula_with_tokenizer,
    recognize_text_with_keys, DetectionBox, FormulaBackend, PPFormulaNetAdapter, RecognitionParams,
    RecognitionResult, TextRecParams,
};
use latexsnipper_runtime::{
    InferenceContext, ModelId, ModelInput, ModelOutput, ModelTask, RuntimeResolver,
};

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::{
    get_backend, get_or_create_session, model_artifact_path, resolve_model_handle, resolve_variant,
};

struct TextRecModel {
    config: latexsnipper_model::ModelConfig,
    model_path: std::path::PathBuf,
    keys_path: std::path::PathBuf,
}

/// Recognizes content in cropped regions stored in context artifacts.
///
/// The `task` field determines which recognition logic to use.
pub struct RecognizerNode {
    name: String,
    task: ModelTask,
}

impl RecognizerNode {
    /// Create a recognizer for a specific task.
    pub fn for_task(task: ModelTask) -> Self {
        let name = match task {
            ModelTask::FormulaRecognition => "recognize_formula".to_string(),
            ModelTask::TextRecognition => "recognize_text".to_string(),
            ModelTask::HandwritingRecognition => "recognize_handwriting".to_string(),
            ModelTask::TableStructure => "recognize_table".to_string(),
            _ => format!("recognize_{:?}", task).to_lowercase(),
        };
        Self { name, task }
    }

    /// Create a formula recognition node.
    pub fn formula() -> Self {
        Self::for_task(ModelTask::FormulaRecognition)
    }

    /// Create a text recognition node.
    pub fn text() -> Self {
        Self::for_task(ModelTask::TextRecognition)
    }

    /// Get the model task this node performs.
    pub fn task(&self) -> ModelTask {
        self.task
    }
}

#[async_trait]
impl PipelineNode for RecognizerNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        if self.task == ModelTask::FormulaRecognition
            && ctx.artifacts.formula_detections.is_empty()
            && ctx
                .metadata
                .get("croppedFormula")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        {
            if let Some(image) = &ctx.image {
                ctx.artifacts.formula_detections.push(DetectionBox::rect(
                    Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32),
                    1.0,
                    0,
                    "cropped_formula".to_owned(),
                ));
            }
        }
        // Try using ModelPackage if available (FormulaBackend path)
        if let Some(package) = ctx.get_model_package(&self.task) {
            match self.recognize_via_package(ctx, &*package).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Package-based recognition failed (e.g., missing vocab).
                    // Fall through to the legacy direct function path.
                    log::warn!(
                        "ModelPackage recognition failed for {:?}: {}. Falling back to direct path.",
                        self.task,
                        e
                    );
                }
            }
        }

        // Fall back to direct function calls
        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        match self.task {
            ModelTask::FormulaRecognition => self.recognize_formulas(ctx, &models).await,
            ModelTask::TextRecognition => self.recognize_texts(ctx, &models).await,
            _ => {
                ctx.diagnostic_warn(
                    &self.name,
                    format!("Unsupported recognition task: {:?}", self.task),
                );
                Ok(())
            }
        }
    }
}

impl RecognizerNode {
    /// Recognize using ModelPackage abstraction.
    async fn recognize_via_package(
        &self,
        ctx: &mut PipelineContext,
        package: &dyn latexsnipper_runtime::ModelPackage,
    ) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let mut blocks = Vec::new();

        match self.task {
            ModelTask::FormulaRecognition => {
                let mut executor = if let Some(e) = ctx.create_model_executor(self.task)? {
                    e
                } else {
                    let backend = get_backend(ctx)?;
                    package.create_executor(backend)?
                };

                let detections = ctx.artifacts.formula_detections.clone();
                for det in &detections {
                    let x = det.rect.x as u32;
                    let y = det.rect.y as u32;
                    let w = det.rect.width as u32;
                    let h = det.rect.height as u32;

                    if w >= 4 && h >= 4 {
                        let cropped = operations::crop(
                            &image,
                            Rect::new(x as f32, y as f32, w as f32, h as f32),
                        );

                        let pixels = cropped.pixels().to_vec();
                        let shape = vec![cropped.height() as usize, cropped.width() as usize, 3];
                        let input = ModelInput {
                            name: "image".to_string(),
                            data: pixels,
                            shape,
                            dtype: latexsnipper_runtime::TensorDtype::UInt8,
                        };

                        let mut inf_ctx = InferenceContext::new();
                        let output = executor.run(input, &mut inf_ctx)?;

                        if let ModelOutput::Formula(results) = output {
                            for result in results {
                                let mut f = Formula::latex(result.latex);
                                f.confidence = result.confidence;
                                f.recognition_provenance = result.provenance.map(Box::new);
                                f.recognition_evidence = result.evidence.map(Box::new);
                                blocks.push(Block::Formula(FormulaBlock {
                                    formula: f,
                                    label: None,
                                    number: None,
                                    environment: None,
                                    geometry: Some(det.rect),
                                    source: Some(
                                        SourceInfo::new()
                                            .with_page(ctx.current_page)
                                            .with_confidence(det.confidence)
                                            .with_region(det.rect),
                                    ),
                                }));
                            }
                        }
                    }
                }
                ctx.artifacts.formula_blocks = blocks;
            }
            ModelTask::TextRecognition => {
                // Shared TextRecognitionService (PreparedModel -> cache -> legacy)
                if let Some(service) = ctx.get_or_init_text_rec_service() {
                    let detections = ctx.artifacts.text_detections.clone();
                    let mut blocks = Vec::new();
                    for det in &detections {
                        let text =
                            match service.recognize_region(&image, &det.rect, det.quad.as_ref()) {
                                Ok(t) => t,
                                Err(e) => {
                                    log::warn!("Text rec failed: {}", e);
                                    continue;
                                }
                            };
                        if !text.is_empty() {
                            blocks.push(Block::Paragraph(ParagraphBlock {
                                inlines: vec![Inline::Text(TextRun::new(text))],
                                geometry: Some(det.rect),
                                source: Some(SourceInfo::new().with_page(ctx.current_page)),
                                style: None,
                            }));
                        }
                    }
                    ctx.artifacts.text_blocks = blocks;
                }
            }
            _ => {
                ctx.diagnostic_warn(
                    &self.name,
                    format!(
                        "Unsupported recognition task via ModelPackage: {:?}",
                        self.task
                    ),
                );
            }
        }

        log::info!(
            "Pipeline: {} recognized {} blocks via ModelPackage",
            self.name,
            ctx.artifacts.formula_blocks.len() + ctx.artifacts.text_blocks.len()
        );
        Ok(())
    }

    async fn recognize_formulas(
        &self,
        ctx: &mut PipelineContext,
        models: &std::path::Path,
    ) -> Result<()> {
        let detections = ctx.artifacts.formula_detections.clone();
        if detections.is_empty() {
            return Ok(());
        }

        let (rec_config, _primary_path, rec_dir) = resolve_variant(ctx, models, "formula-rec")
            .map_err(|_| SnipperError::Model("Formula recognition model not found".into()))?;

        if rec_config.model_type == "pp_formulanet" && !rec_config.runtime_variants.is_empty() {
            let registry = ctx.runtime_registry.clone().ok_or_else(|| {
                SnipperError::Runtime(
                    "PP-FormulaNet runtime variants require RuntimeRegistry".to_owned(),
                )
            })?;
            let model_id = ctx
                .model_variants
                .get("formula-rec")
                .map_or("formula-rec/pp-formulanet-s", String::as_str);
            let resolved = RuntimeResolver::new(&registry).resolve(
                model_id,
                &rec_config.runtime_variants,
                &rec_dir,
                None,
            )?;
            let adapter = PPFormulaNetAdapter::from_resolved_variant(
                &registry,
                &resolved,
                &rec_dir,
                &rec_config,
            )?;
            log::info!(
                "PP-FormulaNet selected runtime variant '{}' ({})",
                resolved.variant_id,
                resolved.runtime
            );
            return self
                .recognize_formula_regions(ctx, detections, |image| adapter.recognize(image));
        }

        let model_files = rec_config
            .pipeline
            .as_ref()
            .and_then(|pipeline| pipeline.model_files.as_ref());
        let encoder_path = model_artifact_path(
            ctx,
            &rec_dir,
            model_files.and_then(|files| files.encoder.as_deref()),
            rec_config.pipeline_encoder_path(&rec_dir),
        )
        .ok_or_else(|| SnipperError::Model("Encoder not found".into()))?;
        let decoder_path = model_artifact_path(
            ctx,
            &rec_dir,
            model_files.and_then(|files| files.decoder.as_deref()),
            rec_config.pipeline_decoder_path(&rec_dir),
        )
        .ok_or_else(|| SnipperError::Model("Decoder not found".into()))?;
        let tokenizer_path = model_artifact_path(
            ctx,
            &rec_dir,
            model_files.and_then(|files| files.tokenizer.as_deref()),
            rec_config.pipeline_tokenizer_path(&rec_dir),
        )
        .ok_or_else(|| SnipperError::Model("Tokenizer not found".into()))?;
        let tokenizer_name = tokenizer_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("tokenizer.json");
        let tokenizer = ctx
            .model_resolver
            .as_ref()
            .and_then(|resolver| {
                let variant = ctx.model_variants.get("formula-rec")?;
                resolver
                    .read_text_artifact(&ModelId::new("formula-rec", variant), tokenizer_name)
                    .ok()
            })
            .and_then(|text| load_tokenizer_from_str(&text).ok());

        let backend = get_backend(ctx)?;
        let enc_handle = resolve_model_handle(ctx, "formula-rec/encoder", encoder_path)?;
        let dec_handle = resolve_model_handle(ctx, "formula-rec/decoder", decoder_path)?;

        let enc_session = get_or_create_session(ctx, "formula_encoder", &backend, &enc_handle)?;
        let dec_session = get_or_create_session(ctx, "formula_decoder", &backend, &dec_handle)?;

        let params = RecognitionParams::from_config(&rec_config);
        let recognize_crop = |image: &latexsnipper_image::SnipperImage| {
            if let Some(tokenizer) = &tokenizer {
                recognize_formula_with_tokenizer(
                    image,
                    &**enc_session,
                    &**dec_session,
                    tokenizer,
                    &params,
                )
            } else {
                recognize_formula(
                    image,
                    &**enc_session,
                    &**dec_session,
                    &tokenizer_path,
                    &params,
                )
            }
        };
        self.recognize_formula_regions(ctx, detections, recognize_crop)
    }

    fn recognize_formula_regions(
        &self,
        ctx: &mut PipelineContext,
        detections: Vec<latexsnipper_inference::types::DetectionBox>,
        recognize_crop: impl Fn(&latexsnipper_image::SnipperImage) -> Result<RecognitionResult>,
    ) -> Result<()> {
        let mut blocks = Vec::new();
        for det in detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if let Some(ref image) = ctx.image {
                if w >= 4 && h >= 4 {
                    let cropped =
                        operations::crop(image, Rect::new(x as f32, y as f32, w as f32, h as f32));
                    let plan = plan_formula_segmentation(&cropped);

                    if plan.groups.is_empty() || plan.groups[0].crops.is_empty() {
                        match recognize_crop(&cropped) {
                            Ok(result) => {
                                let provenance = result.provenance.clone();
                                let evidence = result.postprocess.clone();
                                let mut f = Formula::latex(result.text);
                                f.confidence = result.confidence;
                                f.recognition_provenance = provenance.map(Box::new);
                                f.recognition_evidence = evidence.map(Box::new);
                                blocks.push(Block::Formula(FormulaBlock {
                                    formula: f,
                                    label: None,
                                    number: None,
                                    environment: None,
                                    geometry: Some(Rect::new(
                                        x as f32, y as f32, w as f32, h as f32,
                                    )),
                                    source: Some(
                                        SourceInfo::new()
                                            .with_page(ctx.current_page)
                                            .with_confidence(det.confidence)
                                            .with_region(det.rect),
                                    ),
                                }));
                            }
                            Err(e) => log::warn!("Formula rec failed: {}", e),
                        }
                    } else {
                        let mut all_results = Vec::new();
                        for group in &plan.groups {
                            for crop in &group.crops {
                                let crop_img = latexsnipper_image::SnipperImage::new(
                                    crop.width,
                                    crop.height,
                                    latexsnipper_image::color::PixelFormat::Rgb,
                                    crop.pixels.clone(),
                                );
                                match recognize_crop(&crop_img) {
                                    Ok(result) => all_results.push(result.text),
                                    Err(e) => log::warn!("Formula line rec failed: {}", e),
                                }
                            }
                        }

                        if !all_results.is_empty() {
                            // Multi-line derivations are reconstructed as a
                            // structured aligned environment instead of a
                            // space-joined blob.
                            let merged = match plan.classification {
                                FormulaSegmentationClass::MultiLine => format!(
                                    "\\begin{{aligned}} {} \\end{{aligned}}",
                                    all_results.join("\\\\ ")
                                ),
                                FormulaSegmentationClass::WideSingleLine => all_results.join(" "),
                                _ => all_results.join(" "),
                            };
                            let mut f = Formula::latex(merged);
                            f.confidence = 0.9;
                            blocks.push(Block::Formula(FormulaBlock {
                                formula: f,
                                label: None,
                                number: None,
                                environment: None,
                                geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                                source: Some(SourceInfo::new().with_page(ctx.current_page)),
                            }));
                        }
                    }
                }
            }
        }

        ctx.artifacts.formula_blocks = blocks;
        log::info!(
            "Recognized {} formula blocks",
            ctx.artifacts.formula_blocks.len()
        );
        Ok(())
    }

    async fn recognize_texts(
        &self,
        ctx: &mut PipelineContext,
        models: &std::path::Path,
    ) -> Result<()> {
        let detections = ctx.artifacts.text_detections.clone();
        if detections.is_empty() {
            return Ok(());
        }

        // Try shared text recognition service first (context-owned)
        if let Some(service) = ctx.get_or_init_text_rec_service() {
            let mut blocks = Vec::new();
            if let Some(ref image) = ctx.image {
                for det in &detections {
                    let text = match service.recognize_region(image, &det.rect, det.quad.as_ref()) {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("Text rec failed: {}", e);
                            continue;
                        }
                    };
                    if !text.is_empty() {
                        blocks.push(Block::Paragraph(ParagraphBlock {
                            inlines: vec![Inline::Text(TextRun::new(text))],
                            geometry: Some(det.rect),
                            source: Some(SourceInfo::new().with_page(ctx.current_page)),
                            style: None,
                        }));
                    }
                }
            }
            ctx.artifacts.text_blocks = blocks;
            log::info!(
                "Recognized {} text blocks (shared service)",
                ctx.artifacts.text_blocks.len()
            );
            return Ok(());
        }

        // Fall back to direct function calls (legacy path)
        let rec_model = match select_text_rec_model(models) {
            Ok(m) => m,
            Err(e) => {
                ctx.diagnostic_warn(
                    "recognize_text",
                    format!("Text recognition model not found: {}", e),
                );
                return Ok(());
            }
        };

        let backend = get_backend(ctx)?;
        let handle = resolve_model_handle(ctx, "text-rec", rec_model.model_path)?;

        let session = get_or_create_session(ctx, "text_rec", &backend, &handle)?;

        let params = TextRecParams::from_config(&rec_model.config);
        let (keys, first_char_id) = load_keys(&rec_model.keys_path).unwrap_or_else(|_| {
            session
                .get_character_list()
                .filter(|chars| !chars.is_empty())
                .map(|chars| (chars, 0))
                .unwrap_or((Vec::new(), 1))
        });

        let mut blocks = Vec::new();

        for det in detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if let Some(ref image) = ctx.image {
                if w >= 4 && h >= 4 {
                    let pad_y = (h as f32 * 0.2).max(4.0) as u32;
                    let crop_y = y.saturating_sub(pad_y);
                    let crop_h = h + pad_y * 2;
                    let crop_y_end = (crop_y + crop_h).min(image.height());
                    let final_h = crop_y_end - crop_y;
                    let cropped = operations::crop(
                        image,
                        Rect::new(x as f32, crop_y as f32, w as f32, final_h as f32),
                    );
                    match recognize_text_with_keys(
                        &cropped,
                        &*session,
                        &keys,
                        first_char_id,
                        &params,
                    ) {
                        Ok(result) => {
                            if !result.text.is_empty() {
                                blocks.push(Block::Paragraph(ParagraphBlock {
                                    inlines: vec![Inline::Text(TextRun::new(result.text))],
                                    geometry: Some(Rect::new(
                                        x as f32, y as f32, w as f32, h as f32,
                                    )),
                                    source: Some(
                                        SourceInfo::new()
                                            .with_page(ctx.current_page)
                                            .with_confidence(det.confidence)
                                            .with_region(det.rect),
                                    ),
                                    style: None,
                                }));
                            }
                        }
                        Err(e) => log::warn!("Text rec failed: {}", e),
                    }
                }
            }
        }

        ctx.artifacts.text_blocks = blocks;
        log::info!("Recognized {} text blocks", ctx.artifacts.text_blocks.len());
        Ok(())
    }
}

fn select_text_rec_model(models: &std::path::Path) -> Result<TextRecModel> {
    let variants = latexsnipper_model::ModelConfig::discover_all(models, "text-rec");
    let mut unsupported = Vec::new();

    for (variant, config, variant_dir) in variants {
        let Some(model_path) = config.pipeline_model_path(&variant_dir) else {
            unsupported.push(format!(
                "{}/{} has no ONNX model",
                models.display(),
                variant
            ));
            continue;
        };

        let keys_path = config
            .pipeline_tokenizer_path(&variant_dir)
            .ok_or_else(|| {
                SnipperError::Model(format!(
                    "Text keys not found in {}/{}",
                    models.display(),
                    variant
                ))
            })?;

        return Ok(TextRecModel {
            config,
            model_path,
            keys_path,
        });
    }

    let fallback_dirs = [
        models.join("v6_models/PP-OCRv6_small_rec_infer"),
        models.join("v6_models/PP-OCRv6_medium_rec_infer"),
        models.join("text-rec/ppocrv5-mobile"),
    ];

    for dir in &fallback_dirs {
        if !dir.is_dir() {
            continue;
        }

        let config = match if dir.join("config.json").exists() {
            latexsnipper_model::ModelConfig::load(dir)
        } else {
            latexsnipper_model::ModelConfig::from_paddle_inference_dir(dir)
        } {
            Ok(config) => config,
            Err(e) => {
                unsupported.push(format!("{} cannot be parsed: {}", dir.display(), e));
                continue;
            }
        };

        let Some(model_path) = config.find_model_file(dir) else {
            unsupported.push(format!("{} has no ONNX model", dir.display()));
            continue;
        };

        let keys_path = config.find_tokenizer_file(dir).ok_or_else(|| {
            SnipperError::Model(format!("Text keys not found in {}", dir.display()))
        })?;

        return Ok(TextRecModel {
            config,
            model_path,
            keys_path,
        });
    }

    if unsupported.is_empty() {
        Err(SnipperError::Model(
            "No text recognition model directory found".into(),
        ))
    } else {
        Err(SnipperError::Model(format!(
            "No supported text recognition model found ({})",
            unsupported.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_models_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
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
        fs::create_dir_all(&v6).unwrap();
        write_text_rec_config(&v6);
        fs::write(v6.join("model.onnx"), []).unwrap();
        fs::write(v6.join("ppocr_keys.txt"), "a\nb\n").unwrap();

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
