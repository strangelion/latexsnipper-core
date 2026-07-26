//! Versioned, reproducible formula-recognition benchmark contracts.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const FORMULA_BENCHMARK_SCHEMA_VERSION: u32 = 1;
pub const FORMULA_PREDICTION_SCHEMA_VERSION: u32 = 1;
pub const FORMULA_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaBenchmarkManifest {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    pub normalization_version: String,
    pub seed: u64,
    #[serde(default = "default_minimum_sample_count")]
    pub minimum_sample_count: usize,
    pub samples: Vec<FormulaBenchmarkSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaBenchmarkSample {
    pub id: String,
    pub image: String,
    pub image_sha256: String,
    pub ground_truth_latex: String,
    pub normalized_ground_truth: String,
    pub category: Vec<String>,
    pub source: FormulaSampleSource,
    pub license: String,
    pub difficulty: FormulaDifficulty,
    pub image_quality: String,
    #[serde(default)]
    pub expected_kind: FormulaExpectedKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation: Option<String>,
    #[serde(default)]
    pub notes: String,
}

fn default_minimum_sample_count() -> usize {
    50
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaExpectedKind {
    #[default]
    Formula,
    HardNegative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaSampleSource {
    Synthetic,
    Real,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaDifficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizationRules {
    pub schema_version: u32,
    pub version: String,
    #[serde(default)]
    pub trim: bool,
    #[serde(default)]
    pub strip_outer_math_delimiters: bool,
    #[serde(default)]
    pub collapse_ascii_whitespace: bool,
    #[serde(default)]
    pub command_aliases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaPredictionBundle {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    pub metadata: FormulaRunMetadata,
    pub predictions: Vec<FormulaPrediction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaRunMetadata {
    pub core_commit: String,
    pub model_id: String,
    pub model_version: String,
    pub model_sha256: String,
    pub runtime: String,
    pub runtime_version: String,
    pub provider: String,
    pub os: String,
    pub cpu: String,
    pub gpu: Option<String>,
    pub thread_count: usize,
    pub warmup_iterations: usize,
    pub seed: u64,
    pub timestamp_utc: String,
    pub model_load_ms: f64,
    pub peak_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaPrediction {
    pub sample_id: String,
    pub raw_latex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_latex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected_latex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub correction_triggered: bool,
    #[serde(default)]
    pub review_required: bool,
    #[serde(default)]
    pub top_k: Vec<String>,
    pub latency_ms: f64,
    #[serde(default)]
    pub premature_eos: bool,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaBenchmarkReport {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    pub normalization_version: String,
    pub metadata: FormulaRunMetadata,
    pub metrics: FormulaMetrics,
    pub by_category: BTreeMap<String, FormulaMetrics>,
    pub by_image_quality: BTreeMap<String, FormulaMetrics>,
    pub by_screenshot_scale: BTreeMap<String, FormulaMetrics>,
    pub by_degradation: BTreeMap<String, FormulaMetrics>,
    pub by_sequence_length: BTreeMap<String, FormulaMetrics>,
    pub samples: Vec<FormulaSampleResult>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaMetrics {
    pub sample_count: usize,
    pub formula_sample_count: usize,
    pub hard_negative_sample_count: usize,
    pub exact_match: f64,
    pub normalized_exact_match: f64,
    pub character_error_rate: f64,
    pub token_error_rate: f64,
    pub latex_parse_success: f64,
    pub ast_structure_validity: f64,
    pub balanced_delimiter_rate: f64,
    pub repeated_token_failure_rate: f64,
    pub premature_eos_rate: f64,
    pub truncation_rate: f64,
    pub top_1_agreement: f64,
    pub top_5_agreement: f64,
    pub false_positive_rate: f64,
    pub false_negative_rate: f64,
    pub correction_trigger_rate: f64,
    pub correction_improvement_rate: f64,
    pub correction_regression_rate: f64,
    pub review_required_rate: f64,
    pub confidence_sample_count: usize,
    pub confidence_calibration_error: f64,
    pub cold_latency_ms: f64,
    pub warm_latency_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaSampleResult {
    pub sample_id: String,
    pub categories: Vec<String>,
    pub image_quality: String,
    pub expected_kind: FormulaExpectedKind,
    pub screenshot_scale: Option<String>,
    pub degradation: Option<String>,
    pub sequence_length: usize,
    pub raw: String,
    pub normalized_raw: String,
    pub corrected: String,
    pub expected: String,
    pub actual: String,
    pub normalized_expected: String,
    pub normalized_actual: String,
    pub exact_match: bool,
    pub normalized_exact_match: bool,
    pub raw_normalized_exact_match: bool,
    pub correction_triggered: bool,
    pub correction_improved: bool,
    pub correction_regressed: bool,
    pub review_required: bool,
    pub confidence: Option<f64>,
    pub false_positive: bool,
    pub false_negative: bool,
    pub character_edits: usize,
    pub character_count: usize,
    pub token_edits: usize,
    pub token_count: usize,
    pub latex_parse_success: bool,
    pub ast_structure_valid: bool,
    pub balanced_delimiters: bool,
    pub repeated_token_failure: bool,
    pub premature_eos: bool,
    pub truncated: bool,
    pub top_1_agreement: bool,
    pub top_5_agreement: bool,
    pub latency_ms: f64,
}

#[derive(Debug, Error)]
pub enum FormulaBenchmarkError {
    #[error("unsupported formula benchmark schema version {0}")]
    UnsupportedManifestSchema(u32),
    #[error("unsupported formula prediction schema version {0}")]
    UnsupportedPredictionSchema(u32),
    #[error("unsupported normalization schema version {0}")]
    UnsupportedNormalizationSchema(u32),
    #[error("normalization version mismatch: manifest '{manifest}', rules '{rules}'")]
    NormalizationVersionMismatch { manifest: String, rules: String },
    #[error("prediction dataset '{actual}' does not match manifest '{expected}'")]
    DatasetMismatch { expected: String, actual: String },
    #[error("duplicate benchmark sample '{0}'")]
    DuplicateSample(String),
    #[error("duplicate prediction for sample '{0}'")]
    DuplicatePrediction(String),
    #[error("missing prediction for sample '{0}'")]
    MissingPrediction(String),
    #[error("prediction references unknown sample '{0}'")]
    UnknownPrediction(String),
    #[error("benchmark must contain at least 50 samples, found {0}")]
    TooFewSamples(usize),
    #[error("sample '{0}' has no category")]
    MissingCategory(String),
    #[error("sample '{0}' has no license")]
    MissingLicense(String),
    #[error("sample '{0}' normalized ground truth does not match normalization rules")]
    StaleNormalizedGroundTruth(String),
    #[error("invalid latency for sample '{0}'")]
    InvalidLatency(String),
    #[error("sample '{sample}' has unsafe image path '{path}'")]
    UnsafeImagePath { sample: String, path: String },
    #[error("failed to read image for sample '{sample}' at '{path}': {source}")]
    ImageIo {
        sample: String,
        path: String,
        source: std::io::Error,
    },
    #[error("image checksum mismatch for sample '{sample}': expected {expected}, found {actual}")]
    ImageHashMismatch {
        sample: String,
        expected: String,
        actual: String,
    },
    #[error("failed to write CSV '{path}': {source}")]
    CsvIo {
        path: String,
        source: std::io::Error,
    },
}

pub fn evaluate_formula_benchmark(
    manifest: &FormulaBenchmarkManifest,
    bundle: &FormulaPredictionBundle,
    rules: &NormalizationRules,
) -> Result<FormulaBenchmarkReport, FormulaBenchmarkError> {
    validate_contract(manifest, bundle, rules)?;

    let predictions: HashMap<_, _> = bundle
        .predictions
        .iter()
        .map(|prediction| (prediction.sample_id.as_str(), prediction))
        .collect();
    let mut samples = Vec::with_capacity(manifest.samples.len());
    for sample in &manifest.samples {
        let prediction = predictions
            .get(sample.id.as_str())
            .expect("prediction completeness was validated");
        samples.push(evaluate_sample(sample, prediction, rules));
    }

    let metrics = aggregate(samples.iter());
    let by_category = aggregate_groups(&samples, |sample| {
        sample.categories.iter().map(String::as_str).collect()
    });
    let by_image_quality = aggregate_groups(&samples, |sample| vec![sample.image_quality.as_str()]);
    let by_screenshot_scale =
        aggregate_optional_groups(&samples, |sample| sample.screenshot_scale.as_deref());
    let by_degradation =
        aggregate_optional_groups(&samples, |sample| sample.degradation.as_deref());
    let by_sequence_length = aggregate_groups(&samples, |sample| {
        vec![sequence_length_bucket(sample.sequence_length)]
    });

    Ok(FormulaBenchmarkReport {
        schema_version: FORMULA_REPORT_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id.clone(),
        dataset_version: manifest.dataset_version.clone(),
        normalization_version: manifest.normalization_version.clone(),
        metadata: bundle.metadata.clone(),
        metrics,
        by_category,
        by_image_quality,
        by_screenshot_scale,
        by_degradation,
        by_sequence_length,
        samples,
    })
}

pub fn validate_formula_manifest_files(
    manifest_path: &Path,
    manifest: &FormulaBenchmarkManifest,
) -> Result<(), FormulaBenchmarkError> {
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    for sample in &manifest.samples {
        let relative = Path::new(&sample.image);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(FormulaBenchmarkError::UnsafeImagePath {
                sample: sample.id.clone(),
                path: sample.image.clone(),
            });
        }
        let path = root.join(relative);
        let bytes = fs::read(&path).map_err(|source| FormulaBenchmarkError::ImageIo {
            sample: sample.id.clone(),
            path: path.display().to_string(),
            source,
        })?;
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != sample.image_sha256 {
            return Err(FormulaBenchmarkError::ImageHashMismatch {
                sample: sample.id.clone(),
                expected: sample.image_sha256.clone(),
                actual,
            });
        }
    }
    Ok(())
}

pub fn normalize_formula(value: &str, rules: &NormalizationRules) -> String {
    let mut normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    if rules.trim {
        normalized = normalized.trim().to_owned();
    }
    if rules.strip_outer_math_delimiters {
        normalized = strip_outer_math_delimiters(&normalized).to_owned();
    }
    if rules.collapse_ascii_whitespace {
        normalized = normalized
            .split_ascii_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
    }
    for (from, to) in &rules.command_aliases {
        normalized = normalized.replace(from, to);
    }
    normalized
}

pub fn write_formula_csv(
    path: &Path,
    report: &FormulaBenchmarkReport,
) -> Result<(), FormulaBenchmarkError> {
    let mut csv = String::from(
        "sample_id,categories,image_quality,expected_kind,screenshot_scale,degradation,\
sequence_length,exact_match,normalized_exact_match,raw_normalized_exact_match,\
correction_triggered,correction_improved,correction_regressed,review_required,confidence,\
false_positive,false_negative,\
character_edits,character_count,token_edits,token_count,latex_parse_success,\
ast_structure_valid,balanced_delimiters,repeated_token_failure,premature_eos,truncated,\
top_1_agreement,top_5_agreement,latency_ms,expected,raw,normalized_raw,corrected,actual\n",
    );
    for sample in &report.samples {
        let fields = [
            csv_escape(&sample.sample_id),
            csv_escape(&sample.categories.join("|")),
            csv_escape(&sample.image_quality),
            format!("{:?}", sample.expected_kind).to_ascii_lowercase(),
            csv_escape(sample.screenshot_scale.as_deref().unwrap_or("")),
            csv_escape(sample.degradation.as_deref().unwrap_or("")),
            sample.sequence_length.to_string(),
            sample.exact_match.to_string(),
            sample.normalized_exact_match.to_string(),
            sample.raw_normalized_exact_match.to_string(),
            sample.correction_triggered.to_string(),
            sample.correction_improved.to_string(),
            sample.correction_regressed.to_string(),
            sample.review_required.to_string(),
            sample
                .confidence
                .map(|value| value.to_string())
                .unwrap_or_default(),
            sample.false_positive.to_string(),
            sample.false_negative.to_string(),
            sample.character_edits.to_string(),
            sample.character_count.to_string(),
            sample.token_edits.to_string(),
            sample.token_count.to_string(),
            sample.latex_parse_success.to_string(),
            sample.ast_structure_valid.to_string(),
            sample.balanced_delimiters.to_string(),
            sample.repeated_token_failure.to_string(),
            sample.premature_eos.to_string(),
            sample.truncated.to_string(),
            sample.top_1_agreement.to_string(),
            sample.top_5_agreement.to_string(),
            sample.latency_ms.to_string(),
            csv_escape(&sample.expected),
            csv_escape(&sample.raw),
            csv_escape(&sample.normalized_raw),
            csv_escape(&sample.corrected),
            csv_escape(&sample.actual),
        ];
        csv.push_str(&fields.join(","));
        csv.push('\n');
    }
    fs::write(path, csv).map_err(|source| FormulaBenchmarkError::CsvIo {
        path: path.display().to_string(),
        source,
    })
}

fn validate_contract(
    manifest: &FormulaBenchmarkManifest,
    bundle: &FormulaPredictionBundle,
    rules: &NormalizationRules,
) -> Result<(), FormulaBenchmarkError> {
    if manifest.schema_version != FORMULA_BENCHMARK_SCHEMA_VERSION {
        return Err(FormulaBenchmarkError::UnsupportedManifestSchema(
            manifest.schema_version,
        ));
    }
    if bundle.schema_version != FORMULA_PREDICTION_SCHEMA_VERSION {
        return Err(FormulaBenchmarkError::UnsupportedPredictionSchema(
            bundle.schema_version,
        ));
    }
    if rules.schema_version != 1 {
        return Err(FormulaBenchmarkError::UnsupportedNormalizationSchema(
            rules.schema_version,
        ));
    }
    if manifest.normalization_version != rules.version {
        return Err(FormulaBenchmarkError::NormalizationVersionMismatch {
            manifest: manifest.normalization_version.clone(),
            rules: rules.version.clone(),
        });
    }
    if manifest.samples.len() < manifest.minimum_sample_count {
        return Err(FormulaBenchmarkError::TooFewSamples(manifest.samples.len()));
    }
    if manifest.dataset_id != bundle.dataset_id
        || manifest.dataset_version != bundle.dataset_version
    {
        return Err(FormulaBenchmarkError::DatasetMismatch {
            expected: format!("{}@{}", manifest.dataset_id, manifest.dataset_version),
            actual: format!("{}@{}", bundle.dataset_id, bundle.dataset_version),
        });
    }

    let mut sample_ids = BTreeSet::new();
    for sample in &manifest.samples {
        if !sample_ids.insert(sample.id.as_str()) {
            return Err(FormulaBenchmarkError::DuplicateSample(sample.id.clone()));
        }
        if sample.category.is_empty() {
            return Err(FormulaBenchmarkError::MissingCategory(sample.id.clone()));
        }
        if sample.license.trim().is_empty() {
            return Err(FormulaBenchmarkError::MissingLicense(sample.id.clone()));
        }
        if normalize_formula(&sample.ground_truth_latex, rules) != sample.normalized_ground_truth {
            return Err(FormulaBenchmarkError::StaleNormalizedGroundTruth(
                sample.id.clone(),
            ));
        }
    }

    let mut prediction_ids = BTreeSet::new();
    for prediction in &bundle.predictions {
        if !prediction.latency_ms.is_finite() || prediction.latency_ms < 0.0 {
            return Err(FormulaBenchmarkError::InvalidLatency(
                prediction.sample_id.clone(),
            ));
        }
        if !prediction_ids.insert(prediction.sample_id.as_str()) {
            return Err(FormulaBenchmarkError::DuplicatePrediction(
                prediction.sample_id.clone(),
            ));
        }
        if !sample_ids.contains(prediction.sample_id.as_str()) {
            return Err(FormulaBenchmarkError::UnknownPrediction(
                prediction.sample_id.clone(),
            ));
        }
    }
    if let Some(missing) = sample_ids.difference(&prediction_ids).next() {
        return Err(FormulaBenchmarkError::MissingPrediction(
            (*missing).to_owned(),
        ));
    }
    Ok(())
}

fn evaluate_sample(
    sample: &FormulaBenchmarkSample,
    prediction: &FormulaPrediction,
    rules: &NormalizationRules,
) -> FormulaSampleResult {
    let normalized_raw = prediction
        .normalized_latex
        .clone()
        .unwrap_or_else(|| normalize_formula(&prediction.raw_latex, rules));
    let corrected = prediction
        .corrected_latex
        .as_deref()
        .unwrap_or(&prediction.raw_latex)
        .to_owned();
    let normalized_actual = normalize_formula(&corrected, rules);
    let expected_chars: Vec<_> = sample.normalized_ground_truth.chars().collect();
    let actual_chars: Vec<_> = normalized_actual.chars().collect();
    let expected_tokens = formula_tokens(&sample.normalized_ground_truth);
    let actual_tokens = formula_tokens(&normalized_actual);
    let balanced_delimiters = delimiters_balanced(&corrected);
    let latex_parse_success =
        latexsnipper_inference::parse_formula_latex(&normalized_actual).is_ok();
    let top_k: Vec<_> = prediction
        .top_k
        .iter()
        .map(|candidate| normalize_formula(candidate, rules))
        .collect();

    let raw_normalized_exact_match = sample.expected_kind == FormulaExpectedKind::Formula
        && sample.normalized_ground_truth == normalized_raw;
    let normalized_exact_match = sample.expected_kind == FormulaExpectedKind::Formula
        && sample.normalized_ground_truth == normalized_actual;
    let correction_triggered = prediction.correction_triggered || prediction.raw_latex != corrected;
    let non_empty_prediction = !normalized_actual.trim().is_empty();
    FormulaSampleResult {
        sample_id: sample.id.clone(),
        categories: sample.category.clone(),
        image_quality: sample.image_quality.clone(),
        expected_kind: sample.expected_kind,
        screenshot_scale: sample.screenshot_scale.clone(),
        degradation: sample.degradation.clone(),
        sequence_length: expected_tokens.len(),
        raw: prediction.raw_latex.clone(),
        normalized_raw,
        corrected: corrected.clone(),
        expected: sample.ground_truth_latex.clone(),
        actual: corrected.clone(),
        normalized_expected: sample.normalized_ground_truth.clone(),
        normalized_actual: normalized_actual.clone(),
        exact_match: sample.expected_kind == FormulaExpectedKind::Formula
            && sample.ground_truth_latex == corrected,
        normalized_exact_match,
        raw_normalized_exact_match,
        correction_triggered,
        correction_improved: correction_triggered
            && !raw_normalized_exact_match
            && normalized_exact_match,
        correction_regressed: correction_triggered
            && raw_normalized_exact_match
            && !normalized_exact_match,
        review_required: prediction.review_required,
        confidence: prediction.confidence,
        false_positive: sample.expected_kind == FormulaExpectedKind::HardNegative
            && non_empty_prediction,
        false_negative: sample.expected_kind == FormulaExpectedKind::Formula
            && !non_empty_prediction,
        character_edits: levenshtein(&expected_chars, &actual_chars),
        character_count: expected_chars.len(),
        token_edits: levenshtein(&expected_tokens, &actual_tokens),
        token_count: expected_tokens.len(),
        latex_parse_success,
        ast_structure_valid: latex_parse_success && balanced_delimiters,
        balanced_delimiters,
        repeated_token_failure: repeated_token_failure(&actual_tokens),
        premature_eos: prediction.premature_eos,
        truncated: prediction.truncated,
        top_1_agreement: top_k
            .first()
            .is_some_and(|candidate| candidate == &sample.normalized_ground_truth),
        top_5_agreement: top_k
            .iter()
            .take(5)
            .any(|candidate| candidate == &sample.normalized_ground_truth),
        latency_ms: prediction.latency_ms,
    }
}

fn aggregate<'a>(samples: impl Iterator<Item = &'a FormulaSampleResult>) -> FormulaMetrics {
    let samples: Vec<_> = samples.collect();
    if samples.is_empty() {
        return FormulaMetrics::default();
    }
    let count = samples.len();
    let sum_bool = |predicate: fn(&FormulaSampleResult) -> bool| {
        samples.iter().filter(|sample| predicate(sample)).count() as f64 / count as f64
    };
    let formulas: Vec<_> = samples
        .iter()
        .copied()
        .filter(|sample| sample.expected_kind == FormulaExpectedKind::Formula)
        .collect();
    let negatives: Vec<_> = samples
        .iter()
        .copied()
        .filter(|sample| sample.expected_kind == FormulaExpectedKind::HardNegative)
        .collect();
    let formula_rate = |predicate: fn(&FormulaSampleResult) -> bool| {
        formulas.iter().filter(|sample| predicate(sample)).count() as f64
            / formulas.len().max(1) as f64
    };
    let negative_rate = |predicate: fn(&FormulaSampleResult) -> bool| {
        negatives.iter().filter(|sample| predicate(sample)).count() as f64
            / negatives.len().max(1) as f64
    };
    let character_edits: usize = samples.iter().map(|sample| sample.character_edits).sum();
    let character_count: usize = samples.iter().map(|sample| sample.character_count).sum();
    let token_edits: usize = samples.iter().map(|sample| sample.token_edits).sum();
    let token_count: usize = samples.iter().map(|sample| sample.token_count).sum();
    let cold_latency_ms = samples[0].latency_ms;
    let warm_latency_ms = if count > 1 {
        samples[1..]
            .iter()
            .map(|sample| sample.latency_ms)
            .sum::<f64>()
            / (count - 1) as f64
    } else {
        cold_latency_ms
    };
    let mut latencies: Vec<_> = samples.iter().map(|sample| sample.latency_ms).collect();
    latencies.sort_by(f64::total_cmp);
    FormulaMetrics {
        sample_count: count,
        formula_sample_count: formulas.len(),
        hard_negative_sample_count: negatives.len(),
        exact_match: formula_rate(|sample| sample.exact_match),
        normalized_exact_match: formula_rate(|sample| sample.normalized_exact_match),
        character_error_rate: character_edits as f64 / character_count.max(1) as f64,
        token_error_rate: token_edits as f64 / token_count.max(1) as f64,
        latex_parse_success: sum_bool(|sample| sample.latex_parse_success),
        ast_structure_validity: sum_bool(|sample| sample.ast_structure_valid),
        balanced_delimiter_rate: sum_bool(|sample| sample.balanced_delimiters),
        repeated_token_failure_rate: sum_bool(|sample| sample.repeated_token_failure),
        premature_eos_rate: sum_bool(|sample| sample.premature_eos),
        truncation_rate: sum_bool(|sample| sample.truncated),
        top_1_agreement: sum_bool(|sample| sample.top_1_agreement),
        top_5_agreement: sum_bool(|sample| sample.top_5_agreement),
        false_positive_rate: negative_rate(|sample| sample.false_positive),
        false_negative_rate: formula_rate(|sample| sample.false_negative),
        correction_trigger_rate: formula_rate(|sample| sample.correction_triggered),
        correction_improvement_rate: formula_rate(|sample| sample.correction_improved),
        correction_regression_rate: formula_rate(|sample| sample.correction_regressed),
        review_required_rate: sum_bool(|sample| sample.review_required),
        confidence_sample_count: samples
            .iter()
            .filter(|sample| sample.confidence.is_some())
            .count(),
        confidence_calibration_error: calibration_error(&samples),
        cold_latency_ms,
        warm_latency_ms,
        latency_p50_ms: quantile(&latencies, 0.50),
        latency_p95_ms: quantile(&latencies, 0.95),
    }
}

fn calibration_error(samples: &[&FormulaSampleResult]) -> f64 {
    let confident: Vec<_> = samples
        .iter()
        .copied()
        .filter_map(|sample| {
            sample
                .confidence
                .filter(|value| value.is_finite())
                .map(|confidence| (sample, confidence.clamp(0.0, 1.0)))
        })
        .collect();
    if confident.is_empty() {
        return 0.0;
    }
    (0..10)
        .map(|bin| {
            let lower = bin as f64 / 10.0;
            let upper = (bin + 1) as f64 / 10.0;
            let bucket: Vec<_> = confident
                .iter()
                .copied()
                .filter(|(_, confidence)| {
                    *confidence >= lower
                        && if bin == 9 {
                            *confidence <= upper
                        } else {
                            *confidence < upper
                        }
                })
                .collect();
            if bucket.is_empty() {
                return 0.0;
            }
            let accuracy = bucket
                .iter()
                .filter(|(sample, _)| sample.normalized_exact_match)
                .count() as f64
                / bucket.len() as f64;
            let mean_confidence =
                bucket.iter().map(|(_, confidence)| confidence).sum::<f64>() / bucket.len() as f64;
            (bucket.len() as f64 / confident.len() as f64) * (accuracy - mean_confidence).abs()
        })
        .sum()
}

fn aggregate_groups(
    samples: &[FormulaSampleResult],
    keys: impl Fn(&FormulaSampleResult) -> Vec<&str>,
) -> BTreeMap<String, FormulaMetrics> {
    let mut groups: BTreeMap<String, Vec<&FormulaSampleResult>> = BTreeMap::new();
    for sample in samples {
        for key in keys(sample) {
            groups.entry(key.to_owned()).or_default().push(sample);
        }
    }
    groups
        .into_iter()
        .map(|(key, samples)| (key, aggregate(samples.into_iter())))
        .collect()
}

fn aggregate_optional_groups(
    samples: &[FormulaSampleResult],
    key: impl Fn(&FormulaSampleResult) -> Option<&str>,
) -> BTreeMap<String, FormulaMetrics> {
    let mut groups: BTreeMap<String, Vec<&FormulaSampleResult>> = BTreeMap::new();
    for sample in samples {
        if let Some(key) = key(sample) {
            groups.entry(key.to_owned()).or_default().push(sample);
        }
    }
    groups
        .into_iter()
        .map(|(key, samples)| (key, aggregate(samples.into_iter())))
        .collect()
}

fn strip_outer_math_delimiters(value: &str) -> &str {
    if value.len() >= 4 && value.starts_with("$$") && value.ends_with("$$") {
        value[2..value.len() - 2].trim()
    } else if value.len() >= 2 && value.starts_with('$') && value.ends_with('$') {
        value[1..value.len() - 1].trim()
    } else if value.len() >= 4
        && ((value.starts_with(r"\(") && value.ends_with(r"\)"))
            || (value.starts_with(r"\[") && value.ends_with(r"\]")))
    {
        value[2..value.len() - 2].trim()
    } else {
        value
    }
}

fn delimiters_balanced(value: &str) -> bool {
    let mut stack = Vec::new();
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().copied().enumerate() {
        if is_escaped(bytes, index) {
            continue;
        }
        match byte {
            b'{' | b'[' | b'(' => stack.push(byte),
            b'}' if stack.pop() != Some(b'{') => return false,
            b']' if stack.pop() != Some(b'[') => return false,
            b')' if stack.pop() != Some(b'(') => return false,
            _ => {}
        }
    }
    stack.is_empty() && environments_balanced(value)
}

fn environments_balanced(value: &str) -> bool {
    let mut stack = Vec::new();
    let mut rest = value;
    loop {
        let begin = rest.find(r"\begin{").map(|index| (index, true));
        let end = rest.find(r"\end{").map(|index| (index, false));
        let Some((index, is_begin)) = [begin, end].into_iter().flatten().min_by_key(|item| item.0)
        else {
            return stack.is_empty();
        };
        let name_start = index + if is_begin { 7 } else { 5 };
        let after = &rest[name_start..];
        let Some(name_end) = after.find('}') else {
            return false;
        };
        let name = &after[..name_end];
        if is_begin {
            stack.push(name.to_owned());
        } else if stack.pop().as_deref() != Some(name) {
            return false;
        }
        rest = &after[name_end + 1..];
    }
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

fn formula_tokens(value: &str) -> Vec<String> {
    let chars: Vec<_> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\\' {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_alphabetic() {
                index += 1;
            }
            if index == start + 1 && index < chars.len() {
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
        } else if chars[index].is_alphanumeric() {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_alphanumeric() {
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
        } else if chars[index].is_whitespace() {
            index += 1;
        } else {
            tokens.push(chars[index].to_string());
            index += 1;
        }
    }
    tokens
}

fn repeated_token_failure(tokens: &[String]) -> bool {
    let mut previous: Option<&str> = None;
    let mut run = 0usize;
    for token in tokens {
        if previous == Some(token.as_str()) {
            run += 1;
        } else {
            previous = Some(token);
            run = 1;
        }
        if run >= 8 {
            return true;
        }
    }
    false
}

fn sequence_length_bucket(length: usize) -> &'static str {
    match length {
        0..=15 => "short",
        16..=40 => "medium",
        _ => "long",
    }
}

fn quantile(sorted: &[f64], quantile: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * quantile).round() as usize]
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn levenshtein<T: Eq>(left: &[T], right: &[T]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_value) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_value) in right.iter().enumerate() {
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + usize::from(left_value != right_value));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> NormalizationRules {
        NormalizationRules {
            schema_version: 1,
            version: "formula-normalization-v1".to_owned(),
            trim: true,
            strip_outer_math_delimiters: true,
            collapse_ascii_whitespace: false,
            command_aliases: BTreeMap::new(),
        }
    }

    #[test]
    fn normalization_is_conservative_and_versioned() {
        assert_eq!(
            normalize_formula(r"  $$\frac{a}{b}$$ ", &rules()),
            r"\frac{a}{b}"
        );
        assert_ne!(normalize_formula(r"\dfrac{a}{b}", &rules()), r"\frac{a}{b}");
    }

    #[test]
    fn delimiter_validation_handles_environments() {
        assert!(delimiters_balanced(r"\begin{matrix}a&b\\c&d\end{matrix}"));
        assert!(!delimiters_balanced(r"\begin{matrix}a&b\end{cases}"));
        assert!(!delimiters_balanced(r"\frac{a}{b"));
    }

    #[test]
    fn repeated_tokens_require_a_long_run() {
        assert!(repeated_token_failure(&vec!["x".to_owned(); 8]));
        assert!(!repeated_token_failure(&vec!["x".to_owned(); 7]));
    }

    #[test]
    fn aggregate_uses_micro_error_rates() {
        let sample = FormulaSampleResult {
            sample_id: "one".to_owned(),
            categories: vec!["simple_inline".to_owned()],
            image_quality: "clean".to_owned(),
            expected_kind: FormulaExpectedKind::Formula,
            screenshot_scale: None,
            degradation: None,
            sequence_length: 1,
            raw: "b".to_owned(),
            normalized_raw: "b".to_owned(),
            corrected: "b".to_owned(),
            expected: "a".to_owned(),
            actual: "b".to_owned(),
            normalized_expected: "a".to_owned(),
            normalized_actual: "b".to_owned(),
            exact_match: false,
            normalized_exact_match: false,
            raw_normalized_exact_match: false,
            correction_triggered: false,
            correction_improved: false,
            correction_regressed: false,
            review_required: false,
            confidence: Some(0.2),
            false_positive: false,
            false_negative: false,
            character_edits: 1,
            character_count: 1,
            token_edits: 1,
            token_count: 1,
            latex_parse_success: true,
            ast_structure_valid: true,
            balanced_delimiters: true,
            repeated_token_failure: false,
            premature_eos: false,
            truncated: false,
            top_1_agreement: false,
            top_5_agreement: true,
            latency_ms: 5.0,
        };
        let metrics = aggregate([&sample].into_iter());
        assert_eq!(metrics.character_error_rate, 1.0);
        assert_eq!(metrics.top_5_agreement, 1.0);
        assert_eq!(metrics.latency_p95_ms, 5.0);
    }

    #[test]
    fn checked_in_manifest_has_fifty_verified_images_and_required_categories() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/formula-recognition/v1/manifest.json");
        let manifest: FormulaBenchmarkManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        validate_formula_manifest_files(&manifest_path, &manifest).unwrap();
        assert_eq!(manifest.samples.len(), 50);
        let counts = manifest
            .samples
            .iter()
            .flat_map(|sample| sample.category.iter())
            .fold(BTreeMap::<&str, usize>::new(), |mut counts, category| {
                *counts.entry(category).or_default() += 1;
                counts
            });
        for (category, minimum) in [
            ("simple_inline", 8),
            ("fractions_roots", 6),
            ("superscript_subscript", 6),
            ("integral_sum_limit", 6),
            ("matrix_piecewise", 6),
            ("greek_font_variant", 4),
            ("long_multi_relation", 5),
            ("blurred_low_resolution", 4),
            ("tilted_perspective_noise", 3),
            ("mixed_chinese_english", 2),
        ] {
            assert!(
                counts.get(category).copied().unwrap_or_default() >= minimum,
                "{category} count is below {minimum}"
            );
        }
    }
}
