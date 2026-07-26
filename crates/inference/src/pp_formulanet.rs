//! PP-FormulaNet-S inference backend.
//!
//! Architecture:
//! ```text
//! image (1ch, 384x384)
//!   → official UniMERNet preprocess (crop, resize, pad, grayscale, normalize)
//!   → EncoderSession.run(pixel_values) → [B, 144, 384]
//!   → Decoder fullseq autoregressive (input_ids + encoder_hidden_states)
//!   → ByteLevel BPE decode → LaTeX string
//! ```
//!
//! The decoder uses the full-sequence ONNX with the official block-wise
//! parallel causal mask baked into the ONNX graph (parallel_step=3).
//! Each decode step feeds the entire token history and recomputes all
//! logits; the block-parallel mask ensures each token can attend within
//! its own 3-token block plus all previous blocks.
//!
//! Three decoder ONNX variants exist:
//!   - decoder_fullseq.onnx          No mask (legacy, short-prefix only)
//!   - decoder_fullseq_parallel.onnx Block-wise parallel causal mask (current)
//!   - decoder_step.onnx             Official while-body, KV cache (P1 WIP)

use std::path::Path;
use std::sync::Arc;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::formula_backend::{BackendConfig, FormulaBackend};
use crate::types::RecognitionResult;

/// PP-FormulaNet-S inference backend using fullseq decoder ONNX.
#[deprecated(
    since = "3.1.0",
    note = "Use PPFormulaNetAdapter with RuntimeRegistry. \
            PPFormulaNetBackend always uses the legacy reconstructed ONNX path. \
            The official production path is Paddle Inference native via \
            PPFormulaNetAdapter::from_resolved_variant()."
)]
pub struct PPFormulaNetBackend {
    name: String,
    encoder: Arc<Box<dyn InferenceSession>>,
    decoder: Arc<Box<dyn InferenceSession>>,
    config: BackendConfig,
    /// HuggingFace tokenizer for decoding token IDs → LaTeX string
    tokenizer: tokenizers::Tokenizer,
}

#[allow(deprecated)]
impl PPFormulaNetBackend {
    /// Load from model directory.
    ///
    /// Prefers `decoder_fullseq_parallel.onnx` (official block-wise parallel
    /// causal mask) when available, falls back to `decoder_fullseq.onnx`
    /// (no mask, legacy).
    #[allow(deprecated)]
    pub fn load(
        model_dir: &Path,
        runtime: &dyn latexsnipper_runtime::RuntimeBackend,
    ) -> Result<Self> {
        let config = BackendConfig::from_config(&latexsnipper_model::ModelConfig::load(model_dir)?);

        let encoder_path = model_dir.join("encoder_model.onnx");
        if !encoder_path.exists() {
            return Err(SnipperError::Model(format!(
                "encoder_model.onnx not found in {}",
                model_dir.display()
            )));
        }

        // Prefer parallel-mask decoder, fall back to no-mask legacy
        let parallel_path = model_dir.join("decoder_fullseq_parallel.onnx");
        let legacy_path = model_dir.join("decoder_fullseq.onnx");
        let decoder_path = if parallel_path.exists() {
            log::info!("Using decoder_fullseq_parallel.onnx (block-wise parallel causal mask)");
            parallel_path
        } else if legacy_path.exists() {
            log::warn!("Using decoder_fullseq.onnx (no mask, legacy) — quality may degrade");
            legacy_path
        } else {
            return Err(SnipperError::Model(format!(
                "No decoder ONNX found in {} (expected decoder_fullseq_parallel.onnx or decoder_fullseq.onnx)",
                model_dir.display()
            )));
        };

        let encoder_name = encoder_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let decoder_name = decoder_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let encoder = Arc::new(runtime.create_session(
            &latexsnipper_runtime::ModelHandle::with_path("ppfn_encoder", encoder_path),
            latexsnipper_runtime::AccelerationMode::Cpu,
        )?);

        let decoder = Arc::new(runtime.create_session(
            &latexsnipper_runtime::ModelHandle::with_path("ppfn_decoder", decoder_path),
            latexsnipper_runtime::AccelerationMode::Cpu,
        )?);

        // Load HuggingFace tokenizer (BPE + ByteLevel decoder)
        let tokenizer_path = model_dir.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(SnipperError::Model(format!(
                "tokenizer.json not found in {}",
                model_dir.display()
            )));
        }
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| SnipperError::Model(format!("Failed to load tokenizer: {}", e)))?;
        let vocab_size = tokenizer.get_vocab_size(true);

        let name = "pp-formulanet-s".to_string();

        log::info!(
            "Loaded PP-FormulaNet-S: encoder={}, decoder={}, {} vocab tokens",
            encoder_name,
            decoder_name,
            vocab_size,
        );

        Ok(Self {
            name,
            encoder,
            decoder,
            config,
            tokenizer,
        })
    }

    /// Run the decoder for one full-sequence forward pass.
    /// Returns (logits_data, logits_shape).
    fn decode_step(
        &self,
        token_ids: &[i64],
        encoder_data: &[f32],
        encoder_shape: &[usize],
    ) -> Result<(Vec<f32>, Vec<usize>)> {
        let input = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.to_vec());
        let h = Tensor::float32(
            "encoder_hidden_states",
            encoder_shape.to_vec(),
            encoder_data.to_vec(),
        );

        let outputs = self.decoder.run(&[input, h])?;
        let logits = outputs
            .first()
            .ok_or_else(|| SnipperError::Inference("No decoder output".into()))?;
        Ok((
            logits
                .as_f32_slice()
                .ok_or_else(|| SnipperError::Inference("Decoder output not float32".into()))?
                .to_vec(),
            logits.shape().to_vec(),
        ))
    }

    /// Raw argmax — no softmax, no penalty. For parity testing.
    fn argmax(logits: &[f32]) -> usize {
        logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

#[allow(deprecated)]
impl FormulaBackend for PPFormulaNetBackend {
    fn recognize(&self, image: &SnipperImage) -> Result<RecognitionResult> {
        // 1. Official UniMERNet preprocessing and normalization.
        let img = preprocess_image(image, self.config.img_size)?;

        let pixels = img
            .pixels()
            .iter()
            .map(|&p| p as f32 / 255.0)
            .collect::<Vec<_>>();
        let pixels: Vec<f32> = pixels.iter().map(|&p| (p - 0.7931) / 0.1738).collect();

        let encoder_input = Tensor::float32(
            "pixel_values",
            vec![
                1,
                1,
                self.config.img_size as usize,
                self.config.img_size as usize,
            ],
            pixels,
        );

        // 2. Encode
        let encoder_outputs = self.encoder.run(&[encoder_input])?;
        let hidden = encoder_outputs
            .first()
            .ok_or_else(|| SnipperError::Inference("No encoder output".into()))?;
        let hidden_data = hidden
            .as_f32_slice()
            .ok_or_else(|| SnipperError::Inference("Encoder output not float32".into()))?
            .to_vec();
        let hidden_shape = hidden.shape().to_vec();

        // 3. Autoregressive decode with parallel_step=3.
        //    Official protocol: each step feeds the full history, takes
        //    logits for the LAST 3 positions, argmax each, appends all 3
        //    tokens together, then checks for EOS. This matches
        //    `use_parallel=True, parallel_step=3` token-by-token.
        let parallel_step: usize = 3;
        let max_blocks = self.config.max_tokens / parallel_step;
        let mut token_ids: Vec<i64> = vec![0, 0, 0]; // 3 BOS tokens

        for _block in 0..max_blocks {
            let (logits_data, logits_shape) =
                self.decode_step(&token_ids, &hidden_data, &hidden_shape)?;

            // vocab_size from model output shape — the only source of truth
            let vocab_size = *logits_shape.last().ok_or_else(|| {
                SnipperError::Inference("Decoder logits missing vocab dimension".into())
            })?;

            let seq_len = logits_shape.get(1).copied().unwrap_or(token_ids.len());
            let first_pos = seq_len.saturating_sub(parallel_step);

            let mut next_tokens = Vec::with_capacity(parallel_step);
            for p in 0..parallel_step {
                let offset = (first_pos + p) * vocab_size;
                let end = offset + vocab_size;
                if end > logits_data.len() {
                    break;
                }
                next_tokens.push(Self::argmax(&logits_data[offset..end]) as i64);
            }

            if next_tokens.is_empty() {
                break;
            }

            let block_contains_eos = next_tokens.contains(&self.config.eos_token_id);

            token_ids.extend_from_slice(&next_tokens);

            if block_contains_eos {
                break;
            }
        }

        // Truncate at first EOS (official behaviour)
        if let Some(eos_pos) = token_ids[3..]
            .iter()
            .position(|&id| id == self.config.eos_token_id)
        {
            token_ids.truncate(3 + eos_pos);
        }

        // 4. Decode token IDs → LaTeX via HuggingFace tokenizer.
        //    Skip the 3 BOS prefix, and let the tokenizer strip special
        //    tokens (<s>, </s>, <pad>, <unk>) via skip_special_tokens=true.
        let ids_u32: Vec<u32> = token_ids[3..]
            .iter()
            .map(|&id| {
                u32::try_from(id).map_err(|_| {
                    SnipperError::Inference(format!("Invalid negative token ID: {}", id))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let text = self
            .tokenizer
            .decode(&ids_u32, true)
            .map_err(|e| SnipperError::Model(format!("Tokenizer decode failed: {}", e)))?;

        Ok(RecognitionResult::new(
            text, 0.0, // not meaningful during parity testing
        ))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> &BackendConfig {
        &self.config
    }
}

// ─── Preprocessing ──────────────────────────────────────────────────

/// Port of PP-FormulaNet's official `UniMERNetImgDecode` image transform:
/// 1. Convert the source to RGB and crop its normalized `< 200` content margin.
/// 2. Resize the shortest edge with bilinear interpolation.
/// 3. Apply a bicubic thumbnail so both dimensions fit the target.
/// 4. Center pad with black and convert to grayscale.
pub fn preprocess_image(image: &SnipperImage, target_size: u32) -> Result<SnipperImage> {
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_image::operations::{resize_bicubic, resize_bilinear};

    if target_size == 0 || image.width() == 0 || image.height() == 0 {
        return Err(SnipperError::Inference(
            "PP-FormulaNet preprocessing requires non-zero dimensions".to_owned(),
        ));
    }

    let rgb = to_rgb(image);
    let cropped = crop_content_margin(&rgb);
    let (width, height) = (cropped.width(), cropped.height());
    let (bilinear_width, bilinear_height) = if width <= height {
        (target_size, target_size.saturating_mul(height) / width)
    } else {
        (target_size.saturating_mul(width) / height, target_size)
    };
    let bilinear_width = bilinear_width.max(1);
    let bilinear_height = bilinear_height.max(1);
    let resized = resize_bilinear(&cropped, bilinear_width, bilinear_height);
    let (thumbnail_width, thumbnail_height) =
        thumbnail_dimensions(bilinear_width, bilinear_height, target_size);
    let thumbnail = if (thumbnail_width, thumbnail_height) == (bilinear_width, bilinear_height) {
        resized
    } else {
        resize_bicubic(&resized, thumbnail_width, thumbnail_height)
    };

    let mut padded = SnipperImage::new(
        target_size,
        target_size,
        PixelFormat::Rgb,
        vec![0; (target_size * target_size * 3) as usize],
    );
    let offset_x = (target_size - thumbnail_width) / 2;
    let offset_y = (target_size - thumbnail_height) / 2;
    for row in 0..thumbnail_height {
        let source_start = (row * thumbnail_width * 3) as usize;
        let destination_start = (((offset_y + row) * target_size + offset_x) * 3) as usize;
        let length = (thumbnail_width * 3) as usize;
        padded.pixels_mut()[destination_start..destination_start + length]
            .copy_from_slice(&thumbnail.pixels()[source_start..source_start + length]);
    }
    Ok(to_grayscale(&padded))
}

fn to_rgb(image: &SnipperImage) -> SnipperImage {
    use latexsnipper_image::color::PixelFormat;

    let mut pixels = Vec::with_capacity((image.width() * image.height() * 3) as usize);
    for pixel in image.pixels().chunks_exact(image.format().channels()) {
        let (red, green, blue) = match image.format() {
            PixelFormat::Gray => (pixel[0], pixel[0], pixel[0]),
            PixelFormat::Rgb | PixelFormat::Rgba => (pixel[0], pixel[1], pixel[2]),
            PixelFormat::Bgr | PixelFormat::Bgra => (pixel[2], pixel[1], pixel[0]),
        };
        pixels.extend_from_slice(&[red, green, blue]);
    }
    SnipperImage::new(image.width(), image.height(), PixelFormat::Rgb, pixels)
}

fn to_grayscale(image: &SnipperImage) -> SnipperImage {
    use latexsnipper_image::color::PixelFormat;

    let pixels = image
        .pixels()
        .chunks_exact(3)
        .map(|pixel| {
            (0.299 * f32::from(pixel[0])
                + 0.587 * f32::from(pixel[1])
                + 0.114 * f32::from(pixel[2]))
            .round() as u8
        })
        .collect();
    SnipperImage::new(image.width(), image.height(), PixelFormat::Gray, pixels)
}

fn crop_content_margin(image: &SnipperImage) -> SnipperImage {
    use latexsnipper_ast::Rect;
    use latexsnipper_image::operations::crop;

    let grayscale = to_grayscale(image);
    let Some(&minimum) = grayscale.pixels().iter().min() else {
        return image.clone();
    };
    let maximum = *grayscale.pixels().iter().max().unwrap_or(&minimum);
    if minimum == maximum {
        return image.clone();
    }

    let range = u32::from(maximum - minimum);
    let mut left = image.width();
    let mut right = 0;
    let mut top = image.height();
    let mut bottom = 0;
    for (index, value) in grayscale.pixels().iter().copied().enumerate() {
        let normalized = u32::from(value - minimum) * 255;
        if normalized >= 200 * range {
            continue;
        }
        let x = index as u32 % image.width();
        let y = index as u32 / image.width();
        left = left.min(x);
        right = right.max(x + 1);
        top = top.min(y);
        bottom = bottom.max(y + 1);
    }
    if left >= right || top >= bottom {
        return image.clone();
    }
    crop(
        image,
        Rect::new(
            left as f32,
            top as f32,
            (right - left) as f32,
            (bottom - top) as f32,
        ),
    )
}

fn thumbnail_dimensions(width: u32, height: u32, target: u32) -> (u32, u32) {
    if width <= target && height <= target {
        return (width, height);
    }
    let aspect = f64::from(width) / f64::from(height);
    if aspect <= 1.0 {
        let exact = f64::from(target) * aspect;
        let output_width = closest_aspect_dimension(exact, |candidate| {
            (aspect - f64::from(candidate) / f64::from(target)).abs()
        });
        (output_width, target)
    } else {
        let exact = f64::from(target) / aspect;
        let output_height = closest_aspect_dimension(exact, |candidate| {
            (aspect - f64::from(target) / f64::from(candidate)).abs()
        });
        (target, output_height)
    }
}

fn closest_aspect_dimension(exact: f64, error: impl Fn(u32) -> f64) -> u32 {
    let floor = exact.floor().max(1.0) as u32;
    let ceil = exact.ceil().max(1.0) as u32;
    if error(floor) <= error(ceil) {
        floor
    } else {
        ceil
    }
}

#[cfg(test)]
mod preprocessing_tests {
    use latexsnipper_image::PixelFormat;

    use super::*;

    #[test]
    fn rgba_decode_produces_one_gray_sample_per_pixel() {
        let image = SnipperImage::new(2, 1, PixelFormat::Rgba, vec![255, 0, 0, 17, 0, 255, 0, 99]);
        let result = to_grayscale(&to_rgb(&image));
        assert_eq!(result.pixels(), &[76, 150]);
    }

    #[test]
    fn bgra_channel_order_is_respected() {
        let image = SnipperImage::new(1, 1, PixelFormat::Bgra, vec![0, 0, 255, 42]);
        let result = to_grayscale(&to_rgb(&image));
        assert_eq!(result.pixels(), &[76]);
    }

    #[test]
    fn official_margin_crop_finds_normalized_dark_content() {
        let mut pixels = vec![255; 5 * 3 * 3];
        for x in 1..4 {
            let offset = ((5 + x) * 3) as usize;
            pixels[offset..offset + 3].fill(100);
        }
        let image = SnipperImage::new(5, 3, PixelFormat::Rgb, pixels);
        let cropped = crop_content_margin(&image);
        assert_eq!((cropped.width(), cropped.height()), (3, 1));
        assert!(cropped.pixels().iter().all(|value| *value == 100));
    }

    #[test]
    fn official_preprocess_uses_centered_black_padding() {
        let mut pixels = vec![255; 5 * 3];
        pixels[5 + 1..5 + 4].fill(100);
        let image = SnipperImage::new(5, 3, PixelFormat::Gray, pixels);
        let processed = preprocess_image(&image, 8).unwrap();
        assert_eq!((processed.width(), processed.height()), (8, 8));
        assert_eq!(processed.format(), PixelFormat::Gray);
        assert!(processed.pixels()[..16].iter().all(|value| *value == 0));
        assert!(processed.pixels()[24..40].iter().all(|value| *value == 100));
    }
}
