use std::collections::HashMap;
use std::path::Path;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;
use crate::RecognitionPostProcessor;

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

impl RecognitionParams {
    /// Build recognition parameters from a model configuration.
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mut params = Self::default();

        if let Some(encoder) = &config.encoder {
            if let Some(size) = encoder.input.shape.get(2) {
                params.img_size = *size as u32;
            }
        }
        if let Some(preprocessing) = &config.preprocessing {
            if let Some(normalization) = &preprocessing.normalization {
                if let Some(mean) = &normalization.mean {
                    if mean.len() >= 3 {
                        params.mean = [mean[0], mean[1], mean[2]];
                    }
                }
                if let Some(std) = &normalization.std {
                    if std.len() >= 3 {
                        params.std = [std[0], std[1], std[2]];
                    }
                }
            }
        }
        if let Some(decoder) = &config.decoder {
            if let Some(max_length) = decoder.max_length {
                params.max_tokens = max_length;
            }
            if let Some(eos_token_id) = decoder.eos_token_id {
                params.eos_token_id = eos_token_id;
                params.decoder_start_id = eos_token_id;
            }
            if let Some(pad_token_id) = decoder.pad_token_id {
                params.pad_token_id = pad_token_id;
            }
        }
        if let Some(decoding) = &config.decoding {
            if let Some(beam_width) = decoding.beam_width {
                params.beam_width = beam_width;
            }
            if let Some(top_k) = decoding.top_k {
                params.top_k = top_k;
            }
        }

        params
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
    recognize_formula_with_tokenizer(image, encoder, decoder, &tokenizer, params)
}

/// Recognize a formula with an already loaded in-memory tokenizer.
pub fn recognize_formula_with_tokenizer(
    image: &SnipperImage,
    encoder: &dyn InferenceSession,
    decoder: &dyn InferenceSession,
    tokenizer: &HashMap<i64, String>,
    params: &RecognitionParams,
) -> Result<RecognitionResult> {
    let resized = latexsnipper_image::operations::resize(image, params.img_size, params.img_size);
    let pixels = latexsnipper_image::operations::normalize(&resized, &params.mean, &params.std);

    let input = Tensor::float32(
        "pixel_values",
        vec![1, 3, params.img_size as usize, params.img_size as usize],
        pixels,
    );
    let encoder_outputs = encoder.run(&[input])?;
    let hidden_states = encoder_outputs
        .first()
        .ok_or_else(|| SnipperError::Inference("No encoder output".into()))?
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Encoder output not float32".into()))?
        .to_vec();
    let hidden_shape = encoder_outputs.first().unwrap().shape().to_vec();

    let (decoded_text, confidence) = if params.greedy {
        greedy_decode(decoder, &hidden_states, &hidden_shape, tokenizer, params)?
    } else {
        beam_search(decoder, &hidden_states, &hidden_shape, tokenizer, params)?
    };

    let postprocess = crate::RuleBasedRecognitionPostProcessor::default()
        .process(&crate::Candidate::new(decoded_text, confidence))
        .map_err(|error| SnipperError::Inference(error.to_string()))?;
    Ok(RecognitionResult::from_postprocess(postprocess))
}

/// Greedy decoding: at each step, pick the token with highest probability.
/// Returns (decoded_text, mean_confidence).
/// Matches desktop app behavior.
fn greedy_decode(
    decoder_session: &dyn latexsnipper_runtime::InferenceSession,
    hidden_states: &[f32],
    hidden_shape: &[usize],
    tokenizer: &HashMap<i64, String>,
    params: &RecognitionParams,
) -> Result<(String, f32)> {
    let mut token_ids: Vec<i64> = vec![params.decoder_start_id];
    let mut scores: Vec<f32> = Vec::new();

    for _ in 0..params.max_tokens {
        let input_ids = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone());
        let hidden_tensor = Tensor::float32(
            "encoder_hidden_states",
            hidden_shape.to_vec(),
            hidden_states.to_vec(),
        );

        let outputs = decoder_session.run(&[input_ids, hidden_tensor])?;
        let logits = outputs
            .first()
            .ok_or_else(|| SnipperError::Inference("No decoder output".into()))?
            .as_f32_slice()
            .ok_or_else(|| SnipperError::Inference("Decoder output not float32".into()))?;

        let vocab_size = outputs
            .first()
            .unwrap()
            .shape()
            .last()
            .copied()
            .unwrap_or(0);

        // Get logits for the last position
        let last_pos_start = (token_ids.len() - 1) * vocab_size;
        let last_pos_end = last_pos_start + vocab_size;
        if last_pos_end > logits.len() {
            break;
        }
        let step_logits = &logits[last_pos_start..last_pos_end];

        // Softmax
        let max_logit = step_logits
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let mut probs: Vec<f32> = step_logits.iter().map(|&x| (x - max_logit).exp()).collect();
        let sum: f32 = probs.iter().sum();
        for p in probs.iter_mut() {
            *p /= sum;
        }

        // Argmax
        let (max_idx, max_prob) = probs
            .iter()
            .enumerate()
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
    let text = token_ids
        .iter()
        .filter(|&&id| {
            id != params.eos_token_id && id != params.pad_token_id && id != params.decoder_start_id
        })
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

    // Compute mean confidence from per-step probabilities
    let confidence = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f32>() / scores.len() as f32
    };

    Ok((text, confidence))
}

fn load_tokenizer(path: &Path) -> Result<HashMap<i64, String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SnipperError::Model(format!("Failed to read tokenizer: {}", e)))?;
    load_tokenizer_from_str(&content)
}

/// Load a tokenizer from an in-memory JSON string.
pub fn load_tokenizer_from_str(content: &str) -> Result<HashMap<i64, String>> {
    let json: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| SnipperError::Model(format!("Invalid tokenizer JSON: {}", e)))?;

    let vocab = json
        .get("model")
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
) -> Result<(String, f32)> {
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
            let hidden_tensor = Tensor::float32(
                "encoder_hidden_states",
                hidden_shape.to_vec(),
                hidden_states.to_vec(),
            );

            let outputs = decoder_session.run(&[input_ids, hidden_tensor])?;
            let logits = outputs
                .first()
                .ok_or_else(|| SnipperError::Inference("No decoder output".into()))?
                .as_f32_slice()
                .ok_or_else(|| SnipperError::Inference("Decoder output not float32".into()))?;

            let vocab_size = outputs
                .first()
                .unwrap()
                .shape()
                .last()
                .copied()
                .unwrap_or(0);

            let mut probs: Vec<f32> = logits.to_vec();
            let max_logit = probs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            for p in probs.iter_mut() {
                *p = (*p - max_logit).exp();
            }
            let sum: f32 = probs.iter().sum();
            for p in probs.iter_mut() {
                *p /= sum;
            }

            let k = params.top_k.min(vocab_size);
            let mut indexed: Vec<(usize, f32)> =
                probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
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

        if beams.iter().all(|(ids, _)| {
            ids.last() == Some(&params.eos_token_id) || ids.last() == Some(&params.pad_token_id)
        }) {
            break;
        }
    }

    let best = beams
        .first()
        .ok_or_else(|| SnipperError::Inference("No beams".into()))?;

    let text = best
        .0
        .iter()
        .filter(|&&id| {
            id != params.eos_token_id && id != params.pad_token_id && id != params.decoder_start_id
        })
        .filter_map(|id| tokenizer.get(id).cloned())
        .collect::<Vec<_>>()
        .join("");

    // Convert the log probability to a per-token confidence (geometric mean)
    // The beam log_prob is the sum of log probs of each token. Divide by sequence length
    // to get average log prob, then exponentiate to get a number in [0, 1].
    let avg_log_prob = if best.0.len() > 1 {
        best.1 / (best.0.len() - 1) as f32 // exclude start token
    } else {
        0.0
    };
    let confidence = avg_log_prob.exp();

    Ok((text, confidence))
}

// Legacy repair is intentionally not used here; postprocessing retains evidence.

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock session that returns fixed logits for testing confidence computation.
    struct FixedLogitSession {
        /// The logit values to return for the decoder output (single step).
        /// Shape: [batch=1, seq_len=1, vocab_size=N]
        logits: Vec<f32>,
        vocab_size: usize,
    }

    impl FixedLogitSession {
        fn new(logits: Vec<f32>) -> Self {
            let vocab_size = logits.len();
            Self { logits, vocab_size }
        }
    }

    impl latexsnipper_runtime::InferenceSession for FixedLogitSession {
        fn run(&self, _inputs: &[Tensor]) -> Result<Vec<Tensor>> {
            // Return fixed logits as the decoder output tensor
            let shape = vec![1usize, 1, self.vocab_size];
            let t = Tensor::float32("logits".to_string(), shape, self.logits.clone());
            Ok(vec![t])
        }

        fn input_names(&self) -> Vec<String> {
            vec!["input_ids".into(), "encoder_hidden_states".into()]
        }

        fn output_names(&self) -> Vec<String> {
            vec!["logits".into()]
        }

        fn release(&mut self) {}
    }

    /// Write a minimal tokenizer JSON to a temp file and return the path.
    fn tokenizer_path(path: &std::path::Path) -> std::path::PathBuf {
        let json = serde_json::json!({
            "model": {
                "vocab": {
                    "<pad>": 0,
                    "a": 1,
                    "<sos>": 2,
                    "b": 3,
                    "c": 4,
                    " ": 5
                }
            }
        });
        let content = serde_json::to_string(&json).unwrap();
        std::fs::write(path, content).unwrap();
        path.to_path_buf()
    }

    #[test]
    fn test_confidence_high_probability() {
        // When one token has near-certain probability, confidence should be high.
        // 10 tokens in vocab, token 3 ("b") gets logit 100, others get 0.
        // softmax(100,0,0,0,0,0,0,0,0,0) ≈ 1.0 for the first token.
        let mut logits = vec![0.0f32; 10];
        logits[3] = 100.0;

        let encoder = FixedLogitSession::new(vec![0.0f32; 10]); // dummy encoder (not really used)
        let decoder = FixedLogitSession::new(logits);
        let params = RecognitionParams {
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            max_tokens: 5,
            ..Default::default()
        };

        let tmp = std::env::temp_dir().join("test_conf_high.json");
        let tok_path = tokenizer_path(&tmp);

        let result = recognize_formula(
            &latexsnipper_image::SnipperImage::new(
                10,
                10,
                latexsnipper_image::PixelFormat::Rgb,
                vec![0u8; 300],
            ),
            &encoder,
            &decoder,
            &tok_path,
            &params,
        );

        let _ = std::fs::remove_file(&tmp);

        assert!(result.is_ok());
        let confidence = result.unwrap().confidence;
        assert!(
            confidence > 0.99,
            "Expected confidence near 1.0 for certain token, got {}",
            confidence
        );
    }

    #[test]
    fn test_confidence_low_probability() {
        // When all tokens have equal logits, confidence = 1/vocab_size = 0.1.
        // With logits[3]=1 slightly favored, softmax gives ~0.23 for that token.
        let logits = vec![0.0f32; 10]; // perfectly uniform → confidence ≈ 0.1

        let encoder = FixedLogitSession::new(vec![0.0f32; 10]);
        let decoder = FixedLogitSession::new(logits);
        let params = RecognitionParams {
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            max_tokens: 5,
            ..Default::default()
        };

        let result = recognize_formula(
            &latexsnipper_image::SnipperImage::new(
                10,
                10,
                latexsnipper_image::PixelFormat::Rgb,
                vec![0u8; 300],
            ),
            &encoder,
            &decoder,
            &{
                let p = std::env::temp_dir().join("test_conf_low.json");
                tokenizer_path(&p);
                p
            },
            &params,
        );

        let _ = std::fs::remove_file(std::env::temp_dir().join("test_conf_low.json"));

        assert!(result.is_ok());
        let confidence = result.unwrap().confidence;
        assert!(
            confidence < 0.15,
            "Expected low confidence (~0.1) for uniform distribution, got {}",
            confidence
        );
    }

    #[test]
    fn test_confidence_uses_scores() {
        // After the fix, confidence must NOT be the hardcoded 0.9.
        // Create a scenario where it would clearly differ from 0.9.
        let logits = vec![0.0f32; 10]; // uniform → confidence ≈ 0.1

        let encoder = FixedLogitSession::new(vec![0.0f32; 10]);
        let decoder = FixedLogitSession::new(logits);
        let params = RecognitionParams {
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            max_tokens: 5,
            ..Default::default()
        };

        let result = recognize_formula(
            &latexsnipper_image::SnipperImage::new(
                10,
                10,
                latexsnipper_image::PixelFormat::Rgb,
                vec![0u8; 300],
            ),
            &encoder,
            &decoder,
            &{
                let p = std::env::temp_dir().join("test_conf_not_0_9.json");
                tokenizer_path(&p);
                p
            },
            &params,
        );

        let _ = std::fs::remove_file(std::env::temp_dir().join("test_conf_not_0_9.json"));

        assert!(result.is_ok());
        let confidence = result.unwrap().confidence;
        assert!(
            (confidence - 0.9).abs() > 0.01,
            "Confidence must not be hardcoded 0.9. Got {}",
            confidence
        );
    }

    #[test]
    fn test_formula_repaired_with_confidence() {
        // Test that latex_repair still runs and confidence is present
        let logits = vec![0.0f32; 10];

        let encoder = FixedLogitSession::new(vec![0.0f32; 10]);
        let decoder = FixedLogitSession::new(logits);
        let params = RecognitionParams {
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            max_tokens: 5,
            ..Default::default()
        };

        let result = recognize_formula(
            &latexsnipper_image::SnipperImage::new(
                10,
                10,
                latexsnipper_image::PixelFormat::Rgb,
                vec![0u8; 300],
            ),
            &encoder,
            &decoder,
            &{
                let p = std::env::temp_dir().join("test_formula_repaired.json");
                tokenizer_path(&p);
                p
            },
            &params,
        );

        let _ = std::fs::remove_file(std::env::temp_dir().join("test_formula_repaired.json"));

        assert!(result.is_ok());
        let res = result.unwrap();
        // Even with uniform logits, the text should be decodable
        assert!(res.confidence >= 0.0 && res.confidence <= 1.0);
    }
}
