//! Runtime-neutral PP-FormulaNet-S model adapter.
//!
//! Paddle variants execute the exported full inference program once, so its
//! official while loop, parallel generation, and KV cache remain inside the
//! model. The explicitly declared ONNX fallback retains reconstructed
//! encoder/decoder generation in this adapter.

use std::path::Path;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    ResolvedRuntimeVariant, RunRequest, RuntimeKind, RuntimeRegistry, RuntimeSession, TensorMap,
};
use latexsnipper_tensor::{Tensor, TensorData};
use regex::Regex;

use crate::formula_backend::{BackendConfig, FormulaBackend};
use crate::pp_formulanet::preprocess_image;
use crate::types::RecognitionResult;

enum ExecutionPlan {
    PaddleFullGraph {
        session: Box<dyn RuntimeSession>,
        input_name: String,
    },
    OnnxReconstructed {
        encoder: Box<dyn RuntimeSession>,
        decoder: Box<dyn RuntimeSession>,
    },
}

pub struct PPFormulaNetAdapter {
    name: String,
    variant_id: String,
    execution: ExecutionPlan,
    config: BackendConfig,
    tokenizer: tokenizers::Tokenizer,
}

impl PPFormulaNetAdapter {
    pub fn from_resolved_variant(
        registry: &RuntimeRegistry,
        resolved: &ResolvedRuntimeVariant,
        model_dir: &Path,
        model_config: &ModelConfig,
    ) -> Result<Self> {
        let tokenizer_path = resolved
            .artifacts
            .files
            .get("tokenizer")
            .cloned()
            .unwrap_or_else(|| model_dir.join("tokenizer.json"));
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|error| {
            SnipperError::Model(format!(
                "failed to load PP-FormulaNet tokenizer {}: {error}",
                tokenizer_path.display()
            ))
        })?;

        let execution = match resolved.runtime {
            RuntimeKind::PaddleInference => {
                let session = registry.create_resolved_session(resolved)?;
                let input_name = session
                    .metadata()
                    .inputs
                    .first()
                    .map(|spec| spec.name.clone())
                    .ok_or_else(|| {
                        SnipperError::Model(
                            "PP-FormulaNet Paddle program declares no input".to_owned(),
                        )
                    })?;
                ExecutionPlan::PaddleFullGraph {
                    session,
                    input_name,
                }
            }
            RuntimeKind::OnnxRuntime => ExecutionPlan::OnnxReconstructed {
                encoder: create_onnx_role_session(registry, resolved, "encoder")?,
                decoder: create_onnx_role_session(registry, resolved, "decoder")?,
            },
            ref runtime => {
                return Err(SnipperError::Model(format!(
                    "PP-FormulaNet adapter does not support runtime '{runtime}'"
                )));
            }
        };

        Ok(Self {
            name: "pp-formulanet-s".to_owned(),
            variant_id: resolved.variant_id.clone(),
            execution,
            config: BackendConfig::from_config(model_config),
            tokenizer,
        })
    }

    pub fn variant_id(&self) -> &str {
        &self.variant_id
    }

    pub fn uses_official_full_graph(&self) -> bool {
        matches!(&self.execution, ExecutionPlan::PaddleFullGraph { .. })
    }

    fn recognize_paddle(
        &self,
        image: &SnipperImage,
        session: &dyn RuntimeSession,
        input_name: &str,
    ) -> Result<RecognitionResult> {
        let input = preprocessed_tensor(image, self.config.img_size, input_name)?;
        let response = session.run(RunRequest::new(TensorMap::from([(
            input_name.to_owned(),
            input,
        )])))?;
        let output = response.first_output().ok_or_else(|| {
            SnipperError::Inference("PP-FormulaNet Paddle program returned no output".to_owned())
        })?;
        let token_ids = integer_token_ids(output)?;
        self.decode_tokens(&token_ids)
    }

    fn recognize_onnx(
        &self,
        image: &SnipperImage,
        encoder: &dyn RuntimeSession,
        decoder: &dyn RuntimeSession,
    ) -> Result<RecognitionResult> {
        let encoder_input = preprocessed_tensor(image, self.config.img_size, "pixel_values")?;
        let encoder_response = encoder.run(RunRequest::new(TensorMap::from([(
            "pixel_values".to_owned(),
            encoder_input,
        )])))?;
        let hidden = encoder_response.first_output().ok_or_else(|| {
            SnipperError::Inference("PP-FormulaNet ONNX encoder returned no output".to_owned())
        })?;
        let hidden_data = hidden.as_f32_slice().ok_or_else(|| {
            SnipperError::Inference("PP-FormulaNet encoder output is not f32".to_owned())
        })?;

        let parallel_step = 3usize;
        let max_blocks = self.config.max_tokens / parallel_step;
        let mut token_ids = vec![0i64; parallel_step];
        for _ in 0..max_blocks {
            let request = RunRequest::new(TensorMap::from([
                (
                    "input_ids".to_owned(),
                    Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone()),
                ),
                (
                    "encoder_hidden_states".to_owned(),
                    Tensor::float32(
                        "encoder_hidden_states",
                        hidden.shape().to_vec(),
                        hidden_data.to_vec(),
                    ),
                ),
            ]));
            let response = decoder.run(request)?;
            let logits = response.first_output().ok_or_else(|| {
                SnipperError::Inference("PP-FormulaNet ONNX decoder returned no output".to_owned())
            })?;
            let values = logits.as_f32_slice().ok_or_else(|| {
                SnipperError::Inference("PP-FormulaNet decoder output is not f32".to_owned())
            })?;
            let vocab_size = *logits.shape().last().ok_or_else(|| {
                SnipperError::Inference("decoder logits have no vocabulary dimension".to_owned())
            })?;
            let sequence_length = logits.shape().get(1).copied().unwrap_or(token_ids.len());
            let first_position = sequence_length.saturating_sub(parallel_step);
            let mut next = Vec::with_capacity(parallel_step);
            for position in first_position..sequence_length {
                let start = position.checked_mul(vocab_size).ok_or_else(|| {
                    SnipperError::Inference("decoder logits offset overflow".to_owned())
                })?;
                let end = start.checked_add(vocab_size).ok_or_else(|| {
                    SnipperError::Inference("decoder logits offset overflow".to_owned())
                })?;
                let row = values.get(start..end).ok_or_else(|| {
                    SnipperError::Inference("decoder logits shape/data mismatch".to_owned())
                })?;
                next.push(argmax(row) as i64);
            }
            if next.is_empty() {
                break;
            }
            let reached_eos = next.contains(&self.config.eos_token_id);
            token_ids.extend(next);
            if reached_eos {
                break;
            }
        }
        self.decode_tokens(&token_ids[parallel_step..])
    }

    fn decode_tokens(&self, token_ids: &[i64]) -> Result<RecognitionResult> {
        let end = token_ids
            .iter()
            .position(|token| *token == self.config.eos_token_id)
            .map_or(token_ids.len(), |position| position + 1);
        let ids = token_ids[..end]
            .iter()
            .map(|token| {
                u32::try_from(*token).map_err(|_| {
                    SnipperError::Inference(format!("invalid negative token id {token}"))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let decoded = self.tokenizer.decode(&ids, true).map_err(|error| {
            SnipperError::Model(format!("PP-FormulaNet tokenizer decode failed: {error}"))
        })?;
        Ok(RecognitionResult {
            text: postprocess_unimernet_infer(&decoded),
            confidence: 0.0,
        })
    }
}

impl FormulaBackend for PPFormulaNetAdapter {
    fn recognize(&self, image: &SnipperImage) -> Result<RecognitionResult> {
        match &self.execution {
            ExecutionPlan::PaddleFullGraph {
                session,
                input_name,
            } => self.recognize_paddle(image, &**session, input_name),
            ExecutionPlan::OnnxReconstructed { encoder, decoder } => {
                self.recognize_onnx(image, &**encoder, &**decoder)
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> &BackendConfig {
        &self.config
    }
}

fn create_onnx_role_session(
    registry: &RuntimeRegistry,
    resolved: &ResolvedRuntimeVariant,
    role: &str,
) -> Result<Box<dyn RuntimeSession>> {
    if !resolved.artifacts.files.contains_key(role) {
        return Err(SnipperError::Model(format!(
            "PP-FormulaNet ONNX variant '{}' is missing '{role}'",
            resolved.variant_id
        )));
    }
    let mut options = resolved.options.clone();
    options.extra.insert("artifact".to_owned(), role.into());
    registry.create_session(&resolved.runtime, &resolved.artifacts, &options)
}

fn preprocessed_tensor(image: &SnipperImage, size: u32, name: &str) -> Result<Tensor> {
    let image = preprocess_image(image, size)?;
    let pixels = image
        .pixels()
        .iter()
        .map(|pixel| (*pixel as f32 / 255.0 - 0.7931) / 0.1738)
        .collect();
    Ok(Tensor::float32(
        name,
        vec![1, 1, size as usize, size as usize],
        pixels,
    ))
}

fn integer_token_ids(output: &Tensor) -> Result<Vec<i64>> {
    match output.data() {
        TensorData::Int64(tokens) => Ok(tokens.clone()),
        TensorData::Int32(tokens) => Ok(tokens.iter().copied().map(i64::from).collect()),
        other => Err(SnipperError::Inference(format!(
            "PP-FormulaNet full graph output must contain integer token ids, got {other:?}"
        ))),
    }
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map_or(0, |(index, _)| index)
}

/// Rust port of the whitespace and Chinese-wrapper normalization used by
/// PaddleOCR's UniMERNetDecode in inference mode. Tokenizer special tokens are
/// already removed by `decode(..., true)` before this step.
pub fn postprocess_unimernet_infer(decoded: &str) -> String {
    let chinese_wrapper = Regex::new(r#"\\text\s*\{\s*([^}]*[\p{Han}][^}]*)\s*\}"#)
        .expect("static UniMERNet regular expression");
    let unwrapped = chinese_wrapper.replace_all(decoded, "$1").replace('"', "");
    normalize_formula_whitespace(&unwrapped)
}

fn normalize_formula_whitespace(value: &str) -> String {
    let characters: Vec<char> = value.chars().collect();
    let mut result = String::with_capacity(value.len());
    let mut index = 0;
    while index < characters.len() {
        if !characters[index].is_whitespace() {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        let previous = result.chars().next_back();
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        let next = characters.get(index).copied();
        let preserve = matches!((previous, next), (Some('\\'), _))
            || matches!((previous, next), (Some(left), Some(right)) if left.is_alphabetic() && right.is_alphabetic());
        if preserve && !result.ends_with(' ') {
            result.push(' ');
        }
    }
    result.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_token_outputs_accept_i64_and_i32() {
        assert_eq!(
            integer_token_ids(&Tensor::int64("tokens", vec![1, 2], vec![4, 2])).unwrap(),
            vec![4, 2]
        );
        assert_eq!(
            integer_token_ids(&Tensor::int32("tokens", vec![1, 2], vec![4, 2])).unwrap(),
            vec![4, 2]
        );
    }

    #[test]
    fn official_inference_space_rules_are_deterministic() {
        assert_eq!(
            postprocess_unimernet_infer(r#" \frac { a + b } { 2 } "#),
            r#"\frac{a+b}{2}"#
        );
        assert_eq!(postprocess_unimernet_infer(r#"\sin x"#), r#"\sin x"#);
        assert_eq!(postprocess_unimernet_infer(r#"\text{ 中文 }"#), "中文");
    }

    #[test]
    fn argmax_uses_total_float_order() {
        assert_eq!(argmax(&[-2.0, 3.0, 1.0]), 1);
    }
}
