//! Deterministic, evidence-based comparison of FP32 and INT8 result bundles.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INT8_CONTRACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionSupport {
    Executed,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeterminismClass {
    BitExact,
    NumericallyEquivalent,
    TokenEquivalent,
    SemanticEquivalent,
    RenderEquivalent,
    QualityRegression,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Int8Thresholds {
    pub schema_version: u32,
    pub policy_id: String,
    pub tensor_max_abs_error: f64,
    pub normalized_exact_match_minimum: f64,
    pub cer_maximum: f64,
    pub ter_maximum: f64,
    pub ast_parse_success_minimum: f64,
    pub semantic_mismatch_rate_maximum: f64,
    pub office_read_back_mismatch_rate_maximum: f64,
    pub hard_negative_fpr_maximum: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Int8QualityMetrics {
    pub normalized_exact_match: f64,
    pub cer: f64,
    pub ter: f64,
    pub ast_parse_success: f64,
    pub semantic_mismatch_rate: f64,
    pub office_read_back_mismatch_rate: f64,
    pub hard_negative_fpr: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeterministicResultBundle {
    pub schema_version: u32,
    pub support: ExecutionSupport,
    pub model_sha256: String,
    pub tokenizer_sha256: String,
    pub dataset_sha256: String,
    pub normalization_version: String,
    pub ast_canonicalizer_version: String,
    pub requested_provider: String,
    pub effective_provider: Option<String>,
    pub quantization: String,
    pub tensor: Vec<f64>,
    pub top_k_tokens: Vec<u32>,
    pub token_sequence: Vec<u32>,
    pub raw_latex: String,
    pub normalized_latex: String,
    pub corrected_latex: String,
    pub ast_semantic_sha256: String,
    pub conversion_sha256: String,
    pub normalized_omml_sha256: String,
    pub mathml_structural_sha256: String,
    pub typst_structural_sha256: String,
    pub office_payload_sha256: String,
    pub word_read_back_omml_sha256: String,
    pub quality: Int8QualityMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Int8Comparison {
    pub schema_version: u32,
    pub classification: DeterminismClass,
    pub validated: bool,
    pub max_abs_tensor_error: Option<f64>,
    pub effective_provider: Option<String>,
    pub fallback_detected: bool,
    pub mismatches: Vec<String>,
    pub evidence_sha256: String,
}

pub fn compare_int8(
    baseline: &DeterministicResultBundle,
    candidate: &DeterministicResultBundle,
    thresholds: &Int8Thresholds,
) -> Int8Comparison {
    let mut mismatches = Vec::new();
    let fallback_detected = candidate
        .effective_provider
        .as_deref()
        .is_some_and(|actual| !actual.eq_ignore_ascii_case(&candidate.requested_provider));
    if candidate.support == ExecutionSupport::Unsupported || candidate.effective_provider.is_none()
    {
        return comparison(
            DeterminismClass::Unsupported,
            false,
            None,
            candidate,
            fallback_detected,
            vec!["requested INT8 execution was not supported by the actual provider".to_owned()],
        );
    }
    if fallback_detected {
        mismatches.push("effective provider differs from requested provider".to_owned());
    }
    for (name, left, right) in [
        ("model", &baseline.model_sha256, &candidate.model_sha256),
        (
            "tokenizer",
            &baseline.tokenizer_sha256,
            &candidate.tokenizer_sha256,
        ),
        (
            "dataset",
            &baseline.dataset_sha256,
            &candidate.dataset_sha256,
        ),
        (
            "normalization",
            &baseline.normalization_version,
            &candidate.normalization_version,
        ),
        (
            "AST canonicalizer",
            &baseline.ast_canonicalizer_version,
            &candidate.ast_canonicalizer_version,
        ),
    ] {
        if left != right {
            mismatches.push(format!("{name} identity mismatch"));
        }
    }
    let max_abs_tensor_error = (baseline.tensor.len() == candidate.tensor.len()).then(|| {
        baseline
            .tensor
            .iter()
            .zip(&candidate.tensor)
            .map(|(left, right)| (left - right).abs())
            .fold(0.0, f64::max)
    });
    if max_abs_tensor_error.is_none() {
        mismatches.push("tensor shape mismatch".to_owned());
    }
    let bit_exact = baseline.tensor == candidate.tensor;
    let numeric =
        max_abs_tensor_error.is_some_and(|error| error <= thresholds.tensor_max_abs_error);
    let token_equivalent = baseline.top_k_tokens == candidate.top_k_tokens
        && baseline.token_sequence == candidate.token_sequence;
    let semantic_equivalent = baseline.normalized_latex == candidate.normalized_latex
        && baseline.corrected_latex == candidate.corrected_latex
        && baseline.ast_semantic_sha256 == candidate.ast_semantic_sha256
        && baseline.conversion_sha256 == candidate.conversion_sha256
        && baseline.normalized_omml_sha256 == candidate.normalized_omml_sha256
        && baseline.mathml_structural_sha256 == candidate.mathml_structural_sha256
        && baseline.typst_structural_sha256 == candidate.typst_structural_sha256;
    let render_equivalent = semantic_equivalent
        && baseline.office_payload_sha256 == candidate.office_payload_sha256
        && baseline.word_read_back_omml_sha256 == candidate.word_read_back_omml_sha256;
    let quality_ok = candidate.quality.normalized_exact_match
        >= thresholds.normalized_exact_match_minimum
        && candidate.quality.cer <= thresholds.cer_maximum
        && candidate.quality.ter <= thresholds.ter_maximum
        && candidate.quality.ast_parse_success >= thresholds.ast_parse_success_minimum
        && candidate.quality.semantic_mismatch_rate <= thresholds.semantic_mismatch_rate_maximum
        && candidate.quality.office_read_back_mismatch_rate
            <= thresholds.office_read_back_mismatch_rate_maximum
        && candidate.quality.hard_negative_fpr <= thresholds.hard_negative_fpr_maximum;
    if !semantic_equivalent {
        mismatches.push("AST or conversion semantic output mismatch".to_owned());
    }
    if !render_equivalent {
        mismatches.push("Office payload or Word read-back mismatch".to_owned());
    }
    if !quality_ok {
        mismatches.push("versioned INT8 quality threshold failed".to_owned());
    }
    let classification = if !quality_ok || !semantic_equivalent {
        DeterminismClass::QualityRegression
    } else if bit_exact && render_equivalent {
        DeterminismClass::BitExact
    } else if numeric && render_equivalent {
        DeterminismClass::NumericallyEquivalent
    } else if token_equivalent && render_equivalent {
        DeterminismClass::TokenEquivalent
    } else if semantic_equivalent && render_equivalent {
        DeterminismClass::SemanticEquivalent
    } else if render_equivalent {
        DeterminismClass::RenderEquivalent
    } else {
        DeterminismClass::QualityRegression
    };
    let validated = !fallback_detected
        && quality_ok
        && semantic_equivalent
        && render_equivalent
        && thresholds.schema_version == INT8_CONTRACT_SCHEMA_VERSION;
    comparison(
        classification,
        validated,
        max_abs_tensor_error,
        candidate,
        fallback_detected,
        mismatches,
    )
}

fn comparison(
    classification: DeterminismClass,
    validated: bool,
    max_abs_tensor_error: Option<f64>,
    candidate: &DeterministicResultBundle,
    fallback_detected: bool,
    mismatches: Vec<String>,
) -> Int8Comparison {
    let evidence_sha256 = serde_json::to_vec(candidate)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .unwrap_or_default();
    Int8Comparison {
        schema_version: INT8_CONTRACT_SCHEMA_VERSION,
        classification,
        validated,
        max_abs_tensor_error,
        effective_provider: candidate.effective_provider.clone(),
        fallback_detected,
        mismatches,
        evidence_sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thresholds() -> Int8Thresholds {
        Int8Thresholds {
            schema_version: 1,
            policy_id: "int8-quality-v1".to_owned(),
            tensor_max_abs_error: 0.02,
            normalized_exact_match_minimum: 0.99,
            cer_maximum: 0.01,
            ter_maximum: 0.01,
            ast_parse_success_minimum: 1.0,
            semantic_mismatch_rate_maximum: 0.0,
            office_read_back_mismatch_rate_maximum: 0.0,
            hard_negative_fpr_maximum: 0.01,
        }
    }

    fn bundle(provider: Option<&str>) -> DeterministicResultBundle {
        DeterministicResultBundle {
            schema_version: 1,
            support: ExecutionSupport::Executed,
            model_sha256: "a".repeat(64),
            tokenizer_sha256: "b".repeat(64),
            dataset_sha256: "c".repeat(64),
            normalization_version: "latex-v1".to_owned(),
            ast_canonicalizer_version: "ast-v1".to_owned(),
            requested_provider: "cpu".to_owned(),
            effective_provider: provider.map(str::to_owned),
            quantization: "int8".to_owned(),
            tensor: vec![0.1, 0.2],
            top_k_tokens: vec![1, 2],
            token_sequence: vec![1, 2],
            raw_latex: "x".to_owned(),
            normalized_latex: "x".to_owned(),
            corrected_latex: "x".to_owned(),
            ast_semantic_sha256: "d".repeat(64),
            conversion_sha256: "e".repeat(64),
            normalized_omml_sha256: "f".repeat(64),
            mathml_structural_sha256: "1".repeat(64),
            typst_structural_sha256: "2".repeat(64),
            office_payload_sha256: "3".repeat(64),
            word_read_back_omml_sha256: "f".repeat(64),
            quality: Int8QualityMetrics {
                normalized_exact_match: 1.0,
                cer: 0.0,
                ter: 0.0,
                ast_parse_success: 1.0,
                semantic_mismatch_rate: 0.0,
                office_read_back_mismatch_rate: 0.0,
                hard_negative_fpr: 0.0,
            },
        }
    }

    #[test]
    fn int8_compares_ast_omml_and_office_with_versioned_thresholds() {
        let baseline = bundle(Some("cpu"));
        let mut candidate = baseline.clone();
        candidate.quantization = "int8".to_owned();
        candidate.tensor[0] += 0.01;
        let result = compare_int8(&baseline, &candidate, &thresholds());
        assert!(result.validated);
        assert_eq!(
            result.classification,
            DeterminismClass::NumericallyEquivalent
        );
    }

    #[test]
    fn unsupported_int8_and_provider_fallback_never_pass() {
        let baseline = bundle(Some("cpu"));
        let mut unsupported = bundle(None);
        unsupported.support = ExecutionSupport::Unsupported;
        assert_eq!(
            compare_int8(&baseline, &unsupported, &thresholds()).classification,
            DeterminismClass::Unsupported
        );
        let mut fallback = bundle(Some("cpu"));
        fallback.requested_provider = "cuda".to_owned();
        let result = compare_int8(&baseline, &fallback, &thresholds());
        assert!(!result.validated);
        assert!(result.fallback_detected);
    }
}
