use std::path::Path;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;

const TARGET_H: u32 = 48;
const MAX_W: u32 = 320;

/// Recognize text using CRNN + CTC decode.
/// Inference only depends on Session trait.
pub fn recognize_text(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    keys_path: &Path,
) -> Result<RecognitionResult> {
    let keys = load_keys(keys_path)?;

    let (processed, _orig_w) = preprocess(image);

    let input = Tensor::float32(
        "input",
        vec![1, 3, TARGET_H as usize, MAX_W as usize],
        latexsnipper_image::operations::normalize(&processed, &[-1.0, -1.0, -1.0], &[2.0, 2.0, 2.0]),
    );
    let outputs = session.run(&[input])?;

    let output = outputs.first().ok_or_else(|| SnipperError::Inference("No output".into()))?;
    let logits = output.as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output not float32".into()))?;
    let shape = output.shape().to_vec();

    let (text, confidence) = ctc_decode(logits, &shape, &keys);

    Ok(RecognitionResult { text, confidence })
}

fn preprocess(image: &SnipperImage) -> (SnipperImage, u32) {
    let w = image.width();
    let h = image.height();
    let orig_w = w;

    let scale = TARGET_H as f32 / h as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_w = new_w.min(MAX_W);

    let resized = latexsnipper_image::operations::resize(image, new_w, TARGET_H);

    let padded = if new_w < MAX_W {
        let bpp = resized.bytes_per_pixel();
        let mut pixels = resized.pixels().to_vec();
        let pad_bytes = ((MAX_W - new_w) * TARGET_H * bpp as u32) as usize;
        pixels.extend(vec![0u8; pad_bytes]);
        SnipperImage::new(MAX_W, TARGET_H, resized.format(), pixels)
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

fn ctc_decode(logits: &[f32], shape: &[usize], keys: &[String]) -> (String, f32) {
    let seq_len = shape[1];
    let vocab_size = shape[2];
    let blank_id = 0;

    let mut prev_id = blank_id;
    let mut result = Vec::new();
    let mut confidences = Vec::new();

    for t in 0..seq_len {
        let start = t * vocab_size;
        let end = start + vocab_size;

        if end > logits.len() { break; }

        let mut max_id = 0;
        let mut max_val = f32::NEG_INFINITY;
        let mut sum_exp = 0.0f32;

        for (i, &val) in logits[start..end].iter().enumerate() {
            let exp_val = (val - logits[start..end].iter().cloned().fold(f32::NEG_INFINITY, f32::max)).exp();
            sum_exp += exp_val;
            if val > max_val { max_val = val; max_id = i; }
        }

        let confidence = (max_val - logits[start..end].iter().cloned().fold(f32::NEG_INFINITY, f32::max)).exp() / sum_exp;

        if max_id != blank_id && max_id != prev_id {
            if let Some(ch) = keys.get(max_id) {
                result.push(ch.clone());
                confidences.push(confidence);
            }
        }
        prev_id = max_id;
    }

    let text = result.join("");
    let avg_confidence = if confidences.is_empty() { 0.0 } else { confidences.iter().sum::<f32>() / confidences.len() as f32 };

    (text, avg_confidence)
}
