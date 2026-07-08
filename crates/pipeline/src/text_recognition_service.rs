//! Shared text recognition service.
//!
//! Provides a single point of text OCR that both `RecognizerNode` (main text)
//! and `TableRecognizerNode` (table cells) call into, avoiding dual model
//! loading and parameter drift between the two paths.

use latexsnipper_ast::{Quad, Rect};
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{load_keys, recognize_text_with_keys, TextRecParams};
use latexsnipper_runtime::{AccelerationMode, InferenceSession, ModelHandle, RuntimeBackend};
use std::path::Path;
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
    /// * `models_dir` — path to the models directory
    /// * `variant` — optional variant name (e.g. "v6-small", "openocr-mobile").
    ///   When None, auto-discovers the best available variant.
    /// * `backend` — optional runtime backend from pipeline context.
    ///   When None, creates a default CPU backend.
    /// * `acceleration` — acceleration mode (only used when creating a new backend).
    ///
    /// Returns `None` when no suitable model is found (caller should skip
    /// gracefully rather than fail).
    pub fn try_load(
        models_dir: &Path,
        variant: Option<&str>,
        backend: Option<Arc<dyn RuntimeBackend>>,
        acceleration: AccelerationMode,
    ) -> Option<Self> {
        // Resolve variant
        let (config, variant_dir) = match variant {
            Some(v) => {
                let variant_dir = models_dir.join("text-rec").join(v);
                if !variant_dir.is_dir() {
                    return None;
                }
                let config = latexsnipper_model::ModelConfig::load(&variant_dir)
                    .ok()
                    .or_else(|| {
                        latexsnipper_model::ModelConfig::from_paddle_inference_dir(&variant_dir)
                            .ok()
                    })?;
                (config, variant_dir)
            }
            None => {
                let (config, _, vd) =
                    latexsnipper_model::ModelConfig::find_best(models_dir, "text-rec")?;
                (config, vd)
            }
        };

        let model_path = config.pipeline_model_path(&variant_dir)?;

        // Create or use provided backend
        let session: Box<dyn InferenceSession> = match backend {
            Some(b) => {
                let handle = ModelHandle::with_path("text-rec", model_path);
                b.create_session(&handle, acceleration).ok()?
            }
            #[cfg(target_os = "windows")]
            None => {
                let b = latexsnipper_runtime::providers::onnx::OnnxRuntimeBackend::new(
                    models_dir.to_path_buf(),
                )
                .ok()?;
                let handle = ModelHandle::with_path("text-rec", model_path);
                b.create_session(&handle, acceleration).ok()?
            }
            #[cfg(not(target_os = "windows"))]
            None => return None,
        };

        let params = TextRecParams::from_config(&config);

        // Load character keys
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
                return Ok(result.text);
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

        Ok(result.text)
    }
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
