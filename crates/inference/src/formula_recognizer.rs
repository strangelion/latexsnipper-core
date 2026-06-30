use std::collections::HashMap;
use std::path::Path;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;
use crate::latex_repair;

/// Recognition parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct RecognitionParams {
    pub img_size: u32,
    pub beam_width: usize,
    pub top_k: usize,
    pub max_tokens: usize,
    pub decoder_start_id: i64,
    pub eos_token_id: i64,
    pub pad_token_id: i64,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    /// Use greedy decoding (argmax) instead of beam search.
    /// Desktop app uses greedy by default.
    pub greedy: bool,
}

impl Default for RecognitionParams {
    fn default() -> Self {
        Self {
            img_size: 384,
            beam_width: 3,
            top_k: 5,
            max_tokens: 256,
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            greedy: true,
        }
    }
}

/// Recognize formula using TrOCR encoder + decoder.
pub fn recognize_formula(
    image: &SnipperImage,
    encoder: &dyn InferenceSession,
    decoder: &dyn InferenceSession,
    tokenizer_path: &std::path::Path,
    params: &RecognitionParams,
) -> Result<RecognitionResult> {
    let tokenizer = load_tokenizer(tokenizer_path)?;

    let resized = latexsnipper_image::operations::resize(image, params.img_size, params.img_size);
    let pixels = latexsnipper_image::operations::normalize(&resized, &params.mean, &params.std);

    let input = Tensor::float32("pixel_values", vec![1, 3, params.img_size as usize, params.img_size as usize], pixels);
    let encoder_outputs = encoder.run(&[input])?;
    let hidden_states = encoder_outputs.first()
        .ok_or_else(|| SnipperError::Inference("No encoder output".into()))?
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Encoder output not float32".into()))?
        .to_vec();
    let hidden_shape = encoder_outputs.first().unwrap().shape().to_vec();

    let text = if params.greedy {
        greedy_decode(decoder, &hidden_states, &hidden_shape, &tokenizer, params)?
    } else {
        beam_search(decoder, &hidden_states, &hidden_shape, &tokenizer, params)?
    };

    let text = latex_repair::repair_latex(&text);

    Ok(RecognitionResult { text, confidence: 0.9 })
}

/// Greedy decoding: at each step, pick the token with highest probability.
/// Matches desktop app behavior.
fn greedy_decode(
    decoder_session: &dyn latexsnipper_runtime::InferenceSession,
    hidden_states: &[f32],
    hidden_shape: &[usize],
    tokenizer: &HashMap<i64, String>,
    params: &RecognitionParams,
) -> Result<String> {
    let mut token_ids: Vec<i64> = vec![params.decoder_start_id];
    let mut scores: Vec<f32> = Vec::new();

    for _ in 0..params.max_tokens {
        let input_ids = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone());
        let hidden_tensor = Tensor::float32("encoder_hidden_states", hidden_shape.to_vec(), hidden_states.to_vec());

        let outputs = decoder_session.run(&[input_ids, hidden_tensor])?;
        let logits = outputs.first()
            .ok_or_else(|| SnipperError::Inference("No decoder output".into()))?
            .as_f32_slice()
            .ok_or_else(|| SnipperError::Inference("Decoder output not float32".into()))?;

        let vocab_size = outputs.first().unwrap().shape().last().copied().unwrap_or(0);

        // Get logits for the last position
        let last_pos_start = (token_ids.len() - 1) * vocab_size;
        let last_pos_end = last_pos_start + vocab_size;
        if last_pos_end > logits.len() { break; }
        let step_logits = &logits[last_pos_start..last_pos_end];

        // Softmax
        let max_logit = step_logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mut probs: Vec<f32> = step_logits.iter().map(|&x| (x - max_logit).exp()).collect();
        let sum: f32 = probs.iter().sum();
        for p in probs.iter_mut() { *p /= sum; }

        // Argmax
        let (max_idx, max_prob) = probs.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap_or((0, &0.0));

        scores.push(*max_prob);
        token_ids.push(max_idx as i64);

        if max_idx as i64 == params.eos_token_id || max_idx as i64 == params.pad_token_id {
            break;
        }
    }

    // Decode tokens to text
    // Handle BPE tokens: Ġ prefix means space before character
    let text = token_ids.iter()
        .filter(|&&id| id != params.eos_token_id && id != params.pad_token_id && id != params.decoder_start_id)
        .filter_map(|id| tokenizer.get(id).cloned())
        .map(|token| {
            // BPE space prefix: Ā (U+0100) or Ġ (U+0120) — tokenizer.json uses Ā
            if token.starts_with('\u{0100}') || token.starts_with('\u{0120}') {
                let stripped: String = token.chars().skip(1).collect();
                format!(" {}", stripped)
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
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
    params: &RecognitionParams,
) -> Result<String> {
    let mut beams: Vec<(Vec<i64>, f32)> = vec![(vec![params.decoder_start_id], 0.0)];

    for _ in 0..params.max_tokens {
        let mut all_candidates: Vec<(Vec<i64>, f32)> = Vec::new();

        for (token_ids, log_prob) in &beams {
            let last_token = *token_ids.last().unwrap();
            if last_token == params.eos_token_id || last_token == params.pad_token_id {
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

            let k = params.top_k.min(vocab_size);
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
        all_candidates.truncate(params.beam_width);
        beams = all_candidates;

        if beams.iter().all(|(ids, _)| ids.last() == Some(&params.eos_token_id) || ids.last() == Some(&params.pad_token_id)) {
            break;
        }
    }

    let best = beams.first().ok_or_else(|| SnipperError::Inference("No beams".into()))?;

    let text = best.0.iter()
        .filter(|&&id| id != params.eos_token_id && id != params.pad_token_id && id != params.decoder_start_id)
        .filter_map(|id| tokenizer.get(id).cloned())
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}

// Old repair_latex removed — now using latex_repair::repair_latex
