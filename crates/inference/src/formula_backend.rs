//! Generic formula recognition backend trait and ONNX implementation.
//!
//! Architecture:
//! ```text
//! image
//!   → preprocess (resize + normalize)
//!   → EncoderSession.run(pixel_values) → hidden_states
//!   → DecoderSession.run(input_ids, hidden_states) → logits
//!   → greedy/beam decode → token_ids
//!   → vocab lookup → LaTeX string
//!   → postprocess (latex_repair)
//! ```
//!
//! Any encoder-decoder ONNX pair (TrOCR, PP-FormulaNet, UniMERNet, etc.)
//! can be plugged in by just providing the ONNX files + vocab + config.json.

use std::path::{Path, PathBuf};

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;

use crate::types::RecognitionResult;

/// Backend configuration loaded from config.json.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub img_size: u32,
    pub num_channels: usize,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub decoder_start_id: i64,
    pub eos_token_id: i64,
    pub pad_token_id: i64,
    pub max_tokens: usize,
    pub greedy: bool,
    pub beam_width: usize,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            img_size: 384,
            num_channels: 3,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            decoder_start_id: 2,
            eos_token_id: 2,
            pad_token_id: 0,
            max_tokens: 256,
            greedy: true,
            beam_width: 3,
        }
    }
}

impl BackendConfig {
    /// Build from a model config.
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mut c = Self::default();

        if let Some(input) = &config.input {
            if let Some(size) = input.shape.get(2) {
                c.img_size = *size as u32;
            }
            if let Some(ch) = input.shape.get(1) {
                c.num_channels = *ch as usize;
            }
        }
        if let Some(pre) = &config.preprocessing {
            if let Some(norm) = &pre.normalization {
                if let Some(mean) = &norm.mean {
                    if mean.len() >= 3 {
                        c.mean = [mean[0], mean[1], mean[2]];
                    } else if mean.len() == 1 {
                        // Single-channel (grayscale): replicate to 3 channels
                        c.mean = [mean[0], mean[0], mean[0]];
                    }
                }
                if let Some(std) = &norm.std {
                    if std.len() >= 3 {
                        c.std = [std[0], std[1], std[2]];
                    } else if std.len() == 1 {
                        c.std = [std[0], std[0], std[0]];
                    }
                }
            }
            if let Some(resize) = &pre.resize {
                if let Some(h) = resize.height {
                    c.img_size = h;
                }
            }
        }
        if let Some(dec) = &config.decoder {
            if let Some(v) = dec.eos_token_id {
                c.eos_token_id = v;
                c.decoder_start_id = v;
            }
            if let Some(v) = dec.pad_token_id {
                c.pad_token_id = v;
            }
        }
        if let Some(dec) = &config.decoding {
            if let Some(v) = dec.beam_width {
                c.beam_width = v;
            }
            if let Some(v) = dec.top_k {
                c.max_tokens = v.max(1);
            }
        }

        c
    }
}

/// Generic formula recognition backend trait.
///
/// Implement this for any formula model. The backend handles:
/// image → preprocess → encode → decode → LaTeX string.
pub trait FormulaBackend: Send + Sync {
    /// Recognize a formula from an image.
    fn recognize(&self, image: &SnipperImage) -> Result<RecognitionResult>;

    /// Backend name (for logging).
    fn name(&self) -> &str;

    /// Get the backend configuration.
    fn config(&self) -> &BackendConfig;
}

/// ONNX-based formula backend: encoder ONNX + decoder ONNX + vocab.
///
/// This is the generic implementation that works with any encoder-decoder ONNX pair.
/// The model-specific logic is all in the ONNX graphs; this backend just orchestrates.
///
/// Supported models (by just providing ONNX files):
/// - TrOCR (encoder + decoder ONNX)
/// - PP-FormulaNet (encoder + decoder ONNX)
/// - UniMERNet (encoder + decoder ONNX)
/// - Any future encoder-decoder ONNX model
pub struct OnnxFormulaBackend {
    name: String,
    encoder: Box<dyn InferenceSession>,
    decoder: Box<dyn InferenceSession>,
    vocab: Vec<String>,
    config: BackendConfig,
}

impl OnnxFormulaBackend {
    /// Create from loaded sessions and vocab.
    pub fn new(
        name: String,
        encoder: Box<dyn InferenceSession>,
        decoder: Box<dyn InferenceSession>,
        vocab: Vec<String>,
        config: BackendConfig,
    ) -> Self {
        Self {
            name,
            encoder,
            decoder,
            vocab,
            config,
        }
    }

    /// Load from model directory.
    ///
    /// Expects directory with:
    /// - `*.onnx` files (auto-detect encoder/decoder by name or order)
    /// - `vocab.txt` or `tokenizer.json`
    /// - `config.json`
    pub fn load(
        model_dir: &Path,
        runtime: &dyn latexsnipper_runtime::RuntimeBackend,
    ) -> Result<Self> {
        let config = BackendConfig::from_config(&latexsnipper_model::ModelConfig::load(model_dir)?);

        let encoder_path = find_onnx(model_dir, "encoder")?;
        let decoder_path = find_onnx(model_dir, "decoder")?;

        let encoder = runtime.create_session(
            &latexsnipper_runtime::ModelHandle::with_path("encoder", encoder_path),
            latexsnipper_runtime::AccelerationMode::Cpu,
        )?;
        let decoder = runtime.create_session(
            &latexsnipper_runtime::ModelHandle::with_path("decoder", decoder_path),
            latexsnipper_runtime::AccelerationMode::Cpu,
        )?;

        let vocab = match find_vocab(model_dir) {
            Ok(vocab_path) => load_vocab(&vocab_path)?,
            Err(_) => {
                log::warn!(
                    "No vocab file found in {}, recognition will return empty strings. \
                     Place vocab.txt or tokenizer.json in the model directory.",
                    model_dir.display()
                );
                Vec::new()
            }
        };

        let name = model_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        log::info!(
            "Loaded formula backend: {} (vocab={} tokens, img={})",
            name,
            vocab.len(),
            config.img_size
        );

        Ok(Self::new(name, encoder, decoder, vocab, config))
    }
}

impl FormulaBackend for OnnxFormulaBackend {
    fn recognize(&self, image: &SnipperImage) -> Result<RecognitionResult> {
        // 1. Preprocess
        let resized = latexsnipper_image::operations::resize(
            image,
            self.config.img_size,
            self.config.img_size,
        );
        let normalized = latexsnipper_image::operations::normalize(
            &resized,
            &self.config.mean,
            &self.config.std,
        );

        // Convert to grayscale if model expects single channel
        let pixels = if self.config.num_channels == 1 {
            rgb_to_grayscale(&normalized)
        } else {
            normalized
        };

        let input = latexsnipper_tensor::Tensor::float32(
            "pixel_values",
            vec![
                1,
                self.config.num_channels,
                self.config.img_size as usize,
                self.config.img_size as usize,
            ],
            pixels,
        );

        // 2. Encode
        let encoder_outputs = self.encoder.run(&[input])?;
        let hidden = encoder_outputs
            .first()
            .ok_or_else(|| SnipperError::Inference("No encoder output".into()))?;
        let hidden_data = hidden
            .as_f32_slice()
            .ok_or_else(|| SnipperError::Inference("Encoder output not float32".into()))?
            .to_vec();
        let raw_shape = hidden.shape().to_vec();

        // Some encoders (e.g., PP-FormulaNet-S PPHGNet_B4) output [B, D] instead
        // of [B, T, D]. Insert a sequence dimension so the decoder receives [B, 1, D].
        let hidden_shape = if raw_shape.len() == 2 {
            vec![raw_shape[0], 1, raw_shape[1]]
        } else {
            raw_shape
        };

        // 3. Decode
        let (token_ids, confidence) = if self.config.greedy {
            greedy_decode(&*self.decoder, &hidden_data, &hidden_shape, &self.config)?
        } else {
            beam_decode(&*self.decoder, &hidden_data, &hidden_shape, &self.config)?
        };

        // 4. Token IDs → text
        let text: String = token_ids
            .iter()
            .filter_map(|&id| self.vocab.get(id as usize).cloned())
            .collect();

        let text = crate::latex_repair::repair_latex(&text);

        Ok(RecognitionResult { text, confidence })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> &BackendConfig {
        &self.config
    }
}

// ─── Decoding ───────────────────────────────────────────────────────

fn greedy_decode(
    decoder: &dyn InferenceSession,
    hidden: &[f32],
    hidden_shape: &[usize],
    config: &BackendConfig,
) -> Result<(Vec<i64>, f32)> {
    let mut token_ids = vec![config.decoder_start_id];
    let mut scores = Vec::new();

    for _ in 0..config.max_tokens {
        let input = latexsnipper_tensor::Tensor::int64(
            "input_ids",
            vec![1, token_ids.len()],
            token_ids.clone(),
        );
        let h = latexsnipper_tensor::Tensor::float32(
            "encoder_hidden_states",
            hidden_shape.to_vec(),
            hidden.to_vec(),
        );

        let outputs = decoder.run(&[input, h])?;
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
        let last_pos = (token_ids.len() - 1) * vocab_size;
        if last_pos + vocab_size > logits.len() {
            break;
        }
        let step_logits = &logits[last_pos..last_pos + vocab_size];

        let max_val = step_logits
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let mut probs: Vec<f32> = step_logits.iter().map(|&x| (x - max_val).exp()).collect();

        // Repetition penalty: penalize repeated tokens to break
        // degenerate loops (even with proper causal mask, some
        // encoder-decoder models can get stuck).
        let last_id = *token_ids.last().unwrap_or(&0);
        let run_len = token_ids
            .iter()
            .rev()
            .take_while(|&&t| t == last_id)
            .count();
        if run_len >= 2 {
            probs[last_id as usize] *= 0.5_f32.powi(run_len as i32 - 1);
        }

        let sum: f32 = probs.iter().sum();
        for p in probs.iter_mut() {
            *p /= sum;
        }

        let (idx, prob) = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap_or((0, &0.0));

        scores.push(*prob);
        token_ids.push(idx as i64);

        if idx as i64 == config.eos_token_id || idx as i64 == config.pad_token_id {
            break;
        }
    }

    let confidence = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f32>() / scores.len() as f32
    };

    Ok((token_ids, confidence))
}

fn beam_decode(
    decoder: &dyn InferenceSession,
    hidden: &[f32],
    hidden_shape: &[usize],
    config: &BackendConfig,
) -> Result<(Vec<i64>, f32)> {
    let mut beams: Vec<(Vec<i64>, f32)> = vec![(vec![config.decoder_start_id], 0.0)];

    for _ in 0..config.max_tokens {
        let mut all_candidates = Vec::new();

        for (ids, log_prob) in &beams {
            let last = *ids.last().unwrap();
            if last == config.eos_token_id || last == config.pad_token_id {
                all_candidates.push((ids.clone(), *log_prob));
                continue;
            }

            let input =
                latexsnipper_tensor::Tensor::int64("input_ids", vec![1, ids.len()], ids.clone());
            let h = latexsnipper_tensor::Tensor::float32(
                "encoder_hidden_states",
                hidden_shape.to_vec(),
                hidden.to_vec(),
            );

            let outputs = decoder.run(&[input, h])?;
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
            let last_pos = (ids.len() - 1) * vocab_size;
            if last_pos + vocab_size > logits.len() {
                continue;
            }
            let step_logits = &logits[last_pos..last_pos + vocab_size];

            let max_val = step_logits
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
            let mut probs: Vec<f32> = step_logits.iter().map(|&x| (x - max_val).exp()).collect();

            // Repetition penalty (same as greedy_decode)
            let last_id = *ids.last().unwrap_or(&0);
            let run_len = ids.iter().rev().take_while(|&&t| t == last_id).count();
            if run_len >= 2 {
                probs[last_id as usize] *= 0.5_f32.powi(run_len as i32 - 1);
            }

            let sum: f32 = probs.iter().sum();
            for p in probs.iter_mut() {
                *p /= sum;
            }

            let k = config.beam_width.min(vocab_size);
            let mut indexed: Vec<(usize, f32)> =
                probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            indexed.truncate(k);

            for (idx, prob) in indexed {
                let mut new_ids = ids.clone();
                new_ids.push(idx as i64);
                all_candidates.push((new_ids, log_prob + prob.ln()));
            }
        }

        all_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        all_candidates.truncate(config.beam_width);
        beams = all_candidates;

        if beams.iter().all(|(ids, _)| {
            ids.last() == Some(&config.eos_token_id) || ids.last() == Some(&config.pad_token_id)
        }) {
            break;
        }
    }

    let best = beams
        .first()
        .ok_or_else(|| SnipperError::Inference("No beams".into()))?;

    let avg_log_prob = if best.0.len() > 1 {
        best.1 / (best.0.len() - 1) as f32
    } else {
        0.0
    };

    Ok((best.0.clone(), avg_log_prob.exp()))
}

/// Convert interleaved RGB pixel data to grayscale (single channel).
/// Input: [R,G,B, R,G,B, ...], Output: [Y, Y, ...]
/// Uses standard luminance weights: Y = 0.299R + 0.587G + 0.114B
pub fn rgb_to_grayscale(rgb: &[f32]) -> Vec<f32> {
    rgb.chunks(3)
        .map(|c| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2])
        .collect()
}

// ─── File discovery ─────────────────────────────────────────────────

/// Find the first ONNX file matching a keyword.
fn find_onnx(dir: &Path, keyword: &str) -> Result<PathBuf> {
    let candidates = [
        dir.join(format!("{}_model.onnx", keyword)),
        dir.join(format!("{}.onnx", keyword)),
    ];
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    // Fallback: find any .onnx file containing the keyword.
    // Collect all matches and sort for deterministic selection.
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut matches: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| {
                e.path().extension().is_some_and(|ext| ext == "onnx")
                    && e.path()
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .contains(keyword)
            })
            .map(|e| e.path())
            .collect();
        matches.sort();
        if let Some(path) = matches.into_iter().next() {
            return Ok(path);
        }
    }

    // Last resort: any .onnx file
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "onnx") {
                return Ok(path);
            }
        }
    }

    Err(SnipperError::Model(format!(
        "No ONNX file found for '{}' in {}",
        keyword,
        dir.display()
    )))
}

/// Find vocab file.
fn find_vocab(dir: &Path) -> Result<PathBuf> {
    let candidates = ["vocab.txt", "tokenizer.json", "ppocr_keys.txt", "dict.txt"];
    for name in &candidates {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(SnipperError::Model(format!(
        "No vocab file found in {}",
        dir.display()
    )))
}

/// Load vocab from a text file (one token per line) or JSON.
fn load_vocab(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SnipperError::Model(format!("Failed to read vocab: {}", e)))?;

    if path.extension().is_some_and(|e| e == "json") {
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| SnipperError::Model(format!("Invalid vocab JSON: {}", e)))?;

        if let Some(vocab) = json.get("model").and_then(|m| m.get("vocab")) {
            let mut tokens: Vec<(i64, String)> = Vec::new();
            for (token, id) in vocab.as_object().unwrap_or(&serde_json::Map::new()) {
                if let Some(id_val) = id.as_i64() {
                    tokens.push((id_val, token.clone()));
                }
            }
            tokens.sort_by_key(|(id, _)| *id);
            let max_id = tokens.last().map(|(id, _)| *id).unwrap_or(0) as usize;
            let mut result = vec![String::new(); max_id + 1];
            for (id, token) in tokens {
                result[id as usize] = token;
            }
            return Ok(result);
        }
    }

    // Plain text: one token per line
    let tokens: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    Ok(tokens)
}
