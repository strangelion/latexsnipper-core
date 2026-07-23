//! Shared text recognition service.
//!
//! Provides a single point of text OCR that both `RecognizerNode` (main text)
//! and `TableRecognizerNode` (table cells) call into, avoiding dual model
//! loading and parameter drift between the two paths.

use latexsnipper_ast::{Quad, Rect};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::operations;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{
    load_keys, load_keys_from_str, recognize_text_with_keys, RecognitionResult, TextRecParams,
};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, InferenceSession, ModelExecutionContext, ModelHandle, ModelId,
    RuntimeBackend, SharedModelResolver,
};
use std::path::Path;
use std::sync::Arc;

/// Shared text recognition service.
pub struct TextRecognitionService {
    session: Arc<Box<dyn InferenceSession>>,
    params: TextRecParams,
    keys: Vec<String>,
    first_char_id: usize,
}

impl TextRecognitionService {
    /// Try to load from the filesystem (legacy path).
    pub fn try_load(
        models_dir: &Path,
        variant: Option<&str>,
        backend: Option<Arc<dyn RuntimeBackend>>,
        acceleration: AccelerationMode,
    ) -> Option<Self> {
        let (config, variant_dir) = match variant {
            Some(v) => {
                let variant_dir = models_dir.join("text-rec").join(v);
                if !variant_dir.is_dir() {
                    return None;
                }
                let config = ModelConfig::load(&variant_dir)
                    .ok()
                    .or_else(|| ModelConfig::from_paddle_inference_dir(&variant_dir).ok())?;
                (config, variant_dir)
            }
            None => {
                let (config, _, vd) = ModelConfig::find_best(models_dir, "text-rec")?;
                (config, vd)
            }
        };

        let model_path = config.pipeline_model_path(&variant_dir)?;
        let b = backend?;
        let handle = ModelHandle::with_path("text-rec", model_path);
        let session: Box<dyn InferenceSession> = b.create_session(&handle, acceleration).ok()?;
        let params = TextRecParams::from_config(&config);
        let keys_path = config.pipeline_tokenizer_path(&variant_dir)?;
        let (keys, first_char_id) = load_keys(&keys_path).unwrap_or_else(|_| {
            session
                .get_character_list()
                .filter(|chars| !chars.is_empty())
                .map(|chars| (chars, 0))
                .unwrap_or((Vec::new(), 1))
        });

        Some(Self {
            session: Arc::new(session),
            params,
            keys,
            first_char_id,
        })
    }

    /// Load from an in-memory resolver (WASM path).
    pub fn try_load_from_resolver(
        resolver: &SharedModelResolver,
        variant: &str,
        backend: Arc<dyn RuntimeBackend>,
        acceleration: AccelerationMode,
    ) -> Option<Self> {
        let id = ModelId::new("text-rec", variant);
        let config_text = resolver.read_text_artifact(&id, "config.json").ok()?;
        let config = ModelConfig::from_json_str(&config_text).ok()?;
        let primary_name = config
            .pipeline
            .as_ref()
            .and_then(|pipeline| pipeline.model_files.as_ref())
            .and_then(|files| files.primary.as_deref())
            .unwrap_or("model.onnx");
        let model_handle = resolver.resolve_artifact(&id, primary_name).ok()?;
        let session = backend.create_session(&model_handle, acceleration).ok()?;
        let params = TextRecParams::from_config(&config);
        let keys_name = config
            .decoding
            .as_ref()
            .and_then(|decoding| decoding.keys_file.as_deref())
            .or_else(|| {
                config
                    .decoding
                    .as_ref()
                    .and_then(|decoding| decoding.tokenizer_file.as_deref())
            })
            .unwrap_or("charset.txt");

        let fallback_keys = || {
            session
                .get_character_list()
                .filter(|characters| !characters.is_empty())
                .map(|characters| (characters, 0))
                .unwrap_or((Vec::new(), 1))
        };
        let (keys, first_char_id) = resolver
            .read_text_artifact(&id, keys_name)
            .ok()
            .and_then(|text| load_keys_from_str(&text, keys_name).ok())
            .unwrap_or_else(fallback_keys);

        Some(Self {
            session: Arc::new(session),
            params,
            keys,
            first_char_id,
        })
    }

    /// Create from a [`ModelExecutionContext`] using the resolved runtime.
    ///
    /// Artifact lookup order:
    /// 1. `tokenizer` or `keys` — explicit CTC charset file
    /// 2. `inferenceConfig` — Paddle/OpenOCR inference.yml character dict
    /// 3. Runtime-embedded character list (ORT session metadata)
    pub fn from_context(ctx: &ModelExecutionContext) -> Result<Self> {
        let session = ctx.create_session("model")?;
        let compat: Arc<Box<dyn InferenceSession>> = Arc::new(Box::new(
            latexsnipper_runtime::RuntimeSessionCompatibility::new(session),
        ));

        let config = if let Ok(json) = ctx.read_artifact("config") {
            ModelConfig::from_json_str(&json)?
        } else {
            ModelConfig::minimal()
        };

        let params = TextRecParams::from_config(&config);
        let (keys, first_char_id) = load_text_keys(ctx, &compat)?;

        Ok(Self {
            session: compat,
            params,
            keys,
            first_char_id,
        })
    }

    /// Recognize text in a rectangular crop region.
    pub fn recognize_region(
        &self,
        image: &SnipperImage,
        rect: &Rect,
        quad: Option<&Quad>,
    ) -> Result<String> {
        self.recognize_region_result(image, rect, quad)
            .map(|result| result.text)
    }

    /// Recognize a region while preserving OCR confidence.
    pub fn recognize_region_result(
        &self,
        image: &SnipperImage,
        rect: &Rect,
        quad: Option<&Quad>,
    ) -> Result<RecognitionResult> {
        if let Some(q) = quad {
            let (tw, th) = q.warp_target_size();
            let padding = (th as f32 * 0.1).max(2.0);
            let warped = operations::warp_quad_to_rect(image, q, tw.max(4), th.max(4), padding);
            let result = recognize_text_with_keys(
                &warped,
                self.session.as_ref().as_ref(),
                &self.keys,
                self.first_char_id,
                &self.params,
            )?;
            if !result.text.trim().is_empty() {
                return Ok(result);
            }
        }
        let cropped = crop_rect_with_padding(image, rect);
        let result = recognize_text_with_keys(
            &cropped,
            self.session.as_ref().as_ref(),
            &self.keys,
            self.first_char_id,
            &self.params,
        )?;
        Ok(result)
    }
}

/// Load text recognition keys from resolved runtime artifacts.
///
/// Priority order:
/// 1. `tokenizer` or `keys` artifact role
/// 2. `inferenceConfig` artifact role (Paddle inference.yml with character dict)
/// 3. Runtime-embedded character list
fn load_text_keys(
    ctx: &ModelExecutionContext,
    session: &Arc<Box<dyn InferenceSession>>,
) -> Result<(Vec<String>, usize)> {
    // 1. Explicit tokenizer / keys artifact
    for role in &["tokenizer", "keys"] {
        if let Ok(text) = ctx.read_artifact(role) {
            // Use the real artifact filename so inference.yml is parsed correctly
            if let Some(path) = ctx.artifact_path(role) {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("keys.txt");
                if let Ok((keys, fid)) = load_keys_from_str(&text, filename) {
                    return Ok((keys, fid));
                }
            }
        }
    }

    // 2. inferenceConfig (Paddle/OpenOCR inference.yml)
    if let Ok(text) = ctx.read_artifact("inferenceConfig") {
        if let Ok((keys, fid)) = load_keys_from_str(&text, "inference.yml") {
            return Ok((keys, fid));
        }
    }

    // 3. Runtime-embedded character list
    if let Some(chars) = session.get_character_list() {
        if !chars.is_empty() {
            return Ok((chars, 0));
        }
    }

    Err(SnipperError::Model(
        "No character table found in resolved runtime artifacts".into(),
    ))
}

fn crop_rect_with_padding(image: &SnipperImage, rect: &Rect) -> SnipperImage {
    let x = rect.x as u32;
    let y = rect.y as u32;
    let w = rect.width as u32;
    let h = rect.height as u32;
    if w < 4 || h < 4 {
        return operations::crop(image, Rect::new(0.0, 0.0, 1.0, 1.0));
    }
    let pad_x = (w as f32 * 0.02).max(2.0) as u32;
    let pad_y = (h as f32 * 0.2).max(4.0) as u32;
    let crop_x = x.saturating_sub(pad_x);
    let crop_y = y.saturating_sub(pad_y);
    let crop_w = (w + pad_x * 2).min(image.width().saturating_sub(crop_x));
    let crop_h = (h + pad_y * 2).min(image.height().saturating_sub(crop_y));
    operations::crop(
        image,
        Rect::new(crop_x as f32, crop_y as f32, crop_w as f32, crop_h as f32),
    )
}
