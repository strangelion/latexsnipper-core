//! Shared text recognition service.
//!
//! Provides a single point of text OCR that both `RecognizerNode` (main text)
//! and `TableRecognizerNode` (table cells) call into, avoiding dual model
//! loading and parameter drift between the two paths.

use latexsnipper_ast::{Quad, Rect};
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{
    load_keys, recognize_text_with_keys, TextRecParams,
};
use latexsnipper_runtime::{
    AccelerationMode, InferenceSession, ModelHandle, RuntimeBackend,
};
use std::sync::Arc;

/// Shared text recognition service.
///
/// Holds the loaded ONNX session, compiled `TextRecParams`, and character
/// keys so that all call sites share one model instance with identical config.
pub struct TextRecognitionService {
    session: Arc<Box<dyn InferenceSession>>,
    params: TextRecParams,
    keys: Vec<String>,
    first_char_id: usize,
}

impl TextRecognitionService {
    /// Try to load a text recognition model from the given models directory.
    ///
    /// Returns `None` when no suitable model is found (caller should skip
    /// gracefully rather than fail).
    pub fn try_load(models_dir: &std::path::Path) -> Option<Self> {
        let (config, _model_path, variant_dir) =
            latexsnipper_model::ModelConfig::find_best(models_dir, "text-rec")?;

        let model_path = config.pipeline_model_path(&variant_dir)?;

        let backend = latexsnipper_runtime::providers::onnx::OnnxRuntimeBackend::new(
            models_dir.to_path_buf(),
        )
        .ok()?;

        let handle = ModelHandle::with_path("text-rec", model_path);
        let session: Box<dyn InferenceSession> = backend
            .create_session(&handle, AccelerationMode::Cpu)
            .ok()?;

        let params = TextRecParams::from_config(&config);

        // Load character keys
        let keys_path = config.pipeline_tokenizer_path(&variant_dir)?;
        let (first_char_id, keys) = if let Some(chars) = session.get_character_list() {
            (0, chars)
        } else {
            let (keys, first_char_id) = load_keys(&keys_path).unwrap_or((Vec::new(), 1));
            (first_char_id, keys)
        };

        Some(Self {
            session: Arc::new(session),
            params,
            keys,
            first_char_id,
        })
    }

    /// Recognize text in a rectangular crop region.
    ///
    /// Uses quad-based perspective warp when `quad` is provided, otherwise
    /// falls back to axis-aligned `Rect` crop.
    pub fn recognize_region(
        &self,
        image: &SnipperImage,
        rect: &Rect,
        quad: Option<&Quad>,
    ) -> Result<String> {
        let cropped = if let Some(q) = quad {
            let (tw, th) = q.warp_target_size();
            let padding = (th as f32 * 0.1).max(2.0);
            operations::warp_quad_to_rect(image, q, tw.max(4), th.max(4), padding)
        } else {
            let x = rect.x as u32;
            let y = rect.y as u32;
            let w = rect.width as u32;
            let h = rect.height as u32;
            if w < 4 || h < 4 {
                return Ok(String::new());
            }
            let pad_y = (h as f32 * 0.2).max(4.0) as u32;
            let crop_y = y.saturating_sub(pad_y);
            let crop_h = h + pad_y * 2;
            let crop_y_end = (crop_y + crop_h).min(image.height());
            let final_h = crop_y_end - crop_y;
            operations::crop(image, Rect::new(x as f32, crop_y as f32, w as f32, final_h as f32))
        };

        let result = recognize_text_with_keys(
            &cropped,
            self.session.as_ref().as_ref(),
            &self.keys,
            self.first_char_id,
            &self.params,
        )?;

        Ok(result.text)
    }
}
