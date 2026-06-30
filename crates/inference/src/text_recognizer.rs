use std::path::Path;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;

/// Text recognition parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct TextRecParams {
    pub target_h: u32,
    pub max_w: u32,
    pub blank_id: usize,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Default for TextRecParams {
    fn default() -> Self {
        Self {
            target_h: 48,
            max_w: 320,
            blank_id: 0,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
        }
    }
}

/// Recognize text using CRNN + CTC decode.
pub fn recognize_text(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    keys_path: &Path,
    params: &TextRecParams,
) -> Result<RecognitionResult> {
    let keys = load_keys(keys_path)?;

    let (processed, _orig_w) = preprocess(image, params);

    let input = Tensor::float32(
        "x",
        vec![1, 3, params.target_h as usize, params.max_w as usize],
        latexsnipper_image::operations::normalize(&processed, &params.mean, &params.std),
    );
    let outputs = session.run(&[input])?;

    let output = outputs.first().ok_or_else(|| SnipperError::Inference("No output".into()))?;
    let logits = output.as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output not float32".into()))?;
    let shape = output.shape().to_vec();

    let (text, confidence) = ctc_decode(logits, &shape, &keys, params.blank_id);

    Ok(RecognitionResult { text, confidence })
}

fn preprocess(image: &SnipperImage, params: &TextRecParams) -> (SnipperImage, u32) {
    let w = image.width();
    let h = image.height();
    let orig_w = w;

    let scale = params.target_h as f32 / h as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_w = new_w.min(params.max_w);

    let resized = latexsnipper_image::operations::resize(image, new_w, params.target_h);

    let padded = if new_w < params.max_w {
        let bpp = resized.bytes_per_pixel();
        let mut pixels = resized.pixels().to_vec();
        let pad_bytes = ((params.max_w - new_w) * params.target_h * bpp as u32) as usize;
        pixels.extend(vec![0u8; pad_bytes]);
        SnipperImage::new(params.max_w, params.target_h, resized.format(), pixels)
    } else {
        resized
    };

    (padded, orig_w)
}

fn load_keys(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SnipperError::Model(format!("Failed to read keys: {}", e)))?;
    let keys: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    Ok(keys)
}

fn ctc_decode(logits: &[f32], shape: &[usize], keys: &[String], _blank_id: usize) -> (String, f32) {
    let seq_len = shape[1];
    let vocab_size = shape[2];

    let mut prev_id = 0usize;
    let mut result = String::new();
    let mut confidences = Vec::new();

    for t in 0..seq_len {
        let start = t * vocab_size;
        let end = start + vocab_size;
        if end > logits.len() { break; }

        let slice = &logits[start..end];

        // Find best token (argmax)
        let mut best_id = 0;
        let mut best_val = f32::NEG_INFINITY;
        for (i, &val) in slice.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_id = i;
            }
        }

        // PaddleOCR CTC: 0=blank, 1=space, 2+=character (offset by 2)
        if best_id != 0 && best_id != prev_id {
            if best_id >= 2 {
                if let Some(ch) = keys.get(best_id - 2) {
                    result.push_str(ch);
                    confidences.push(1.0);
                }
            } else if best_id == 1 {
                result.push(' ');
                confidences.push(1.0);
            }
        }
        prev_id = best_id;
    }

    let avg_confidence = if confidences.is_empty() { 0.0 }
    else { confidences.iter().sum::<f32>() / confidences.len() as f32 };

    (result, avg_confidence)
}
