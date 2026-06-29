use std::collections::HashMap;
use std::path::Path;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;

const IMG_SIZE: u32 = 384;
const BEAM_WIDTH: usize = 3;
const TOP_K: usize = 5;
const MAX_TOKENS: usize = 512;
const DECODER_START_ID: i64 = 2;
const EOS_ID: i64 = 2;
const PAD_ID: i64 = 0;

/// Recognize formula using TrOCR encoder + beam search decoder.
/// Inference only depends on Session trait.
pub fn recognize_formula(
    image: &SnipperImage,
    encoder: &dyn InferenceSession,
    decoder: &dyn InferenceSession,
    tokenizer_path: &std::path::Path,
) -> Result<RecognitionResult> {
    let tokenizer = load_tokenizer(tokenizer_path)?;

    let resized = latexsnipper_image::operations::resize(image, IMG_SIZE, IMG_SIZE);
    let mean = [0.5, 0.5, 0.5];
    let std = [0.5, 0.5, 0.5];
    let pixels = latexsnipper_image::operations::normalize(&resized, &mean, &std);

    let input = Tensor::float32("pixel_values", vec![1, 3, IMG_SIZE as usize, IMG_SIZE as usize], pixels);
    let encoder_outputs = encoder.run(&[input])?;
    let hidden_states = encoder_outputs.first()
        .ok_or_else(|| SnipperError::Inference("No encoder output".into()))?
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Encoder output not float32".into()))?
        .to_vec();
    let hidden_shape = encoder_outputs.first().unwrap().shape().to_vec();

    let text = beam_search(decoder, &hidden_states, &hidden_shape, &tokenizer)?;

    let text = repair_latex(&text);

    Ok(RecognitionResult { text, confidence: 0.9 })
}

fn load_tokenizer(path: &Path) -> Result<HashMap<i64, String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SnipperError::Model(format!("Failed to read tokenizer: {}", e)))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| SnipperError::Model(format!("Invalid tokenizer JSON: {}", e)))?;

    let vocab = json.get("model")
        .and_then(|m| m.get("vocab"))
        .ok_or_else(|| SnipperError::Model("Missing model.vocab in tokenizer".into()))?;

    let mut token_to_id: HashMap<String, i64> = HashMap::new();
    for (token, id) in vocab.as_object().unwrap() {
        if let Some(id_val) = id.as_i64() {
            token_to_id.insert(token.clone(), id_val);
        }
    }

    let mut id_to_token = HashMap::new();
    for (token, id) in token_to_id {
        id_to_token.insert(id, token);
    }

    Ok(id_to_token)
}

fn beam_search(
    decoder_session: &dyn latexsnipper_runtime::InferenceSession,
    hidden_states: &[f32],
    hidden_shape: &[usize],
    tokenizer: &HashMap<i64, String>,
) -> Result<String> {
    let mut beams: Vec<(Vec<i64>, f32)> = vec![(vec![DECODER_START_ID], 0.0)];

    for _ in 0..MAX_TOKENS {
        let mut all_candidates: Vec<(Vec<i64>, f32)> = Vec::new();

        for (token_ids, log_prob) in &beams {
            let last_token = *token_ids.last().unwrap();
            if last_token == EOS_ID || last_token == PAD_ID {
                all_candidates.push((token_ids.clone(), *log_prob));
                continue;
            }

            let input_ids = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone());
            let hidden_tensor = Tensor::float32("encoder_hidden_states", hidden_shape.to_vec(), hidden_states.to_vec());

            let outputs = decoder_session.run(&[input_ids, hidden_tensor])?;
            let logits = outputs.first()
                .ok_or_else(|| SnipperError::Inference("No decoder output".into()))?
                .as_f32_slice()
                .ok_or_else(|| SnipperError::Inference("Decoder output not float32".into()))?;

            let vocab_size = outputs.first().unwrap().shape().last().copied().unwrap_or(0);

            let mut probs: Vec<f32> = logits.to_vec();
            let max_logit = probs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            for p in probs.iter_mut() { *p = (*p - max_logit).exp(); }
            let sum: f32 = probs.iter().sum();
            for p in probs.iter_mut() { *p /= sum; }

            let k = TOP_K.min(vocab_size);
            let mut indexed: Vec<(usize, f32)> = probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            indexed.truncate(k);

            for (idx, prob) in indexed {
                let mut new_ids = token_ids.clone();
                new_ids.push(idx as i64);
                all_candidates.push((new_ids, log_prob + prob.ln()));
            }
        }

        all_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        all_candidates.truncate(BEAM_WIDTH);
        beams = all_candidates;

        if beams.iter().all(|(ids, _)| ids.last() == Some(&EOS_ID) || ids.last() == Some(&PAD_ID)) {
            break;
        }
    }

    let best = beams.first().ok_or_else(|| SnipperError::Inference("No beams".into()))?;

    let text = best.0.iter()
        .filter(|&&id| id != EOS_ID && id != PAD_ID && id != DECODER_START_ID)
        .filter_map(|id| tokenizer.get(id).cloned())
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}

fn repair_latex(text: &str) -> String {
    let mut result = text.to_string();
    result = result.trim_end_matches('\\').to_string();
    let open = result.matches('{').count();
    let close = result.matches('}').count();
    if open > close {
        for _ in 0..(open - close) { result.push('}'); }
    }
    if result.contains("\\frac{") && !result.contains("\\frac{}{") {
        result = result.replace("\\frac{", "\\frac{}{");
    }
    result
}
