//! PP-FormulaNet-S model adapter — Paddle Inference native full graph.
//!
//! The Paddle variant executes the exported full inference program: official
//! preprocessing, encoder, decoder parallel_step, while loop, KV cache, and
//! token generation all remain inside the PIR program.
//!
//! PP-FormulaNet-S does not support ONNX. Use TrOCR (ONNX Runtime) as the
//! formula recognition fallback when Paddle Inference is unavailable.

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
use crate::{
    Candidate, RecognitionPostProcessor, RecognitionProvenance, RuleBasedRecognitionPostProcessor,
    TransformationEvidence,
};

pub struct PPFormulaNetAdapter {
    name: String,
    model_id: String,
    variant_id: String,
    runtime: String,
    effective_provider: String,
    session: Box<dyn RuntimeSession>,
    input_name: String,
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
        if resolved.runtime != RuntimeKind::PaddleInference {
            return Err(SnipperError::Model(format!(
                "PP-FormulaNet-S requires Paddle Inference, got '{}'",
                resolved.runtime
            )));
        }

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

        let session = registry.create_resolved_session(resolved)?;
        let input_name = session
            .metadata()
            .inputs
            .first()
            .map(|spec| spec.name.clone())
            .ok_or_else(|| {
                SnipperError::Model("PP-FormulaNet Paddle program declares no input".to_owned())
            })?;
        let runtime = session.metadata().runtime.to_string();
        let effective_provider = session
            .metadata()
            .effective_provider
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());

        Ok(Self {
            name: "pp-formulanet-s".to_owned(),
            model_id: resolved.model_id.clone(),
            variant_id: resolved.variant_id.clone(),
            runtime,
            effective_provider,
            session,
            input_name,
            config: BackendConfig::from_config(model_config),
            tokenizer,
        })
    }

    pub fn variant_id(&self) -> &str {
        &self.variant_id
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
        let normalized = postprocess_unimernet_infer(&decoded);
        let official_transformation = (decoded != normalized).then(|| {
            TransformationEvidence::automatic(
                "paddle-unimernet-normalization",
                &decoded,
                &normalized,
                "apply the versioned official UniMERNet inference whitespace and Chinese wrapper normalization",
                "unimernet-infer-v1",
            )
        });
        let postprocess = RuleBasedRecognitionPostProcessor::default()
            .process(&Candidate::new(&normalized, 0.0))
            .map_err(|error| SnipperError::Inference(error.to_string()))?;
        let mut transformations = Vec::new();
        transformations.extend(official_transformation);
        transformations.extend(postprocess.transformations.clone());
        let mut result = RecognitionResult::from_postprocess(postprocess);
        result.raw_text = Some(decoded);
        result.normalized_text = Some(normalized);
        let normalized_confidence = result.confidence;
        result = result.with_provenance(RecognitionProvenance {
            model_id: self.model_id.clone(),
            model_version: self.variant_id.clone(),
            runtime: self.runtime.clone(),
            provider: self.effective_provider.clone(),
            source_region: None,
            raw_confidence: Some(0.0),
            normalized_confidence: Some(normalized_confidence),
            transformations,
        });
        Ok(result)
    }
}

impl FormulaBackend for PPFormulaNetAdapter {
    fn recognize(&self, image: &SnipperImage) -> Result<RecognitionResult> {
        let input = preprocessed_tensor(image, self.config.img_size, &self.input_name)?;
        let response = self.session.run(RunRequest::new(TensorMap::from([(
            self.input_name.clone(),
            input,
        )])))?;
        let output = response.first_output().ok_or_else(|| {
            SnipperError::Inference("PP-FormulaNet Paddle program returned no output".to_owned())
        })?;
        let token_ids = integer_token_ids(output)?;
        self.decode_tokens(&token_ids)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> &BackendConfig {
        &self.config
    }
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
            || matches!(
                (previous, next),
                (Some(left), Some(right)) if left.is_alphabetic() && right.is_alphabetic()
            );
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
}
